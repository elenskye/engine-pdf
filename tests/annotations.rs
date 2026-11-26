//! 已有标注的读取，以及新标注的保存、绘制和移除。

#![cfg(all(target_os = "macos", feature = "samples"))]

use engine_pdf::edit::{
    AnnotationKind, Color, Mark, MarkKind, Plan, Point, Quad, Rect, Save, Shape, StampFormat,
    Stroke, Target, apply,
};
use engine_pdf::samples::{Sample, raw_pdf};
use engine_pdf::{Cancel, Document, ImageFormat};

#[test]
fn saves_draws_and_removes_core_annotations() {
    let original = raw_pdf(
        "",
        &[(
            "/MediaBox [0 0 400 420]",
            "BT /F1 12 Tf 20 400 Td (Keep original text) Tj ET",
        )],
    );
    let source = Document::from_bytes(&original).unwrap();
    let color = Color {
        red: 0.8,
        green: 0.1,
        blue: 0.2,
        alpha: 0.6,
    };
    let blue = Color {
        red: 0.15,
        green: 0.4,
        blue: 0.9,
        alpha: 0.7,
    };
    let stroke = Stroke {
        width: 2.0,
        color: blue,
    };
    let target = Target::Url {
        url: "https://example.com/annotation".into(),
    };
    let expected = [
        AnnotationKind::Highlight,
        AnnotationKind::Underline,
        AnnotationKind::Strikeout,
        AnnotationKind::Note,
        AnnotationKind::TextBox,
        AnnotationKind::Ink,
        AnnotationKind::Rectangle,
        AnnotationKind::Ellipse,
        AnnotationKind::Line,
        AnnotationKind::Arrow,
        AnnotationKind::Link {
            target: target.clone(),
        },
    ];
    let marks: Vec<_> = expected
        .iter()
        .enumerate()
        .map(|(index, kind)| {
            let y = 18.0 + 32.0 * index as f64;
            let rect = Rect {
                x: 42.0,
                y,
                width: if *kind == AnnotationKind::Note {
                    24.0
                } else {
                    110.0
                },
                height: 24.0,
            };
            let point = |x, y| Point {
                x: rect.x + x,
                y: rect.y + y,
            };
            let quads = || {
                vec![Quad {
                    top_left: point(4.0, 20.0),
                    top_right: point(100.0, 20.0),
                    bottom_left: point(4.0, 3.0),
                    bottom_right: point(100.0, 3.0),
                }]
            };
            let kind = match kind {
                AnnotationKind::Highlight => MarkKind::Highlight {
                    quads: quads(),
                    color,
                },
                AnnotationKind::Underline => MarkKind::Underline {
                    quads: quads(),
                    color,
                },
                AnnotationKind::Strikeout => MarkKind::Strikeout {
                    quads: quads(),
                    color,
                },
                AnnotationKind::Note => MarkKind::Note { color },
                AnnotationKind::TextBox => MarkKind::TextBox {
                    text: "Some editable text".into(),
                    font_size: 12.0,
                    color,
                    border: Some(stroke),
                },
                AnnotationKind::Ink => MarkKind::Ink {
                    strokes: vec![vec![point(4.0, 5.0), point(30.0, 19.0), point(80.0, 7.0)]],
                    stroke,
                },
                AnnotationKind::Rectangle => MarkKind::Shape {
                    shape: Shape::Rectangle,
                    stroke,
                    fill: Some(color),
                },
                AnnotationKind::Ellipse => MarkKind::Shape {
                    shape: Shape::Ellipse,
                    stroke,
                    fill: None,
                },
                AnnotationKind::Line => MarkKind::Shape {
                    shape: Shape::Line {
                        start: point(4.0, 5.0),
                        end: point(95.0, 19.0),
                    },
                    stroke,
                    fill: None,
                },
                AnnotationKind::Arrow => MarkKind::Shape {
                    shape: Shape::Arrow {
                        start: point(4.0, 5.0),
                        end: point(95.0, 19.0),
                    },
                    stroke,
                    fill: None,
                },
                AnnotationKind::Link { target } => MarkKind::Link {
                    target: target.clone(),
                },
                _ => unreachable!(),
            };
            Mark {
                source: 0,
                page: 1,
                rect,
                author: Some("Fixture author".into()),
                contents: Some(format!("Comment {index}")),
                kind,
            }
        })
        .collect();
    let save = Save::default();
    let cancel = Cancel::new();
    let output = apply(
        &source,
        &[],
        &Plan {
            add: marks.clone(),
            ..Plan::default()
        },
        &save,
        &cancel,
    )
    .unwrap();
    let output = Document::from_bytes(&output).unwrap();
    let annotations = output.annotations(1).unwrap();
    assert_eq!(annotations.len(), expected.len());
    assert_eq!(output.text(1).unwrap(), source.text(1).unwrap());
    let before = source.render(1, ImageFormat::Gray, 420).unwrap();
    let after = output.render(1, ImageFormat::Gray, 420).unwrap();
    for (index, ((annotation, kind), mark)) in
        annotations.iter().zip(expected).zip(&marks).enumerate()
    {
        assert_eq!(annotation.kind, kind);
        assert_eq!(annotation.rect, mark.rect);
        assert_eq!(annotation.author, mark.author, "第 {index} 个标注作者不符");
        assert_eq!(
            annotation.contents, mark.contents,
            "第 {index} 个标注内容不符"
        );
        let color_matches = |actual: Color, expected: Color| {
            [
                actual.red - expected.red,
                actual.green - expected.green,
                actual.blue - expected.blue,
                actual.alpha - expected.alpha,
            ]
            .into_iter()
            .all(|difference| difference.abs() < 1e-6)
        };
        match (annotation.mark.as_ref().unwrap(), &mark.kind) {
            (
                MarkKind::Highlight { quads: actual, .. },
                MarkKind::Highlight {
                    quads: expected, ..
                },
            )
            | (
                MarkKind::Underline { quads: actual, .. },
                MarkKind::Underline {
                    quads: expected, ..
                },
            )
            | (
                MarkKind::Strikeout { quads: actual, .. },
                MarkKind::Strikeout {
                    quads: expected, ..
                },
            ) => {
                assert_eq!(actual, expected);
            }
            (
                MarkKind::TextBox {
                    text,
                    font_size,
                    border,
                    ..
                },
                MarkKind::TextBox { text: expected, .. },
            ) => {
                assert_eq!(text, expected);
                assert_eq!(*font_size, 12.0);
                assert_eq!(border.unwrap().width, stroke.width);
                assert!(color_matches(border.unwrap().color, stroke.color));
                assert!(color_matches(annotation.color.unwrap(), color));
            }
            (
                MarkKind::Ink {
                    strokes: actual,
                    stroke,
                },
                MarkKind::Ink {
                    strokes: expected,
                    stroke: expected_stroke,
                },
            ) => {
                assert_eq!(actual, expected);
                assert_eq!(stroke.width, expected_stroke.width);
                assert!(color_matches(stroke.color, expected_stroke.color));
            }
            (
                MarkKind::Shape {
                    shape: actual,
                    stroke,
                    fill,
                },
                MarkKind::Shape {
                    shape: expected,
                    stroke: expected_stroke,
                    fill: expected_fill,
                },
            ) => {
                assert_eq!(actual, expected);
                assert_eq!(stroke.width, expected_stroke.width);
                assert!(color_matches(stroke.color, expected_stroke.color));
                assert!(match (fill, expected_fill) {
                    (Some(actual), Some(expected)) => color_matches(*actual, *expected),
                    (None, None) => true,
                    _ => false,
                });
            }
            (MarkKind::Note { .. }, MarkKind::Note { .. }) => {}
            (MarkKind::Link { target: actual }, MarkKind::Link { target: expected }) => {
                assert_eq!(actual, expected)
            }
            pair => panic!("第 {index} 个标注的读回类型不符：{pair:?}"),
        }
        if !matches!(annotation.kind, AnnotationKind::Link { .. }) {
            let changed =
                ((420 - (mark.rect.y as usize + 24))..(420 - mark.rect.y as usize)).any(|row| {
                    let start = row * 400 + mark.rect.x as usize;
                    let end = start + mark.rect.width as usize;
                    before.bytes[start..end] != after.bytes[start..end]
                });
            assert!(changed, "第 {index} 个标注没有画出来");
        }
    }
    let removal = Plan {
        remove: annotations.into_iter().map(|a| a.id).collect(),
        ..Plan::default()
    };
    let removed = apply(&output, &[], &removal, &save, &cancel).unwrap();
    let structure = lopdf::Document::load_mem(&removed).unwrap();
    let remaining = structure
        .get_page_annotations(structure.get_pages()[&1])
        .unwrap();
    assert!(remaining.is_empty(), "删除后仍有标注：{remaining:?}");
    let removed = Document::from_bytes(&removed).unwrap();
    assert!(removed.annotations(1).unwrap().is_empty());
    let after_remove = removed.render(1, ImageFormat::Gray, 420).unwrap();
    assert_eq!(removed.text(1).unwrap(), source.text(1).unwrap());
    assert!(
        after_remove.bytes[40 * 400..] == before.bytes[40 * 400..],
        "标注区域仍有像素残留"
    );
    assert!(source.annotations(1).unwrap().is_empty());
}

