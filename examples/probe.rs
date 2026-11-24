use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::{Duration, Instant};

use engine_pdf::{
    Cancel, Content, Document, Flow, NoTextReason, ReadOptions, Render, pages, read_each,
};

const USAGE: &str =
    "cargo run --release --example probe -- <PDF> [--pages <页码>] [--render <目录>] [--text]";

fn main() -> ExitCode {
    if let Err(error) = run() {
        eprintln!("probe：{error}");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = env::args_os().skip(1);
    let path = args.next().ok_or(USAGE)?;
    if path == "--help" || path == "-h" {
        println!("{USAGE}");
        println!(
            "默认读取所有页。--render 为每页写出最长边 1568 像素的 JPEG，--text 打印每页交回的文字。"
        );
        return Ok(());
    }

    let mut spec = None;
    let mut render_dir = None;
    let mut print_text = false;
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--pages") => {
                spec = Some(
                    args.next()
                        .ok_or("--pages 后需要页码。")?
                        .into_string()
                        .map_err(|_| "页码必须是 UTF-8。")?,
                );
            }
            Some("--render") => {
                render_dir = Some(PathBuf::from(args.next().ok_or("--render 后需要目录。")?));
            }
            Some("--text") => print_text = true,
            Some("--ocr") => return Err("OCR 将在 v0.3.0 提供。".into()),
            _ => return Err(format!("选项看不懂。\n{USAGE}").into()),
        }
    }

    let document = Document::open(path)?;
    let total = document.page_count();
    let selected = pages::parse(spec.as_deref(), total, total as usize)?;
    if let Some(dir) = &render_dir {
        fs::create_dir_all(dir)?;
    }
    let opts = ReadOptions {
        render: render_dir.as_ref().map(|_| Render::jpeg(1568)),
        ..ReadOptions::default()
    };
    println!(
        "文档共 {total} 页，选读 {} 页。字数为所交付文字中的非空白字符数。",
        selected.len()
    );
    println!("每页耗时计入读取、分类和所选渲染，排除打印和写文件。");

    let mut write_error = None;
    let mut elapsed = Duration::ZERO;
    let mut start = Instant::now();
    let summary = read_each(&document, &selected, &opts, &Cancel::new(), |page| {
        let duration = start.elapsed();
        elapsed += duration;
        let (kind, chars) = match &page.content {
            Content::Text(text) => ("Text", text.chars().filter(|c| !c.is_whitespace()).count()),
            Content::Blank => ("Blank", 0),
            Content::NoText { reason, partial } => (
                match reason {
                    NoTextReason::OcrOff => "NoText(OcrOff)",
                    NoTextReason::Garbled => "NoText(Garbled)",
                },
                partial.chars().filter(|c| !c.is_whitespace()).count(),
            ),
            Content::Failed(_) => ("Failed", 0),
        };
        println!(
            "第 {:>4} 页  标签 {:?}  {:<16}  {:>5} 字  {:>8.3} ms",
            page.index,
            page.label.as_deref().unwrap_or("—"),
            kind,
            chars,
            duration.as_secs_f64() * 1000.0,
        );
        if let Content::Failed(error) = &page.content {
            eprintln!("第 {} 页：{error}", page.index);
        }
        match (print_text, &page.content) {
            (true, Content::Text(text) | Content::NoText { partial: text, .. })
                if !text.trim().is_empty() =>
            {
                println!("{}\n", text.trim_end());
            }
            _ => {}
        }
        if let (Some(dir), Some(image)) = (&render_dir, &page.image) {
            let path = dir.join(format!("page-{:04}.jpg", page.index));
            if let Err(error) = fs::write(path, &image.bytes) {
                write_error = Some(format!("第 {} 页的 JPEG 写入失败。{error}", page.index));
                return Flow::Stop;
            }
        }
        start = Instant::now();
        Flow::Continue
    })?;

    if let Some(error) = write_error {
        return Err(error.into());
    }
    println!(
        "已读 {} 页，文字 {}，空白 {}，无字 {}，失败 {}，页图 {}。累计 {:.3} ms。",
        summary.processed,
        summary.text,
        summary.blank,
        summary.no_text,
        summary.failed,
        summary.images,
        elapsed.as_secs_f64() * 1000.0,
    );
    if summary.failed != 0 {
        return Err("部分页面读取失败。".into());
    }
    Ok(())
}
