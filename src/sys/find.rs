//! 逐物理页查找：每页取哪份文字、命中处的框从哪来（design §9.2，`decisions/0021`、`0022`）。

use std::cell::OnceCell;
use std::ops::Range;

use engine_core::find::Finder;
use objc2_core_graphics::CGAffineTransformInvert;
use objc2_foundation::NSRange;
use objc2_pdf_kit::{PDFDisplayBox, PDFPage};

use crate::edit::Rect;
use crate::find::{Location, Options, Source};
use crate::ligatures::Repaired;
use crate::ocr::{CharacterSpans, OcrBounds, OcrLine};
use crate::{Cancel, Content, Document, Error, OcrOptions, ReadOptions};

use super::cache::{self, Key};
use super::layer::{Layer, align, mapped_range, unique_range, valid_rect};
use super::pdf::{embedded_ocr, label, latin, text};

pub(crate) fn scan_find(
    doc: &Document,
    pages: &[u32],
    options: &Options,
    cancel: &Cancel,
    progress: &dyn Fn(usize, u32),
    finder: &mut Finder<Location>,
) -> Result<(), Error> {
    let pdf = doc.native();
    let file = pdf.fingerprint();
    let page_data = |index| {
        cache::get_or_insert(
            file,
            Key::Page(index),
            || PageData::read(doc, index),
            PageData::cost,
        )
    };
    // 所选页大多有完整文字层时，字少的页多半是插图、封面，只识别完全没字的页；
    // 大多是扫描页时，盖了页码、水印的页也照样识别（`decisions/0022`）。
    let sample = pages.iter().step_by(pages.len().div_ceil(8).max(1));
    let (texts, sampled) = sample.fold((0, 0), |(texts, sampled), &index| {
        let full = page_data(index).is_ok_and(|data| amount(data.text()) == Amount::Full);
        (texts + usize::from(full), sampled + 1)
    });
    let mostly_text = texts * 2 > sampled;
    for (completed, &index) in pages.iter().enumerate() {
        cancel.check()?;
        let data = page_data(index)?;
        let locate = |rects| Location {
            page: index,
            label: data.label.clone(),
            rects,
        };
        // PDFium 的字形框：只在要用时取，取过就留在缓存里。
        let layer = OnceCell::new();
        let layer = || {
            layer
                .get_or_init(|| {
                    cache::get_or_insert(
                        file,
                        Key::Layer(index),
                        || Ok(pdf.layer(index)),
                        |layer| layer.as_ref().map_or(0, Layer::cost),
                    )
                    .ok()
                })
                .as_deref()
                .and_then(Option::as_ref)
        };
        let text = data.text();
        let scan_text = |finder: &mut Finder<Location>| {
            finder.scan(text, Source::Text, |range| {
                let original = data.original_range(range.clone());
                let rects = pdf
                    .with_page(index, |page| {
                        Ok(selection_rects(page, &data.raw, original.clone()))
                    })
                    .ok()
                    .flatten()
                    .map(|rects| {
                        tighten(rects, &text[range.clone()], || {
                            let layer = layer()?;
                            Some(layer.line_extents(layer.map(text, range.clone())?))
                        })
                    })
                    .or_else(|| layer().and_then(|layer| layer.locate(text, range)));
                locate(rects.unwrap_or_default())
            });
        };
        let scan_layer = |finder: &mut Finder<Location>| {
            if let Some(layer) = layer() {
                finder.scan(&layer.text, Source::Text, |range| {
                    locate(layer.rects(range))
                });
            }
        };
        let vision = |ocr: &OcrOptions| {
            cache::get_or_insert(
                file,
                Key::Vision(index, ocr.languages.clone()),
                || pdf.with_page(index, |page| recognize(page, index, ocr, cancel)),
                VisionPage::cost,
            )
        };

        if let Some(stored) = &data.stored {
            // 引擎写的可搜索副本：按存下的识别原文找，框依次从文字层、PDFium、现做的识别里核实。
            let pairs = align(stored, &data.raw);
            let recognized = OnceCell::new();
            finder.scan(stored, Source::Ocr, |range| {
                let rects = pairs
                    .as_deref()
                    .and_then(|pairs| mapped_range(pairs, &data.raw, range.clone()))
                    .or_else(|| unique_range(stored, &data.raw, range.clone()))
                    .and_then(|native| {
                        pdf.with_page(index, |page| Ok(selection_rects(page, &data.raw, native)))
                            .ok()
                            .flatten()
                    })
                    .or_else(|| layer().and_then(|layer| layer.locate(stored, range.clone())))
                    .or_else(|| {
                        let (vision, pairs) = recognized
                            .get_or_init(|| {
                                let vision = vision(options.ocr.as_ref()?).ok()?;
                                let pairs = align(stored, &vision.text);
                                Some((vision, pairs))
                            })
                            .as_ref()?;
                        let target = pairs
                            .as_deref()
                            .and_then(|pairs| mapped_range(pairs, &vision.text, range.clone()))
                            .or_else(|| unique_range(stored, &vision.text, range))?;
                        Some(vision.rects(target))
                    });
                locate(rects.unwrap_or_default())
            });
        } else if amount(text) == Amount::Full {
            scan_text(finder);
        } else if data.reference.as_deref().map(amount) == Some(Amount::Full) {
            // PDFKit 读不出、PDFium 读得出的文字层（早期 OCR 软件的输出）。
            scan_layer(finder);
        } else {
            // 文字层不够：页面不是近乎全白就识别整页；没识别出字再找文字层里那几个字。
            let worth = !mostly_text || amount(text) == Amount::None;
            let recognized = match &options.ocr {
                Some(ocr) if worth && looks_scanned(doc, file, index, &data.raw, cancel)? => {
                    Some(vision(ocr)?)
                }
                _ => None,
            };
            match recognized.filter(|vision| amount(&vision.text) != Amount::None) {
                Some(vision) => finder.scan(&vision.text, Source::Ocr, |range| {
                    locate(vision.rects(range))
                }),
                None if amount(text) == Amount::Few => scan_text(finder),
                None if data.reference.as_deref().map(amount) == Some(Amount::Few) => {
                    scan_layer(finder)
                }
                None => {}
            }
        }
        cancel.check()?;
        progress(completed + 1, index);
    }
    cancel.check()?;
    Ok(())
}