#[test]
fn flattens_a_stamp_without_losing_chinese_text_or_link() {
    let source = Document::from_bytes(&Sample::TextZh.generate().unwrap()).unwrap();
    let image = raw_pdf("", &[("/MediaBox [0 0 12 12]", "1 0 0 rg 0 0 12 12 re f")]);
    let image = Document::from_bytes(&image)
        .unwrap()
        .render(1, ImageFormat::Png, 24)
        .unwrap();
    let rect = Rect {
        x: 32.0,
        y: 34.0,
        width: 50.0,
        height: 24.0,
    };
    let target = Target::Page { page: 3, y: 250.0 };
    let plan = Plan {
        add: vec![
            Mark {
                source: 0,
                page: 1,
                rect,
                author: None,
                contents: None,
                kind: MarkKind::Image {
                    format: StampFormat::Png,
                    bytes: image.bytes,
                },
            },
            Mark {
                source: 0,
                page: 1,
                rect,
                author: None,
                contents: None,
                kind: MarkKind::Link {
                    target: target.clone(),
                },
            },
        ],
        ..Plan::default()
    };
    let cancel = Cancel::new();
    let marked_bytes = apply(&source, &[], &plan, &Save::default(), &cancel).unwrap();
    let marked = Document::from_bytes(&marked_bytes).unwrap();
    assert!(
        matches!(&marked.annotations(1).unwrap()[0].mark, Some(MarkKind::Image { format: StampFormat::Png, bytes }) if bytes.starts_with(b"\x89PNG"))
    );
    let flat_bytes = apply(
        &marked,
        &[],
        &Plan::default(),
        &Save {
            flatten: true,
            ..Save::default()
        },
        &cancel,
    )
    .unwrap();
    let flat = Document::from_bytes(&flat_bytes).unwrap();
    let annotations = flat.annotations(1).unwrap();
    assert_eq!(annotations.len(), 1);
    assert_eq!(annotations[0].kind, AnnotationKind::Link { target });
    for page in 1..=source.page_count() {
        assert_eq!(flat.text(page).unwrap(), source.text(page).unwrap());
    }
    let before = source.render(1, ImageFormat::Gray, 400).unwrap();
    let after = flat.render(1, ImageFormat::Gray, 400).unwrap();
    let pixel = (after.height as usize - 23) * after.width as usize + 24;
    assert_eq!(before.bytes[pixel], 255);
    assert!(after.bytes[pixel] < 200);
}

