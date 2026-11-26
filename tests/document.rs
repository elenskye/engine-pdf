//! `Document`：打开、错误、文字层、页标签、页图、拆页。

#![cfg(all(target_os = "macos", feature = "samples"))]

use std::io::Write as _;

use engine_pdf::samples::{Sample, raw_pdf};
use engine_pdf::{Cancel, Document, Error, ImageFormat};

#[test]
fn opens_paths_and_reports_distinct_input_errors() {
    let path = std::env::temp_dir().join(format!("engine-pdf 测试-{}.pdf", std::process::id()));
    let mut file = std::fs::File::create_new(&path).unwrap();
    file.write_all(&Sample::Labels.generate().unwrap()).unwrap();
    drop(file);

    let opened = Document::open(&path);
    std::fs::remove_file(&path).unwrap();
    let doc = opened.unwrap();
    assert_eq!(doc.page_count(), 6);
    assert_eq!(
        (1..=6)
            .map(|page| doc.label(page).unwrap())
            .collect::<Vec<_>>(),
        ["i", "ii", "iii", "1", "2", "3"]
    );
    assert_eq!(doc.label(0), None);
    assert_eq!(doc.label(7), None);
    let unlabeled = Document::from_bytes(&Sample::TextEn.generate().unwrap()).unwrap();
    assert_eq!(unlabeled.label(1), None);
    assert!(matches!(
        doc.text(7),
        Err(Error::PageOutOfRange { page: 7, total: 6 })
    ));
    assert!(matches!(Document::open(&path), Err(Error::Io(_))));

    let sample_error = |sample: Sample| Document::from_bytes(&sample.generate().unwrap());
    assert!(matches!(sample_error(Sample::Locked), Err(Error::Locked)));
    assert!(matches!(sample_error(Sample::Damaged), Err(Error::Damaged)));
    assert!(matches!(sample_error(Sample::NotPdf), Err(Error::NotPdf)));
}

#[test]
fn reads_original_text_in_fixture_order() {
    let without_spaces = |text: &str| {
        text.chars()
            .filter(|c| !c.is_whitespace())
            .collect::<String>()
    };
    for (sample, count) in [
        (Sample::TextEn, 3),
        (Sample::TextZh, 3),
        (Sample::TwoColumnEn, 1),
        (Sample::TwoColumnZh, 1),
    ] {
        let doc = Document::from_bytes(&sample.generate().unwrap()).unwrap();
        assert_eq!(doc.page_count(), count);
        for page in 1..=count {
            assert_eq!(
                without_spaces(&doc.text(page).unwrap()),
                without_spaces(&sample.source_text(page).unwrap()),
                "{sample:?} 第 {page} 页"
            );
        }
    }
}

#[test]
fn split_preserves_text_and_formula_page_pixels() {
    let source = Document::from_bytes(&Sample::Mixed.generate().unwrap()).unwrap();
    let cancel = Cancel::new();
    let expected_text = source.text(1).unwrap();
    let expected_formula = source.render(8, ImageFormat::Gray, 512).unwrap();
    assert!(matches!(
        source.split(&[8, 11], &cancel),
        Err(Error::PageOutOfRange {
            page: 11,
            total: 10
        })
    ));
    let bytes = source.split(&[8, 1, 8, 1], &cancel).unwrap();
    let split = Document::from_bytes(&bytes).unwrap();
    assert_eq!(split.page_count(), 4);
    for page in [2, 4] {
        assert_eq!(split.text(page).unwrap(), expected_text);
    }
    for page in [1, 3] {
        assert!(split.text(page).unwrap().is_empty());
        let formula = split.render(page, ImageFormat::Gray, 512).unwrap();
        assert_eq!(
            (formula.width, formula.height),
            (expected_formula.width, expected_formula.height)
        );
        assert!(
            formula.bytes == expected_formula.bytes,
            "原第 8 页在拆页后第 {page} 页的灰度像素不一致"
        );
    }
    assert_eq!(source.page_count(), 10);
    assert_eq!(source.text(1).unwrap(), expected_text);
}

#[test]
fn renders_encoded_formats_and_rotated_gray_pixels() {
    let pdf = raw_pdf(
        "",
        &[(
            "/MediaBox [20 40 120 240] /Rotate 90",
            "0 g 30 210 20 20 re f",
        )],
    );
    let doc = Document::from_bytes(&pdf).unwrap();
    let gray = doc.render(1, ImageFormat::Gray, 256).unwrap();
    assert_eq!(
        (gray.width, gray.height, gray.bytes.len()),
        (256, 128, 256 * 128)
    );
    assert_eq!(gray.bytes[25 * 256 + 230], 0);
    assert_eq!(gray.bytes[0], 255);
    assert_eq!(gray.bytes[102 * 256 + 25], 255);
    let png = doc.render(1, ImageFormat::Png, 256).unwrap();
    assert_eq!((png.width, png.height), (256, 128));
    assert!(png.bytes.starts_with(b"\x89PNG\r\n\x1a\n"));
    assert_eq!(&png.bytes[16..24], &[0, 0, 1, 0, 0, 0, 0, 128]);
    let jpeg = doc.render(1, ImageFormat::Jpeg, 256).unwrap();
    assert_eq!((jpeg.width, jpeg.height), (256, 128));
    assert!(jpeg.bytes.starts_with(b"\xff\xd8") && jpeg.bytes.ends_with(b"\xff\xd9"));
    assert!(matches!(
        doc.render(1, ImageFormat::Gray, 0),
        Err(Error::Render { page: 1 })
    ));
}

#[test]
fn accepts_a_header_after_stray_bytes() {
    let mut bytes = b"stray bytes from a download\r\n".to_vec();
    bytes.extend(Sample::TextEn.generate().unwrap());
    let doc = Document::from_bytes(&bytes).unwrap();
    assert_eq!(doc.page_count(), 3);
    assert!(doc.text(1).unwrap().contains("harbor"));
}

#[test]
fn renders_the_crop_box_that_readers_show() {
    // MediaBox 200×200，CropBox 是中间 100×100：左下角的黑块在裁掉的区域里，中间的黑块在里面。
    let pdf = raw_pdf(
        "",
        &[(
            "/MediaBox [0 0 200 200] /CropBox [50 50 150 150]",
            "0 g 0 0 40 40 re f 60 60 20 20 re f",
        )],
    );
    let doc = Document::from_bytes(&pdf).unwrap();
    let gray = doc.render(1, ImageFormat::Gray, 100).unwrap();
    assert_eq!((gray.width, gray.height), (100, 100));
    let at = |x: usize, y_from_bottom: usize| gray.bytes[(99 - y_from_bottom) * 100 + x];
    assert_eq!(at(20, 20), 0, "CropBox 里的黑块要画出来");
    assert_eq!(
        gray.bytes.iter().filter(|&&v| v < 128).count(),
        400,
        "只有这一个 20×20 的黑块"
    );
}