/// 一页查找要用的文字，按文件内容缓存。
struct PageData {
    label: Option<String>,
    /// PDFKit 原文：选区、字框都按它的下标取。
    raw: String,
    /// 补过连字的原文（开了 `pdfium` 且 PDFKit 取丢了连字时才有）。
    repaired: Option<Repaired>,
    /// 引擎写进可搜索副本的识别原文。
    stored: Option<String>,
    /// PDFium 同一页的字（开了 `pdfium`，且要补连字或 PDFKit 读得不够时才取）。
    reference: Option<String>,
}

impl PageData {
    fn read(doc: &Document, index: u32) -> Result<Self, Error> {
        let pdf = doc.native();
        let (label, raw, stored) = pdf.with_page(index, |page| {
            let label = pdf.has_labels().then(|| label(page)).flatten();
            Ok((label, text(page), embedded_ocr(page)))
        })?;
        let reference = match (&stored, amount(&raw)) {
            (Some(_), _) => None,
            (None, Amount::Full) => pdf.ligature_reference(index, &raw),
            // 文字层不够时 PDFium 的字也是后备。
            (None, _) => pdf.reference(index),
        };
        let repaired = reference
            .as_deref()
            .filter(|_| latin(&raw))
            .and_then(|reference| crate::ligatures::repair(&raw, reference));
        Ok(Self {
            label,
            raw,
            repaired,
            stored,
            reference,
        })
    }

    fn cost(&self) -> usize {
        self.raw.len() * 2
            + self.repaired.as_ref().map_or(0, |r| r.text.len() * 2)
            + self.stored.as_ref().map_or(0, String::len)
            + self.reference.as_ref().map_or(0, String::len)
    }

    /// 拿来找的文字：补过连字的，或 PDFKit 原文。
    fn text(&self) -> &str {
        self.repaired.as_ref().map_or(&self.raw, |r| &r.text)
    }

    fn original_range(&self, range: Range<usize>) -> Range<usize> {
        self.repaired
            .as_ref()
            .map_or(range.clone(), |r| r.original_range(range))
    }
}

/// 一段文字层够不够代表整页（和 `read` 同一门槛）。
#[derive(Clone, Copy, PartialEq, Eq)]
enum Amount {
    None,
    Few,
    Full,
}

