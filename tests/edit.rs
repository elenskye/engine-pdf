//! 页面、目录和文档属性的写入与读回。

#![cfg(all(target_os = "macos", feature = "samples"))]

use engine_pdf::edit::{
    InfoPatch, Mark, MarkKind, OutlineItem, Pick, Plan, Rect, Save, Target, apply,
};
use engine_pdf::samples::{Sample, raw_pdf_with_objects};
use engine_pdf::{Cancel, Document, Error, ImageFormat};

#[test]
fn fills_live_forms_alongside_annotations_and_metadata() {
    use engine_pdf::edit::{AnnotationKind, Color, FieldValue, Fill};

    let doc = Document::from_bytes(&Sample::Form.generate().unwrap()).unwrap();
    let cancel = Cancel::new();
    let note = Mark {
        source: 0,
        page: 1,
        rect: Rect {
            x: 300.0,
            y: 34.0,
            width: 24.0,
            height: 24.0,
        },
        author: None,
        contents: Some("核对表单".into()),
        kind: MarkKind::Note {
            color: Color {
                red: 1.0,
                green: 1.0,
                blue: 0.0,
                alpha: 1.0,
            },
        },
    };
    let value = FieldValue::Text("填写后的名字".into());
    let plan = Plan {
        add: vec![note],
        fill: vec![Fill {
            name: "name".into(),
            value: value.clone(),
        }],
        info: Some(InfoPatch {
            title: Some("表单与便签".into()),
            ..InfoPatch::default()
        }),
        ..Plan::default()
    };
    let bytes = apply(&doc, &[], &plan, &Save::default(), &cancel).unwrap();
    let output = Document::from_bytes(&bytes).unwrap();
    assert_eq!(output.info().title.as_deref(), Some("表单与便签"));
    let old_fields = doc.fields(&cancel).unwrap();
    let fields = output.fields(&cancel).unwrap();
    assert_eq!(fields.len(), old_fields.len());
    assert_eq!(
        fields
            .iter()
            .find(|field| field.name == "name")
            .unwrap()
            .value,
        Some(value)
    );
    assert_eq!(
        old_fields
            .iter()
            .find(|field| field.name == "name")
            .unwrap()
            .value,
        Some(FieldValue::Text("Before".into()))
    );
    let annotations = output.annotations(1).unwrap();
    assert_eq!(annotations.len(), 1);
    assert_eq!(annotations[0].kind, AnnotationKind::Note);
    assert_eq!(annotations[0].contents.as_deref(), Some("核对表单"));
    assert!(annotations[0].author.is_none());
}

#[test]
fn fills_and_flattens_form_before_reordering() {
    use engine_pdf::edit::{FieldValue, Fill};

    let form = Document::from_bytes(&Sample::Form.generate().unwrap()).unwrap();
    let article = Document::from_bytes(&Sample::TextEn.generate().unwrap()).unwrap();
    let cancel = Cancel::new();
    let plan = Plan {
        pages: Some(vec![
            Pick {
                source: 1,
                page: 3,
                rotate: 0,
            },
            Pick {
                source: 0,
                page: 1,
                rotate: 90,
            },
        ]),
        fill: vec![Fill {
            name: "name".into(),
            value: FieldValue::Text("Form integrated".into()),
        }],
        ..Plan::default()
    };
    let bytes = apply(
        &form,
        &[&article],
        &plan,
        &Save {
            flatten: true,
            ..Save::default()
        },
        &cancel,
    )
    .unwrap();
    let output = Document::from_bytes(&bytes).unwrap();
    assert_eq!(output.page_count(), 2);
    assert_eq!(output.text(1).unwrap(), article.text(3).unwrap());
    assert!(output.text(2).unwrap().contains("Form integrated"));
    assert!(output.fields(&cancel).unwrap().is_empty());
    assert!(output.annotations(2).unwrap().is_empty());
}

