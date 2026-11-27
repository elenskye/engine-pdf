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
use crate::samples::{LABEL_TEXT, LOCKED_PASSWORD, Page, Sample, raw_pdf, raw_pdf_with_objects};
use crate::{Error, ImageFormat};

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
        Sample::Outline | Sample::Form | Sample::Annotated | Sample::Links => {
            Ok(structure_pdf(sample))
        }
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

/// 仿扫描件：把一页画成最长边 2400 像素的灰度图，再贴进一份只有图、没有文字层的单页 PDF。
pub(super) fn scan_page(page: &objc2_pdf_kit::PDFPage, index: u32) -> Result<Vec<u8>, Error> {
    let error = || Error::Render { page: index };
    let (bitmap, size) = super::render::draw_page(page, index, ImageFormat::Gray, 2400)?;
    let image = CGBitmapContextCreateImage(Some(&bitmap)).ok_or_else(error)?;
    let data = CFMutableData::new(None, 0).ok_or_else(error)?;
    let consumer = CGDataConsumer::with_cf_data(Some(&data)).ok_or_else(error)?;
    let media_box = rect(0.0, 0.0, size.width, size.height);
    // SAFETY: media_box 在调用期间有效，使用默认 PDF 文档属性。
    let pdf = unsafe { CGPDFContextCreate(Some(&consumer), &media_box, None) }.ok_or_else(error)?;
    // SAFETY: pdf 是有效的 PDF context，当前没有打开的页，沿用默认页属性。
    unsafe { CGPDFContextBeginPage(Some(&pdf), None) };
    CGContext::draw_image(Some(&pdf), media_box, Some(&image));
    CGPDFContextEndPage(Some(&pdf));
    CGPDFContextClose(Some(&pdf));
    Ok(data.to_vec())
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

fn structure_pdf(sample: Sample) -> Vec<u8> {
    let [note, text_box, arrow, stamp] = [
        (23, 23, "1 0.75 0 rg 1 2 m 1 22 l 22 22 l 22 7 l 7 7 l h f 0 g 5 16 13 1 re f 5 12 10 1 re f\n"),
        (180, 31, "0.9 0.95 1 rg 0 0 180 31 re f 0 g BT /F1 14 Tf 8 10 Td (A text box) Tj ET\n"),
        (81, 48, "0 0.5 0 RG 2 w 5 5 m 74 41 l S 62 40 m 74 41 l 66 32 l S\n"),
        // 2×2 RGB 图片，四个像素按 ASCIIHex 编码。
        (48, 36, "q 48 0 0 36 0 0 cm BI /W 2 /H 2 /BPC 8 /CS /RGB /F /AHx ID e43b3be8b9282975c443a047> EI Q\n"),
    ]
    .map(|(width, height, contents)| {
        format!(
            "<< /Type /XObject /Subtype /Form /BBox [0 0 {width} {height}] /Resources << /Font << /F1 3 0 R >> >> /Length {} >>\nstream\n{contents}endstream",
            contents.len(),
        )
    });
    let (catalog, page_entries, extra_objects): (&str, &[&str], &[&str]) = match sample {
        // 三页的页对象依次是 4、6、8，额外对象从 10 起。
        Sample::Outline => (
            "/Outlines 10 0 R",
            &["", "", ""],
            &[
                "<< /Type /Outlines /First 11 0 R /Last 14 0 R /Count 4 >>",
                "<< /Title (Introduction) /Parent 10 0 R /Next 13 0 R /First 12 0 R /Last 12 0 R /Count 1 /Dest [4 0 R /XYZ 0 320 null] >>",
                "<< /Title (Details) /Parent 11 0 R /Dest [6 0 R /XYZ 0 310 null] >>",
                "<< /Title (Appendix) /Parent 10 0 R /Prev 11 0 R /Next 14 0 R /Dest [8 0 R /XYZ 0 300 null] >>",
                "<< /Title (Reference) /Parent 10 0 R /Prev 13 0 R /A << /S /URI /URI (https://example.com/reference) >> >>",
            ],
        ),
        Sample::Form => (
            "/AcroForm << /Fields [6 0 R 7 0 R 8 0 R 11 0 R] /NeedAppearances true /DR << /Font << /Helv 3 0 R >> >> /DA (/Helv 12 Tf 0 g) >>",
            &["/Annots [6 0 R 7 0 R 9 0 R 10 0 R 11 0 R]"],
            &[
                "<< /Type /Annot /Subtype /Widget /FT /Tx /T (name) /V (Before) /Rect [30 270 210 294] /F 4 /P 4 0 R >>",
                "<< /Type /Annot /Subtype /Widget /FT /Btn /T (agree) /V /Off /AS /Off /Rect [30 220 50 240] /F 4 /P 4 0 R /AP << /N << /Off 12 0 R /Yes 13 0 R >> >> >>",
                "<< /FT /Btn /Ff 32768 /T (choice) /V /One /Kids [9 0 R 10 0 R] >>",
                "<< /Type /Annot /Subtype /Widget /Parent 8 0 R /AS /One /Rect [30 170 50 190] /F 4 /P 4 0 R /AP << /N << /Off 12 0 R /One 13 0 R >> >> >>",
                "<< /Type /Annot /Subtype /Widget /Parent 8 0 R /AS /Off /Rect [65 170 85 190] /F 4 /P 4 0 R /AP << /N << /Off 12 0 R /Two 13 0 R >> >> >>",
                "<< /Type /Annot /Subtype /Widget /FT /Ch /Ff 131072 /T (selection) /Opt [(Alpha) (Beta)] /V (Alpha) /Rect [30 120 140 144] /F 4 /P 4 0 R >>",
                "<< /Type /XObject /Subtype /Form /BBox [0 0 20 20] /Length 15 >>\nstream\n0 0 20 20 re S\nendstream",
                "<< /Type /XObject /Subtype /Form /BBox [0 0 20 20] /Length 15 >>\nstream\n2 2 16 16 re f\nendstream",
            ],
        ),
        Sample::Annotated => (
            "",
            &[
                "/Annots [6 0 R 7 0 R 8 0 R 9 0 R 10 0 R 11 0 R 12 0 R 13 0 R 14 0 R 15 0 R 16 0 R 17 0 R]",
            ],
            &[
                "<< /Type /Annot /Subtype /Highlight /Rect [29 326 74 344] /QuadPoints [29 344 74 344 29 326 74 326] /C [1 1 0] /F 4 >>",
                "<< /Type /Annot /Subtype /Underline /Rect [77 326 132 344] /QuadPoints [77 344 132 344 77 326 132 326] /C [0 0.5 0] /F 4 >>",
                "<< /Type /Annot /Subtype /StrikeOut /Rect [135 326 185 344] /QuadPoints [135 344 185 344 135 326 185 326] /C [1 0 0] /F 4 >>",
                "<< /Type /Annot /Subtype /Text /Rect [30 255 53 278] /Contents (A note) /T (Sample author) /C [1 0.75 0] /AP << /N 18 0 R >> /F 4 >>",
                "<< /Type /Annot /Subtype /FreeText /Rect [90 247 270 278] /Contents (A text box) /DA (/F1 14 Tf 0 g) /C [0.9 0.95 1] /AP << /N 19 0 R >> /F 4 >>",
                "<< /Type /Annot /Subtype /Ink /Rect [28 171 110 226] /InkList [[30 180 50 220 70 175 105 210]] /C [0.4 0.1 0.7] /BS << /W 3 >> /F 4 >>",
                "<< /Type /Annot /Subtype /Square /Rect [128 176 184 222] /C [0.8 0.2 0.1] /BS << /W 2 >> /F 4 >>",
                "<< /Type /Annot /Subtype /Circle /Rect [208 176 264 222] /C [0.1 0.5 0.8] /BS << /W 2 >> /F 4 >>",
                "<< /Type /Annot /Subtype /Line /Rect [28 105 109 149] /L [31 108 106 146] /C [0 0.5 0] /BS << /W 2 >> /F 4 >>",
                "<< /Type /Annot /Subtype /Line /Rect [125 103 206 151] /L [130 108 199 144] /LE [/None /OpenArrow] /C [0 0.5 0] /BS << /W 2 >> /AP << /N 20 0 R >> /F 4 >>",
                "<< /Type /Annot /Subtype /Stamp /Rect [231 108 279 144] /AP << /N 21 0 R >> /F 4 >>",
                "<< /Type /Annot /Subtype /Link /Rect [30 54 111 77] /A << /S /URI /URI (https://example.com/annotation) >> /Border [0 0 1] >>",
                &note,
                &text_box,
                &arrow,
                &stamp,
            ],
        ),
        Sample::Links => (
            "",
            &["/Annots [8 0 R 9 0 R]", ""],
            &[
                "<< /Type /Annot /Subtype /Link /Rect [30 300 150 320] /Dest [6 0 R /XYZ 0 310 null] /Border [0 0 0] >>",
                "<< /Type /Annot /Subtype /Link /Rect [30 270 150 290] /A << /S /URI /URI (https://example.com/link) >> /Border [0 0 0] >>",
            ],
        ),
        _ => unreachable!(),
    };
    let entries: Vec<_> = page_entries
        .iter()
        .map(|extra| format!("/MediaBox [0 0 400 400] {extra}"))
        .collect();
    let streams: Vec<_> = (1..=page_entries.len())
        .map(|page| {
            let text = sample.source_text(page as u32).unwrap();
            text.lines()
                .zip([(30, 330), (225, 276), (225, 225), (225, 175), (225, 125)])
                .map(|(line, (x, y))| format!("BT /F1 12 Tf {x} {y} Td ({line}) Tj ET\n"))
                .collect::<String>()
        })
        .collect();
    let pages: Vec<_> = entries
        .iter()
        .zip(&streams)
        .map(|(entry, stream)| (entry.as_str(), stream.as_str()))
        .collect();
    raw_pdf_with_objects(catalog, &pages, extra_objects)
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
            (Outline, 3, &[1, 2, 3]),
            (Form, 1, &[1]),
            (Annotated, 1, &[1]),
            (Links, 2, &[1, 2]),
        ] {
            let pdf = Pdf::from_bytes(&sample.generate().unwrap()).unwrap();
            assert_eq!(pdf.page_count(), count, "{sample:?}");
            for page in 1..=count {
                let (label, text, _) = pdf.page(page).unwrap();
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