fn amount(text: &str) -> Amount {
    let defaults = ReadOptions::default();
    let (characters, garbled) = crate::read::text_quality(text, defaults.garbled_min_fraction);
    if garbled || characters == 0 {
        Amount::None
    } else if characters < defaults.text_min_chars {
        Amount::Few
    } else {
        Amount::Full
    }
}

/// 按 read 的分类：文字层不够、页面又不是近乎全白（扫描页、整页图），就该识别。
fn looks_scanned(
    doc: &Document,
    file: cache::Fingerprint,
    index: u32,
    raw: &str,
    cancel: &Cancel,
) -> Result<bool, Error> {
    let scanned = cache::get_or_insert(
        file,
        Key::Scanned(index),
        || {
            let defaults = ReadOptions {
                render: None,
                ..ReadOptions::default()
            };
            let content = crate::read::classify(doc, index, raw.to_owned(), &defaults, cancel)?;
            Ok(matches!(content, Content::NoText { .. }))
        },
        |_| 0,
    )?;
    Ok(*scanned)
}

/// 个别中文字体的 PDFKit 行框比字高出一大截（李蓉蓉那篇 24.8 pt 框对 10 pt 的字），会罩到下一行。
/// 先拿中文字宽粗看是不是这样，是才取 PDFium 量的整行上下沿，把框收到那一行（上下各留一点）。
fn tighten(
    rects: Vec<Rect>,
    matched: &str,
    extents: impl FnOnce() -> Option<Vec<(f64, f64)>>,
) -> Vec<Rect> {
    let wide = matched.chars().filter(|&ch| crate::find::wide(ch)).count();
    if wide < 2 {
        return rects;
    }
    let advance = rects.iter().map(|rect| rect.width).sum::<f64>() / wide as f64;
    if !rects.iter().any(|rect| rect.height > TALL * advance) {
        return rects;
    }
    let Some(extents) = extents() else {
        return rects;
    };
    rects
        .into_iter()
        .map(|rect| shrink(rect, &extents))
        .collect()
}

/// 框比这一行字高出这么多倍才算偏高。正常的行框在 1.0–1.4 倍。
const TALL: f64 = 1.6;

fn shrink(rect: Rect, extents: &[(f64, f64)]) -> Rect {
    let top = rect.y + rect.height;
    let line = extents
        .iter()
        .filter_map(|&(bottom, line_top)| {
            let overlap = line_top.min(top) - bottom.max(rect.y);
            (overlap > 0.0).then_some((overlap, bottom, line_top))
        })
        .max_by(|a, b| a.0.total_cmp(&b.0));
    let Some((_, bottom, line_top)) = line else {
        return rect;
    };
    let height = line_top - bottom;
    if height <= 0.0 || rect.height <= TALL * height {
        return rect;
    }
    let pad = 0.15 * height;
    let y = (bottom - pad).max(rect.y);
    Rect {
        y,
        height: (line_top + pad).min(top) - y,
        ..rect
    }
}

struct VisionPage {
    text: String,
    lines: Vec<(Range<usize>, OcrLine, CharacterSpans)>,
    direction: (f64, f64),
}

impl VisionPage {
    fn cost(&self) -> usize {
        self.text.len() * 3 + self.lines.len() * 96
    }

    fn rects(&self, range: Range<usize>) -> Vec<Rect> {
        let (a, b) = self.direction;
        self.lines
            .iter()
            .filter_map(|(at, line, spans)| {
                let start = range.start.max(at.start);
                let end = range.end.min(at.end);
                (start < end).then(|| {
                    ocr_rect(
                        &line.text,
                        start - at.start..end - at.start,
                        line.bounds,
                        spans,
                        a,
                        b,
                    )
                })
            })
            .collect()
    }
}