#[test]
fn assembles_page_order_and_rotates_copies_independently() {
    let main = Document::from_bytes(&Sample::TextEn.generate().unwrap()).unwrap();
    let other = Document::from_bytes(&Sample::Outline.generate().unwrap()).unwrap();
    let original = main.render(2, ImageFormat::Gray, 256).unwrap();
    let plan = Plan {
        pages: Some(vec![
            Pick {
                source: 0,
                page: 2,
                rotate: 90,
            },
            Pick {
                source: 1,
                page: 3,
                rotate: 0,
            },
            Pick {
                source: 0,
                page: 2,
                rotate: 180,
            },
        ]),
        ..Plan::default()
    };
    let bytes = apply(&main, &[&other], &plan, &Save::default(), &Cancel::new()).unwrap();
    let output = Document::from_bytes(&bytes).unwrap();
    assert_eq!(output.page_count(), 3);
    assert_eq!(output.text(2).unwrap(), other.text(3).unwrap());
    assert_eq!(output.text(1).unwrap(), main.text(2).unwrap());
    assert_eq!(output.text(3).unwrap(), main.text(2).unwrap());
    let image = output.render(1, ImageFormat::Gray, 256).unwrap();
    assert_eq!(
        (image.width, image.height),
        (original.height, original.width)
    );
    let image = output.render(3, ImageFormat::Gray, 256).unwrap();
    assert_eq!(
        (image.width, image.height),
        (original.width, original.height)
    );
    assert_ne!(image.bytes, original.bytes);
    assert_eq!(
        main.render(2, ImageFormat::Gray, 256).unwrap().bytes,
        original.bytes
    );
}

#[test]
fn preserves_chinese_text_and_appearance_in_mixed_rotated_copies() {
    let latin = Document::from_bytes(&Sample::TextEn.generate().unwrap()).unwrap();
    let chinese = Document::from_bytes(&Sample::TextZh.generate().unwrap()).unwrap();
    let outline = vec![OutlineItem {
        title: "中文页".into(),
        target: Target::Page { page: 2, y: 231.0 },
        children: vec![],
    }];
    let bytes = apply(
        &latin,
        &[&chinese],
        &Plan {
            pages: Some(vec![
                Pick {
                    source: 0,
                    page: 2,
                    rotate: 0,
                },
                Pick {
                    source: 1,
                    page: 3,
                    rotate: 90,
                },
                Pick {
                    source: 1,
                    page: 1,
                    rotate: 0,
                },
                Pick {
                    source: 1,
                    page: 3,
                    rotate: 180,
                },
            ]),
            add: vec![Mark {
                source: 0,
                page: 2,
                rect: Rect {
                    x: 20.0,
                    y: 30.0,
                    width: 50.0,
                    height: 15.0,
                },
                author: None,
                contents: None,
                kind: MarkKind::Link {
                    target: Target::Page { page: 3, y: 240.0 },
                },
            }],
            outline: Some(outline.clone()),
            info: Some(InfoPatch {
                title: Some("中英混排".into()),
                creator: Some("engine-pdf-tests".into()),
                ..InfoPatch::default()
            }),
            ..Plan::default()
        },
        &Save::default(),
        &Cancel::new(),
    )
    .unwrap();
    let output = Document::from_bytes(&bytes).unwrap();
    assert_eq!(output.page_count(), 4);
    assert_eq!(output.text(1).unwrap(), latin.text(2).unwrap());
    assert_eq!(output.text(2).unwrap(), chinese.text(3).unwrap());
    assert_eq!(output.text(3).unwrap(), chinese.text(1).unwrap());
    assert_eq!(output.text(4).unwrap(), chinese.text(3).unwrap());
    assert_eq!(output.outline(), outline);
    assert_eq!(output.info().title.as_deref(), Some("中英混排"));
    assert_eq!(output.info().creator.as_deref(), Some("engine-pdf-tests"));

    let structure = lopdf::Document::load_mem(&bytes).unwrap();
    let pages = structure.get_pages();
    let annotations = structure.get_page_annotations(pages[&1]).unwrap();
    let link = annotations[0];
    assert_eq!(link.get(b"Subtype").unwrap().as_name().unwrap(), b"Link");
    let destination = if let Ok(destination) = link.get_deref(b"Dest", &structure) {
        destination
    } else {
        link.get_deref(b"A", &structure)
            .unwrap()
            .as_dict()
            .unwrap()
            .get_deref(b"D", &structure)
            .unwrap()
    };
    let destination = destination.as_array().unwrap();
    assert_eq!(destination[0].as_reference().unwrap(), pages[&3]);
    assert_eq!(destination[1].as_name().unwrap(), b"XYZ");
    assert_eq!(destination[3].as_float().unwrap(), 240.0);

    let unchanged = chinese.render(1, ImageFormat::Gray, 320).unwrap();
    assert_eq!(
        output.render(3, ImageFormat::Gray, 320).unwrap().bytes,
        unchanged.bytes
    );
    let original = chinese.render(3, ImageFormat::Gray, 320).unwrap();
    let right_angle = output.render(2, ImageFormat::Gray, 320).unwrap();
    assert_eq!(
        (right_angle.width, right_angle.height),
        (original.height, original.width)
    );
    let half_turn = output.render(4, ImageFormat::Gray, 320).unwrap();
    assert_eq!(
        (half_turn.width, half_turn.height),
        (original.width, original.height)
    );
    assert_ne!(half_turn.bytes, original.bytes);
}

