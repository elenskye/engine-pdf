use std::collections::{BTreeMap, HashMap, HashSet};

use lopdf::{
    Dictionary, Document, Object, ObjectId, StringFormat, decode_text_string, dictionary,
    text_string,
};
use objc2_core_foundation::{CGPoint, CGRect, CGSize};
use objc2_core_graphics::CGContext;
use objc2_foundation::ns_string;
use objc2_pdf_kit::{PDFAnnotation, PDFDisplayBox};

use crate::edit::{Field, FieldKind, FieldOption, FieldValue, FieldWidget, Fill, Rect};
use crate::{Cancel, Error};

use super::Pdf;
use super::objects::{self, array, value};

const READ_ONLY: i64 = 1;
const RADIO: i64 = 1 << 15;
const PUSH_BUTTON: i64 = 1 << 16;
const COMBO: i64 = 1 << 17;
const EDITABLE_COMBO: i64 = 1 << 18;
const RADIOS_IN_UNISON: i64 = 1 << 25;

struct Widget {
    id: ObjectId,
    location: FieldWidget,
    on_states: Vec<Vec<u8>>,
    active_state: Option<Vec<u8>>,
}

struct RadioOption {
    export: String,
    state: Vec<u8>,
    widget: ObjectId,
    selected: bool,
}

struct FormField {
    id: ObjectId,
    public: Field,
    widgets: Vec<Widget>,
    editable_combo: bool,
    radio_options: Vec<RadioOption>,
    max_length: Option<usize>,
    textual_appearance: bool,
    radios_in_unison: bool,
}

#[derive(Clone, Copy, Default)]
struct Inherited<'a> {
    field_type: &'a [u8],
    flags: i64,
    value: Option<&'a Object>,
    options: &'a [Object],
    max_length: Option<usize>,
}

pub(crate) fn fields(pdf: &Pdf, cancel: &Cancel) -> Result<Vec<Field>, Error> {
    cancel.check()?;
    if !pdf.has_form() {
        return Ok(Vec::new());
    }
    let document = pdf.objects()?;
    Ok(read_form(&document, cancel)?
        .into_iter()
        .map(|field| field.public)
        .collect())
}