fn recognize(
    page: &PDFPage,
    index: u32,
    options: &OcrOptions,
    cancel: &Cancel,
) -> Result<VisionPage, Error> {
    cancel.check()?;
    let observations =
        super::ocr::recognize_page(page, index, options, super::ocr::Positions::Search);
    cancel.check()?;
    let (ocr, spans) = crate::ocr::assemble(observations?);
    // SAFETY: 页面属于当前线程。CropBox 是 Vision 绘制时使用的显示框。
    let direction =
        CGAffineTransformInvert(unsafe { page.transformForBox(PDFDisplayBox::CropBox) });
    let mut position = 0;
    let lines = ocr
        .lines
        .into_iter()
        .zip(spans)
        .map(|(line, spans)| {
            let start = position
                + ocr.text[position..]
                    .find(&line.text)
                    .expect("识别行按阅读顺序写入同一段文字");
            position = start + line.text.len();
            (start..position, line, spans)
        })
        .collect();
    Ok(VisionPage {
        text: ocr.text,
        lines,
        direction: (direction.a, direction.b),
    })
}

fn selection_rects(page: &PDFPage, text: &str, range: Range<usize>) -> Option<Vec<Rect>> {
    let expected = text.get(range.clone())?;
    let start = text[..range.start].encode_utf16().count();
    let length = expected.encode_utf16().count();
    // SAFETY: 页在所属线程，范围经 UTF-16 转换及 PDFKit 字符数检查。
    let selection = unsafe {
        if length == 0 || start + length > page.numberOfCharacters() {
            return None;
        }
        page.selectionForRange(NSRange::new(start, length))?
    };
    // SAFETY: 选区及行选区均在同一线程，且仍属于当前页。
    let selected = unsafe { selection.string()? }.to_string();
    if !same_glyphs(&selected, expected) {
        return None;
    }
    let lines = unsafe { selection.selectionsByLine() };
    let rects = lines
        .iter()
        .filter_map(|line| {
            // SAFETY: 该行来自当前页的选区。
            let bounds = unsafe { line.boundsForPage(page) };
            let rect = Rect {
                x: bounds.origin.x,
                y: bounds.origin.y,
                width: bounds.size.width,
                height: bounds.size.height,
            };
            valid_rect(rect).then_some(rect)
        })
        .collect::<Vec<_>>();
    (!rects.is_empty()).then_some(rects)
}

fn same_glyphs(left: &str, right: &str) -> bool {
    left.chars()
        .filter(|c| !c.is_whitespace())
        .eq(right.chars().filter(|c| !c.is_whitespace()))
}

fn ocr_rect(
    text: &str,
    range: Range<usize>,
    bounds: OcrBounds,
    spans: &CharacterSpans,
    a: f64,
    b: f64,
) -> Rect {
    let mut rect = Rect {
        x: bounds.x,
        y: bounds.y,
        width: bounds.width,
        height: bounds.height,
    };
    if spans.is_empty() {
        return rect;
    }
    let first = text[..range.start].chars().count();
    let last = first + text[range].chars().count() - 1;
    let start = spans[first].0.clamp(0.0, 1.0);
    let end = spans[last].1.clamp(start, 1.0);
    if end <= start {
        return rect;
    }
    if a.abs() >= b.abs() {
        rect.x += rect.width * if a >= 0.0 { start } else { 1.0 - end };
        rect.width *= end - start;
    } else {
        rect.y += rect.height * if b >= 0.0 { start } else { 1.0 - end };
        rect.height *= end - start;
    }
    rect
}

#[cfg(test)]
mod tests {
    use super::{Rect, tighten};

    fn rect(y: f64, height: f64) -> Rect {
        Rect {
            x: 0.0,
            y,
            width: 41.0,
            height,
        }
    }

    #[test]
    fn tightens_only_line_boxes_far_taller_than_the_line() {
        // 李蓉蓉那篇：四个字 41 pt 宽，框 24.8 pt 高，这一行字在 297–307。
        let tall = rect(282.7, 24.8);
        let fixed = tighten(vec![tall], "种族差异", || Some(vec![(297.0, 307.0)]));
        assert!((fixed[0].y - 295.5).abs() < 1e-9, "{:?}", fixed[0]);
        assert!((fixed[0].y + fixed[0].height - 307.5).abs() < 1e-6);
        // 正常的框、英文、量不到这一行时都不动。
        let normal = rect(453.3, 12.1);
        assert_eq!(
            tighten(vec![normal], "政治面貌", || unreachable!()),
            [normal]
        );
        assert_eq!(tighten(vec![tall], "abcd", || unreachable!()), [tall]);
        assert_eq!(tighten(vec![tall], "种族差异", || None), [tall]);
    }
}
