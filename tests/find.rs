#![cfg(all(target_os = "macos", feature = "samples"))]

use engine_pdf::{
    Cancel, Document, Error, OcrOptions,
    find::{Match, Options, Query, Source, find},
    samples::{Sample, raw_pdf, raw_pdf_with_objects},
    searchable,
};

#[test]
fn finds_normalized_text_and_keeps_original_text_with_line_boxes() {
    let document = Document::from_bytes(&Sample::Mixed.generate().unwrap()).unwrap();
    let cancel = Cancel::new();
    let options = Options { ocr: None };
    for (page, query, original, boxes) in [
        (1, "ＭＯＲＮＩＮＧ　ＴＩＤＥ", "morning tide", 1),
        (1, "sunrise. Three small", "sunrise.\nThree small", 2),
        (4, "記錄清晨", "记录清晨", 1),
    ] {
        let found = find(
            &document,
            &[page],
            &Query::new(query),
            &options,
            &cancel,
            &|_, _| {},
        )
        .unwrap();
        assert_eq!(found.total, 1, "{query}");
        let hit = &found.hits[0];
        assert_eq!(hit.matched, original);
        assert_eq!(hit.source, Source::Text);
        assert_eq!(hit.how, Match::Phrase);
        assert_eq!(hit.location.page, page);
        assert_eq!(hit.location.rects.len(), boxes);
        assert!(
            hit.location
                .rects
                .iter()
                .all(|rect| rect.x >= 50.0 && rect.y >= 680.0 && rect.width > 5.0)
        );
    }
}

#[test]
fn falls_back_to_words_on_the_same_page_and_keeps_total_when_capped() {
    let document = Document::from_bytes(&Sample::Mixed.generate().unwrap()).unwrap();
    let cancel = Cancel::new();
    let options = Options { ocr: None };
    let mut query = Query::new("keeper fog");
    query.max_hits = 1;
    let found = find(&document, &[1, 6], &query, &options, &cancel, &|_, _| {}).unwrap();
    assert_eq!(found.total, 2);
    assert!(found.truncated);
    assert_eq!(found.hits.len(), 1);
    assert_eq!(found.hits[0].how, Match::Words);
    assert_eq!(found.hits[0].location.page, 1);

    query.max_hits = 0;
    let found = find(&document, &[1], &query, &options, &cancel, &|_, _| {}).unwrap();
    assert_eq!(found.total, 2);
    assert!(found.truncated);
    assert!(found.hits.is_empty());

    let missing = Query::new("keeper 港口");
    let found = find(&document, &[1, 4], &missing, &options, &cancel, &|_, _| {}).unwrap();
    assert_eq!(found.total, 0);
}

#[test]
fn carries_physical_page_and_label_and_respects_cancel_and_crop() {
    let labels = Document::from_bytes(&Sample::Labels.generate().unwrap()).unwrap();
    let options = Options { ocr: None };
    let cancel = Cancel::new();
    let query = Query::new("Preface");
    let found = find(&labels, &[2], &query, &options, &cancel, &|_, _| {}).unwrap();
    assert_eq!(found.hits[0].location.page, 2);
    assert_eq!(found.hits[0].location.label.as_deref(), Some("ii"));
    let result = find(
        &labels,
        &[1, 2, 3],
        &query,
        &options,
        &cancel,
        &|completed, page| {
            assert_eq!((completed, page), (1, 1));
            cancel.cancel();
        },
    );
    assert!(matches!(result, Err(Error::Cancelled)));

    let bytes = raw_pdf(
        "",
        &[(
            "/MediaBox [0 0 300 400] /CropBox [30 50 270 350] /Rotate 90",
            "BT /F1 14 Tf 90 240 Td (anchor text) Tj ET",
        )],
    );
    let document = Document::from_bytes(&bytes).unwrap();
    let found = find(
        &document,
        &[1],
        &Query::new("anchor"),
        &options,
        &Cancel::new(),
        &|_, _| {},
    )
    .unwrap();
    let rect = found.hits[0].location.rects[0];
    assert!((rect.x - 90.0).abs() < 2.0, "{rect:?}");
    assert!(rect.y >= 225.0 && rect.y <= 245.0, "{rect:?}");
}

#[test]
fn uses_vision_for_raw_scans_and_reuses_located_ocr_when_disabled() {
    let bytes = Sample::Mixed.generate().unwrap();
    let original = Document::from_bytes(&bytes).unwrap();
    let cancel = Cancel::new();
    let query = Query::new("quiet valley");
    let without_ocr = Options { ocr: None };
    assert_eq!(
        find(&original, &[2], &query, &without_ocr, &cancel, &|_, _| {})
            .unwrap()
            .total,
        0
    );
    let fresh = find(
        &original,
        &[2],
        &query,
        &Options::default(),
        &cancel,
        &|_, _| {},
    )
    .unwrap();
    assert_eq!(fresh.total, 1);
    assert_eq!(fresh.hits[0].source, Source::Ocr);
    assert!(!fresh.hits[0].location.rects.is_empty());

    let searchable = searchable(&original, &[2], &OcrOptions::default(), &cancel, &|_| {}).unwrap();
    let doc = Document::from_bytes(&searchable).unwrap();
    let cached = find(&doc, &[2], &query, &without_ocr, &cancel, &|_, _| {}).unwrap();
    assert_eq!(cached.total, 1);
    assert_eq!(cached.hits[0].source, Source::Ocr);
    let [rect] = cached.hits[0].location.rects.as_slice() else {
        panic!("预期一个 OCR 行框")
    };
    assert!(
        rect.x > 50.0 && rect.y > 650.0 && rect.width > 20.0,
        "{rect:?}"
    );
}

