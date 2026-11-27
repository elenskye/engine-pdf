#![cfg(all(target_os = "macos", feature = "samples", feature = "pdfium"))]

use engine_pdf::edit::{self, Color, Mark, MarkKind, ObjectOp, Plan, Rect, Redaction, Save};
use engine_pdf::samples::raw_pdf;
use engine_pdf::{Cancel, Content, Document, Error, ImageFormat, ReadOptions, read};
use lopdf::{Document as Objects, Object, Stream, dictionary};

const PAGE: &str = "/MediaBox [0 0 240 240]";

#[test]
fn removes_moved_text_and_new_annotation_in_the_final_area() {
    let bytes = raw_pdf(
        "",
        &[
            (
                PAGE,
                "BT /F1 18 Tf 20 184 Td (HIDDEN) Tj ET BT /F1 18 Tf 20 60 Td (KEEP) Tj ET",
            ),
            (PAGE, "BT /F1 18 Tf 20 60 Td (UNCHANGED) Tj ET"),
        ],
    );
    let mut objects = Objects::load_mem(&bytes).unwrap();
    let old_ocr = objects.add_object(Stream::new(dictionary! {}, b"SECRET OCR RECORD".to_vec()));
    let target = objects.get_pages()[&1];
    objects
        .get_dictionary_mut(target)
        .unwrap()
        .set("EnginePDFOCRText", old_ocr);
    let mut bytes = Vec::new();
    objects.save_to(&mut bytes).unwrap();
    let source = Document::from_bytes(&bytes).unwrap();
    let moving = source.objects(1).unwrap()[0].id;
    let area = Rect {
        x: 100.0,
        y: 165.0,
        width: 120.0,
        height: 50.0,
    };
    let note = |x, y, text: &str| Mark {
        kind: MarkKind::Note {
            color: Color {
                red: 1.0,
                green: 0.8,
                blue: 0.0,
                alpha: 1.0,
            },
        },
        source: 0,
        page: 1,
        author: None,
        contents: Some(text.into()),
        rect: Rect {
            x,
            y,
            width: 15.0,
            height: 15.0,
        },
    };
    let plan = Plan {
        objects: vec![ObjectOp::Move {
            page: 1,
            id: moving,
            dx: 110.0,
            dy: 0.0,
        }],
        add: vec![
            note(150.0, 185.0, "HIDDEN NOTE"),
            note(190.0, 80.0, "KEEP NOTE"),
        ],
        redact: vec![Redaction {
            page: 1,
            rects: vec![area],
        }],
        ..Plan::default()
    };
    let cancel = Cancel::new();
    let bytes = edit::apply(&source, &[], &plan, &Save::default(), &cancel).unwrap();
    let result = Document::from_bytes(&bytes).unwrap();
    assert_eq!(result.text(1).unwrap().trim(), "KEEP");
    assert_eq!(source.text(2).unwrap(), result.text(2).unwrap());
    let annotations = result.annotations(1).unwrap();
    assert_eq!(annotations.len(), 1);
    assert_eq!(annotations[0].contents.as_deref(), Some("KEEP NOTE"));
    let rendered = result.render(1, ImageFormat::Gray, 240).unwrap();
    assert_eq!(rendered.bytes[45 * 240 + 160], 0);
    let parsed = Objects::load_mem(&bytes).unwrap();
    assert!(
        !parsed
            .get_dictionary(parsed.get_pages()[&1])
            .unwrap()
            .has(b"EnginePDFOCRText")
    );
    assert!(
        !bytes
            .windows(17)
            .any(|window| window == b"SECRET OCR RECORD")
    );
}

