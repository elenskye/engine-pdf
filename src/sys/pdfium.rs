//! PDFium 只读：列页面对象、取字框、解图。改动由 `content` 写在原内容流上（`decisions/0019`）。

use std::collections::BTreeMap;
use std::sync::OnceLock;

use pdfium_render::prelude::*;

use crate::edit::{ObjectKind, ObjectOp, PageObject, Rect};
use crate::{Cancel, Error};

use super::content::{Kind, PageContent};
use super::{Pdf, check, objects, password};

static PDFIUM: OnceLock<Pdfium> = OnceLock::new();

pub(super) fn open(bytes: &[u8]) -> Result<PdfDocument<'_>, Error> {
    PDFIUM
        .get_or_init(Pdfium::default)
        .load_pdf_from_byte_slice(bytes, None)
        .map_err(|_| Error::Damaged)
}

pub(crate) fn page_objects(source: &Pdf, page: u32) -> Result<Vec<PageObject>, Error> {
    let total = source.page_count();
    if page == 0 || page > total {
        return Err(Error::PageOutOfRange { page, total });
    }
    // SAFETY: 在所属线程查询 PDFKit 文档的加密状态。
    let clear = if unsafe { source.native().isEncrypted() } {
        Some(password::plaintext(source)?)
    } else {
        None
    };
    let pdf = open(clear.as_deref().unwrap_or_else(|| source.original_bytes()))?;
    let number = page;
    let page = pdf
        .pages()
        .get((page - 1) as i32)
        .map_err(|_| Error::ObjectEditFailed { page: number })?;
    Ok(page
        .objects()
        .iter()
        .enumerate()
        .filter_map(|(index, object)| {
            let bounds = object.bounds().ok()?.to_rect();
            let kind = kind(&object);
            Some(PageObject {
                id: index as u32 + 1,
                kind,
                bounds: rect(bounds),
                text: (kind == ObjectKind::Text)
                    .then(|| object.as_text_object().map(PdfPageTextObject::text))
                    .flatten(),
            })
        })
        .collect())
}

/// PDFium 给这一页列出的对象：种类和范围，供 [`PageContent::matches`] 逐个对上指令。
pub(super) fn inventory(page: &PdfPage<'_>) -> Vec<(Option<Kind>, Rect)> {
    page.objects()
        .iter()
        .map(|object| {
            let kind = match object {
                PdfPageObject::Text(_) => Some(Kind::Text),
                PdfPageObject::Image(_) => Some(Kind::Image),
                PdfPageObject::Path(_) => Some(Kind::Path),
                PdfPageObject::XObjectForm(_) => Some(Kind::Form),
                PdfPageObject::Shading(_) => Some(Kind::Shading),
                PdfPageObject::Unsupported(_) => None,
            };
            let bounds = object.bounds().map_or(
                Rect {
                    x: 0.0,
                    y: 0.0,
                    width: 0.0,
                    height: 0.0,
                },
                |bounds| rect(bounds.to_rect()),
            );
            (kind, bounds)
        })
        .collect()
}

/// 按 `Document::objects` 的标识移动、删除主文档的文字和图片，其余指令原样保留。
pub(super) fn edit_objects(
    bytes: &[u8],
    ops: &[ObjectOp],
    total: u32,
    cancel: &Cancel,
) -> Result<Vec<u8>, Error> {
    let mut ops_by_page = BTreeMap::<u32, BTreeMap<u32, Vec<ObjectOp>>>::new();
    for &op in ops {
        let (page, id) = match op {
            ObjectOp::Move { page, id, dx, dy } => {
                if !(dx as f32).is_finite() || !(dy as f32).is_finite() {
                    return Err(Error::InvalidGeometry);
                }
                (page, id)
            }
            ObjectOp::Delete { page, id } => (page, id),
        };
        if page == 0 || page > total {
            return Err(Error::PageOutOfRange { page, total });
        }
        ops_by_page
            .entry(page)
            .or_default()
            .entry(id)
            .or_default()
            .push(op);
    }

    let pdf = open(bytes)?;
    if pdf.pages().len() as u32 != total {
        return Err(Error::Damaged);
    }
    let mut output = objects::load_plain(bytes)?;
    let ids = output.get_pages();
    let mut changes = Vec::new();
    for (&number, ops) in &ops_by_page {
        cancel.check()?;
        let failed = || Error::ObjectEditFailed { page: number };
        let page = pdf.pages().get((number - 1) as i32).map_err(|_| failed())?;
        let page_id = *ids.get(&number).ok_or_else(failed)?;
        let mut content = PageContent::parse(&output, page_id)
            .filter(|content| content.matches(&inventory(&page)))
            .ok_or_else(failed)?;
        let objects: Vec<_> = page.objects().iter().collect();
        let mut allowed = Vec::new();
        for (&id, ops) in ops {
            let missing = || Error::ObjectNotFound { page: number, id };
            let refused = |_| Error::ObjectNotEditable { page: number, id };
            let index = id.checked_sub(1).ok_or_else(missing)? as usize;
            let object = objects.get(index).ok_or_else(missing)?;
            if !matches!(kind(object), ObjectKind::Text | ObjectKind::Image) {
                return Err(Error::ObjectNotEditable { page: number, id });
            }
            let bounds = rect(object.bounds().map_err(|_| failed())?.to_rect());
            let (mut dx, mut dy) = (0.0, 0.0);
            for (position, op) in ops.iter().enumerate() {
                match *op {
                    ObjectOp::Move { dx: x, dy: y, .. } => {
                        content.translate(&output, index, x, y).map_err(refused)?;
                        (dx, dy) = (dx + x, dy + y);
                    }
                    ObjectOp::Delete { .. } => {
                        if position + 1 != ops.len() {
                            return Err(missing());
                        }
                        content.delete(&output, index).map_err(refused)?;
                    }
                }
            }
            allowed.push(bounds);
            allowed.push(Rect {
                x: bounds.x + dx,
                y: bounds.y + dy,
                ..bounds
            });
        }
        content.write(&mut output, page_id)?;
        changes.push((number, allowed));
    }
    cancel.check()?;
    output.prune_objects();
    let output = objects::save(&mut output, cancel)?;
    // 只许动过的对象那里变：PDFKit 画出来的别处像素和取出的字都要和原页一样。
    {
        let (before, after) = (Pdf::from_bytes(bytes)?, Pdf::from_bytes(&output)?);
        let after_objects = open(&output)?;
        for (number, allowed) in changes {
            cancel.check()?;
            if !check::unchanged_outside(
                (&before, &pdf),
                (&after, &after_objects),
                number,
                &allowed,
            )? {
                return Err(Error::ObjectEditFailed { page: number });
            }
        }
    }
    Ok(output)
}

pub(super) fn rect(bounds: PdfRect) -> Rect {
    Rect {
        x: bounds.left().value.into(),
        y: bounds.bottom().value.into(),
        width: bounds.width().value.into(),
        height: bounds.height().value.into(),
    }
}

pub(super) fn kind(object: &PdfPageObject<'_>) -> ObjectKind {
    match object {
        // 不可见 OCR 字形由 searchable 管理。
        PdfPageObject::Text(text) if text.render_mode() != PdfPageTextRenderMode::Invisible => {
            ObjectKind::Text
        }
        PdfPageObject::Image(_) => ObjectKind::Image,
        PdfPageObject::Path(_) => ObjectKind::Path,
        _ => ObjectKind::Other,
    }
}
