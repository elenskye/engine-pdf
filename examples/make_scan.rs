//! 仿扫描件：把真实 PDF 的一页做成只有图的 PDF，原页的文字层另存成标准答案，给 OCR 验收算字错率用。
//!
//! ```bash
//! cargo run --features samples --example make_scan -- <PDF> <物理页码> [输出目录，默认 local/scans]
//! ```
//!
//! 产物来自真实文件，只放 `local/`，不进 Git。

use std::{env, fs, path::PathBuf};

use engine_pdf::{Document, samples};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = env::args_os().skip(1);
    let usage = "用法：make_scan <PDF> <物理页码> [输出目录]";
    let input = PathBuf::from(args.next().ok_or(usage)?);
    let page: u32 = args
        .next()
        .and_then(|page| page.into_string().ok())
        .and_then(|page| page.parse().ok())
        .ok_or(usage)?;
    let out = args
        .next()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("local/scans"));

    let doc = Document::open(&input)?;
    let reference = doc.text(page)?;
    if reference.trim().is_empty() {
        return Err(format!("第 {page} 页没有文字层，做不了标准答案。").into());
    }
    let scan = samples::scan(&doc, page)?;
    fs::create_dir_all(&out)?;
    let stem = input.file_stem().ok_or(usage)?.to_string_lossy();
    let base = out.join(format!("{stem}-page-{page}"));
    fs::write(base.with_extension("pdf"), scan)?;
    fs::write(base.with_extension("txt"), reference)?;
    println!("{}", base.with_extension("pdf").display());
    println!("{}", base.with_extension("txt").display());
    Ok(())
}
