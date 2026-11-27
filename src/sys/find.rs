use std::cell::OnceCell;
use std::ops::Range;

use engine_core::find::Finder;
use objc2_core_graphics::CGAffineTransformInvert;
use objc2_foundation::NSRange;
use objc2_pdf_kit::{PDFDisplayBox, PDFPage};

use crate::edit::Rect;
use crate::find::{Location, Options, Source};
use crate::ocr::{CharacterSpans, OcrBounds, OcrLine};
use crate::{Cancel, Content, Document, Error, OcrOptions, ReadOptions};

use super::pdf::{embedded_ocr, label, text};

pub(crate) fn scan_find(
    doc: &Document,
    pages: &[u32],
    options: &Options,
    cancel: &Cancel,
    progress: &dyn Fn(usize, u32),
    finder: &mut Finder<Location>,
) -> Result<(), Error> {
    #[cfg(feature = "pdfium")]
    let pdfium = OnceCell::new();
    // 和 read 用同一套门槛：文字层不到 20 个字的扫描页照样识别整页。
    let defaults = ReadOptions {
        render: None,
        ..ReadOptions::default()
    };
    let amount = |text: &str| {
        let (characters, garbled) = crate::read::text_quality(text, defaults.garbled_min_fraction);
        if garbled || characters == 0 {
            Amount::None
        } else if characters < defaults.text_min_chars {
            Amount::Few
        } else {
            Amount::Full
        }
    };
    for (completed, &index) in pages.iter().enumerate() {
        cancel.check()?;
        doc.native().with_page(index, |page| {
            let label = doc.native().has_labels().then(|| label(page)).flatten();
            let locate = |rects| Location {
                page: index,
                label: label.clone(),
                rects,
            };
            let raw = text(page);
            // PDFKit 读不出、PDFium 读得出的文字层（早期 OCR 软件的输出）。只在开了 pdfium 时有。
            let layer = OnceCell::new();
            let layer = || {
                layer
                    .get_or_init(|| {
                        #[cfg(feature = "pdfium")]
                        {
                            let pdf = pdfium
                                .get_or_init(|| super::pdfium::open(doc.original_bytes()).ok());
                            pdf.as_ref().and_then(|pdf| pdfium_page(pdf, index))
                        }
                        #[cfg(not(feature = "pdfium"))]
                        None::<TextPage>
                    })
                    .as_ref()
            };
            let scan_raw = |finder: &mut Finder<Location>| {
                finder.scan(&raw, Source::Text, |range| {
                    let rects = selection_rects(page, &raw, range.clone())
                        .or_else(|| layer().and_then(|layer| layer.locate(&raw, range)));
                    locate(rects.unwrap_or_default())
                });
            };
            let scan_layer = |finder: &mut Finder<Location>, layer: &TextPage| {
                finder.scan(&layer.text, Source::Text, |range| {
                    locate(layer.rects(range))
                });
            };

            if let Some(stored) = embedded_ocr(page) {
                // 引擎写的可搜索副本：按存下的识别原文找，框依次从文字层、PDFium、现做的识别里核实。
                let pairs = align(&stored, &raw);
                let vision = OnceCell::new();
                finder.scan(&stored, Source::Ocr, |range| {
                    let rects = pairs
                        .as_deref()
                        .and_then(|pairs| mapped_range(pairs, &raw, range.clone()))
                        .or_else(|| unique_range(&stored, &raw, range.clone()))
                        .and_then(|range| selection_rects(page, &raw, range))
                        .or_else(|| layer().and_then(|layer| layer.locate(&stored, range.clone())))
                        .or_else(|| {
                            let (vision, pairs) = vision
                                .get_or_init(|| {
                                    let ocr = options.ocr.as_ref()?;
                                    let vision = recognize(page, index, ocr, cancel).ok()?;
                                    let pairs = align(&stored, &vision.text);
                                    Some((vision, pairs))
                                })
                                .as_ref()?;
                            let target = pairs
                                .as_deref()
                                .and_then(|pairs| mapped_range(pairs, &vision.text, range.clone()))
                                .or_else(|| unique_range(&stored, &vision.text, range))?;
                            Some(vision.rects(target))
                        });
                    locate(rects.unwrap_or_default())
                });
            } else if amount(&raw) == Amount::Full {
                scan_raw(finder);
            } else if let Some(layer) = layer().filter(|layer| amount(&layer.text) == Amount::Full)
            {
                scan_layer(finder, layer);
            } else {
                // 文字层不够：页面不是近乎全白就识别整页；没识别出字再找文字层里那几个字。
                let vision = match &options.ocr {
                    Some(ocr) if looks_scanned(doc, index, &raw, &defaults, cancel)? => {
                        Some(recognize(page, index, ocr, cancel)?)
                    }
                    _ => None,
                };
                match vision.filter(|vision| amount(&vision.text) != Amount::None) {
                    Some(vision) => finder.scan(&vision.text, Source::Ocr, |range| {
                        locate(vision.rects(range))
                    }),
                    None if amount(&raw) == Amount::Few => scan_raw(finder),
                    None => {
                        if let Some(layer) =
                            layer().filter(|layer| amount(&layer.text) != Amount::None)
                        {
                            scan_layer(finder, layer);
                        }
                    }
                }
            }
            cancel.check()?;
            Ok(())
        })?;
        progress(completed + 1, index);
    }
    cancel.check()?;
    Ok(())
}

