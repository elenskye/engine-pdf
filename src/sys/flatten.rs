use lopdf::{Dictionary, Document, Object, ObjectId, Stream};
use objc2_core_graphics::CGContext;
use objc2_pdf_kit::{PDFDisplayBox, PDFPage};

use crate::{Cancel, Error};

use super::objects::{self, inherited};
use super::{Pdf, forms::acro_form};

/// 烧平所选标注的外观，保留原页内容流及链接。
pub(super) fn flatten(bytes: &[u8], widgets_only: bool, cancel: &Cancel) -> Result<Vec<u8>, Error> {
    cancel.check()?;
    let mut document = objects::load_plain(bytes)?;
    let has_form = acro_form(&document)?.is_some();
    let pages = document.get_pages();
    let pdf = Pdf::from_bytes(bytes)?;
    if pages.len() != pdf.page_count() as usize {
        return Err(Error::SaveFailed);
    }

    let mut changed = has_form;
    for (number, id) in pages {
        cancel.check()?;
        let page = document.get_dictionary(id).map_err(|_| Error::SaveFailed)?;
        let Ok(annotations) = page.get(b"Annots") else {
            continue;
        };
        let (_, annotations) = document
            .dereference(annotations)
            .map_err(|_| Error::SaveFailed)?;
        let annotations = annotations.as_array().map_err(|_| Error::SaveFailed)?;
        let mut retained = Vec::new();
        for reference in annotations {
            let (_, annotation) = document
                .dereference(reference)
                .map_err(|_| Error::SaveFailed)?;
            let annotation = annotation.as_dict().map_err(|_| Error::SaveFailed)?;
            let subtype = annotation
                .get_deref(b"Subtype", &document)
                .and_then(Object::as_name)
                .ok();
            let keep = if widgets_only {
                subtype != Some(b"Widget")
            } else {
                subtype == Some(b"Link")
            };
            if keep {
                retained.push(reference.clone());
            }
        }
        if retained.len() == annotations.len() {
            continue;
        }
        changed = true;
        if let Some(appearance) = pdf.with_page(number, |native| {
            draw_appearance(native, widgets_only, &mut document, cancel)
        })? {
            append_appearance(&mut document, id, appearance)?;
        }
        let page = document
            .get_dictionary_mut(id)
            .map_err(|_| Error::SaveFailed)?;
        if retained.is_empty() {
            page.remove(b"Annots");
        } else {
            page.set("Annots", retained);
        }
    }

    if !changed {
        cancel.check()?;
        return Ok(bytes.to_vec());
    }
    document
        .catalog_mut()
        .map_err(|_| Error::SaveFailed)?
        .remove(b"AcroForm");
    document.prune_objects();
    objects::save(&mut document, cancel)
}

fn draw_appearance(
    page: &PDFPage,
    widgets_only: bool,
    document: &mut Document,
    cancel: &Cancel,
) -> Result<Option<ObjectId>, Error> {
    // SAFETY: 这是烧平期间私有打开的文档。去掉 PDFKit 绘制时附加的页面旋转，
    // 导入外观后原页仍保留原来的 Rotate，因此显示时只旋转一次。
    unsafe { page.setRotation(0) };
    // SAFETY: 页面及其标注均在所属文档的线程上读取。
    let annotations = unsafe { page.annotations() };
    let visible: Vec<_> = annotations
        .iter()
        .filter(|annotation| {
            // SAFETY: 数组保留当前线程的标注，只读取类型及显示标志。
            unsafe {
                let subtype = annotation.r#type().map(|value| value.to_string());
                let burn = if widgets_only {
                    subtype.as_deref() == Some("Widget")
                } else {
                    !matches!(subtype.as_deref(), Some("Link" | "Popup"))
                };
                burn && annotation.shouldDisplay()
            }
        })
        .collect();
    if visible.is_empty() {
        return Ok(None);
    }

    // SAFETY: 页面有效，位于当前线程。
    let bounds = unsafe { page.boundsForBox(PDFDisplayBox::MediaBox) };
    objects::drawn_form(document, bounds, cancel, |context| {
        CGContext::translate_ctm(Some(context), bounds.origin.x, bounds.origin.y);
        for annotation in visible {
            CGContext::save_g_state(Some(context));
            // SAFETY: 标注和 PDF context 都在当前线程。绘制外观，不调用标注或表单动作。
            unsafe { annotation.drawWithBox_inContext(PDFDisplayBox::MediaBox, context) };
            CGContext::restore_g_state(Some(context));
        }
    })
    .map(Some)
}

fn append_appearance(
    document: &mut Document,
    page_id: ObjectId,
    appearance: ObjectId,
) -> Result<(), Error> {
    let page = document
        .get_dictionary(page_id)
        .map_err(|_| Error::SaveFailed)?;
    let mut resources = match inherited(document, page, b"Resources") {
        Some(value) => value.as_dict().map_err(|_| Error::SaveFailed)?.clone(),
        None => Dictionary::new(),
    };
    let mut xobjects = match resources.get_deref(b"XObject", document) {
        Ok(value) => value.as_dict().map_err(|_| Error::SaveFailed)?.clone(),
        Err(_) => Dictionary::new(),
    };
    let mut name = format!("EnginePdfFlatten{}", appearance.0);
    while xobjects.has(name.as_bytes()) {
        name.push('_');
    }
    xobjects.set(name.as_str(), appearance);
    resources.set("XObject", xobjects);

    let mut contents = match page.get(b"Contents") {
        Ok(value) => match document
            .dereference(value)
            .map_err(|_| Error::SaveFailed)?
            .1
        {
            Object::Array(contents) => contents.clone(),
            Object::Null => Vec::new(),
            _ => vec![value.clone()],
        },
        Err(_) => Vec::new(),
    };
    // 把原内容的图形状态限定在 q/Q 中，外观从页面自身的坐标与状态开始绘制。
    let start = document.add_object(Stream::new(Dictionary::new(), b"q\n".to_vec()));
    let draw = format!("\nQ\nq\n/{name} Do\nQ\n").into_bytes();
    let draw = document.add_object(Stream::new(Dictionary::new(), draw));
    contents.insert(0, Object::Reference(start));
    contents.push(Object::Reference(draw));

    let page = document
        .get_dictionary_mut(page_id)
        .map_err(|_| Error::SaveFailed)?;
    page.set("Resources", resources);
    page.set("Contents", contents);
    Ok(())
}
