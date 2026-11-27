//! 按原文件物理页查找。文字归一化及命中规则来自 engine-core。

use engine_core::find::{Finder, normalize};
pub use engine_core::find::{Found, Hit, Match, Query, Source};
use serde::{Deserialize, Serialize};

use crate::{Cancel, Document, Error, OcrOptions, edit::Rect};

#[derive(Clone, Debug)]
pub struct Options {
    /// 缺少可定位文字时是否用本机 OCR。关闭时仍可查找已有的文字层。
    pub ocr: Option<OcrOptions>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            ocr: Some(OcrOptions::default()),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Location {
    pub page: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// 命中处的原 PDF 坐标框。多行各给一框，无法核实时为空。
    pub rects: Vec<Rect>,
}

/// 在指定物理页内找整句，整个范围无整句命中时找同页共现词。
/// 回调依次收到处理页数和刚处理完的物理页码。
pub fn find(
    doc: &Document,
    pages: &[u32],
    query: &Query,
    options: &Options,
    cancel: &Cancel,
    progress: &dyn Fn(usize, u32),
) -> Result<Found<Location>, Error> {
    if !cfg!(target_os = "macos") {
        return Err(Error::Unsupported);
    }
    cancel.check()?;
    let total = doc.page_count();
    if let Some(&page) = pages.iter().find(|&&page| page == 0 || page > total) {
        return Err(Error::PageOutOfRange { page, total });
    }
    let mut finder = Finder::new(query);
    if !normalize(&query.text).text.is_empty() {
        crate::sys::scan_find(doc, pages, options, cancel, progress, &mut finder)?;
    }
    Ok(finder.finish())
}
