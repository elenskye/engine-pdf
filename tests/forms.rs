#![cfg(all(target_os = "macos", feature = "samples"))]

use engine_pdf::edit::{self, FieldKind, FieldValue, Fill, Plan, Save};
use engine_pdf::samples::{Sample, raw_pdf};
use engine_pdf::{Cancel, Document, Error};

#[test]
fn reads_logical_fields_and_all_widget_locations() {
    let document = Document::from_bytes(&Sample::Form.generate().unwrap()).unwrap();
    let fields = document.fields(&Cancel::default()).unwrap();
    assert_eq!(fields.len(), 4);
    let field = |name| fields.iter().find(|field| field.name == name).unwrap();
    assert!(matches!(field("name").kind, FieldKind::Text));
    assert_eq!(field("name").value, Some(FieldValue::Text("Before".into())));
    assert!(matches!(field("agree").kind, FieldKind::Checkbox));
    assert_eq!(field("agree").value, Some(FieldValue::Checked(false)));
    let radio = field("choice");
    assert!(matches!(radio.kind, FieldKind::Radio));
    assert_eq!(radio.value, Some(FieldValue::Text("One".into())));
    assert_eq!(radio.widgets.len(), 2);
    assert_eq!(radio.widgets[0].page, 1);
    assert_eq!(radio.widgets[0].rect.x, 30.0);
    assert_eq!(radio.widgets[1].rect.x, 65.0);
    assert_eq!(radio.options[1].value, "Two");
    let dropdown = field("selection");
    assert!(matches!(dropdown.kind, FieldKind::Dropdown));
    assert_eq!(dropdown.value, Some(FieldValue::Text("Alpha".into())));
    assert_eq!(dropdown.options[1].label, "Beta");
}

#[test]
fn fills_values_and_keeps_the_original_form() {
    let bytes = Sample::Form.generate().unwrap();
    let document = Document::from_bytes(&bytes).unwrap();
    let cancel = Cancel::default();
    let values = [
        ("name", FieldValue::Text("New café — 中文".into())),
        ("agree", FieldValue::Checked(true)),
        ("choice", FieldValue::Text("Two".into())),
        ("selection", FieldValue::Text("Beta".into())),
    ];
    let plan = Plan {
        fill: values
            .iter()
            .map(|(name, value)| Fill {
                name: (*name).into(),
                value: value.clone(),
            })
            .collect(),
        ..Plan::default()
    };
    let filled = edit::apply(&document, &[], &plan, &Save::default(), &cancel).unwrap();
    let filled = Document::from_bytes(&filled).unwrap();
    let fields = filled.fields(&cancel).unwrap();
    for (name, value) in values {
        assert_eq!(
            fields
                .iter()
                .find(|field| field.name == name)
                .unwrap()
                .value,
            Some(value)
        );
    }
    assert_eq!(document.text(1).unwrap(), filled.text(1).unwrap());
    assert_eq!(
        document.fields(&cancel).unwrap()[0].value,
        Some(FieldValue::Text("Before".into()))
    );
}

#[test]
fn rejects_xfa_and_honors_cancellation() {
    let bytes = raw_pdf(
        "/AcroForm << /Fields [] /XFA [(template) (unsupported)] >>",
        &[("/MediaBox [0 0 100 100]", "")],
    );
    let document = Document::from_bytes(&bytes).unwrap();
    let cancel = Cancel::default();
    assert!(matches!(
        document.fields(&cancel),
        Err(Error::UnsupportedForm)
    ));
    cancel.cancel();
    assert!(matches!(document.fields(&cancel), Err(Error::Cancelled)));
}

#[test]
fn rejects_unknown_fields_and_invalid_values() {
    let document = Document::from_bytes(&Sample::Form.generate().unwrap()).unwrap();
    let fill = |name: &str, value| {
        let plan = Plan {
            fill: vec![Fill {
                name: name.into(),
                value,
            }],
            ..Plan::default()
        };
        edit::apply(&document, &[], &plan, &Save::default(), &Cancel::default())
    };
    assert!(matches!(
        fill("missing", FieldValue::Checked(true)),
        Err(Error::FieldNotFound)
    ));
    for (name, value) in [
        ("name", FieldValue::Checked(false)),
        ("choice", FieldValue::Text("Three".into())),
        ("selection", FieldValue::Text("Gamma".into())),
    ] {
        assert!(matches!(fill(name, value), Err(Error::InvalidFieldValue)));
    }
}

#[test]
fn respects_read_only_field_flags() {
    let mut source = lopdf::Document::load_mem(&Sample::Form.generate().unwrap()).unwrap();
    let catalog = source.catalog().unwrap();
    let form = source.get_dict_in_dict(catalog, b"AcroForm").unwrap();
    let field = form.get(b"Fields").unwrap().as_array().unwrap()[0]
        .as_reference()
        .unwrap();
    source.get_dictionary_mut(field).unwrap().set("Ff", 1);
    let mut bytes = Vec::new();
    source.save_to(&mut bytes).unwrap();
    let document = Document::from_bytes(&bytes).unwrap();
    let cancel = Cancel::default();
    assert!(document.fields(&cancel).unwrap()[0].read_only);
    let plan = Plan {
        fill: vec![Fill {
            name: "name".into(),
            value: FieldValue::Text("After".into()),
        }],
        ..Plan::default()
    };
    assert!(matches!(
        edit::apply(&document, &[], &plan, &Save::default(), &cancel),
        Err(Error::FieldReadOnly)
    ));
}