#[test]
fn reads_replaces_and_clears_outline_and_writes_document_info() {
    let source = Document::from_bytes(&Sample::Outline.generate().unwrap()).unwrap();
    let existing = source.outline();
    assert_eq!(existing[0].title, "Introduction");
    assert_eq!(
        existing[0].children[0].target,
        Target::Page { page: 2, y: 310.0 }
    );
    assert_eq!(
        existing[2].target,
        Target::Url {
            url: "https://example.com/reference".into()
        }
    );
    let new_outline = vec![OutlineItem {
        title: "新目录".into(),
        target: Target::None,
        children: vec![OutlineItem {
            title: "阅读第 3 页".into(),
            target: Target::Page { page: 3, y: 235.0 },
            children: vec![OutlineItem {
                title: "参考网页".into(),
                target: Target::Url {
                    url: "https://example.com/?lang=zh".into(),
                },
                children: vec![],
            }],
        }],
    }];
    let plan = Plan {
        outline: Some(new_outline.clone()),
        info: Some(InfoPatch {
            title: Some("测试文档".into()),
            author: Some("测试作者".into()),
            subject: Some("测试主题".into()),
            creator: Some("测试生成者".into()),
            keywords: Some(vec!["测试词".into(), "phrase in English".into()]),
        }),
        ..Plan::default()
    };
    let save = Save::default();
    let cancel = Cancel::new();
    let bytes = apply(&source, &[], &plan, &save, &cancel).unwrap();
    let output = Document::from_bytes(&bytes).unwrap();
    assert_eq!(output.outline(), new_outline);
    let info = output.info();
    let patch = plan.info.unwrap();
    assert_eq!(info.title, patch.title);
    assert_eq!(info.author, patch.author);
    assert_eq!(info.subject, patch.subject);
    assert_eq!(info.creator, patch.creator);
    assert_eq!(info.keywords, patch.keywords.unwrap());

    let clear = Plan {
        outline: Some(vec![]),
        ..Plan::default()
    };
    let cleared = apply(&output, &[], &clear, &save, &cancel).unwrap();
    let cleared = Document::from_bytes(&cleared).unwrap();
    assert!(cleared.outline().is_empty());
    assert_eq!(cleared.info().title, patch.title);
    assert_eq!(source.outline(), existing);
}

#[test]
fn retains_contributing_outlines_and_remaps_the_first_copy_of_each_target() {
    let source = Document::from_bytes(&Sample::Outline.generate().unwrap()).unwrap();
    let plan = Plan {
        pages: Some(vec![
            Pick {
                source: 1,
                page: 2,
                rotate: 0,
            },
            Pick {
                source: 0,
                page: 1,
                rotate: 0,
            },
            Pick {
                source: 1,
                page: 2,
                rotate: 0,
            },
        ]),
        ..Plan::default()
    };
    let bytes = apply(&source, &[&source], &plan, &Save::default(), &Cancel::new()).unwrap();
    let outline = Document::from_bytes(&bytes).unwrap().outline();
    assert_eq!(outline.len(), 6);
    assert_eq!(outline[0].target, Target::Page { page: 2, y: 320.0 });
    assert_eq!(outline[0].children[0].title, "Details");
    assert_eq!(outline[0].children[0].target, Target::None);
    assert_eq!(outline[3].title, "Introduction");
    assert_eq!(outline[3].target, Target::None);
    assert_eq!(
        outline[3].children[0].target,
        Target::Page { page: 1, y: 310.0 }
    );
}