/// 一段文字层够不够代表整页。
#[derive(Clone, Copy, PartialEq, Eq)]
enum Amount {
    None,
    Few,
    Full,
}

/// 按 read 的分类：文字层不够、页面又不是近乎全白（扫描页、整页图），就该识别。
fn looks_scanned(
    doc: &Document,
    index: u32,
    raw: &str,
    defaults: &ReadOptions,
    cancel: &Cancel,
) -> Result<bool, Error> {
    let content = crate::read::classify(doc, index, raw.to_owned(), defaults, cancel)?;
    Ok(matches!(content, Content::NoText { .. }))
}

struct VisionPage {
    text: String,
    lines: Vec<(Range<usize>, OcrLine, CharacterSpans)>,
    direction: (f64, f64),
}

impl VisionPage {
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

// 对齐取字顺序相同、空白不同的两个页面文本。每对记录非空白字符的 UTF-8 开头。
fn align(left: &str, right: &str) -> Option<Vec<(usize, usize)>> {
    let mut rhs = right.char_indices().filter(|(_, c)| !c.is_whitespace());
    let pairs = left
        .char_indices()
        .filter(|(_, c)| !c.is_whitespace())
        .map(|(left, ch)| {
            let (right, other) = rhs.next()?;
            (ch == other).then_some((left, right))
        })
        .collect::<Option<Vec<_>>>()?;
    rhs.next().is_none().then_some(pairs)
}

fn mapped_range(
    alignment: &[(usize, usize)],
    target: &str,
    range: Range<usize>,
) -> Option<Range<usize>> {
    let start = alignment.partition_point(|&(offset, _)| offset < range.start);
    let end = alignment.partition_point(|&(offset, _)| offset < range.end);
    if end <= start {
        return None;
    }
    let first = alignment[start].1;
    let last = alignment[end - 1].1;
    Some(first..last + target[last..].chars().next()?.len_utf8())
}

// 阅读顺序不同时，用两个后端都能唯一定位的原样片段。
fn unique_range(original: &str, target: &str, range: Range<usize>) -> Option<Range<usize>> {
    let fragment = &original[range];
    let mut matches = target.match_indices(fragment);
    let (start, _) = matches.next()?;
    (matches.next().is_none() && original.match_indices(fragment).count() == 1)
        .then_some(start..start + fragment.len())
}

fn valid_rect(rect: Rect) -> bool {
    [rect.x, rect.y, rect.width, rect.height]
        .into_iter()
        .all(f64::is_finite)
        && rect.width > 0.0
        && rect.height > 0.0
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

struct TextPage {
    text: String,
    glyphs: Vec<(usize, Option<Rect>)>,
}

impl TextPage {
    fn locate(&self, original: &str, range: Range<usize>) -> Option<Vec<Rect>> {
        let mapped = align(original, &self.text)
            .and_then(|alignment| mapped_range(&alignment, &self.text, range.clone()))
            .or_else(|| unique_range(original, &self.text, range))?;
        let rects = self.rects(mapped);
        (!rects.is_empty()).then_some(rects)
    }

    fn rects(&self, range: Range<usize>) -> Vec<Rect> {
        let mut result = Vec::new();
        let mut line: Option<Rect> = None;
        let start = self
            .glyphs
            .partition_point(|(offset, _)| *offset < range.start);
        for &(offset, bounds) in &self.glyphs[start..] {
            if offset >= range.end {
                break;
            }
            if let Some(rect) = bounds {
                if let Some(previous) = &mut line {
                    let x = previous.x.min(rect.x);
                    let y = previous.y.min(rect.y);
                    previous.width = (previous.x + previous.width).max(rect.x + rect.width) - x;
                    previous.height = (previous.y + previous.height).max(rect.y + rect.height) - y;
                    previous.x = x;
                    previous.y = y;
                } else {
                    line = Some(rect);
                }
            } else if self.text[offset..].starts_with(['\r', '\n']) {
                result.extend(line.take());
            }
        }
        result.extend(line);
        result
    }
}

#[cfg(feature = "pdfium")]
fn pdfium_page(pdf: &pdfium_render::prelude::PdfDocument<'_>, page: u32) -> Option<TextPage> {
    let page = pdf.pages().get(i32::try_from(page - 1).ok()?).ok()?;
    let native = page.text().ok()?;
    let mut text = String::new();
    let mut glyphs = Vec::new();
    for glyph in native.chars().iter() {
        let Some(ch) = glyph.unicode_char().filter(|ch| *ch != '\0') else {
            continue;
        };
        let bounds = if ch.is_whitespace() {
            None
        } else {
            glyph
                .tight_bounds()
                .ok()
                .map(super::pdfium::rect)
                .filter(|rect| valid_rect(*rect))
        };
        glyphs.push((text.len(), bounds));
        text.push(ch);
    }
    Some(TextPage { text, glyphs })
}
