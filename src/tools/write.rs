use std::cell::Cell;

use serde_json::{Value, json};

use crate::compose::{Assembly, ImageSource};
use crate::edit::{Lock, Permissions, Plan, Save};
use crate::{Document, Error, compose, edit, mathpix};

use super::{
    Ctx, Output, Quote, ToolError, attachment, bad_args, decode, file, languages, open, open_main,
    report, secret, selected,
};

fn pdf_output(bytes: Vec<u8>, save: &Save, ctx: &Ctx<'_>) -> Result<Output, ToolError> {
    ctx.cancel.check()?;
    let doc = match &save.lock {
        Some(lock) => Document::from_bytes_with_password(&bytes, &lock.owner)?,
        None => Document::from_bytes(&bytes)?,
    };
    let count = doc.page_count();
    Ok(Output {
        text: format!("已生成 {count} 页 PDF，见附件 `output.pdf`。"),
        data: json!({"page_count":count, "attachment":"output.pdf"}),
        attachments: vec![attachment("output.pdf", "application/pdf", bytes)],
        ..Output::default()
    })
}

pub(super) fn edit(
    _: &mathpix::Options,
    args: &Value,
    ctx: &Ctx<'_>,
    _: Option<&Quote>,
) -> Result<Output, ToolError> {
    let plan: Plan = decode(&args["plan"], "请按 PDF 改动单格式提供 plan。")?;
    if !plan.redact.is_empty() {
        return Err(bad_args("永久涂黑请单独使用 pdf_redact，并请用户确认。"));
    }
    let main = open_main(args, ctx)?;
    let sources = args["sources"].as_array().expect("清单校验过的附加文件");
    let others = sources
        .iter()
        .enumerate()
        .map(|(index, value)| {
            let file = value.as_str().expect("清单校验过的文件标识");
            open(file, ctx, &format!("pdf_source_{}_password", index + 1))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let others = others.iter().collect::<Vec<_>>();
    let save = Save {
        flatten: args["flatten"] == true,
        remove_password: args["password_action"] == "remove",
        lock: if args["password_action"] == "set" {
            let file = file(args);
            let owner = secret(
                ctx,
                file,
                "pdf_new_owner_password",
                "pdf_new_owner_password",
            )
            .ok_or_else(|| bad_args("请让用户从调用上下文提供新的 PDF 权限密码。"))?;
            let user = secret(ctx, file, "pdf_new_user_password", "pdf_new_user_password");
            let permissions = &args["permissions"];
            Some(Lock {
                owner,
                user,
                permissions: Permissions {
                    print: permissions["print"].as_bool().unwrap_or(true),
                    copy: permissions["copy"].as_bool().unwrap_or(true),
                    modify: permissions["modify"].as_bool().unwrap_or(true),
                    annotate: permissions["annotate"].as_bool().unwrap_or(true),
                },
            })
        } else if args.get("permissions").is_some() {
            return Err(bad_args("只在 password_action 为 set 时指定新权限。"));
        } else {
            None
        },
    };
    pdf_output(
        edit::apply(&main, &others, &plan, &save, ctx.cancel)?,
        &save,
        ctx,
    )
}

pub(super) fn redact(
    _: &mathpix::Options,
    args: &Value,
    ctx: &Ctx<'_>,
    _: Option<&Quote>,
) -> Result<Output, ToolError> {
    if !cfg!(feature = "pdfium") {
        return Err(Error::PdfiumUnavailable.into());
    }
    let areas: Vec<edit::Redaction> = decode(&args["areas"], "请为每个物理页提供有效的涂黑框。")?;
    if areas.is_empty() || areas.iter().any(|area| area.rects.is_empty()) {
        return Err(bad_args("请至少选择一个要永久涂黑的区域。"));
    }
    let doc = open_main(args, ctx)?;
    let plan = Plan {
        redact: areas,
        ..Plan::default()
    };
    let save = Save::default();
    pdf_output(
        edit::apply(&doc, &[], &plan, &save, ctx.cancel)?,
        &save,
        ctx,
    )
}

pub(super) fn searchable(
    _: &mathpix::Options,
    args: &Value,
    ctx: &Ctx<'_>,
    _: Option<&Quote>,
) -> Result<Output, ToolError> {
    let doc = open_main(args, ctx)?;
    let mut pages = selected(args)?;
    let mut seen = std::collections::HashSet::new();
    pages.retain(|page| seen.insert(*page));
    let count = Cell::new(0);
    let bytes = crate::searchable(&doc, &pages, &languages(args), ctx.cancel, &|page| {
        count.set(count.get() + 1);
        report(
            ctx,
            count.get(),
            pages.len(),
            format!("已处理第 {page} 页。"),
        );
    })?;
    pdf_output(bytes, &Save::default(), ctx)
}

pub(super) fn images(
    _: &mathpix::Options,
    args: &Value,
    ctx: &Ctx<'_>,
    _: Option<&Quote>,
) -> Result<Output, ToolError> {
    let files: Vec<String> = decode(&args["files"], "请按顺序提供已授权的图片文件。")?;
    let bytes = read_sources(&files, ctx)?;
    let images = bytes
        .iter()
        .map(|file| ImageSource::Bytes(file))
        .collect::<Vec<_>>();
    pdf_output(compose::images(&images, ctx.cancel)?, &Save::default(), ctx)
}

pub(super) fn assemble(
    _: &mathpix::Options,
    args: &Value,
    ctx: &Ctx<'_>,
    _: Option<&Quote>,
) -> Result<Output, ToolError> {
    let files: Vec<String> = decode(&args["files"], "请按顺序提供已授权的 PDF 文件。")?;
    let bytes = read_sources(&files, ctx)?;
    let parts = bytes.iter().map(Vec::as_slice).collect::<Vec<_>>();
    let how = Assembly {
        links: if let Some(links) = args.get("links") {
            decode(links, "请使用输出文件的物理页码指定链接位置和目标。")?
        } else {
            Vec::new()
        },
        outline: decode(&args["outline"], "请使用输出文件的物理页码指定目录目标。")?,
        info: decode(&args["info"], "请提供有效的 PDF 元数据。")?,
    };
    pdf_output(
        compose::assemble(&parts, &how, ctx.cancel)?,
        &Save::default(),
        ctx,
    )
}

fn read_sources(files: &[String], ctx: &Ctx<'_>) -> Result<Vec<Vec<u8>>, ToolError> {
    if files.is_empty() {
        return Err(bad_args("请至少选择一份输入文件。"));
    }
    files
        .iter()
        .map(|file| {
            ctx.cancel.check()?;
            ctx.files.read(file)
        })
        .collect()
}

pub(super) fn split(
    _: &mathpix::Options,
    args: &Value,
    ctx: &Ctx<'_>,
    _: Option<&Quote>,
) -> Result<Output, ToolError> {
    let doc = open_main(args, ctx)?;
    pdf_output(
        doc.split(&selected(args)?, ctx.cancel)?,
        &Save::default(),
        ctx,
    )
}
