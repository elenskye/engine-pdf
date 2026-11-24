use std::ptr::{null, null_mut};

use objc2::rc::autoreleasepool;
use objc2_core_foundation::{
    CFAttributedString, CFDictionary, CFMutableData, CFString, CGPoint, CGRect, CGSize,
};
use objc2_core_graphics::{
    CGBitmapContextCreate, CGBitmapContextCreateImage, CGColorSpace, CGContext, CGDataConsumer,
    CGImageAlphaInfo, CGPDFContextBeginPage, CGPDFContextClose, CGPDFContextCreate,
    CGPDFContextEndPage,
};
use objc2_core_text::{CTFont, CTLine, kCTFontAttributeName};

use super::pdf::Pdf;
use crate::Error;
use crate::samples::{LABEL_TEXT, LOCKED_PASSWORD, Page, Sample, raw_pdf};

const WIDTH: usize = 612;
const HEIGHT: usize = 792;
const SCALE: usize = 3;

pub(crate) fn generate(sample: Sample) -> Result<Vec<u8>, Error> {
    match sample {
        Sample::NotPdf => Ok(b"This file is plain text, not a PDF.\n".to_vec()),
        Sample::Damaged => {
            let mut pdf = generate(Sample::TextEn)?;
            pdf.truncate(16);
            Ok(pdf)
        }
        Sample::Labels => Ok(labels_pdf()),
        _ => {
            let pdf = draw_pdf(sample)?;
            if sample == Sample::Locked {
                Pdf::from_bytes(&pdf)?.with_sample_password(LOCKED_PASSWORD)
            } else {
                Ok(pdf)
            }
        }
    }
}

fn draw_pdf(sample: Sample) -> Result<Vec<u8>, Error> {
    let data = CFMutableData::new(None, 0).ok_or(Error::Render { page: 1 })?;
    let consumer = CGDataConsumer::with_cf_data(Some(&data)).ok_or(Error::Render { page: 1 })?;
    let media_box = rect(0.0, 0.0, WIDTH as f64, HEIGHT as f64);
    // SAFETY: media_box 在调用期间有效，使用默认 PDF 文档属性。
    let pdf = unsafe { CGPDFContextCreate(Some(&consumer), &media_box, None) }
        .ok_or(Error::Render { page: 1 })?;

    for (index, page) in sample.pages().iter().enumerate() {
        autoreleasepool(|_| -> Result<(), Error> {
            let page_number = index as u32 + 1;
            let bitmap = if sample == Sample::ScannedTwoColumn
                || matches!(page, Page::Scan(_) | Page::Image | Page::Formula)
            {
                let gray =
                    CGColorSpace::new_device_gray().ok_or(Error::Render { page: page_number })?;
                // SAFETY: 空指针让 CoreGraphics 自行分配位图。8 位灰度，无 alpha。
                let bitmap = unsafe {
                    CGBitmapContextCreate(
                        null_mut(),
                        WIDTH * SCALE,
                        HEIGHT * SCALE,
                        8,
                        0,
                        Some(&gray),
                        CGImageAlphaInfo::None.0,
                    )
                }
                .ok_or(Error::Render { page: page_number })?;
                CGContext::scale_ctm(Some(&bitmap), SCALE as f64, SCALE as f64);
                CGContext::set_gray_fill_color(Some(&bitmap), 1.0, 1.0);
                CGContext::fill_rect(Some(&bitmap), media_box);
                CGContext::set_gray_fill_color(Some(&bitmap), 0.0, 1.0);
                Some(bitmap)
            } else {
                None
            };
            // SAFETY: pdf 是有效的 PDF context，当前没有打开的页，沿用默认页属性。
            unsafe { CGPDFContextBeginPage(Some(&pdf), None) };
            draw_page(bitmap.as_deref().unwrap_or(&pdf), page, page_number)?;
            if let Some(bitmap) = bitmap {
                let image = CGBitmapContextCreateImage(Some(&bitmap))
                    .ok_or(Error::Render { page: page_number })?;
                CGContext::draw_image(Some(&pdf), media_box, Some(&image));
            }
            CGPDFContextEndPage(Some(&pdf));
            Ok(())
        })?;
    }
    CGPDFContextClose(Some(&pdf));
    Ok(data.to_vec())
}

fn draw_page(context: &CGContext, page: &Page, page_number: u32) -> Result<(), Error> {
    match page {
        Page::Text(lines) | Page::Scan(lines) => {
            draw_lines(context, lines, 55.0, 728.0, 15.0, page_number)?;
        }
        Page::Columns { left, right } => {
            draw_lines(context, left, 48.0, 728.0, 12.0, page_number)?;
            draw_lines(context, right, 326.0, 728.0, 12.0, page_number)?;
        }
        Page::Blank => {}
        Page::Image => {
            CGContext::set_gray_fill_color(Some(context), 0.9, 1.0);
            CGContext::fill_rect(Some(context), rect(80.0, 290.0, 452.0, 280.0));
            CGContext::set_gray_fill_color(Some(context), 0.25, 1.0);
            for (index, height) in [85.0, 140.0, 110.0, 190.0].into_iter().enumerate() {
                CGContext::fill_rect(
                    Some(context),
                    rect(125.0 + 101.0 * index as f64, 315.0, 55.0, height),
                );
            }
        }
        Page::Formula => {
            draw_lines(
                context,
                &["f(x) = x² + 2x + 1", "∫₀¹ x² dx = 1/3"],
                80.0,
                681.0,
                24.0,
                page_number,
            )?;
            for (row, values) in [
                ["x", "f(x)", "x²"],
                ["0", "1", "0"],
                ["1", "4", "1"],
                ["2", "9", "4"],
            ]
            .into_iter()
            .enumerate()
            {
                let y = 517.0 - 38.0 * row as f64;
                for (column, value) in values.into_iter().enumerate() {
                    let x = 80.0 + 130.0 * column as f64;
                    CGContext::stroke_rect(Some(context), rect(x, y, 130.0, 38.0));
                    draw_lines(context, &[value], x + 18.0, y + 12.0, 17.0, page_number)?;
                }
            }
        }
    }
    Ok(())
}

