use std::fmt::Write;

use serde_json::{Value, json};

#[cfg(feature = "pdfium")]
use crate::edit::ObjectKind;
use crate::edit::{AnnotationKind, OutlineItem, Target};
use crate::{Content, Error, Flow, Image, ImageFormat, Ocr, ReadOptions, Render, mathpix};

use super::{
    Ctx, Output, Quote, ToolError, attachment, file, languages, open_main, report, selected, uint,
};

pub(super) fn info(
    _: &mathpix::Options,
    args: &Value,
    ctx: &Ctx<'_>,
    _: Option<&Quote>,
) -> Result<Output, ToolError> {
    let doc = open_main(args, ctx)?;
    let info = doc.info();
    let mut out = Output {
        text: format!(
            "{}页 PDF，{}。",
            doc.page_count(),
            if info.encrypted {
                "已加密"
            } else {
                "未加密"
            }
        ),
        data: json!({"page_count": doc.page_count(), "info": info}),
        ..Output::default()
    };
    for (name, value) in [("标题", info.title), ("作者", info.author)] {
        if let Some(value) = value {
            let _ = write!(out.text, "\n- {name}　{value}");
        }
    }
    if args["image_counts"] == true {
        let counts = doc.image_counts(ctx.cancel)?;
        let mut csv = String::from("page,image_count\n");
        for (page, count) in &counts {
            let _ = writeln!(csv, "{page},{count}");
        }
        let _ = write!(out.text, "\n{} 页包含图片。", counts.len());
        out.data["pages_with_images"] = json!(counts.len());
        out.data["image_counts_attachment"] = json!("image-counts.csv");
        out.attachments
            .push(attachment("image-counts.csv", "text/csv", csv.into_bytes()));
    }
    Ok(out)
}

fn image_attachment(out: &mut Output, name: &str, image: Image) -> String {
    let (extension, mime) = match image.format {
        ImageFormat::Jpeg => ("jpg", "image/jpeg"),
        ImageFormat::Png => ("png", "image/png"),
        ImageFormat::Gray => unreachable!("工具清单只支持 JPEG 和 PNG"),
    };
    let name = format!("{name}.{extension}");
    out.attachments.push(attachment(&name, mime, image.bytes));
    name
}

fn excerpt(text: &str) -> (&str, bool) {
    match text.char_indices().nth(4000) {
        Some((end, _)) => (&text[..end], true),
        None => (text, false),
    }
}

fn heading(markdown: &mut String, index: u32, label: Option<&str>) {
    let _ = write!(markdown, "## 第 {index} 页");
    if let Some(label) = label {
        let _ = write!(markdown, "（{label}）");
    }
    markdown.push_str("\n\n");
}

pub(super) fn read(
    _: &mathpix::Options,
    args: &Value,
    ctx: &Ctx<'_>,
    _: Option<&Quote>,
) -> Result<Output, ToolError> {
    let doc = open_main(args, ctx)?;
    let start = uint(args, "start_page");
    let total = doc.page_count();
    let implicit = args.get("pages").is_none();
    let pages = if implicit {
        if start > total {
            return Err(Error::PageOutOfRange { page: start, total }.into());
        }
        (start..=start.saturating_add(4).min(total)).collect()
    } else {
        let pages = selected(args)?;
        if let Some(&page) = pages.iter().find(|&&page| page > total) {
            return Err(Error::PageOutOfRange { page, total }.into());
        }
        pages
    };
    let max_side = uint(args, "max_side");
    let opts = ReadOptions {
        render: (args["images"] == true).then(|| {
            if args["format"] == "png" {
                Render::png(max_side)
            } else {
                Render::jpeg(max_side)
            }
        }),
        ocr: if args["ocr"] == true {
            Ocr::Auto(languages(args))
        } else {
            Ocr::Off
        },
        ..ReadOptions::default()
    };

    let mut data = Vec::with_capacity(pages.len());
    let mut out = Output::default();
    crate::read_each(&doc, &pages, &opts, ctx.cancel, |read| {
        heading(&mut out.text, read.index, read.label.as_deref());
        let name = format!("page-{}-{}", read.index, data.len() + 1);
        let mut record = json!({"index": read.index});
        if let Some(label) = read.label {
            record["label"] = json!(label);
        }
        let (source, text) = match read.content {
            Content::Text(text) => ("text", text),
            Content::SearchableText(text) => ("ocr", text),
            Content::Ocr(result) => ("ocr", result.text),
            Content::Blank => {
                out.text.push_str("空白页。\n");
                ("blank", String::new())
            }
            Content::NoText { partial, .. } => {
                out.text.push_str("本页没有可用的完整文字。\n");
                ("no_text", partial)
            }
            Content::Failed(error) => {
                let _ = writeln!(out.text, "{error}");
                record["error"] = json!(ToolError::from(error));
                ("failed", String::new())
            }
        };
        record["source"] = json!(source);
        if !text.is_empty() {
            let (visible, truncated) = excerpt(&text);
            let _ = writeln!(out.text, "{visible}");
            record["text"] = json!(visible);
            if truncated {
                let path = format!("{name}.txt");
                record["text_attachment"] = json!(path);
                let _ = writeln!(out.text, "\n全文见附件 `{path}`。");
                out.attachments
                    .push(attachment(path, "text/plain", text.into_bytes()));
            }
        }
        if let Some(image) = read.image {
            record["image_attachment"] = json!(image_attachment(&mut out, &name, image));
        }
        out.text.push('\n');
        data.push(record);
        report(
            ctx,
            data.len(),
            pages.len(),
            format!("已读取第 {} 页。", read.index),
        );
        Flow::Continue
    })?;
    ctx.cancel.check()?;
    out.data = json!({"pages": data});
    if let Some(&last) = pages.last().filter(|&&last| implicit && last < total) {
        out.next = Some(json!({
            "file": file(args), "start_page": last + 1, "images": args["images"],
            "format": args["format"], "max_side": args["max_side"], "ocr": args["ocr"],
            "languages": args["languages"],
        }));
    }
    Ok(out)
}