/// 读写只针对同一布局下的字段及控件，保留现有页树、目录和交互结构。
pub(super) fn fill(bytes: &[u8], fills: &[Fill], cancel: &Cancel) -> Result<Vec<u8>, Error> {
    cancel.check()?;
    let mut document = objects::load_plain(bytes)?;
    let fields = read_form(&document, cancel)?;
    let mut refreshed = HashSet::new();
    for fill in fills {
        cancel.check()?;
        let mut matching = fields.iter().filter(|field| field.public.name == fill.name);
        let field = matching.next().ok_or(Error::FieldNotFound)?;
        if matching.next().is_some() {
            return Err(Error::InvalidFieldValue);
        }
        if field.public.read_only {
            return Err(Error::FieldReadOnly);
        }
        let mut radio_widget = None;
        let (value, index) = match (field.public.kind, &fill.value) {
            (FieldKind::Text, FieldValue::Text(text)) => {
                if field
                    .max_length
                    .is_some_and(|max| text.chars().count() > max)
                {
                    return Err(Error::InvalidFieldValue);
                }
                refreshed.insert(field.id);
                (text_string(text), None)
            }
            (FieldKind::Dropdown, FieldValue::Text(text)) => {
                let index = field
                    .public
                    .options
                    .iter()
                    .position(|option| option.value == *text);
                if index.is_none() && !field.editable_combo {
                    return Err(Error::InvalidFieldValue);
                }
                refreshed.insert(field.id);
                (text_string(text), index)
            }
            (FieldKind::Checkbox, FieldValue::Checked(checked)) => {
                let state = if *checked {
                    let mut states = field.widgets.iter().flat_map(|widget| &widget.on_states);
                    let selected = states.next().cloned().unwrap_or_else(|| b"Yes".to_vec());
                    if states.any(|state| *state != selected) {
                        return Err(Error::InvalidFieldValue);
                    }
                    selected
                } else {
                    b"Off".to_vec()
                };
                (Object::Name(state), None)
            }
            (FieldKind::Radio, FieldValue::Text(text)) => {
                let option = field
                    .radio_options
                    .iter()
                    .find(|option| option.export == *text)
                    .ok_or(Error::InvalidFieldValue)?;
                radio_widget = Some(option.widget);
                (Object::Name(option.state.clone()), None)
            }
            (FieldKind::Other, _) => return Err(Error::UnsupportedForm),
            _ => return Err(Error::InvalidFieldValue),
        };

        let entry = document
            .get_dictionary_mut(field.id)
            .map_err(|_| Error::SaveFailed)?;
        entry.set("V", value.clone());
        if field.public.kind == FieldKind::Text {
            entry.remove(b"RV");
        }
        if field.public.kind == FieldKind::Dropdown {
            if let Some(index) = index {
                entry.set("I", vec![Object::Integer(index as i64)]);
            } else {
                entry.remove(b"I");
            }
        }
        for widget in &field.widgets {
            let entry = document
                .get_dictionary_mut(widget.id)
                .map_err(|_| Error::SaveFailed)?;
            if widget.id != field.id {
                entry.remove(b"V");
            }
            if let Object::Name(state) = &value {
                let matching_state = widget.on_states.contains(state)
                    || (field.public.kind == FieldKind::Checkbox && widget.on_states.is_empty());
                let selected = matching_state
                    && (field.public.kind != FieldKind::Radio
                        || field.radios_in_unison
                        || radio_widget == Some(widget.id));
                entry.set(
                    "AS",
                    Object::Name(if selected {
                        state.clone()
                    } else {
                        b"Off".to_vec()
                    }),
                );
            } else {
                entry.remove(b"AP");
            }
        }
    }
    if !refreshed.is_empty() {
        add_appearances(&mut document, &fields, &refreshed, cancel)?;
    }
    objects::save(&mut document, cancel)
}

fn add_appearances(
    document: &mut Document,
    fields: &[FormField],
    refreshed: &HashSet<ObjectId>,
    cancel: &Cancel,
) -> Result<(), Error> {
    let form = document
        .catalog()
        .map_err(|_| Error::SaveFailed)?
        .get(b"AcroForm")
        .map_err(|_| Error::SaveFailed)?;
    let (form_id, form) = document.dereference(form).map_err(|_| Error::SaveFailed)?;
    let generate_missing = value(
        document,
        form.as_dict().map_err(|_| Error::SaveFailed)?,
        b"NeedAppearances",
    )
    .is_some_and(|value| matches!(value, Object::Boolean(true)));
    let mut pages: BTreeMap<_, Vec<_>> = BTreeMap::new();
    for field in fields.iter().filter(|field| field.textual_appearance) {
        for widget in &field.widgets {
            if refreshed.contains(&field.id)
                || (generate_missing
                    && !document
                        .get_dictionary(widget.id)
                        .map_err(|_| Error::SaveFailed)?
                        .has(b"AP"))
            {
                pages
                    .entry(widget.location.page)
                    .or_default()
                    .push((widget, field.public.name.as_str()));
            }
        }
    }
    let scratch = Pdf::from_bytes(&objects::save(document, cancel)?)?;
    for (page, widgets) in pages {
        cancel.check()?;
        scratch.with_page(page, |native| {
            // SAFETY: scratch、页面及控件都在当前线程，页面保留控件。
            let (annotations, crop_origin) = unsafe {
                // 外观在未旋转的控件坐标系绘制，最终页仍保留原旋转。
                native.setRotation(0);
                (
                    native.annotations(),
                    native.boundsForBox(PDFDisplayBox::CropBox).origin,
                )
            };
            for (widget, name) in widgets {
                let rect = widget.location.rect;
                let mut matches = annotations.iter().filter(|item| {
                    // SAFETY: 本页已保留控件，读取同线程文档的标注属性。
                    unsafe {
                        if item.r#type().as_deref() != Some(ns_string!("Widget")) {
                            return false;
                        }
                        let bounds = item.bounds();
                        (bounds.origin.x - rect.x).abs() < 0.001
                            && (bounds.origin.y - rect.y).abs() < 0.001
                            && (bounds.size.width - rect.width).abs() < 0.001
                            && (bounds.size.height - rect.height).abs() < 0.001
                    }
                });
                let first = matches.next().ok_or(Error::SaveFailed)?;
                let item = if let Some(second) = matches.next() {
                    [first, second]
                        .into_iter()
                        .chain(matches)
                        .find(|item| {
                            // SAFETY: 候选项仍由 annotations 持有。
                            unsafe { item.fieldName() }
                                .is_some_and(|field_name| field_name.to_string() == name)
                        })
                        .ok_or(Error::SaveFailed)?
                } else {
                    first
                };
                if rect.width > 0.0 && rect.height > 0.0 {
                    let appearance = draw_appearance(document, &item, rect, crop_origin, cancel)?;
                    document
                        .get_dictionary_mut(widget.id)
                        .map_err(|_| Error::SaveFailed)?
                        .set("AP", dictionary! { "N" => appearance });
                }
            }
            Ok(())
        })?;
    }
    let form = if let Some(form_id) = form_id {
        document.get_dictionary_mut(form_id)
    } else {
        document
            .catalog_mut()
            .and_then(|catalog| catalog.get_mut(b"AcroForm"))
            .and_then(Object::as_dict_mut)
    }
    .map_err(|_| Error::SaveFailed)?;
    form.remove(b"NeedAppearances");
    Ok(())
}