#[test]
fn rasterizes_partial_text_and_adds_back_searchable_words_outside_the_box() {
    let bytes = raw_pdf(
        "",
        &[(
            PAGE,
            "BT /F1 20 Tf 20 170 Td (PUBLIC     SECRET) Tj ET BT /F1 22 Tf 20 100 Td (KEEP OUTSIDE) Tj ET",
        )],
    );
    let source = Document::from_bytes(&bytes).unwrap();
    let cancel = Cancel::new();
    let plan = Plan {
        redact: vec![Redaction {
            page: 1,
            rects: vec![Rect {
                x: 118.0,
                y: 156.0,
                width: 111.0,
                height: 48.0,
            }],
        }],
        ..Plan::default()
    };
    let bytes = edit::apply(&source, &[], &plan, &Save::default(), &cancel).unwrap();
    let result = Document::from_bytes(&bytes).unwrap();
    let text = result.text(1).unwrap();
    assert!(text.contains("PUBLIC"), "{text}");
    assert!(text.contains("KEEP OUTSIDE"), "{text}");
    assert!(!text.contains("SECRET"), "{text}");
    let pages = read(
        &result,
        &[1],
        &ReadOptions {
            render: None,
            ..ReadOptions::default()
        },
        &cancel,
    )
    .unwrap();
    assert!(
        matches!(&pages[0].content, Content::SearchableText(text) if text.contains("KEEP OUTSIDE"))
    );
}

#[test]
fn patches_partial_image_pixels_and_rejects_a_still_shared_original() {
    let area = Rect {
        x: 100.0,
        y: 95.0,
        width: 75.0,
        height: 70.0,
    };
    let plan = Plan {
        redact: vec![Redaction {
            page: 1,
            rects: vec![area],
        }],
        ..Plan::default()
    };
    let cancel = Cancel::new();
    let (bytes, original) = image_pdf(false);
    let source = Document::from_bytes(&bytes).unwrap();
    let output = edit::apply(&source, &[], &plan, &Save::default(), &cancel).unwrap();
    let pdf = Objects::load_mem(&output).unwrap();
    let images = pdf.get_page_images(pdf.get_pages()[&1]).unwrap();
    assert_eq!(images.len(), 1);
    let image = pdf.get_object(images[0].id).unwrap().as_stream().unwrap();
    let pixels = image.get_plain_content().unwrap();
    assert_eq!(pixels.len(), original.len());
    assert_eq!(&pixels[(3 * 8 + 5) * 3..][..3], &[0, 0, 0]);
    assert_eq!(
        &pixels[(6 * 8 + 1) * 3..][..3],
        &original[(6 * 8 + 1) * 3..][..3]
    );
    assert!(
        !pdf.objects
            .values()
            .any(|object| matches!(object, Object::Stream(stream) if stream.content == original))
    );

    let (bytes, _) = image_pdf(true);
    let shared = Document::from_bytes(&bytes).unwrap();
    assert!(matches!(
        edit::apply(&shared, &[], &plan, &Save::default(), &cancel),
        Err(Error::RedactionSharedImage { page: 1 })
    ));

    let mut plan = plan;
    plan.redact.push(Redaction {
        page: 2,
        rects: vec![Rect { x: 60.0, ..area }],
    });
    assert!(matches!(
        edit::apply(&shared, &[], &plan, &Save::default(), &cancel),
        Err(Error::RedactionSharedImage { page: 1 })
    ));
    plan.redact[1].rects[0] = area;
    let output = edit::apply(&shared, &[], &plan, &Save::default(), &cancel).unwrap();
    assert_eq!(Document::from_bytes(&output).unwrap().page_count(), 2);
}

#[test]
fn rasterizes_a_skewed_image_on_a_rotated_crop_box() {
    let (bytes, original) = image_pdf(false);
    let mut data = Objects::load_mem(&bytes).unwrap();
    let id = data.get_pages()[&1];
    let content = data.get_page_contents(id)[0];
    data.get_object_mut(content)
        .unwrap()
        .as_stream_mut()
        .unwrap()
        .set_plain_content(b"q 90 52 -52 90 90 45 cm /Picture Do Q".to_vec());
    let page = data.get_dictionary_mut(id).unwrap();
    page.set(
        "CropBox",
        vec![10.into(), 20.into(), 230.into(), 220.into()],
    );
    page.set("Rotate", 90);
    let mut bytes = Vec::new();
    data.save_to(&mut bytes).unwrap();
    let source = Document::from_bytes(&bytes).unwrap();
    let plan = Plan {
        redact: vec![Redaction {
            page: 1,
            rects: vec![Rect {
                x: 95.0,
                y: 100.0,
                width: 40.0,
                height: 50.0,
            }],
        }],
        ..Plan::default()
    };
    let output = edit::apply(&source, &[], &plan, &Save::default(), &Cancel::new()).unwrap();
    let pdf = Objects::load_mem(&output).unwrap();
    let images = pdf.get_page_images(pdf.get_pages()[&1]).unwrap();
    assert_eq!(images.len(), 1);
    assert!(images[0].width > 8);
    assert!(
        !pdf.objects
            .values()
            .any(|object| matches!(object, Object::Stream(stream) if stream.content == original))
    );
}