#[test]
fn preserves_cached_text_hits_when_location_cannot_be_verified() {
    let bytes = raw_pdf_with_objects(
        "",
        &[(
            "/MediaBox [0 0 300 400] /EnginePDFOCRText 6 0 R",
            "BT /F1 14 Tf 90 240 Td (other visible text) Tj ET",
        )],
        &["<< /Length 18 >>\nstream\nstored phrase only\nendstream"],
    );
    let document = Document::from_bytes(&bytes).unwrap();
    let found = find(
        &document,
        &[1],
        &Query::new("stored phrase"),
        &Options { ocr: None },
        &Cancel::new(),
        &|_, _| {},
    )
    .unwrap();
    assert_eq!(found.total, 1);
    let hit = &found.hits[0];
    assert_eq!(hit.matched, "stored phrase");
    assert_eq!(hit.source, Source::Ocr);
    assert_eq!(hit.location.page, 1);
    assert!(hit.location.rects.is_empty());
}

#[test]
fn locates_cached_ocr_when_its_reading_order_differs_from_the_pdf_layer() {
    let saved = "unique target\nother visible text";
    let stream = format!("<< /Length {} >>\nstream\n{saved}\nendstream", saved.len());
    let bytes = raw_pdf_with_objects(
        "",
        &[(
            "/MediaBox [0 0 300 400] /EnginePDFOCRText 6 0 R",
            "BT /F1 14 Tf 50 300 Td (other visible text) Tj 0 -50 Td (unique target) Tj ET",
        )],
        &[&stream],
    );
    let document = Document::from_bytes(&bytes).unwrap();
    assert!(document.text(1).unwrap().starts_with("other visible text"));
    let found = find(
        &document,
        &[1],
        &Query::new("unique target"),
        &Options { ocr: None },
        &Cancel::new(),
        &|_, _| {},
    )
    .unwrap();
    let hit = &found.hits[0];
    let [rect] = hit.location.rects.as_slice() else {
        panic!("可从文字层定位的 OCR 命中应有一个框");
    };
    assert_eq!(hit.source, Source::Ocr);
    assert_eq!(hit.matched, "unique target");
    assert!(rect.y > 230.0 && rect.y < 260.0, "{rect:?}");
}

#[test]
fn recognizes_scans_that_carry_only_a_short_text_stamp() {
    use lopdf::{Object, dictionary};

    // 扫描软件常在扫描页上盖一小段文字（页码、水印）：read 会识别这种页，find 也要。
    let mixed = Document::from_bytes(&Sample::Mixed.generate().unwrap()).unwrap();
    let mut pdf =
        lopdf::Document::load_mem(&engine_pdf::samples::scan(&mixed, 1).unwrap()).unwrap();
    let page = pdf.page_iter().next().unwrap();
    let font = pdf.add_object(dictionary! {
        "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica",
    });
    let resources = pdf.get_or_create_resources(page).unwrap();
    let resources = match resources {
        Object::Dictionary(dictionary) => dictionary,
        Object::Reference(_) => panic!("仿扫描件的资源应直接写在页字典里"),
        _ => unreachable!(),
    };
    resources.set("Font", dictionary! { "F9" => font });
    pdf.add_page_contents(page, b"BT /F9 8 Tf 20 20 Td (p. 12) Tj ET".to_vec())
        .unwrap();
    let mut bytes = Vec::new();
    pdf.save_to(&mut bytes).unwrap();

    let document = Document::from_bytes(&bytes).unwrap();
    assert_eq!(document.text(1).unwrap().trim(), "p. 12");
    let found = find(
        &document,
        &[1],
        &Query::new("morning tide"),
        &Options::default(),
        &Cancel::new(),
        &|_, _| {},
    )
    .unwrap();
    assert_eq!(found.total, 1);
    assert_eq!(found.hits[0].source, Source::Ocr);
    assert!(!found.hits[0].location.rects.is_empty());

    // 关掉 OCR 时，这几个字照样能找。
    let found = find(
        &document,
        &[1],
        &Query::new("p. 12"),
        &Options { ocr: None },
        &Cancel::new(),
        &|_, _| {},
    )
    .unwrap();
    assert_eq!((found.total, found.hits[0].source), (1, Source::Text));
}
