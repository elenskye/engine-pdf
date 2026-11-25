//! Vision 的单页识别、坐标、读序和自动分类。

#![cfg(all(target_os = "macos", feature = "samples"))]

use engine_pdf::samples::{self, Sample, raw_pdf};
use engine_pdf::{
    Cancel, Content, Document, Error, Flow, NoTextReason, Ocr, OcrOptions, OcrReason, ReadOptions,
    read_each,
};

#[test]
fn recognizes_only_the_expected_mixed_pages() {
    let doc = Document::from_bytes(&Sample::Mixed.generate().unwrap()).unwrap();
    let opts = ReadOptions {
        ocr: Ocr::auto(),
        ..ReadOptions::default()
    };
    let mut classes = Vec::new();
    let summary = read_each(
        &doc,
        &[1, 2, 3, 4, 5, 6, 7, 8, 9, 10],
        &opts,
        &Cancel::new(),
        |page| {
            classes.push(match page.content {
                Content::Text(_) => "text",
                Content::Ocr(result) => {
                    assert_eq!(result.why, OcrReason::NoTextLayer);
                    assert!((0.0..=1.0).contains(&result.confidence));
                    if let Some(expected) = Sample::Mixed.source_text(page.index) {
                        let limit = if page.index == 2 { 0.02 } else { 0.05 };
                        check_character_error_rate(&expected, &result.text, limit);
                    }
                    "ocr"
                }
                Content::Blank => "blank",
                Content::NoText {
                    reason: NoTextReason::NothingRecognized,
                    partial,
                } => {
                    assert!(partial.trim().is_empty());
                    "no_text"
                }
                other => panic!("第 {} 页分类不符：{other:?}", page.index),
            });
            Flow::Continue
        },
    )
    .unwrap();
    assert_eq!(
        classes,
        [
            "text", "ocr", "blank", "text", "ocr", "no_text", "text", "ocr", "blank", "text"
        ]
    );
    assert_eq!(
        (summary.text, summary.ocr, summary.blank, summary.no_text),
        (4, 3, 2, 1)
    );
}

#[test]
fn reads_two_column_scan_left_then_right() {
    let sample = Sample::ScannedTwoColumn;
    let doc = Document::from_bytes(&sample.generate().unwrap()).unwrap();
    let result = doc.ocr(1, &OcrOptions::default()).unwrap();
    check_character_error_rate(&sample.source_text(1).unwrap(), &result.text, 0.05);
    let blocks: Vec<_> = result.text.split("\n\n").collect();
    assert_eq!(blocks.len(), 4, "{}", result.text);
    assert!(
        blocks
            .iter()
            .zip(["01", "02", "03", "04"])
            .all(|(block, prefix)| block.starts_with(prefix)),
        "{}",
        result.text
    );
    assert!(result.lines[..4].iter().all(|line| line.bounds.x < 306.0));
    assert!(result.lines[4..].iter().all(|line| line.bounds.x > 306.0));
}

#[test]
fn maps_rotated_crop_to_original_pdf_coordinates() {
    let bytes = raw_pdf(
        "",
        &[(
            "/MediaBox [0 0 600 800] /CropBox [40 50 560 740] /Rotate 90",
            "BT /F1 32 Tf 0 1 -1 0 180 100 Tm (Rotated page) Tj ET",
        )],
    );
    let doc = Document::from_bytes(&bytes).unwrap();
    let options = OcrOptions {
        languages: vec!["en-US".into()],
    };
    let result = doc.ocr(1, &options).unwrap();
    assert_eq!(result.why, OcrReason::Requested);
    assert_eq!(result.text, "Rotated page");
    let bounds = result.lines[0].bounds;
    assert!((150.0..185.0).contains(&bounds.x), "{bounds:?}");
    assert!((97.0..105.0).contains(&bounds.y), "{bounds:?}");
    assert!(bounds.height > bounds.width * 3.0, "{bounds:?}");
    assert!(matches!(
        doc.ocr(2, &options),
        Err(Error::PageOutOfRange { page: 2, total: 1 })
    ));
}

#[test]
fn scan_helper_turns_a_text_page_into_an_image_page_that_ocr_reads_back() {
    let source = Document::from_bytes(&Sample::TextEn.generate().unwrap()).unwrap();
    let scan = Document::from_bytes(&samples::scan(&source, 2).unwrap()).unwrap();
    assert_eq!(scan.page_count(), 1);
    assert!(
        scan.text(1).unwrap().trim().is_empty(),
        "仿扫描件不能带文字层"
    );
    let ocr = scan.ocr(1, &OcrOptions::default()).unwrap();
    check_character_error_rate(&source.text(2).unwrap(), &ocr.text, 0.02);
}

fn check_character_error_rate(expected: &str, actual: &str, limit: f64) {
    let expected: Vec<char> = expected.chars().filter(|c| !c.is_whitespace()).collect();
    let mut distances: Vec<usize> = (0..=expected.len()).collect();
    for (index, found) in actual.chars().filter(|c| !c.is_whitespace()).enumerate() {
        let mut diagonal = distances[0];
        distances[0] = index + 1;
        for (column, &target) in expected.iter().enumerate() {
            let above = distances[column + 1];
            distances[column + 1] = (diagonal + usize::from(found != target))
                .min(above + 1)
                .min(distances[column] + 1);
            diagonal = above;
        }
    }
    let rate = distances[expected.len()] as f64 / expected.len() as f64;
    assert!(
        rate <= limit,
        "字错率 {:.2}% 超出门槛 {:.2}%：\n{actual}",
        rate * 100.0,
        limit * 100.0,
    );
}