#[test]
fn redacts_a_tagged_page_in_place_and_drops_alternate_text() {
    use engine_pdf::samples::raw_pdf_with_objects;

    let bytes = raw_pdf_with_objects(
        "/StructTreeRoot 6 0 R /MarkInfo << /Marked true >>",
        &[(
            "/MediaBox [0 0 240 240] /StructParents 0",
            concat!(
                "/P <</MCID 0>> BDC BT /F1 18 Tf 20 184 Td (HIDDEN) Tj ET EMC ",
                "/P <</MCID 1>> BDC BT /F1 18 Tf 20 60 Td (KEEP) Tj ET EMC ",
                "0 0 1 RG 2 w 10 190 m 230 190 l S"
            ),
        )],
        &[
            "<< /Type /StructTreeRoot /K [7 0 R 9 0 R] /ParentTree 8 0 R >>",
            "<< /Type /StructElem /S /P /P 6 0 R /Pg 4 0 R /K 0 /ActualText (TOPSECRET) >>",
            "<< /Nums [0 [7 0 R 9 0 R]] >>",
            "<< /Type /StructElem /S /P /P 6 0 R /Pg 4 0 R /K 1 >>",
        ],
    );
    let source = Document::from_bytes(&bytes).unwrap();
    let plan = Plan {
        redact: vec![Redaction {
            page: 1,
            rects: vec![Rect {
                x: 15.0,
                y: 176.0,
                width: 90.0,
                height: 28.0,
            }],
        }],
        ..Plan::default()
    };
    let output = edit::apply(&source, &[], &plan, &Save::default(), &Cancel::new()).unwrap();
    let result = Document::from_bytes(&output).unwrap();
    assert_eq!(result.text(1).unwrap().trim(), "KEEP");
    let pdf = Objects::load_mem(&output).unwrap();
    // 横穿涂黑框的线两端都在框外，不必整页重画：页上没有图，标签结构还在。
    let page = pdf.get_pages()[&1];
    assert!(pdf.get_page_images(page).unwrap().is_empty());
    assert!(pdf.get_dictionary(page).unwrap().has(b"StructParents"));
    // 结构树里的替代文字不能留着原文（字典照原样写在文件里），内容流里也没有原来的字。
    assert!(!output.windows(9).any(|window| window == b"TOPSECRET"));
    assert!(pdf.objects.values().all(|object| {
        match object {
            Object::Stream(stream) => !stream
                .get_plain_content()
                .unwrap_or_default()
                .windows(6)
                .any(|window| window == b"HIDDEN"),
            _ => true,
        }
    }));
}

fn image_pdf(shared: bool) -> (Vec<u8>, Vec<u8>) {
    let ops = (PAGE, "q 120 0 0 120 40 45 cm /Picture Do Q");
    let pages = if shared { vec![ops, ops] } else { vec![ops] };
    let bytes = raw_pdf("", &pages);
    let mut pdf = Objects::load_mem(&bytes).unwrap();
    let pixels: Vec<_> = (0..8)
        .flat_map(|i| (0..8).flat_map(move |j| [210, 45 + i * 15, 25 + j * 10]))
        .collect();
    let image = pdf.add_object(Stream::new(
        dictionary! {
            "Type" => "XObject", "Subtype" => "Image", "Width" => 8, "Height" => 8,
            "ColorSpace" => "DeviceRGB", "BitsPerComponent" => 8,
        },
        pixels.clone(),
    ));
    for page in pdf.get_pages().into_values() {
        pdf.get_dictionary_mut(page).unwrap().set(
            "Resources",
            dictionary! {
                "XObject" => dictionary! { "Picture" => image },
            },
        );
    }
    let mut output = Vec::new();
    pdf.save_to(&mut output).unwrap();
    (output, pixels)
}