pub(super) fn render(
    _: &mathpix::Options,
    args: &Value,
    ctx: &Ctx<'_>,
    _: Option<&Quote>,
) -> Result<Output, ToolError> {
    let doc = open_main(args, ctx)?;
    let page_number = uint(args, "page");
    let format = if args["format"] == "png" {
        ImageFormat::Png
    } else {
        ImageFormat::Jpeg
    };
    let image = doc.render(page_number, format, uint(args, "max_side"))?;
    ctx.cancel.check()?;
    let (width, height) = (image.width, image.height);
    let mut out = Output {
        text: format!("第 {page_number} 页，{width} × {height} 像素。"),
        ..Output::default()
    };
    let name = image_attachment(&mut out, &format!("page-{page_number}"), image);
    out.data = json!({"page":page_number, "format":args["format"], "width":width, "height":height, "attachment":name});
    Ok(out)
}

pub(super) fn ocr(
    _: &mathpix::Options,
    args: &Value,
    ctx: &Ctx<'_>,
    _: Option<&Quote>,
) -> Result<Output, ToolError> {
    let doc = open_main(args, ctx)?;
    let pages = selected(args)?;
    let options = languages(args);
    let mut out = Output::default();
    let mut records = Vec::with_capacity(pages.len());
    for page in pages.iter().copied() {
        ctx.cancel.check()?;
        let ocr = doc.ocr(page, &options);
        ctx.cancel.check()?;
        let ocr = ocr?;
        let label = doc.label(page);
        heading(&mut out.text, page, label.as_deref());
        let (visible, truncated) = excerpt(&ocr.text);
        let _ = write!(out.text, "识别置信度 {:.0}%。\n\n", ocr.confidence * 100.0);
        if visible.is_empty() {
            out.text.push_str("未识别到文字。\n");
        } else {
            let _ = writeln!(out.text, "{visible}");
            if truncated {
                out.text.push_str("\n完整结果见结构化的识别行。\n");
            }
        }
        out.text.push('\n');
        let mut record = json!({"index":page, "confidence":ocr.confidence, "lines":ocr.lines});
        if let Some(label) = label {
            record["label"] = json!(label);
        }
        records.push(record);
        report(
            ctx,
            records.len(),
            pages.len(),
            format!("已识别第 {page} 页。"),
        );
    }
    ctx.cancel.check()?;
    out.data = json!({"pages": records});
    Ok(out)
}

fn item_range<T>(items: &[T], args: &Value) -> std::ops::Range<usize> {
    let start = (uint(args, "offset") as usize).min(items.len());
    start
        ..start
            .saturating_add(uint(args, "limit") as usize)
            .min(items.len())
}

fn items(
    args: &Value,
    page_number: Option<u32>,
    title: &str,
    text: String,
    records: Value,
    end: usize,
    total: usize,
) -> Output {
    let mut data = json!({"items":records, "total":total});
    let mut next =
        (end < total).then(|| json!({"file":file(args), "offset":end, "limit":args["limit"]}));
    if let Some(page) = page_number {
        data["page"] = json!(page);
        if let Some(next) = &mut next {
            next["page"] = json!(page);
        }
    }
    Output {
        text: format!("{title}共 {total} 项。{text}"),
        data,
        next,
        ..Output::default()
    }
}

fn outline_items<'a>(
    entries: &'a [OutlineItem],
    path: &mut Vec<usize>,
    out: &mut Vec<(Vec<usize>, &'a OutlineItem)>,
    ctx: &Ctx<'_>,
) -> Result<(), ToolError> {
    for (index, item) in entries.iter().enumerate() {
        ctx.cancel.check()?;
        path.push(index + 1);
        out.push((path.clone(), item));
        outline_items(&item.children, path, out, ctx)?;
        path.pop();
    }
    Ok(())
}