fn draw_lines(
    context: &CGContext,
    lines: &[&str],
    x: f64,
    y: f64,
    size: f64,
    page: u32,
) -> Result<(), Error> {
    let font_name = if lines
        .iter()
        .any(|line| line.chars().any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c)))
    {
        "PingFangSC-Regular"
    } else {
        "Helvetica"
    };
    // SAFETY: 字体名在调用期间有效，空矩阵使用系统默认值。
    let font = unsafe { CTFont::with_name(&CFString::from_str(font_name), size, null()) };
    // SAFETY: kCTFontAttributeName 是系统提供的 CFString 常量，与 CTFont 值配对。
    let attributes = CFDictionary::from_slices(&[unsafe { kCTFontAttributeName }], &[&*font]);
    for (index, line) in lines
        .iter()
        .enumerate()
        .filter(|(_, line)| !line.is_empty())
    {
        let string = CFString::from_str(line);
        // SAFETY: 字符串和字体字典都是有效的 CF 对象，字典只含 CoreText 的字体属性。
        let line = unsafe {
            let text = CFAttributedString::new(None, Some(&string), Some(attributes.as_opaque()))
                .ok_or(Error::Render { page })?;
            CTLine::with_attributed_string(&text)
        };
        CGContext::set_text_position(Some(context), x, y - size * 1.6 * index as f64);
        // SAFETY: line 及目标 PDF 或位图 context 均有效并在当前线程使用。
        unsafe { line.draw(context) };
    }
    Ok(())
}

fn rect(x: f64, y: f64, width: f64, height: f64) -> CGRect {
    CGRect::new(CGPoint::new(x, y), CGSize::new(width, height))
}

// 前 3 页标 i–iii，之后从 1 起。
fn labels_pdf() -> Vec<u8> {
    let media = format!("/MediaBox [0 0 {WIDTH} {HEIGHT}]");
    let streams = LABEL_TEXT
        .iter()
        .map(|text| format!("BT /F1 18 Tf 55 728 Td ({text}) Tj ET"))
        .collect::<Vec<_>>();
    let pages = streams
        .iter()
        .map(|stream| (media.as_str(), stream.as_str()))
        .collect::<Vec<_>>();
    raw_pdf(
        "/PageLabels << /Nums [0 << /S /r >> 3 << /S /D >>] >>",
        &pages,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixtures_match_page_counts_labels_and_text_layers() {
        use Sample::*;

        for (sample, count, text_pages) in [
            (TextEn, 3, &[1, 2, 3][..]),
            (TextZh, 3, &[1, 2, 3]),
            (TwoColumnEn, 1, &[1]),
            (TwoColumnZh, 1, &[1]),
            (Mixed, 10, &[1, 4, 7, 10]),
            (ScannedTwoColumn, 1, &[]),
            (Labels, 6, &[1, 2, 3, 4, 5, 6]),
        ] {
            let pdf = Pdf::from_bytes(&sample.generate().unwrap()).unwrap();
            assert_eq!(pdf.page_count(), count, "{sample:?}");
            for page in 1..=count {
                let (label, text) = pdf.page(page).unwrap();
                if sample == Labels {
                    assert_eq!(
                        label.as_deref(),
                        Some(["i", "ii", "iii", "1", "2", "3"][(page - 1) as usize])
                    );
                }
                if text_pages.contains(&page) {
                    let expected = sample.source_text(page).unwrap();
                    assert_eq!(
                        text.chars()
                            .filter(|ch| !ch.is_whitespace())
                            .collect::<String>(),
                        expected
                            .chars()
                            .filter(|ch| !ch.is_whitespace())
                            .collect::<String>(),
                        "{sample:?} 第 {page} 页，PDFKit 原文为 {text:?}",
                    );
                } else {
                    assert!(
                        text.trim().is_empty(),
                        "{sample:?} 第 {page} 页意外有文字层"
                    );
                }
            }
        }
    }

    #[test]
    fn invalid_fixtures_produce_distinct_errors() {
        assert!(matches!(
            Pdf::from_bytes(&Sample::Locked.generate().unwrap()),
            Err(Error::Locked)
        ));
        assert!(matches!(
            Pdf::from_bytes(&Sample::Damaged.generate().unwrap()),
            Err(Error::Damaged)
        ));
        assert!(matches!(
            Pdf::from_bytes(&Sample::NotPdf.generate().unwrap()),
            Err(Error::NotPdf)
        ));
    }
}