#[test]
fn preserves_no_op_bytes_and_protects_live_form_layout() {
    let bytes = Sample::Form.generate().unwrap();
    let form = Document::from_bytes(&bytes).unwrap();
    let save = Save::default();
    let cancel = Cancel::new();
    assert_eq!(
        apply(&form, &[], &Plan::default(), &save, &cancel).unwrap(),
        bytes
    );
    let plan = Plan {
        pages: Some(vec![Pick {
            source: 0,
            page: 1,
            rotate: 90,
        }]),
        ..Plan::default()
    };
    assert!(matches!(
        apply(&form, &[], &plan, &save, &cancel),
        Err(Error::FormEditUnavailable)
    ));
    let ordinary = Document::from_bytes(&Sample::Links.generate().unwrap()).unwrap();
    let plan = Plan {
        pages: Some(vec![Pick {
            source: 1,
            page: 1,
            rotate: 0,
        }]),
        ..Plan::default()
    };
    assert!(matches!(
        apply(&ordinary, &[&form], &plan, &save, &cancel),
        Err(Error::FormEditUnavailable)
    ));
}

#[test]
fn reports_invalid_page_selections_and_cancellation() {
    let doc = Document::from_bytes(&Sample::Links.generate().unwrap()).unwrap();
    let mut plan = Plan {
        pages: Some(vec![]),
        ..Plan::default()
    };
    let save = Save::default();
    let cancel = Cancel::new();
    assert!(matches!(
        apply(&doc, &[], &plan, &save, &cancel),
        Err(Error::BadPageSpec(_))
    ));
    plan.pages = Some(vec![Pick {
        source: 7,
        page: 1,
        rotate: 0,
    }]);
    assert!(matches!(
        apply(&doc, &[], &plan, &save, &cancel),
        Err(Error::InvalidSource { source: 7 })
    ));
    plan.pages = Some(vec![Pick {
        source: 0,
        page: 3,
        rotate: 0,
    }]);
    assert!(matches!(
        apply(&doc, &[], &plan, &save, &cancel),
        Err(Error::PageOutOfRange { page: 3, total: 2 })
    ));
    plan.pages = Some(vec![Pick {
        source: 0,
        page: 1,
        rotate: 45,
    }]);
    assert!(matches!(
        apply(&doc, &[], &plan, &save, &cancel),
        Err(Error::InvalidRotation { rotate: 45 })
    ));
    cancel.cancel();
    assert!(matches!(
        apply(&doc, &[], &Plan::default(), &save, &cancel),
        Err(Error::Cancelled)
    ));
}

/// 三页：第 1 页有指向第 2 页的具名链接和一个找不到目标的具名链接；页标签 i、ii、5；空表单字典；带结构树。
fn structured_sample() -> Vec<u8> {
    let marked =
        |text: &str| format!("/P << /MCID 0 >> BDC BT /F1 12 Tf 20 200 Td ({text}) Tj ET EMC");
    let pages = [
        marked("First page"),
        marked("Second page"),
        marked("Third page"),
    ];
    let links = "/Annots [<< /Type /Annot /Subtype /Link /Rect [20 20 80 40] /Dest (second) >> \
        << /Type /Annot /Subtype /Link /Rect [20 50 80 70] /Dest (nowhere) >>]";
    raw_pdf_with_objects(
        "/PageLabels << /Nums [0 << /S /r >> 2 << /S /D /St 5 >>] >> \
         /Names << /Dests << /Names [(second) [6 0 R /XYZ 0 150 null]] >> >> \
         /AcroForm << /Fields [] >> /MarkInfo << /Marked true >> /StructTreeRoot 10 0 R",
        &[
            (
                &format!("/MediaBox [0 0 300 300] /StructParents 0 {links}"),
                &pages[0],
            ),
            ("/MediaBox [0 0 300 300] /StructParents 1", &pages[1]),
            ("/MediaBox [0 0 300 300] /StructParents 2", &pages[2]),
        ],
        &[
            "<< /Type /StructTreeRoot /K [11 0 R 12 0 R 13 0 R] /ParentTree << /Nums [0 [11 0 R] 1 [12 0 R] 2 [13 0 R]] >> /ParentTreeNextKey 3 >>",
            "<< /Type /StructElem /S /P /P 10 0 R /Pg 4 0 R /K 0 >>",
            "<< /Type /StructElem /S /P /P 10 0 R /Pg 6 0 R /K 0 >>",
            "<< /Type /StructElem /S /P /P 10 0 R /Pg 8 0 R /K 0 >>",
        ],
    )
}

