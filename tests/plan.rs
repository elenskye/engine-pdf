use engine_pdf::edit::{FieldValue, MarkKind, Plan, Target};
use serde_json::{Value, json};

#[test]
fn shared_plan_fixtures_round_trip() {
    let structure = include_str!("fixtures/plan/structure.json");
    let annotations = include_str!("fixtures/plan/annotations.json");
    let content = include_str!("fixtures/plan/content.json");
    for fixture in [structure, annotations, content] {
        let json: Value = serde_json::from_str(fixture).unwrap();
        let plan: Plan = serde_json::from_value(json.clone()).unwrap();
        assert_eq!(serde_json::to_value(plan).unwrap(), json);
    }

    let plan: Plan = serde_json::from_str(structure).unwrap();
    assert_eq!(plan.pages.as_ref().unwrap()[1].source, 1);
    assert!(matches!(
        plan.add[0].kind,
        MarkKind::Link {
            target: Target::Page { page: 2, y: 310.0 }
        }
    ));

    let plan: Plan = serde_json::from_str(annotations).unwrap();
    assert!(matches!(
        plan.add[1].kind,
        MarkKind::TextBox {
            font_size: 12.0,
            ..
        }
    ));
    assert_eq!(plan.fill[1].value, FieldValue::Checked(true));
}

#[test]
fn empty_plan_and_explicit_outline_removal_are_distinct() {
    let empty: Plan = serde_json::from_value(json!({})).unwrap();
    let clear_outline: Plan = serde_json::from_value(json!({ "outline": [] })).unwrap();
    assert_eq!(empty, Plan::default());
    assert_eq!(clear_outline.outline, Some(Vec::new()));
    assert!(serde_json::from_value::<Plan>(json!({ "lock": {} })).is_err());
    assert!(serde_json::from_value::<Plan>(json!({ "info": { "producer": "App" } })).is_err());
}

#[cfg(all(target_os = "macos", feature = "samples"))]
#[test]
fn shared_structure_fixture_writes_pages_outline_and_links() {
    use engine_pdf::edit::{self, AnnotationKind, Save};
    use engine_pdf::samples::Sample;
    use engine_pdf::{Cancel, Document, ImageFormat};

    let origin = Document::from_bytes(&Sample::Mixed.generate().unwrap()).unwrap();
    let insert = Document::from_bytes(&Sample::TextZh.generate().unwrap()).unwrap();
    let plan: Plan = serde_json::from_str(include_str!("fixtures/plan/structure.json")).unwrap();
    let output = edit::apply(&origin, &[&insert], &plan, &Save::default(), &Cancel::new()).unwrap();
    let output = Document::from_bytes(&output).unwrap();
    assert_eq!(output.page_count(), 3);
    let original = origin.render(2, ImageFormat::Gray, 256).unwrap();
    let rotated = output.render(1, ImageFormat::Gray, 256).unwrap();
    assert_eq!(
        (rotated.width, rotated.height),
        (original.height, original.width)
    );
    assert_eq!(output.text(2).unwrap(), insert.text(1).unwrap());
    assert_eq!(output.text(3).unwrap(), origin.text(2).unwrap());
    assert_eq!(output.info().title.as_deref(), Some("Edition one"));
    let outline = output.outline();
    assert_eq!(outline[0].title, "Part I");
    assert!(matches!(
        outline[0].children[0].target,
        Target::Page { page: 3, .. }
    ));
    for page in [1, 3] {
        assert!(
            output
                .annotations(page)
                .unwrap()
                .iter()
                .any(|mark| matches!(
                    mark.kind,
                    AnnotationKind::Link {
                        target: Target::Page { page: 2, .. }
                    }
                ))
        );
    }
    assert!(output.annotations(2).unwrap().iter().any(|mark| matches!(
        &mark.kind, AnnotationKind::Link { target: Target::Url { url } } if url == "https://example.com/reference"
    )));
}

#[cfg(all(target_os = "macos", feature = "samples"))]
#[test]
fn shared_annotations_fixture_writes_marks_and_live_form_values() {
    use engine_pdf::edit::{self, AnnotationKind, Save};
    use engine_pdf::samples::Sample;
    use engine_pdf::{Cancel, Document};

    let source = Document::from_bytes(&Sample::Form.generate().unwrap()).unwrap();
    let cancel = Cancel::new();
    let save = Save::default();
    let plan: Plan = serde_json::from_str(include_str!("fixtures/plan/annotations.json")).unwrap();
    let mut removed = plan.add[0].clone();
    let MarkKind::Highlight { color, .. } = removed.kind else {
        unreachable!()
    };
    removed.kind = MarkKind::Note { color };
    let original = Plan {
        add: vec![removed],
        ..Plan::default()
    };
    let annotated = edit::apply(&source, &[], &original, &save, &cancel).unwrap();
    let annotated = Document::from_bytes(&annotated).unwrap();
    let bytes = edit::apply(&annotated, &[], &plan, &save, &cancel).unwrap();
    let output = Document::from_bytes(&bytes).unwrap();
    let marks = output.annotations(1).unwrap();
    assert_eq!(marks.len(), 2);
    assert!(
        marks
            .iter()
            .any(|mark| matches!(mark.kind, AnnotationKind::Highlight)
                && mark.contents.as_deref() == Some("Check this."))
    );
    assert!(marks.iter().any(
        |mark| matches!(mark.mark, Some(MarkKind::TextBox { ref text, .. }) if text == "Revision")
    ));
    let fields = output.fields(&cancel).unwrap();
    for fill in plan.fill {
        let field = fields.iter().find(|field| field.name == fill.name).unwrap();
        assert_eq!(field.value.as_ref(), Some(&fill.value));
    }
}
