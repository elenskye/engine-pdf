//! 改页之后的核对：只许改动的区域变，别处的画面和文字都要和原页一样（`decisions/0019`）。

use objc2_core_foundation::{CGAffineTransform, CGPoint, CGRect, CGSize};
use objc2_core_graphics::CGRectApplyAffineTransform;
use objc2_pdf_kit::PDFDisplayBox;

use crate::edit::Rect;
use crate::{Error, Image, ImageFormat};

use pdfium_render::prelude::*;

use super::{Pdf, render};

/// 比较前后两份文件的同一页：`allowed` 之外的像素（PDFKit 画，不画批注）和字（PDFium 取，
/// 按字框中心算）都要和原来一样。落在 `allowed` 里的字不比：移到别的字上时阅读器会把重叠的字去重。
pub(super) fn unchanged_outside(
    before: (&Pdf, &PdfDocument<'_>),
    after: (&Pdf, &PdfDocument<'_>),
    page: u32,
    allowed: &[Rect],
) -> Result<bool, Error> {
    let (old, transform, bounds) = content_pixels(before.0, page)?;
    let (new, _, _) = content_pixels(after.0, page)?;
    if (old.width, old.height) != (new.width, new.height) {
        return Ok(false);
    }
    let (width, height) = (old.width as usize, old.height as usize);
    let allowed: Vec<_> = allowed.iter().map(|area| expand(*area, MARGIN)).collect();
    let mut skip = vec![false; width * height];
    for area in &allowed {
        let area = from_cg_rect(CGRectApplyAffineTransform(to_cg_rect(*area), transform));
        let [left, top, right, bottom] = pixel_area(area, bounds, width, height, true);
        for y in top..bottom {
            skip[y * width + left..y * width + right].fill(true);
        }
    }
    let changed = old
        .bytes
        .iter()
        .zip(&new.bytes)
        .zip(&skip)
        .filter(|((a, b), skip)| !**skip && a.abs_diff(**b) > 64)
        .count();
    // PDFKit 两次画同一个字形可能差一点亚像素，容许万分之一的像素；丢掉一段字、字距变了都远超这个数。
    if changed > width * height / 10_000 {
        return Ok(false);
    }
    let outside = |doc: &PdfDocument<'_>| -> Result<Vec<char>, Error> {
        let mut chars: Vec<char> = page_chars(doc, page)?
            .into_iter()
            .filter(|&(_, b)| !allowed.iter().any(|a| centered(*a, b)))
            .map(|(ch, _)| ch)
            .collect();
        chars.sort_unstable();
        Ok(chars)
    };
    Ok(outside(before.1)? == outside(after.1)?)
}

/// PDFium 取出的非空白字和字框（不含它在字与字之间补的空格）。
pub(super) fn page_chars(doc: &PdfDocument<'_>, page: u32) -> Result<Vec<(char, Rect)>, Error> {
    let failed = || Error::Render { page };
    let page = doc.pages().get((page - 1) as i32).map_err(|_| failed())?;
    let text = page.text().map_err(|_| failed())?;
    Ok(text
        .chars()
        .iter()
        .filter_map(|ch| {
            // 行末连字符 PDFium 按上下一行的位置记成 U+0002 或 `-`，一律当 `-`。
            let unicode = ch
                .unicode_char()
                .map(|c| if c == '\u{2}' { '-' } else { c })
                .filter(|c| !c.is_whitespace())?;
            let bounds = ch.tight_bounds().or_else(|_| ch.loose_bounds()).ok()?;
            Some((unicode, super::pdfium::rect(bounds)))
        })
        .collect())
}

/// `b` 的中心落在 `a` 里。
pub(super) fn centered(a: Rect, b: Rect) -> bool {
    let (x, y) = (b.x + b.width / 2.0, b.y + b.height / 2.0);
    a.x <= x && x <= right(a) && a.y <= y && y <= top(a)
}

/// 允许改动的框外再放宽一点：PDFium 的对象框贴着字形，抗锯齿会多出半个像素。
const MARGIN: f64 = 2.0;

/// 不画批注的灰度页图，以及从 PDF 坐标到显示坐标的变换和显示范围。
pub(super) fn content_pixels(
    pdf: &Pdf,
    page: u32,
) -> Result<(Image, CGAffineTransform, Rect), Error> {
    pdf.with_page(page, |native| {
        // SAFETY: 页面在所属线程，临时隐藏批注后照原样恢复。
        unsafe {
            let shown = native.displaysAnnotations();
            native.setDisplaysAnnotations(false);
            let image = render::render(native, page, ImageFormat::Gray, 1600);
            native.setDisplaysAnnotations(shown);
            let transform = native.transformForBox(PDFDisplayBox::CropBox);
            let (crop, rotation) = (
                native.boundsForBox(PDFDisplayBox::CropBox),
                native.rotation(),
            );
            let mut bounds = Rect {
                x: 0.0,
                y: 0.0,
                width: crop.size.width,
                height: crop.size.height,
            };
            if rotation.rem_euclid(180) == 90 {
                std::mem::swap(&mut bounds.width, &mut bounds.height);
            }
            Ok((image?, transform, bounds))
        }
    })
}

pub(super) fn right(r: Rect) -> f64 {
    r.x + r.width
}

pub(super) fn top(r: Rect) -> f64 {
    r.y + r.height
}

pub(super) fn expand(r: Rect, by: f64) -> Rect {
    Rect {
        x: r.x - by,
        y: r.y - by,
        width: r.width + 2.0 * by,
        height: r.height + 2.0 * by,
    }
}

pub(super) fn intersection(a: Rect, b: Rect) -> Option<Rect> {
    let x = a.x.max(b.x);
    let y = a.y.max(b.y);
    let width = right(a).min(right(b)) - x;
    let height = top(a).min(top(b)) - y;
    (width > 0.0 && height > 0.0).then_some(Rect {
        x,
        y,
        width,
        height,
    })
}

pub(super) fn contains(a: Rect, b: Rect) -> bool {
    a.x <= b.x && a.y <= b.y && right(a) >= right(b) && top(a) >= top(b)
}

pub(super) fn to_cg_rect(r: Rect) -> CGRect {
    CGRect::new(CGPoint::new(r.x, r.y), CGSize::new(r.width, r.height))
}

pub(super) fn from_cg_rect(r: CGRect) -> Rect {
    Rect {
        x: r.origin.x,
        y: r.origin.y,
        width: r.size.width,
        height: r.size.height,
    }
}

/// 显示坐标的框换成像素范围 `[左, 上, 右, 下)`。`cover` 取所有沾到的像素，否则只取整个落在框里的像素。
pub(super) fn pixel_area(
    area: Rect,
    bounds: Rect,
    width: usize,
    height: usize,
    cover: bool,
) -> [usize; 4] {
    let x = |point: f64| (point - bounds.x) * width as f64 / bounds.width;
    let y = |point: f64| (top(bounds) - point) * height as f64 / bounds.height;
    let start = |pixel: f64, limit: usize| {
        (if cover { pixel.floor() } else { pixel.ceil() }).clamp(0.0, limit as f64) as usize
    };
    let end = |pixel: f64, limit: usize| {
        (if cover { pixel.ceil() } else { pixel.floor() }).clamp(0.0, limit as f64) as usize
    };
    [
        start(x(area.x), width),
        start(y(top(area)), height),
        end(x(right(area)), width),
        end(y(area.y), height),
    ]
}