/// 用 PDFKit 画控件现在的外观，做成控件局部坐标的 Form XObject。
fn draw_appearance(
    document: &mut Document,
    widget: &PDFAnnotation,
    rect: Rect,
    crop_origin: CGPoint,
    cancel: &Cancel,
) -> Result<ObjectId, Error> {
    let media = CGRect::new(CGPoint::ZERO, CGSize::new(rect.width, rect.height));
    objects::drawn_form(document, media, cancel, |context| {
        CGContext::translate_ctm(
            Some(context),
            crop_origin.x - rect.x,
            crop_origin.y - rect.y,
        );
        // SAFETY: 控件由当前线程的页面保留，只绘制外观，不执行动作。
        unsafe { widget.drawWithBox_inContext(PDFDisplayBox::CropBox, context) };
    })
}

pub(super) fn acro_form(document: &Document) -> Result<Option<&Dictionary>, Error> {
    let catalog = document.catalog().map_err(|_| Error::Damaged)?;
    let Some(form) = value(document, catalog, b"AcroForm") else {
        return Ok(None);
    };
    let form = form.as_dict().map_err(|_| Error::Damaged)?;
    if value(document, form, b"XFA").is_some() {
        return Err(Error::UnsupportedForm);
    }
    Ok(Some(form))
}

