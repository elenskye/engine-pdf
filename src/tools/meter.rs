use std::fmt::Write;

use serde_json::{Value, json};

use crate::{Cancel, Document, Error, mathpix};

use super::{
    Ctx, Files, Output, Progress, Quote, ToolError, Usage, attachment, bad_args, file, selected,
};

pub(super) fn pages(
    _: &mathpix::Options,
    args: &Value,
    files: &dyn Files,
    cancel: &Cancel,
) -> Result<f64, ToolError> {
    let doc = open(args, files, cancel)?;
    let quote = mathpix::quote(&doc, &selected(args)?)?;
    Ok(quote.pages().len() as f64)
}

fn open(args: &Value, files: &dyn Files, cancel: &Cancel) -> Result<Document, ToolError> {
    cancel.check()?;
    let bytes = files.read(file(args))?;
    cancel.check()?;
    let doc = match Document::from_bytes(&bytes) {
        Err(Error::Locked) => return Err(ToolError::of("password_action_required")),
        result => result?,
    };
    if doc.info().encrypted {
        return Err(ToolError::of("password_action_required"));
    }
    Ok(doc)
}

/// 这次调用的页单价：App 在 Prices 里改过就用改过的，否则用清单的默认价。
/// Core 对零金额不问确认，所以两条路径都在本机拒绝非正单价。
fn price(ctx: &Ctx<'_>) -> Result<f64, ToolError> {
    let price = ctx
        .prices
        .get(mathpix::SERVICE, mathpix::UNIT)
        .unwrap_or(mathpix::PRICE_PER_PAGE);
    if price.is_finite() && price > 0.0 {
        Ok(price)
    } else {
        Err(bad_args("请在宿主设置中提供大于零的 Mathpix 页单价。"))
    }
}

pub(super) fn quote(
    _: &mathpix::Options,
    args: &Value,
    ctx: &Ctx<'_>,
    _: Option<&Quote>,
) -> Result<Output, ToolError> {
    let price = price(ctx)?;
    let doc = open(args, ctx.files, ctx.cancel)?;
    let native = mathpix::quote_at_price(&doc, &selected(args)?, price)?;
    let quote = Quote::from(&native);
    let mut data = json!(quote);
    data["pages"] = json!(native.pages());
    data["usd_per_unit"] = json!(price);
    Ok(Output {
        text: format!(
            "Mathpix 选中物理页 {}，每页 ${price}，预计 ${}。",
            native.pages_text(),
            quote.usd
        ),
        data,
        ..Output::default()
    })
}

pub(super) fn run(
    options: &mathpix::Options,
    args: &Value,
    ctx: &Ctx<'_>,
    quote: Option<&Quote>,
) -> Result<Output, ToolError> {
    let quote = quote.expect("计费工具已报价");
    let price = price(ctx)?;
    let doc = open(args, ctx.files, ctx.cancel)?;
    let opts = mathpix::Options {
        price_per_page: price,
        lines: args["lines"] == true,
        mmd_zip: args["mmd_zip"] == true,
        ..options.clone()
    };
    let native = mathpix::quote_with_options(&doc, &selected(args)?, &opts)?;
    if native.pages().len() as f64 != quote.units {
        return Err(bad_args("文件或选页已变化，请重新报价并确认。"));
    }
    let key = |name| ctx.keys.get(name).ok_or_else(|| ToolError::of("auth"));
    let keys = mathpix::Keys::from_zeroizing(key("mathpix_app_id")?, key("mathpix_app_key")?);
    let recognized = mathpix::run(&doc, &native, &keys, &opts, ctx.cancel, &|progress| {
        (ctx.progress)(Progress {
            completed: progress.num_pages_completed.unwrap_or_default().into(),
            total: progress.num_pages.map(u64::from),
            message: "Mathpix 正在识别。".into(),
        });
    })?;
    let mapping = match recognized.mapping {
        mathpix::Mapping::Exact => "exact",
        mathpix::Mapping::Approximate => "approximate",
    };
    let mut text = format!("Mathpix 已处理物理页 {}。", native.pages_text());
    if recognized.mapping == mathpix::Mapping::Approximate {
        text.push_str("\n页码映射仅供参考。");
    }
    for page in &recognized.pages {
        let summary = page.mmd.chars().take(240).collect::<String>();
        if !summary.trim().is_empty() {
            let _ = write!(text, "\n\n## 第 {} 页\n\n{summary}", page.index);
        }
    }
    let mut data = json!({
        "pages": native.pages(), "mapping":mapping, "pdf_id":recognized.usage.pdf_id,
        "remote_deleted":recognized.usage.remote_deleted, "mmd_attachment":"mathpix.mmd",
    });
    let lines = recognized.lines_json();
    let mut attachments = vec![attachment(
        "mathpix.mmd",
        "text/plain",
        recognized.mmd.into_bytes(),
    )];
    if let Some(lines) = lines {
        data["lines_attachment"] = json!("mathpix-lines.json");
        attachments.push(attachment(
            "mathpix-lines.json",
            "application/json",
            lines.to_string().into_bytes(),
        ));
    }
    if let Some(zip) = recognized.mmd_zip {
        data["zip_attachment"] = json!("mathpix-mmd.zip");
        attachments.push(attachment("mathpix-mmd.zip", "application/zip", zip));
    }
    Ok(Output {
        text,
        data,
        attachments,
        usage: Some(Usage::from(&recognized.usage)),
        next: None,
    })
}