#[test]
fn reordering_keeps_labels_named_links_and_structure() {
    let bytes = structured_sample();
    let source = Document::from_bytes(&bytes).unwrap();
    let labels: Vec<_> = (1..=3).map(|page| source.label(page)).collect();
    assert_eq!(
        labels,
        [Some("i".into()), Some("ii".into()), Some("5".into())]
    );
    // 空的 `/AcroForm` 不算表单，不挡调整页序。
    assert!(source.fields(&Cancel::new()).unwrap().is_empty());
    let reverse: Vec<_> = (1..=3)
        .rev()
        .map(|page| Pick {
            source: 0,
            page,
            rotate: 0,
        })
        .collect();
    let output = apply(
        &source,
        &[],
        &Plan {
            pages: Some(reverse),
            ..Plan::default()
        },
        &Save::default(),
        &Cancel::new(),
    )
    .unwrap();
    let output_doc = Document::from_bytes(&output).unwrap();
    for (index, page) in (1..=3).rev().enumerate() {
        let index = index as u32 + 1;
        assert_eq!(output_doc.label(index), source.label(page));
        assert_eq!(output_doc.text(index).unwrap(), source.text(page).unwrap());
    }
    // 具名链接换成新页码；找不到目标的原样留着。
    use engine_pdf::edit::AnnotationKind;
    let links: Vec<_> = output_doc
        .annotations(3)
        .unwrap()
        .into_iter()
        .map(|annotation| annotation.kind)
        .collect();
    assert_eq!(
        links,
        [
            AnnotationKind::Link {
                target: Target::Page { page: 2, y: 150.0 }
            },
            AnnotationKind::Link {
                target: Target::None
            },
        ]
    );
    // 结构树跟着页走：元素的 `/Pg` 指向输出里对应的页，页上的 `/StructParents` 保留。
    let structure = lopdf::Document::load_mem(&output).unwrap();
    let pages = structure.get_pages();
    let catalog = structure.catalog().unwrap();
    let root = catalog.get_deref(b"StructTreeRoot", &structure).unwrap();
    let kids = root
        .as_dict()
        .unwrap()
        .get_deref(b"K", &structure)
        .unwrap()
        .as_array()
        .unwrap();
    let first = structure
        .get_dictionary(kids[0].as_reference().unwrap())
        .unwrap();
    assert_eq!(first.get(b"Pg").unwrap().as_reference().unwrap(), pages[&3]);
    let last_page = structure.get_dictionary(pages[&3]).unwrap();
    assert_eq!(
        last_page.get(b"StructParents").unwrap().as_i64().unwrap(),
        0
    );
}

#[test]
fn in_place_edits_keep_everything_else_byte_for_byte() {
    let bytes = structured_sample();
    let source = Document::from_bytes(&bytes).unwrap();
    let note = Mark {
        source: 0,
        page: 2,
        rect: Rect {
            x: 200.0,
            y: 200.0,
            width: 20.0,
            height: 20.0,
        },
        author: None,
        contents: Some("看这里".into()),
        kind: MarkKind::Note {
            color: engine_pdf::edit::Color {
                red: 1.0,
                green: 0.8,
                blue: 0.0,
                alpha: 1.0,
            },
        },
    };
    let mut rotate: Vec<_> = (1..=3)
        .map(|page| Pick {
            source: 0,
            page,
            rotate: 0,
        })
        .collect();
    rotate[2].rotate = 90;
    let output = apply(
        &source,
        &[],
        &Plan {
            pages: Some(rotate),
            add: vec![note],
            ..Plan::default()
        },
        &Save::default(),
        &Cancel::new(),
    )
    .unwrap();
    let original = lopdf::Document::load_mem(&bytes).unwrap();
    let edited = lopdf::Document::load_mem(&output).unwrap();
    // 内容流、Catalog 的页标签、具名目标、结构树都不动，只多一个便签、第 3 页转了 90°。
    for id in [5, 7, 9, 10, 11, 12, 13] {
        assert_eq!(original.objects[&(id, 0)], edited.objects[&(id, 0)]);
    }
    for key in [
        b"PageLabels".as_slice(),
        b"Names",
        b"StructTreeRoot",
        b"AcroForm",
    ] {
        assert_eq!(
            original.catalog().unwrap().get(key).unwrap(),
            edited.catalog().unwrap().get(key).unwrap()
        );
    }
    let page = edited.get_dictionary((8, 0)).unwrap();
    assert_eq!(page.get(b"Rotate").unwrap().as_i64().unwrap(), 90);
    let output = Document::from_bytes(&output).unwrap();
    assert_eq!(output.annotations(1).unwrap().len(), 2);
    assert_eq!(
        output.annotations(2).unwrap()[0].contents.as_deref(),
        Some("看这里")
    );
}
