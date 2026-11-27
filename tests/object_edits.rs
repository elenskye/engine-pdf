#![cfg(all(target_os = "macos", feature = "samples"))]

use engine_pdf::edit::{self, ObjectOp, Plan, Save};
use engine_pdf::samples::raw_pdf;
use engine_pdf::{Cancel, Document, Error};

const PAGE: &str = "/MediaBox [0 0 240 240]";

#[cfg(feature = "pdfium")]
#[test]
fn moves_and_deletes_by_original_object_id() {
    use engine_pdf::ImageFormat;
    use engine_pdf::edit::{ObjectKind, Pick};

    let source = Document::from_bytes(&raw_pdf(
        "/PageLabels << /Nums [0 << /S /D /P (p-) >>] >>",
        &[
            (
                PAGE,
                "BT /F1 20 Tf 30 180 Td (Delete me) Tj ET BT /F1 20 Tf 30 130 Td (Move me) Tj ET",
            ),
            (PAGE, "BT /F1 20 Tf 30 180 Td (Untouched) Tj ET"),
        ],
    ))
    .unwrap();
    let items = source.objects(1).unwrap();
    assert_eq!(items[0].text.as_deref(), Some("Delete me"));
    assert_eq!(items[1].kind, ObjectKind::Text);
    let cancel = Cancel::new();
    let plan = Plan {
        objects: vec![
            ObjectOp::Delete {
                page: 1,
                id: items[0].id,
            },
            ObjectOp::Move {
                page: 1,
                id: items[1].id,
                dx: 25.0,
                dy: -30.0,
            },
            ObjectOp::Move {
                page: 1,
                id: items[1].id,
                dx: 5.0,
                dy: 0.0,
            },
        ],
        ..Plan::default()
    };
    let bytes = edit::apply(&source, &[], &plan, &Save::default(), &cancel).unwrap();
    let edited = Document::from_bytes(&bytes).unwrap();
    let object = edited.objects(1).unwrap().remove(0);
    assert_eq!(edited.text(1).unwrap().trim(), "Move me");
    assert!((object.bounds.x - items[1].bounds.x - 30.0).abs() < 0.1);
    assert!((object.bounds.y - items[1].bounds.y + 30.0).abs() < 0.1);
    assert_eq!(edited.label(1).as_deref(), Some("p-1"));
    let before = source.render(1, ImageFormat::Gray, 240).unwrap();
    let after = edited.render(1, ImageFormat::Gray, 240).unwrap();
    assert_ne!(before.bytes, after.bytes);
    assert_eq!(source.text(2).unwrap(), edited.text(2).unwrap());

    let reorder = Plan {
        pages: Some(vec![
            Pick {
                source: 0,
                page: 2,
                rotate: 0,
            },
            Pick {
                source: 0,
                page: 1,
                rotate: 0,
            },
        ]),
        ..plan
    };
    let bytes = edit::apply(&source, &[], &reorder, &Save::default(), &cancel).unwrap();
    let edited = Document::from_bytes(&bytes).unwrap();
    assert_eq!(edited.text(1).unwrap().trim(), "Untouched");
    assert_eq!(edited.text(2).unwrap().trim(), "Move me");
}

#[cfg(feature = "pdfium")]
#[test]
fn moves_and_deletes_an_image() {
    use engine_pdf::ImageFormat;
    use engine_pdf::edit::ObjectKind;
    use engine_pdf::samples::Sample;

    let source = Document::from_bytes(&Sample::ScannedTwoColumn.generate().unwrap()).unwrap();
    let image = source
        .objects(1)
        .unwrap()
        .into_iter()
        .find(|object| object.kind == ObjectKind::Image)
        .unwrap();
    let plan = Plan {
        objects: vec![ObjectOp::Move {
            page: 1,
            id: image.id,
            dx: 12.0,
            dy: -15.0,
        }],
        ..Plan::default()
    };
    let cancel = Cancel::new();
    let save = Save::default();
    let bytes = edit::apply(&source, &[], &plan, &save, &cancel).unwrap();
    let output = Document::from_bytes(&bytes).unwrap();
    let moved = output
        .objects(1)
        .unwrap()
        .into_iter()
        .find(|object| object.kind == ObjectKind::Image)
        .unwrap();
    assert!((moved.bounds.x - image.bounds.x - 12.0).abs() < 0.1);
    assert!((moved.bounds.y - image.bounds.y + 15.0).abs() < 0.1);
    let plan = Plan {
        objects: vec![ObjectOp::Delete {
            page: 1,
            id: moved.id,
        }],
        ..Plan::default()
    };
    let bytes = edit::apply(&output, &[], &plan, &save, &cancel).unwrap();
    let output = Document::from_bytes(&bytes).unwrap();
    assert!(
        !output
            .objects(1)
            .unwrap()
            .iter()
            .any(|object| object.kind == ObjectKind::Image)
    );
    assert!(
        output
            .render(1, ImageFormat::Gray, 240)
            .unwrap()
            .bytes
            .iter()
            .all(|&pixel| pixel == 255)
    );
}

#[cfg(feature = "pdfium")]
#[test]
fn keeps_the_rest_of_the_page_byte_for_byte() {
    use engine_pdf::ImageFormat;

    // 第二行靠 TJ 的字距排版：整页重写会丢掉字距，只改一条指令则不会。
    let source = Document::from_bytes(&raw_pdf(
        "",
        &[(
            PAGE,
            "BT /F1 20 Tf 30 180 Td (Move me) Tj ET BT /F1 20 Tf 20 60 Td [(W) 900 (ide) -1500 (gaps)] TJ (tail) Tj ET",
        )],
    ))
    .unwrap();
    let items = source.objects(1).unwrap();
    let plan = Plan {
        objects: vec![ObjectOp::Move {
            page: 1,
            id: items[0].id,
            dx: 0.0,
            dy: 20.0,
        }],
        ..Plan::default()
    };
    let cancel = Cancel::new();
    let bytes = edit::apply(&source, &[], &plan, &Save::default(), &cancel).unwrap();
    let edited = Document::from_bytes(&bytes).unwrap();
    let rows =
        |doc: &Document| doc.render(1, ImageFormat::Gray, 240).unwrap().bytes[120 * 240..].to_vec();
    assert_eq!(rows(&source), rows(&edited));

    // 删掉后面还接着画字的一段要知道字宽；标准 14 字体没有 /Widths，拒绝而不是挪动后面的字。
    let plan = Plan {
        objects: vec![ObjectOp::Delete {
            page: 1,
            id: items[1].id,
        }],
        ..Plan::default()
    };
    assert!(matches!(
        edit::apply(&source, &[], &plan, &Save::default(), &cancel),
        Err(Error::ObjectNotEditable { page: 1, .. })
    ));
}

#[test]
fn reports_unavailable_or_missing_page_objects() {
    let source = Document::from_bytes(&raw_pdf("", &[(PAGE, "")])).unwrap();
    let plan = Plan {
        objects: vec![ObjectOp::Delete { page: 1, id: 5 }],
        ..Plan::default()
    };
    let result = edit::apply(&source, &[], &plan, &Save::default(), &Cancel::new());
    if cfg!(feature = "pdfium") {
        assert!(matches!(
            result,
            Err(Error::ObjectNotFound { page: 1, id: 5 })
        ));
    } else {
        assert!(matches!(result, Err(Error::PdfiumUnavailable)));
    }
}
