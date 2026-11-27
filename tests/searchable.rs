#![cfg(all(target_os = "macos", feature = "samples"))]

use std::cell::RefCell;

use engine_pdf::edit::{Lock, Permissions, Pick, Plan, Save, apply};
use engine_pdf::samples::{Sample, raw_pdf};
use engine_pdf::{
    Cancel, Content, Document, Error, ImageFormat, Ocr, OcrOptions, ReadOptions, read, searchable,
};
use zeroize::Zeroizing;

#[test]
fn writes_only_selected_scans_and_reads_them_without_ocr() {
    let original = Sample::Mixed.generate().unwrap();
    let source = Document::from_bytes(&original).unwrap();
    let visited = RefCell::new(Vec::new());
    let opts = OcrOptions::default();
    let cancel = Cancel::new();
    let output = searchable(&source, &[5, 1, 3, 2, 5], &opts, &cancel, &|page| {
        visited.borrow_mut().push(page);
    })
    .unwrap();
    assert_eq!(visited.into_inner(), [5, 1, 3, 2]);
    let pdf = Document::from_bytes(&output).unwrap();
    assert_eq!(pdf.page_count(), source.page_count());
    for page in 1..=pdf.page_count() {
        assert_eq!(
            pdf.render(page, ImageFormat::Gray, 480).unwrap().bytes,
            source.render(page, ImageFormat::Gray, 480).unwrap().bytes,
            "第 {page} 页画面变化",
        );
        if ![2, 5].contains(&page) {
            assert_eq!(pdf.text(page).unwrap(), source.text(page).unwrap());
        }
    }
    assert!(pdf.text(2).unwrap().contains("quiet valley"));
    assert!(pdf.text(5).unwrap().contains("木桥"));

    let pages = read(
        &pdf,
        &[1, 2, 3, 5],
        &ReadOptions {
            ocr: Ocr::Off,
            render: None,
            ..ReadOptions::default()
        },
        &cancel,
    )
    .unwrap();
    assert!(matches!(&pages[0].content, Content::Text(_)));
    assert!(
        matches!(&pages[1].content, Content::SearchableText(text) if text.contains("quiet valley"))
    );
    assert!(matches!(&pages[2].content, Content::Blank));
    assert!(matches!(&pages[3].content, Content::SearchableText(text) if text.contains("木桥")));
    assert_eq!(
        searchable(&pdf, &[2, 5], &opts, &cancel, &|_| {}).unwrap(),
        output
    );

    // 换页序后，识别文字跟着页走。
    let plan = Plan {
        pages: Some(
            [5, 2]
                .map(|page| Pick {
                    source: 0,
                    page,
                    rotate: 0,
                })
                .to_vec(),
        ),
        ..Plan::default()
    };
    let reordered =
        Document::from_bytes(&apply(&pdf, &[], &plan, &Save::default(), &cancel).unwrap()).unwrap();
    let pages = read(
        &reordered,
        &[1, 2],
        &ReadOptions {
            ocr: Ocr::Off,
            render: None,
            ..ReadOptions::default()
        },
        &cancel,
    )
    .unwrap();
    assert!(matches!(&pages[0].content, Content::SearchableText(text) if text.contains("木桥")));
    assert!(
        matches!(&pages[1].content, Content::SearchableText(text) if text.contains("quiet valley"))
    );
}

#[test]
fn keeps_a_short_existing_text_layer_and_does_not_return_partial_output() {
    let bytes = raw_pdf(
        "",
        &[(
            "/MediaBox [0 0 300 300]",
            "0.2 g 20 20 200 200 re f BT /F1 12 Tf 20 240 Td (12) Tj ET",
        )],
    );
    let doc = Document::from_bytes(&bytes).unwrap();
    let opts = OcrOptions::default();
    let cancel = Cancel::new();
    assert_eq!(
        searchable(&doc, &[1], &opts, &cancel, &|_| {}).unwrap(),
        bytes
    );
    assert!(matches!(
        searchable(&doc, &[1, 2], &opts, &cancel, &|_| panic!("选页必须先验证")),
        Err(Error::PageOutOfRange { page: 2, total: 1 })
    ));
    assert!(matches!(
        searchable(&doc, &[1], &opts, &cancel, &|_| cancel.cancel()),
        Err(Error::Cancelled)
    ));
}

#[test]
fn requires_explicit_password_removal() {
    let source = Document::from_bytes(&Sample::Mixed.generate().unwrap()).unwrap();
    let owner = Zeroizing::new("fixture-owner".to_owned());
    let cancel = Cancel::new();
    let opts = OcrOptions::default();
    let encrypted = apply(
        &source,
        &[],
        &Plan::default(),
        &Save {
            lock: Some(Lock {
                owner: owner.clone(),
                user: None,
                permissions: Permissions::default(),
            }),
            ..Save::default()
        },
        &cancel,
    )
    .unwrap();
    let reader = Document::from_bytes(&encrypted).unwrap();
    assert!(matches!(
        searchable(&reader, &[2], &opts, &cancel, &|_| {}),
        Err(Error::OwnerPasswordRequired)
    ));
    let authorized = Document::from_bytes_with_password(&encrypted, &owner).unwrap();
    assert!(matches!(
        searchable(&authorized, &[2], &opts, &cancel, &|_| {}),
        Err(Error::PasswordActionRequired)
    ));
}
