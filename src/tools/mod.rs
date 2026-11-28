//! PDF 工具清单、调用入口和宿主约定。见 docs/toolkit.md。

mod find;
mod meter;
mod read;
mod write;

use std::sync::LazyLock;

pub use engine_core::call::{
    Approve, Attachment, AttachmentContent, Ctx, Files, Keys, Output, Prepared, Prices, Progress,
    Quote, ToolError, Toolkit, Usage,
};
pub use engine_core::overrides::{Override, Overrides, to_anthropic, to_openai};
pub use engine_core::tool::{Confirm, ToolSpec};

use engine_core::call::Handler;
use serde::de::DeserializeOwned;
use serde_json::Value;
use zeroize::Zeroizing;

use crate::{Document, mathpix};

pub fn manifest() -> &'static [ToolSpec] {
    toolkit().manifest()
}

pub fn call(name: &str, args: &Value, ctx: &Ctx<'_>) -> Result<Output, ToolError> {
    toolkit().call(name, args, ctx)
}

pub fn toolkit() -> &'static Toolkit<mathpix::Options> {
    static DEFAULT: LazyLock<Toolkit<mathpix::Options>> =
        LazyLock::new(|| toolkit_with_mathpix(mathpix::Options::default()));
    &DEFAULT
}

/// 命令行用的：花钱的工具不加 `--yes` 只报价（总仓库 GUIDE「一」），每次都先问；另照 `mathpix::spend` 的规矩。
/// App 给模型用 `toolkit_with_mathpix`：照规矩该问时才问。
pub fn terminal_toolkit(options: mathpix::Options) -> Toolkit<mathpix::Options> {
    let always = Override {
        confirm: Some(Confirm::Always),
        ..Override::default()
    };
    toolkit_with_mathpix(options)
        .with_overrides(&[("pdf_mathpix".to_string(), always)].into())
        .expect("内置的命令行覆盖")
}

/// 给 App 注入 Mathpix 选项，或在同一套入口上模拟传输。单价由调用上下文的 Prices 决定。
pub fn toolkit_with_mathpix(options: mathpix::Options) -> Toolkit<mathpix::Options> {
    let handlers = [
        (
            "pdf_info",
            include_str!("../../tools/pdf_info.json"),
            Handler::new(read::info),
        ),
        (
            "pdf_read",
            include_str!("../../tools/pdf_read.json"),
            Handler::new(read::read),
        ),
        (
            "pdf_render",
            include_str!("../../tools/pdf_render.json"),
            Handler::new(read::render),
        ),
        (
            "pdf_ocr",
            include_str!("../../tools/pdf_ocr.json"),
            Handler::new(read::ocr),
        ),
        (
            "pdf_outline",
            include_str!("../../tools/pdf_outline.json"),
            Handler::new(read::outline),
        ),
        (
            "pdf_annotations",
            include_str!("../../tools/pdf_annotations.json"),
            Handler::new(read::annotations),
        ),
        (
            "pdf_fields",
            include_str!("../../tools/pdf_fields.json"),
            Handler::new(read::fields),
        ),
        (
            "pdf_objects",
            include_str!("../../tools/pdf_objects.json"),
            Handler::new(read::objects),
        ),
        (
            "pdf_edit",
            include_str!("../../tools/pdf_edit.json"),
            Handler::new(write::edit),
        ),
        (
            "pdf_redact",
            include_str!("../../tools/pdf_redact.json"),
            Handler::new(write::redact),
        ),
        (
            "pdf_searchable",
            include_str!("../../tools/pdf_searchable.json"),
            Handler::new(write::searchable),
        ),
        (
            "pdf_compose_images",
            include_str!("../../tools/pdf_compose_images.json"),
            Handler::new(write::images),
        ),
        (
            "pdf_assemble",
            include_str!("../../tools/pdf_assemble.json"),
            Handler::new(write::assemble),
        ),
        (
            "pdf_split",
            include_str!("../../tools/pdf_split.json"),
            Handler::new(write::split),
        ),
        (
            "pdf_find",
            include_str!("../../tools/pdf_find.json"),
            Handler::new(find::find),
        ),
        (
            "pdf_mathpix_quote",
            include_str!("../../tools/pdf_mathpix_quote.json"),
            Handler::new(meter::quote),
        ),
        (
            "pdf_mathpix",
            include_str!("../../tools/pdf_mathpix.json"),
            Handler::metered(meter::run, meter::pages),
        ),
    ];
    let mut manifest = Vec::with_capacity(handlers.len());
    let implementations = handlers
        .into_iter()
        .map(|(name, source, handler)| {
            manifest.push(serde_json::from_str(source).expect("内置 PDF 工具清单 JSON"));
            (name, handler)
        })
        .collect::<Vec<_>>();
    Toolkit::new(options, manifest, implementations).expect("内置 PDF 工具清单与实现一致")
}

fn bad_args(hint: &str) -> ToolError {
    ToolError::new("bad_args", "PDF 工具参数不合规。", hint, false)
}

fn decode<T: DeserializeOwned>(value: &Value, hint: &str) -> Result<T, ToolError> {
    serde_json::from_value(value.clone()).map_err(|_| bad_args(hint))
}

fn file(args: &Value) -> &str {
    args["file"].as_str().expect("清单校验过的文件标识")
}

fn secret(ctx: &Ctx<'_>, file: &str, kind: &str, fallback: &str) -> Option<Zeroizing<String>> {
    ctx.keys
        .get(&format!("{kind}:{file}"))
        .or_else(|| ctx.keys.get(fallback))
}

fn open(file: &str, ctx: &Ctx<'_>, password_slot: &str) -> Result<Document, ToolError> {
    ctx.cancel.check()?;
    let bytes = ctx.files.read(file)?;
    ctx.cancel.check()?;
    match secret(ctx, file, "pdf_password", password_slot) {
        Some(password) => Ok(Document::from_bytes_with_password(&bytes, &password)?),
        None => Ok(Document::from_bytes(&bytes)?),
    }
}

fn open_main(args: &Value, ctx: &Ctx<'_>) -> Result<Document, ToolError> {
    open(file(args), ctx, "pdf_password")
}

/// 清单校验过、补过默认值的非负整数参数（页码、像素、分页位置）。
fn uint(args: &Value, name: &str) -> u32 {
    args[name].as_u64().expect("清单校验过的整数参数") as u32
}

fn selected(args: &Value) -> Result<Vec<u32>, ToolError> {
    let pages: Vec<u32> = decode(&args["pages"], "请按原文件物理页选择页面。")?;
    if pages.is_empty() {
        return Err(bad_args("请至少选择一页。"));
    }
    Ok(pages)
}

fn languages(args: &Value) -> crate::OcrOptions {
    crate::OcrOptions {
        languages: serde_json::from_value(args["languages"].clone()).expect("清单校验过的语言"),
    }
}

fn report(ctx: &Ctx<'_>, completed: usize, total: usize, message: String) {
    (ctx.progress)(Progress {
        completed: completed as u64,
        total: Some(total as u64),
        message,
    });
}

fn attachment(filename: impl Into<String>, mime_type: &str, bytes: Vec<u8>) -> Attachment {
    Attachment {
        filename: filename.into(),
        mime_type: mime_type.into(),
        content: AttachmentContent::Bytes(bytes),
    }
}