pub(super) fn outline(
    _: &mathpix::Options,
    args: &Value,
    ctx: &Ctx<'_>,
    _: Option<&Quote>,
) -> Result<Output, ToolError> {
    let mut entries = Vec::new();
    let outline = open_main(args, ctx)?.outline();
    outline_items(&outline, &mut Vec::new(), &mut entries, ctx)?;
    let range = item_range(&entries, args);
    let mut text = String::new();
    let candidates = entries[range.clone()]
        .iter()
        .map(|(path, item)| {
            let OutlineItem { title, target, .. } = item;
            let destination = match target {
                Target::Page { page, .. } => format!("第 {page} 页"),
                Target::Url { url } => url.clone(),
                Target::None => "无阅读目标".into(),
            };
            let _ = write!(
                text,
                "\n{}- {title}（{destination}）",
                "  ".repeat(path.len() - 1)
            );
            json!({"path":path,"title":title,"target":target})
        })
        .collect::<Vec<_>>();
    Ok(items(
        args,
        None,
        "目录",
        text,
        json!(candidates),
        range.end,
        entries.len(),
    ))
}

pub(super) fn annotations(
    _: &mathpix::Options,
    args: &Value,
    ctx: &Ctx<'_>,
    _: Option<&Quote>,
) -> Result<Output, ToolError> {
    let page = uint(args, "page");
    let entries = open_main(args, ctx)?.annotations(page)?;
    let range = item_range(&entries, args);
    let mut text = String::new();
    for entry in &entries[range.clone()] {
        let label = match &entry.kind {
            AnnotationKind::Highlight => "高亮",
            AnnotationKind::Underline => "下划线",
            AnnotationKind::Strikeout => "删除线",
            AnnotationKind::Note => "便签",
            AnnotationKind::TextBox => "文本框",
            AnnotationKind::Ink => "绘图",
            AnnotationKind::Rectangle => "矩形",
            AnnotationKind::Ellipse => "椭圆",
            AnnotationKind::Line => "直线",
            AnnotationKind::Arrow => "箭头",
            AnnotationKind::Image => "图章",
            AnnotationKind::Link { .. } => "链接",
            AnnotationKind::Other { subtype } => subtype,
        };
        let _ = write!(
            text,
            "\n- #{} {label}（{:.1}, {:.1}）{}",
            entry.id.index,
            entry.rect.x,
            entry.rect.y,
            entry.contents.as_deref().unwrap_or_default()
        );
    }
    Ok(items(
        args,
        Some(page),
        &format!("第 {page} 页标注"),
        text,
        json!(&entries[range.clone()]),
        range.end,
        entries.len(),
    ))
}

pub(super) fn fields(
    _: &mathpix::Options,
    args: &Value,
    ctx: &Ctx<'_>,
    _: Option<&Quote>,
) -> Result<Output, ToolError> {
    let entries = open_main(args, ctx)?.fields(ctx.cancel)?;
    let range = item_range(&entries, args);
    let mut text = String::new();
    for entry in &entries[range.clone()] {
        let kind = json!(entry.kind);
        let value = json!(entry.value);
        let _ = write!(
            text,
            "\n- {}（{}） {}",
            entry.name,
            kind.as_str().unwrap_or_default(),
            value
        );
    }
    Ok(items(
        args,
        None,
        "表单字段",
        text,
        json!(&entries[range.clone()]),
        range.end,
        entries.len(),
    ))
}

#[cfg(feature = "pdfium")]
pub(super) fn objects(
    _: &mathpix::Options,
    args: &Value,
    ctx: &Ctx<'_>,
    _: Option<&Quote>,
) -> Result<Output, ToolError> {
    let page = uint(args, "page");
    let entries = open_main(args, ctx)?.objects(page)?;
    let range = item_range(&entries, args);
    let mut text = String::new();
    for entry in &entries[range.clone()] {
        let kind = match entry.kind {
            ObjectKind::Text => "文字",
            ObjectKind::Image => "图片",
            ObjectKind::Path => "路径",
            ObjectKind::Other => "其他对象",
        };
        let _ = write!(
            text,
            "\n- #{} {kind}（{:.1}, {:.1}）",
            entry.id, entry.bounds.x, entry.bounds.y
        );
        if let Some(content) = &entry.text {
            let mut shown = content.chars().take(60).collect::<String>();
            if shown.len() < content.len() {
                shown.push('…');
            }
            let _ = write!(text, " {}", shown.replace('\n', " "));
        }
    }
    Ok(items(
        args,
        Some(page),
        &format!("第 {page} 页对象"),
        text,
        json!(&entries[range.clone()]),
        range.end,
        entries.len(),
    ))
}

#[cfg(not(feature = "pdfium"))]
pub(super) fn objects(
    _: &mathpix::Options,
    _: &Value,
    _: &Ctx<'_>,
    _: Option<&Quote>,
) -> Result<Output, ToolError> {
    Err(Error::PdfiumUnavailable.into())
}