#[test]
fn reads_existing_annotation_types_and_link_targets() {
    let sample = Document::from_bytes(&Sample::Annotated.generate().unwrap()).unwrap();
    let annotations = sample.annotations(1).unwrap();
    assert_eq!(annotations.len(), 12);
    assert_eq!(annotations[0].kind, AnnotationKind::Highlight);
    assert_eq!(annotations[3].kind, AnnotationKind::Note);
    assert_eq!(annotations[3].author.as_deref(), Some("Sample author"));
    assert_eq!(annotations[9].kind, AnnotationKind::Arrow);
    assert_eq!(annotations[10].kind, AnnotationKind::Image);
    assert_eq!(annotations[11].id.index, 11);

    let links = Document::from_bytes(&Sample::Links.generate().unwrap()).unwrap();
    let annotations = links.annotations(1).unwrap();
    assert_eq!(
        annotations[0].kind,
        AnnotationKind::Link {
            target: Target::Page { page: 2, y: 310.0 }
        }
    );
    assert_eq!(
        annotations[1].kind,
        AnnotationKind::Link {
            target: Target::Url {
                url: "https://example.com/link".into()
            }
        }
    );
}

#[test]
fn chinese_text_box_keeps_its_embedded_font() {
    let source = Document::from_bytes(&raw_pdf("", &[("/MediaBox [0 0 300 200]", "")])).unwrap();
    let text = "中文文本框 Text";
    let mark = Mark {
        source: 0,
        page: 1,
        rect: Rect {
            x: 20.0,
            y: 100.0,
            width: 160.0,
            height: 24.0,
        },
        author: None,
        contents: None,
        kind: MarkKind::TextBox {
            text: text.into(),
            font_size: 14.0,
            color: Color {
                red: 0.9,
                green: 0.2,
                blue: 0.1,
                alpha: 1.0,
            },
            border: None,
        },
    };
    let bytes = apply(
        &source,
        &[],
        &Plan {
            add: vec![mark],
            ..Plan::default()
        },
        &Save::default(),
        &Cancel::new(),
    )
    .unwrap();
    // PDFKit 写的 CFF 字体流没有 `/Length`，搬到输出里不能变成空流，否则「预览」画出乱码。
    let structure = lopdf::Document::load_mem(&bytes).unwrap();
    let mut fonts = 0;
    for object in structure.objects.values() {
        let Ok(descriptor) = object.as_dict() else {
            continue;
        };
        for key in [b"FontFile".as_slice(), b"FontFile2", b"FontFile3"] {
            if let Ok(file) = descriptor.get_deref(key, &structure) {
                fonts += 1;
                assert!(!file.as_stream().unwrap().content.is_empty());
            }
        }
    }
    assert!(fonts > 0);
    let output = Document::from_bytes(&bytes).unwrap();
    assert!(matches!(
        &output.annotations(1).unwrap()[0].mark,
        Some(MarkKind::TextBox { text: read, .. }) if read == text
    ));
}
