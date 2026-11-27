use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::{Duration, Instant};

use engine_pdf::{
    Cancel, Content, Document, Error, Flow, NoTextReason, Ocr, ReadOptions, Render, mathpix, pages,
    read_each,
};

const USAGE: &str = concat!(
    "cargo run --release --example probe -- <PDF> [--pages <页码>] [--ocr] [--render <目录>] [--text]\n",
    "cargo run --release --example probe -- <PDF> --mathpix <页码> [--text]",
    "\ncargo run --release --example probe -- <PDF> --labels",
);

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
            "默认读取所有页。--ocr 在本机识别缺少正常文字层的页。--render 写出最长边 1568 像素的 JPEG。"
        );
        println!(
            "--mathpix 将指定页上传到 Mathpix 并产生费用，凭据从 MATHPIX_APP_ID 和 MATHPIX_APP_KEY 读取。"
        );
        println!("--text 打印文字或 Mathpix 的识别结果。");
        return Ok(());
    }

    let mut spec = None;
    let mut mathpix_spec = None;
    let mut render_dir = None;
    let mut print_text = false;
    let mut use_ocr = false;
    let mut labels = false;
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
            Some("--ocr") => use_ocr = true,
            Some("--labels") => labels = true,
            Some("--mathpix") => {
                if mathpix_spec.is_some() {
                    return Err("请只指定一次 --mathpix。".into());
                }
                mathpix_spec = Some(
                    args.next()
                        .ok_or("--mathpix 后需要页码。")?
                        .into_string()
                        .map_err(|_| "页码必须是 UTF-8。")?,
                );
            }
            _ => return Err(format!("选项看不懂。\n{USAGE}").into()),
        }
    }

    if labels
        && (spec.is_some()
            || use_ocr
            || render_dir.is_some()
            || mathpix_spec.is_some()
            || print_text)
    {
        return Err("--labels 请单独使用。".into());
    }
    if mathpix_spec.is_some() && (spec.is_some() || use_ocr || render_dir.is_some()) {
        return Err("--mathpix 自带选页，请单独使用，可加 --text。".into());
    }
    let document = Document::open(path)?;
    if labels {
        let labels: Vec<_> = (1..=document.page_count())
            .map(|page| document.label(page))
            .collect();
        println!("{}", serde_json::to_string(&labels)?);
        return Ok(());
    }
    if let Some(spec) = mathpix_spec {
        return recognize_mathpix(&document, &spec, print_text);
    }
    let total = document.page_count();
    let selected = pages::parse(spec.as_deref(), total, total as usize)?;
    if let Some(dir) = &render_dir {
        fs::create_dir_all(dir)?;
    }
    let opts = ReadOptions {
        ocr: if use_ocr { Ocr::auto() } else { Ocr::Off },
        render: render_dir.as_ref().map(|_| Render::jpeg(1568)),
        ..ReadOptions::default()
    };
    println!(
        "文档共 {total} 页，选读 {} 页。字数为所交付文字中的非空白字符数。",
        selected.len()
    );
    println!("每页耗时计入读取、分类、识别和渲染，排除打印和写文件。");

    let mut write_error = None;
    let mut elapsed = Duration::ZERO;
    let mut start = Instant::now();
    let summary = read_each(&document, &selected, &opts, &Cancel::new(), |page| {
        let duration = start.elapsed();
        elapsed += duration;
        let (kind, chars) = match &page.content {
            Content::Text(text) => ("Text", text.chars().filter(|c| !c.is_whitespace()).count()),
            Content::SearchableText(text) => (
                "SearchableText",
                text.chars().filter(|c| !c.is_whitespace()).count(),
            ),
            Content::Ocr(ocr) => (
                "Ocr",
                ocr.text.chars().filter(|c| !c.is_whitespace()).count(),
            ),
            Content::Blank => ("Blank", 0),
            Content::NoText { reason, partial } => (
                match reason {
                    NoTextReason::OcrOff => "NoText(OcrOff)",
                    NoTextReason::Garbled => "NoText(Garbled)",
                    NoTextReason::NothingRecognized => "NoText(NothingRecognized)",
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
            (
                true,
                Content::Text(text)
                | Content::SearchableText(text)
                | Content::Ocr(engine_pdf::OcrText { text, .. })
                | Content::NoText { partial: text, .. },
            ) if !text.trim().is_empty() => {
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
        "已读 {} 页，文字 {}，识别 {}，空白 {}，无字 {}，失败 {}，页图 {}。累计 {:.3} ms。",
        summary.processed,
        summary.text,
        summary.ocr,
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

fn recognize_mathpix(
    document: &Document,
    spec: &str,
    print_text: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let total = document.page_count();
    // 付费选页必须明确，范围右端写错时直接提示。
    for part in spec.split([',', '，']).map(str::trim) {
        if part.is_empty() {
            return Err("--mathpix 需要明确的物理页码（如 8 或 1,3-4）。".into());
        }
        if let Some(page) = part
            .split_once('-')
            .and_then(|(_, end)| end.trim().parse::<u32>().ok())
            .filter(|&end| end > total)
        {
            return Err(Error::PageOutOfRange { page, total }.into());
        }
    }
    let selected = pages::parse(Some(spec), total, total as usize)?;
    let quote = mathpix::quote(document, &selected)?;
    let app_id = env::var("MATHPIX_APP_ID")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .ok_or("请设置 MATHPIX_APP_ID。")?;
    let app_key = env::var("MATHPIX_APP_KEY")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .ok_or("请设置 MATHPIX_APP_KEY。")?;
    let keys = mathpix::Keys::new(app_id, app_key);

    println!(
        "Mathpix 将上传原文件第 {} 页（共 {} 页），预计 ${:.3}。",
        quote.pages_text(),
        quote.pages().len(),
        quote.usd(),
    );
    let start = Instant::now();
    let result = mathpix::run(
        document,
        &quote,
        &keys,
        // 带上 lines.json，好核对每页映射回的原页码。
        &mathpix::Options {
            lines: true,
            ..mathpix::Options::default()
        },
        &Cancel::new(),
        &|_| {},
    );
    let usage = match &result {
        Ok(recognized) => &recognized.usage,
        Err(failure) => &failure.usage,
    };
    let cleanup = match usage.remote_deleted {
        Some(true) => "服务已确认删除远端文件",
        Some(false) => "服务未确认删除远端文件",
        None => "未获得可清理的远端任务",
    };
    println!(
        "Mathpix 用量 {} 页，{}${:.3}。{cleanup}。耗时 {:.3} 秒。",
        usage.pages_billed,
        if usage.estimated { "估计 " } else { "" },
        usage.usd,
        start.elapsed().as_secs_f64(),
    );
    let result = result.map_err(|failure| failure.error)?;
    let mapping = match result.mapping {
        mathpix::Mapping::Exact => "精确",
        mathpix::Mapping::Approximate => "近似",
    };
    println!("页码映射为{mapping}。");
    for page in result.pages {
        let lines = page.lines.as_ref().map_or("没有".to_string(), |lines| {
            format!("第 {} 页", lines["page"])
        });
        println!(
            "原文件第 {:>4} 页  {:>5} 字  lines.json 标的是{lines}",
            page.index,
            page.mmd.chars().filter(|ch| !ch.is_whitespace()).count(),
        );
        if print_text && !page.mmd.is_empty() {
            println!("{}\n", page.mmd.trim_end());
        }
    }
    Ok(())
}
