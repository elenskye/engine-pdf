#![cfg(all(target_os = "macos", feature = "samples"))]

use engine_pdf::compose::{Assembly, AssemblyLink, assemble};
use engine_pdf::edit::{InfoPatch, OutlineItem, Rect, Target};
use engine_pdf::samples::raw_pdf;
use engine_pdf::{Cancel, Document, Error};

#[test]
fn assembles_entire_parts_with_links_outline_and_info() {
    let page = "/MediaBox [0 0 300 200]";
    let first = raw_pdf(
        "",
        &[
            (page, "BT /F1 18 Tf 20 100 Td (First) Tj ET"),
            (page, "BT /F1 18 Tf 20 100 Td (Second) Tj ET"),
        ],
    );
    let second = raw_pdf("", &[(page, "BT /F1 18 Tf 20 100 Td (Third) Tj ET")]);
    let rect = Rect::from_top(200.0, 20.0, 30.0, 60.0, 18.0);
    let target = Target::Page { page: 3, y: 165.0 };
    let outline = vec![OutlineItem {
        title: "第一部分".into(),
        target: Target::Page { page: 1, y: 180.0 },
        children: vec![OutlineItem {
            title: "第三页".into(),
            target: target.clone(),
            children: Vec::new(),
        }],
    }];
    let how = Assembly {
        links: vec![
            AssemblyLink {
                page: 2,
                rect,
                target,
            },
            AssemblyLink {
                page: 3,
                rect,
                target: Target::Url {
                    url: "https://example.com/".into(),
                },
            },
        ],
        outline: Some(outline.clone()),
        info: Some(InfoPatch {
            title: Some("见字如面".into()),
            creator: Some("Pomelo".into()),
            ..InfoPatch::default()
        }),
    };
    let cancel = Cancel::new();
    let bytes = assemble(&[&first, &second], &how, &cancel).unwrap();
    let output = Document::from_bytes(&bytes).unwrap();
    assert_eq!(output.page_count(), 3);
    for (page, text) in ["First", "Second", "Third"].into_iter().enumerate() {
        assert_eq!(output.text(page as u32 + 1).unwrap(), text);
    }
    assert_eq!(output.outline(), outline);
    assert_eq!(output.info().title.as_deref(), Some("见字如面"));
    assert_eq!(output.info().creator.as_deref(), Some("Pomelo"));
    assert_eq!(
        assemble(&[&first], &Assembly::default(), &cancel).unwrap(),
        first
    );

    assert!(matches!(
        assemble(&[], &how, &cancel),
        Err(Error::EmptyAssembly)
    ));
    cancel.cancel();
    assert!(matches!(
        assemble(&[b"not a PDF"], &how, &cancel),
        Err(Error::Cancelled)
    ));
}