fn read_form(document: &Document, cancel: &Cancel) -> Result<Vec<FormField>, Error> {
    let Some(acro_form) = acro_form(document)? else {
        return Ok(Vec::new());
    };
    let Some(fields) = value(document, acro_form, b"Fields") else {
        return Ok(Vec::new());
    };
    let fields = fields.as_array().map_err(|_| Error::Damaged)?;
    let mut widget_pages = HashMap::new();
    for (page, id) in document.get_pages() {
        cancel.check()?;
        let dictionary = document.get_dictionary(id).map_err(|_| Error::Damaged)?;
        if let Some(annots) = array(document, dictionary, b"Annots") {
            for annotation in annots {
                let (annotation_id, _) = document
                    .dereference(annotation)
                    .map_err(|_| Error::Damaged)?;
                if let Some(annotation_id) = annotation_id {
                    widget_pages.insert(annotation_id, page);
                }
            }
        }
    }
    let mut collected = Vec::new();
    let mut pending: Vec<_> = fields
        .iter()
        .rev()
        .map(|field| (field, String::new(), Inherited::default()))
        .collect();
    let mut seen = HashSet::new();
    while let Some((entry, mut name, inherited)) = pending.pop() {
        cancel.check()?;
        let (id, entry) = document.dereference(entry).map_err(|_| Error::Damaged)?;
        let id = id.ok_or(Error::Damaged)?;
        if !seen.insert(id) {
            return Err(Error::Damaged);
        }
        let entry = entry.as_dict().map_err(|_| Error::Damaged)?;
        if let Some(part) = value(document, entry, b"T").and_then(form_string) {
            if !name.is_empty() {
                name.push('.');
            }
            name.push_str(&part);
        }
        let inherited = Inherited {
            field_type: value(document, entry, b"FT")
                .and_then(|kind| kind.as_name().ok())
                .unwrap_or(inherited.field_type),
            flags: value(document, entry, b"Ff")
                .and_then(|flags| flags.as_i64().ok())
                .unwrap_or(inherited.flags),
            value: value(document, entry, b"V").or(inherited.value),
            options: array(document, entry, b"Opt").unwrap_or(inherited.options),
            max_length: value(document, entry, b"MaxLen")
                .and_then(|value| value.as_i64().ok())
                .and_then(|length| usize::try_from(length).ok())
                .or(inherited.max_length),
        };
        let mut widgets = Vec::new();
        if let Some(widget) = read_widget(document, &widget_pages, id, entry) {
            widgets.push(widget);
        }
        if let Some(children) = array(document, entry, b"Kids") {
            let mut child_fields = Vec::new();
            for child in children {
                let (id, dictionary) = document.dereference(child).map_err(|_| Error::Damaged)?;
                let id = id.ok_or(Error::Damaged)?;
                let dictionary = dictionary.as_dict().map_err(|_| Error::Damaged)?;
                let own_field = dictionary.has(b"T") || dictionary.has(b"Kids");
                if !own_field
                    && let Some(widget) = read_widget(document, &widget_pages, id, dictionary)
                {
                    widgets.push(widget);
                } else if own_field || !is_widget(document, dictionary) {
                    child_fields.push((child, name.clone(), inherited));
                }
            }
            pending.extend(child_fields.into_iter().rev());
        }
        if name.is_empty() || widgets.is_empty() {
            continue;
        }
        let kind = match inherited.field_type {
            b"Tx" => FieldKind::Text,
            b"Btn" if inherited.flags & PUSH_BUTTON != 0 => FieldKind::Other,
            b"Btn" if inherited.flags & RADIO != 0 => FieldKind::Radio,
            b"Btn" => FieldKind::Checkbox,
            b"Ch" if inherited.flags & COMBO != 0 => FieldKind::Dropdown,
            _ => FieldKind::Other,
        };
        let radio_options = if matches!(kind, FieldKind::Radio) {
            widgets
                .iter()
                .enumerate()
                .flat_map(|(index, widget)| {
                    widget.on_states.iter().filter_map(move |state| {
                        let export = inherited
                            .options
                            .get(index)
                            .and_then(|item| document.dereference(item).ok())
                            .and_then(|(_, item)| form_string(item))
                            .or_else(|| name_string(state))?;
                        Some(RadioOption {
                            export,
                            state: state.clone(),
                            widget: widget.id,
                            selected: widget.active_state.as_ref() == Some(state),
                        })
                    })
                })
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        let field_value = match kind {
            FieldKind::Checkbox => inherited
                .value
                .and_then(form_string)
                .map(|value| FieldValue::Checked(value != "Off"))
                .or(Some(FieldValue::Checked(false))),
            FieldKind::Radio => inherited.value.and_then(|value| {
                let raw = value.as_name().or_else(|_| value.as_str()).ok()?;
                if raw == b"Off" {
                    return None;
                }
                let export = radio_options
                    .iter()
                    .filter(|option| option.state == raw)
                    .max_by_key(|option| option.selected)
                    .map(|option| option.export.clone())
                    .or_else(|| form_string(value))?;
                Some(FieldValue::Text(export))
            }),
            _ => inherited.value.and_then(form_string).map(FieldValue::Text),
        };
        let options = match kind {
            FieldKind::Dropdown => inherited
                .options
                .iter()
                .filter_map(|option| choice_option(document, option))
                .collect(),
            FieldKind::Radio => radio_options
                .iter()
                .map(|option| FieldOption {
                    value: option.export.clone(),
                    label: option.export.clone(),
                })
                .collect(),
            _ => Vec::new(),
        };
        collected.push(FormField {
            id,
            public: Field {
                name,
                kind,
                value: field_value,
                options,
                widgets: widgets.iter().map(|widget| widget.location).collect(),
                read_only: inherited.flags & READ_ONLY != 0,
            },
            widgets,
            editable_combo: inherited.flags & EDITABLE_COMBO != 0,
            radio_options,
            max_length: inherited.max_length,
            textual_appearance: matches!(inherited.field_type, b"Tx" | b"Ch"),
            radios_in_unison: inherited.flags & RADIOS_IN_UNISON != 0,
        });
    }
    cancel.check()?;
    Ok(collected)
}

fn form_string(value: &Object) -> Option<String> {
    match value {
        Object::Name(name) => name_string(name),
        _ => decode_text_string(value).ok(),
    }
}

fn name_string(name: &[u8]) -> Option<String> {
    String::from_utf8(name.to_vec())
        .ok()
        .or_else(|| decode_text_string(&Object::String(name.to_vec(), StringFormat::Literal)).ok())
}

fn choice_option(document: &Document, choice: &Object) -> Option<FieldOption> {
    let (_, choice) = document.dereference(choice).ok()?;
    if let Object::Array(values) = choice {
        let [export, label] = values.as_slice() else {
            return None;
        };
        Some(FieldOption {
            value: form_string(document.dereference(export).ok()?.1)?,
            label: form_string(document.dereference(label).ok()?.1)?,
        })
    } else {
        let text = form_string(choice)?;
        Some(FieldOption {
            value: text.clone(),
            label: text,
        })
    }
}

fn is_widget(document: &Document, entry: &Dictionary) -> bool {
    value(document, entry, b"Subtype").and_then(|kind| kind.as_name().ok()) == Some(b"Widget")
}

fn read_widget(
    document: &Document,
    widget_pages: &HashMap<ObjectId, u32>,
    id: ObjectId,
    entry: &Dictionary,
) -> Option<Widget> {
    if !is_widget(document, entry) {
        return None;
    }
    let page = *widget_pages.get(&id)?;
    let [x0, y0, x1, y1] = array(document, entry, b"Rect")? else {
        return None;
    };
    let coordinate = |value| {
        document
            .dereference(value)
            .ok()?
            .1
            .as_float()
            .ok()
            .map(f64::from)
    };
    let (x0, y0, x1, y1) = (
        coordinate(x0)?,
        coordinate(y0)?,
        coordinate(x1)?,
        coordinate(y1)?,
    );
    if ![x0, y0, x1, y1]
        .iter()
        .all(|coordinate| coordinate.is_finite())
    {
        return None;
    }
    let on_states = value(document, entry, b"AP")
        .and_then(|appearance| appearance.as_dict().ok())
        .and_then(|appearance| value(document, appearance, b"N"))
        .and_then(|normal| normal.as_dict().ok())
        .map(|normal| {
            normal
                .iter()
                .filter(|(state, _)| *state != b"Off")
                .map(|(state, _)| state.clone())
                .collect()
        })
        .unwrap_or_default();
    Some(Widget {
        id,
        location: FieldWidget {
            page,
            rect: Rect {
                x: x0.min(x1),
                y: y0.min(y1),
                width: (x1 - x0).abs(),
                height: (y1 - y0).abs(),
            },
        },
        on_states,
        active_state: value(document, entry, b"AS")
            .and_then(|value| value.as_name().ok())
            .map(<[u8]>::to_vec),
    })
}
