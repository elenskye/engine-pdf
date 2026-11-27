//! 涂黑：在最终页面上去掉框里的内容并盖上黑框，逐页验证后才交出（`decisions/0018`、`0019`）。
//!
//! 先在原内容流上只删框里的对象、遮掉图片里框内的像素（`content`）。删不干净、或页面别处跟着变了，
//! 就把整页画成图再抹黑，用 Vision 补回框外的搜索层。

use std::collections::{BTreeMap, HashSet};
use std::slice;

use lopdf::content::{Content, Operation};
use lopdf::{Document, Object, ObjectId, Stream, dictionary};
use objc2_core_graphics::{
    CGAffineTransformInvert, CGBitmapContextGetBytesPerRow, CGBitmapContextGetData,
    CGBitmapContextGetHeight, CGBitmapContextGetWidth, CGRectApplyAffineTransform,
};
use objc2_pdf_kit::{PDFDisplayBox, PDFPage};
use pdfium_render::prelude::*;

use crate::edit::{Pick, Rect, Redaction};
use crate::ocr::{self, Observation};
use crate::{Cancel, Error, ImageFormat, OcrLine, OcrOptions};

use super::check::{
    centered, contains, from_cg_rect, intersection, page_chars, pixel_area, right, to_cg_rect, top,
    unchanged_outside,
};
use super::content::PageContent;
use super::objects::{self, array, inherited, number, value};
use super::pdf::{OCR_TEXT_KEY, Pdf};
use super::tags::{self, Scope};
use super::{pdfium, render, searchable};

struct ImageUse {
    page: u32,
    bytes: Vec<u8>,
    coverage: ImageCoverage,
}

enum ImageCoverage {
    Full,
    Partial(Vec<Rect>),
    Unknown,
}

/// 涂黑时记下的东西：改完必须从文件里消失的旧对象、框里的图片、删掉的批注。
#[derive(Default)]
struct Traces {
    streams: HashSet<ObjectId>,
    images: Vec<ImageUse>,
    annotations: HashSet<ObjectId>,
}

pub(super) fn apply(
    bytes: &[u8],
    redactions: &[Redaction],
    picks: &[Pick],
    total: u32,
    cancel: &Cancel,
) -> Result<Vec<u8>, Error> {
    let pages = targets(redactions, picks, total)?;
    if pages.is_empty() {
        return Ok(bytes.to_vec());
    }
    cancel.check()?;
    let source = Pdf::from_bytes(bytes)?;
    let inventory = pdfium::open(bytes)?;
    let mut out = objects::load_plain(bytes)?;
    own_resources(&mut out, cancel)?;
    let ids = out.get_pages();

    let mut traces = Traces::default();
    let mut areas_by_page = BTreeMap::new();
    let mut erased = BTreeMap::new();
    let mut raster = Vec::new();
    for (&index, areas) in &pages {
        cancel.check()?;
        let failed = || Error::RedactionFailed { page: index };
        let id = *ids.get(&index).ok_or_else(failed)?;
        let areas = clip(&source, index, areas)?;
        // 旧内容流、OCR 文字、缩略图和私有数据都抄着原页，改完后它们必须从文件里消失。
        let private = [OCR_TEXT_KEY.to_bytes(), b"Thumb".as_slice(), b"PieceInfo"];
        let page = out.get_dictionary(id).map_err(|_| failed())?;
        for key in private.iter().copied().chain([b"Contents".as_slice()]) {
            if let Ok(value) = page.get(key) {
                record_streams(&out, value, &mut traces.streams);
            }
        }
        remove_annotations(&mut out, id, &areas, &mut traces, index)?;
        let page = out.get_dictionary_mut(id).map_err(|_| failed())?;
        for key in private {
            page.remove(key);
        }
        let objects = inventory
            .pages()
            .get((index - 1) as i32)
            .map_err(|_| failed())?;
        match erase(&mut out, id, &objects, index, &areas, &mut traces)? {
            Some(change) => {
                erased.insert(index, change);
            }
            None => raster.push(index),
        }
        areas_by_page.insert(index, areas);
    }

    // 原内容流上的改动只许动框里：别处跟着变了、或框里还取得到字，这一页改成整页位图。
    if !erased.is_empty() {
        let saved = objects::save(&mut out.clone(), cancel)?;
        let after = Pdf::from_bytes(&saved)?;
        let after_objects = pdfium::open(&saved)?;
        for (&index, allowed) in &erased {
            cancel.check()?;
            if !unchanged_outside(
                (&source, &inventory),
                (&after, &after_objects),
                index,
                allowed,
            )? || verify_page(&after, &after_objects, index, &areas_by_page[&index]).is_err()
            {
                raster.push(index);
            }
        }
    }
    for &index in &raster {
        cancel.check()?;
        let areas = &areas_by_page[&index];
        source.with_page(index, |page| {
            rasterize(&mut out, ids[&index], page, index, areas)
        })?;
    }
    let redacted: HashSet<_> = pages.keys().map(|index| ids[index]).collect();
    let cleared: HashSet<_> = raster.iter().map(|index| ids[index]).collect();
    tags::forget(
        &mut out,
        &Scope {
            pages: &redacted,
            cleared: &cleared,
            annotations: &traces.annotations,
        },
    );

    // 图像的软遮罩和备用图也可能保留原像素。
    for object in out.objects.values() {
        let Ok(stream) = object.as_stream() else {
            continue;
        };
        if traces
            .images
            .iter()
            .any(|image| image.bytes == stream.content)
            && value(&out, &stream.dict, b"Subtype").and_then(|v| v.as_name().ok())
                == Some(b"Image")
        {
            for key in [b"Mask".as_slice(), b"SMask", b"Alternates"] {
                if let Ok(reference) = stream.dict.get(key) {
                    record_streams(&out, reference, &mut traces.streams);
                }
            }
        }
    }
    out.prune_objects();
    check_shared_images(&out, &traces.images, cancel)?;
    if traces.streams.iter().any(|id| out.objects.contains_key(id)) {
        return Err(Error::RedactionFailed {
            page: *pages.keys().next().unwrap(),
        });
    }

    if !raster.is_empty() {
        restore_search_layer(&mut out, &ids, &raster, &areas_by_page, cancel)?;
    }
    let output = objects::save(&mut out, cancel)?;
    {
        let doc = Pdf::from_bytes(&output)?;
        let objects = pdfium::open(&output)?;
        for (&index, areas) in &areas_by_page {
            cancel.check()?;
            verify_page(&doc, &objects, index, areas)?;
        }
    }
    cancel.check()?;
    Ok(output)
}

/// 涂黑框按输出页归组：同一原始页的每个输出副本都要涂。
fn targets(
    redactions: &[Redaction],
    picks: &[Pick],
    total: u32,
) -> Result<BTreeMap<u32, Vec<Rect>>, Error> {
    let mut by_source = BTreeMap::<u32, Vec<Rect>>::new();
    for item in redactions {
        if item.page == 0 || item.page > total {
            return Err(Error::PageOutOfRange {
                page: item.page,
                total,
            });
        }
        for &area in &item.rects {
            if area.width <= 0.0
                || area.height <= 0.0
                || [area.x, area.y, right(area), top(area)]
                    .iter()
                    .any(|&v| !(v as f32).is_finite())
            {
                return Err(Error::InvalidGeometry);
            }
            by_source.entry(item.page).or_default().push(area);
        }
    }
    if let Some(&page) = by_source.keys().find(|&&page| {
        !picks
            .iter()
            .any(|pick| pick.source == 0 && pick.page == page)
    }) {
        return Err(Error::RedactionFailed { page });
    }
    Ok(picks
        .iter()
        .enumerate()
        .filter(|(_, pick)| pick.source == 0)
        .filter_map(|(index, pick)| Some((index as u32 + 1, by_source.get(&pick.page)?.clone())))
        .collect())
}

/// 涂黑框要落在 CropBox 里（PDF.js 换算可能带来极小的误差）。
fn clip(source: &Pdf, index: u32, areas: &[Rect]) -> Result<Vec<Rect>, Error> {
    source.with_page(index, |page| {
        // SAFETY: PDFKit 页面在所属线程，读取有效裁切框。
        let crop = from_cg_rect(unsafe { page.boundsForBox(PDFDisplayBox::CropBox) });
        areas
            .iter()
            .map(|area| {
                intersection(*area, crop)
                    .filter(|clipped| {
                        (clipped.width - area.width).abs() < 0.001
                            && (clipped.height - area.height).abs() < 0.001
                    })
                    .ok_or(Error::InvalidGeometry)
            })
            .collect()
    })
}

/// 把继承的资源具体化到页，免得涂黑页换掉资源后，页树上仍挂着旧图像。
fn own_resources(out: &mut Document, cancel: &Cancel) -> Result<(), Error> {
    for id in out.get_pages().into_values() {
        cancel.check()?;
        let page = out.get_dictionary(id).map_err(|_| Error::SaveFailed)?;
        if let Some(resources) = inherited(out, page, b"Resources")
            .filter(|_| !page.has(b"Resources"))
            .cloned()
        {
            out.get_dictionary_mut(id)
                .map_err(|_| Error::SaveFailed)?
                .set("Resources", resources);
        }
    }
    for object in out.objects.values_mut() {
        if object.type_name().ok() == Some(b"Pages") {
            object
                .as_dict_mut()
                .map_err(|_| Error::SaveFailed)?
                .remove(b"Resources");
        }
    }
    Ok(())
}

/// 在原内容流上删掉框里的对象、遮掉图片里框内的像素，再盖黑框。返回画面会变的范围；
/// 做不到（对象清单对不上、字压着框边、复杂对象）时返回 `None`，这一页改走整页位图。
fn erase(
    out: &mut Document,
    id: ObjectId,
    page: &PdfPage<'_>,
    index: u32,
    areas: &[Rect],
    traces: &mut Traces,
) -> Result<Option<Vec<Rect>>, Error> {
    let failed = || Error::RedactionFailed { page: index };
    let mut allowed = areas.to_vec();
    let mut content =
        PageContent::parse(out, id).filter(|content| content.matches(&pdfium::inventory(page)));
    let text = page.text().ok();
    for (position, object) in page.objects().iter().enumerate() {
        let bounds = pdfium::rect(object.bounds().map_err(|_| failed())?.to_rect());
        if !areas
            .iter()
            .any(|area| intersection(*area, bounds).is_some())
        {
            continue;
        }
        let inside = areas.iter().any(|area| contains(*area, bounds));
        // 不论走哪条路都记下框里的图，最后查文件里别处有没有留着原图。
        match &object {
            PdfPageObject::Image(image) => traces.images.push(ImageUse {
                page: index,
                bytes: image.get_raw_image_data().map_err(|_| failed())?,
                coverage: coverage(image, bounds, areas, inside),
            }),
            PdfPageObject::XObjectForm(form) => {
                record_form_images(form, index, &mut traces.images).map_err(|_| failed())?;
            }
            _ => {}
        }
        let Some(edits) = content.as_mut() else {
            continue;
        };
        let done = match &object {
            PdfPageObject::Text(_) if inside => edits.delete(out, position).is_ok(),
            // 字框沾到涂黑框，但没有一个字形落进框里：不用动。
            PdfPageObject::Text(text_object) => text
                .as_ref()
                .is_some_and(|text| clear_of(text, text_object, areas)),
            PdfPageObject::Path(_) | PdfPageObject::Image(_) if inside => {
                edits.delete(out, position).is_ok()
            }
            PdfPageObject::Path(_) => edits.path_clear_of(position, areas),
            PdfPageObject::Image(image) => match masked(out, edits, position, image, bounds, areas)
            {
                Some((masked, changed)) => {
                    edits.replace_image(position, masked);
                    allowed.extend(changed);
                    true
                }
                None => false,
            },
            _ => false,
        };
        if done {
            edits.strip_alternates(out, position);
        } else {
            content = None;
        }
    }
    let Some(mut content) = content else {
        return Ok(None);
    };
    content.cover(areas);
    content.write(out, id)?;
    Ok(Some(allowed))
}

fn coverage(
    image: &PdfPageImageObject<'_>,
    bounds: Rect,
    areas: &[Rect],
    inside: bool,
) -> ImageCoverage {
    if inside {
        return ImageCoverage::Full;
    }
    if !image.matrix().is_ok_and(|matrix| {
        matrix.a() > 0.0 && matrix.d() > 0.0 && matrix.b() == 0.0 && matrix.c() == 0.0
    }) {
        return ImageCoverage::Unknown;
    }
    ImageCoverage::Partial(
        areas
            .iter()
            .filter_map(|area| intersection(*area, bounds))
            .map(|area| Rect {
                x: (area.x - bounds.x) / bounds.width,
                y: (area.y - bounds.y) / bounds.height,
                width: area.width / bounds.width,
                height: area.height / bounds.height,
            })
            .collect(),
    )
}

fn clear_of(text: &PdfPageText<'_>, object: &PdfPageTextObject<'_>, areas: &[Rect]) -> bool {
    text.chars_for_object(object).is_ok_and(|chars| {
        chars.iter().all(|ch| {
            blank(&ch)
                || ch.tight_bounds().is_ok_and(|bounds| {
                    let bounds = pdfium::rect(bounds);
                    !areas
                        .iter()
                        .any(|area| intersection(*area, bounds).is_some())
                })
        })
    })
}

/// 框里的像素抹黑后另存一张图（RGB），交回新图的对象号和抹黑的范围（按整像素，再放宽一个像素给
/// 阅读器的插值）。带透明、遮罩或斜放的图不处理。
fn masked(
    out: &mut Document,
    content: &PageContent,
    position: usize,
    image: &PdfPageImageObject<'_>,
    bounds: Rect,
    areas: &[Rect],
) -> Option<(ObjectId, Vec<Rect>)> {
    let matrix = image.matrix().ok()?;
    if matrix.a() <= 0.0
        || matrix.d() <= 0.0
        || matrix.b() != 0.0
        || matrix.c() != 0.0
        || image.has_transparency()
    {
        return None;
    }
    let dict = content.image(out, position)?;
    if [b"ImageMask".as_slice(), b"Mask", b"SMask", b"SMaskInData"]
        .iter()
        .any(|key| dict.has(key))
    {
        return None;
    }
    let bitmap = image.get_raw_bitmap().ok()?;
    let (width, height) = (bitmap.width() as usize, bitmap.height() as usize);
    if width == 0 || height == 0 {
        return None;
    }
    let rgba = bitmap.as_rgba_bytes();
    if rgba.len() != width * height * 4 {
        return None;
    }
    let mut pixels: Vec<u8> = rgba
        .chunks_exact(4)
        .flat_map(|pixel| [pixel[0], pixel[1], pixel[2]])
        .collect();
    let (cell_x, cell_y) = (bounds.width / width as f64, bounds.height / height as f64);
    let mut changed = Vec::new();
    for area in areas.iter().filter_map(|area| intersection(*area, bounds)) {
        let [left, row, right, end] = pixel_area(area, bounds, width, height, true);
        for y in row..end {
            pixels[(y * width + left) * 3..(y * width + right) * 3].fill(0);
        }
        changed.push(Rect {
            x: bounds.x + (left as f64 - 1.0) * cell_x,
            y: top(bounds) - (end as f64 + 1.0) * cell_y,
            width: (right - left + 2) as f64 * cell_x,
            height: (end - row + 2) as f64 * cell_y,
        });
    }
    let mut stream = Stream::new(
        dictionary! {
            "Type" => "XObject", "Subtype" => "Image",
            "Width" => width as i64, "Height" => height as i64,
            "ColorSpace" => "DeviceRGB", "BitsPerComponent" => 8,
        },
        pixels,
    );
    stream.compress().ok()?;
    Some((out.add_object(stream), changed))
}

/// 同一张图的副本不能各遮一块（拼起来就还原了），原图也不能在别处还被引用。
fn check_shared_images(out: &Document, images: &[ImageUse], cancel: &Cancel) -> Result<(), Error> {
    for (index, image) in images.iter().enumerate() {
        cancel.check()?;
        let conflict = images[index + 1..].iter().any(|other| {
            other.bytes == image.bytes
                && match (&image.coverage, &other.coverage) {
                    (ImageCoverage::Full, _) | (_, ImageCoverage::Full) => false,
                    (ImageCoverage::Partial(a), ImageCoverage::Partial(b)) => a != b,
                    _ => true,
                }
        });
        let still_used = out.objects.values().any(|object| {
            object.as_stream().ok().is_some_and(|stream| {
                value(out, &stream.dict, b"Subtype").and_then(|v| v.as_name().ok())
                    == Some(b"Image")
                    && stream.content == image.bytes
            })
        });
        if conflict || still_used {
            return Err(Error::RedactionSharedImage { page: image.page });
        }
    }
    Ok(())
}

fn record_streams(doc: &Document, object: &Object, previous: &mut HashSet<ObjectId>) {
    if let Ok((id, object)) = doc.dereference(object) {
        if let Some(id) = id {
            if !previous.insert(id) {
                return;
            }
        }
        if let Ok(items) = object.as_array() {
            for item in items {
                record_streams(doc, item, previous);
            }
        }
    }
}

fn record_form_images(
    form: &PdfPageXObjectFormObject<'_>,
    page: u32,
    images: &mut Vec<ImageUse>,
) -> Result<(), PdfiumError> {
    for object in form.iter() {
        match &object {
            PdfPageObject::Image(image) => images.push(ImageUse {
                page,
                bytes: image.get_raw_image_data()?,
                coverage: ImageCoverage::Unknown,
            }),
            PdfPageObject::XObjectForm(form) => record_form_images(form, page, images)?,
            _ => {}
        }
    }
    Ok(())
}

fn remove_annotations(
    document: &mut Document,
    page: ObjectId,
    areas: &[Rect],
    traces: &mut Traces,
    number_of_page: u32,
) -> Result<(), Error> {
    let failed = || Error::RedactionFailed {
        page: number_of_page,
    };
    let annotations = array(
        document,
        document.get_dictionary(page).map_err(|_| failed())?,
        b"Annots",
    )
    .unwrap_or_default()
    .to_vec();
    let mut removals = vec![false; annotations.len()];
    for (index, annotation) in annotations.iter().enumerate() {
        let (_, resolved) = document.dereference(annotation).map_err(|_| failed())?;
        let dict = resolved.as_dict().map_err(|_| failed())?;
        let Some([x, y, right, top]) = array(document, dict, b"Rect") else {
            return Err(failed());
        };
        let (Some(x), Some(y), Some(right), Some(top)) = (
            number(document, x),
            number(document, y),
            number(document, right),
            number(document, top),
        ) else {
            return Err(failed());
        };
        let bounds = Rect {
            x: x.min(right).into(),
            y: y.min(top).into(),
            width: (right - x).abs().into(),
            height: (top - y).abs().into(),
        };
        if areas
            .iter()
            .any(|area| intersection(*area, bounds).is_some())
        {
            if value(document, dict, b"Subtype").and_then(|object| object.as_name().ok())
                == Some(b"Widget")
            {
                return Err(failed());
            }
            removals[index] = true;
        }
    }
    // Popup、父批注和回复可在框外，按关联一起移除，防止内容从另一侧恢复。
    loop {
        let mut changed = false;
        for (index, annotation) in annotations.iter().enumerate() {
            let (_, resolved) = document.dereference(annotation).map_err(|_| failed())?;
            let dict = resolved.as_dict().map_err(|_| failed())?;
            for key in [b"Popup".as_slice(), b"Parent", b"IRT"] {
                let Ok(link) = dict.get(key) else { continue };
                if let Some(other) = annotations.iter().position(|item| item == link) {
                    if removals[index] != removals[other] {
                        removals[index] = true;
                        removals[other] = true;
                        changed = true;
                    }
                }
            }
        }
        if !changed {
            break;
        }
    }
    let mut kept = Vec::new();
    for (annotation, remove) in annotations.into_iter().zip(removals) {
        if remove {
            record_streams(document, &annotation, &mut traces.streams);
            if let Ok(id) = annotation.as_reference() {
                traces.annotations.insert(id);
            }
            if let Some(appearance) = document
                .dereference(&annotation)
                .ok()
                .and_then(|(_, value)| value.as_dict().ok())
                .and_then(|annotation| annotation.get(b"AP").ok())
            {
                record_appearance_images(
                    document,
                    appearance,
                    &mut HashSet::new(),
                    &mut traces.images,
                    number_of_page,
                )?;
            }
        } else {
            kept.push(annotation);
        }
    }
    let page = document.get_dictionary_mut(page).map_err(|_| failed())?;
    if kept.is_empty() {
        page.remove(b"Annots");
    } else {
        page.set("Annots", kept);
    }
    Ok(())
}

fn record_appearance_images(
    document: &Document,
    object: &Object,
    seen: &mut HashSet<ObjectId>,
    images: &mut Vec<ImageUse>,
    page: u32,
) -> Result<(), Error> {
    let (id, object) = document
        .dereference(object)
        .map_err(|_| Error::RedactionFailed { page })?;
    if id.is_some_and(|id| !seen.insert(id)) {
        return Ok(());
    }
    if let Ok(stream) = object.as_stream() {
        if value(document, &stream.dict, b"Subtype").and_then(|o| o.as_name().ok())
            == Some(b"Image")
        {
            images.push(ImageUse {
                page,
                bytes: stream.content.clone(),
                coverage: ImageCoverage::Full,
            });
        } else if let Some(xobjects) = value(document, &stream.dict, b"Resources")
            .and_then(|o| o.as_dict().ok())
            .and_then(|resources| value(document, resources, b"XObject"))
        {
            record_appearance_images(document, xobjects, seen, images, page)?;
        }
    } else if let Ok(dictionary) = object.as_dict() {
        for (_, item) in dictionary.iter() {
            record_appearance_images(document, item, seen, images, page)?;
        }
    }
    Ok(())
}

/// 整页画成图（不画批注），抹黑框里的像素，替换这一页的内容和资源。
fn rasterize(
    output: &mut Document,
    id: ObjectId,
    page: &PDFPage,
    index: u32,
    areas: &[Rect],
) -> Result<(), Error> {
    let failed = || Error::RedactionFailed { page: index };
    // SAFETY: 页面只在所属线程临时隐藏批注。框外的原批注保留为独立对象。
    let (bitmap, display, transform) = unsafe {
        let annotations = page.displaysAnnotations();
        page.setDisplaysAnnotations(false);
        let result = render::draw_page(page, index, ImageFormat::Png, 4096);
        page.setDisplaysAnnotations(annotations);
        let (bitmap, size) = result?;
        (bitmap, size, page.transformForBox(PDFDisplayBox::CropBox))
    };
    let width = CGBitmapContextGetWidth(Some(&bitmap));
    let height = CGBitmapContextGetHeight(Some(&bitmap));
    let stride = CGBitmapContextGetBytesPerRow(Some(&bitmap));
    let data = CGBitmapContextGetData(Some(&bitmap)).cast::<u8>();
    if data.is_null() || stride < width * 4 {
        return Err(failed());
    }
    // SAFETY: 位图由 draw_page 分配为 RGBX，读取窗口受步长和高度约束。
    let bitmap_bytes = unsafe { slice::from_raw_parts(data, stride * height) };
    let mut pixels: Vec<_> = bitmap_bytes
        .chunks_exact(stride)
        .flat_map(|row| {
            row[..width * 4]
                .chunks_exact(4)
                .flat_map(|pixel| [pixel[0], pixel[1], pixel[2]])
        })
        .collect();
    let bounds = Rect {
        x: 0.0,
        y: 0.0,
        width: display.width,
        height: display.height,
    };
    for area in areas {
        let area = from_cg_rect(CGRectApplyAffineTransform(to_cg_rect(*area), transform));
        let [left, row, right, end] = pixel_area(area, bounds, width, height, true);
        for y in row..end {
            let offset = y * width * 3;
            pixels[offset + left * 3..offset + right * 3].fill(0);
        }
    }
    // 只有黑白灰的页（大多数论文）存成灰度图，文件小三分之二。
    let gray = pixels
        .chunks_exact(3)
        .all(|pixel| pixel[0] == pixel[1] && pixel[1] == pixel[2]);
    let (space, pixels) = if gray {
        ("DeviceGray", pixels.into_iter().step_by(3).collect())
    } else {
        ("DeviceRGB", pixels)
    };
    let mut image = Stream::new(
        dictionary! {
            "Type" => "XObject", "Subtype" => "Image",
            "Width" => width as i64, "Height" => height as i64,
            "ColorSpace" => space, "BitsPerComponent" => 8,
        },
        pixels,
    );
    image.compress().map_err(|_| failed())?;
    let image = output.add_object(image);
    let inverse = CGAffineTransformInvert(transform);
    let matrix = [
        inverse.a * display.width,
        inverse.b * display.width,
        inverse.c * display.height,
        inverse.d * display.height,
        inverse.tx,
        inverse.ty,
    ];
    let mut ops = vec![
        Operation::new("q", vec![]),
        Operation::new("cm", matrix.map(|value| Object::Real(value as f32)).into()),
        Operation::new("Do", vec!["RedactedPage".into()]),
        Operation::new("Q", vec![]),
    ];
    // 位图已经抹掉原像素。矢量覆盖使阅读器缩放重采样时框边仍保持全黑。
    ops.push(Operation::new("g", vec![0.into()]));
    for area in areas {
        let rect =
            [area.x, area.y, area.width, area.height].map(|value| Object::Real(value as f32));
        ops.push(Operation::new("re", rect.into()));
    }
    ops.push(Operation::new("f", vec![]));
    let content = Content { operations: ops }.encode().map_err(|_| failed())?;
    let mut content = Stream::new(dictionary! {}, content);
    content.compress().map_err(|_| failed())?;
    let content = output.add_object(content);
    let page = output.get_dictionary_mut(id).map_err(|_| failed())?;
    page.set("Contents", content);
    page.set(
        "Resources",
        dictionary! { "XObject" => dictionary! { "RedactedPage" => image } },
    );
    Ok(())
}

/// 整页位图上的字只剩画面：用 Vision 认出框外的行，补回不可见的搜索层。
fn restore_search_layer(
    out: &mut Document,
    ids: &BTreeMap<u32, ObjectId>,
    raster: &[u32],
    areas_by_page: &BTreeMap<u32, Vec<Rect>>,
    cancel: &Cancel,
) -> Result<(), Error> {
    let rendered = Pdf::from_bytes(&objects::save(out, cancel)?)?;
    let font = searchable::add_font_file(out);
    for &index in raster {
        cancel.check()?;
        let areas = &areas_by_page[&index];
        let basis = rendered.with_page(index, |page| {
            // SAFETY: 与 OCR 画图使用同一线程上的原生页和 CropBox。
            let inverse =
                CGAffineTransformInvert(unsafe { page.transformForBox(PDFDisplayBox::CropBox) });
            Ok([inverse.a, inverse.b, inverse.c, inverse.d])
        })?;
        let upright = basis[0] > 0.5 && basis[3] > 0.5;
        let (result, spans) = ocr::assemble(
            rendered
                .ocr(index, &OcrOptions::default(), true)?
                .into_iter()
                .flat_map(|line| {
                    areas.iter().fold(vec![line], |pieces, area| {
                        pieces
                            .into_iter()
                            .flat_map(|piece| outside(piece, *area, upright))
                            .collect()
                    })
                })
                .collect(),
        );
        if result.lines.is_empty() {
            continue;
        }
        searchable::add_layer(out, ids[&index], font, &result, &spans, basis)?;
    }
    out.prune_objects();
    Ok(())
}

/// 整页图上框里已经全黑，认出的字都在框外，但行框可能沾到涂黑框。只沾到上下边的行把行框收回框外；
/// 横穿涂黑框的行按字切开，只留框外的字。转过的页不切，沾到框的行整行不要。
fn outside((mut line, visual, spans): Observation, area: Rect, upright: bool) -> Vec<Observation> {
    let b = Rect {
        x: line.bounds.x,
        y: line.bounds.y,
        width: line.bounds.width,
        height: line.bounds.height,
    };
    let Some(overlap) = intersection(area, b) else {
        return vec![(line, visual, spans)];
    };
    if !upright {
        return Vec::new();
    }
    if overlap.height < b.height / 2.0 {
        if area.y > b.y + b.height / 2.0 {
            line.bounds.height = area.y - b.y;
        } else {
            line.bounds.y = top(area);
            line.bounds.height = top(b) - top(area);
        }
        return vec![(line, visual, spans)];
    }
    let chars: Vec<char> = line.text.chars().collect();
    if spans.len() != chars.len() {
        return Vec::new();
    }
    let kept = |i: usize| {
        let (left, right_edge) = spans[i];
        !chars[i].is_whitespace()
            && (b.x + right_edge * b.width <= area.x || b.x + left * b.width >= right(area))
    };
    let mut pieces = Vec::new();
    let mut start = 0;
    while start < chars.len() {
        if !kept(start) {
            start += 1;
            continue;
        }
        // 一段连续留下的字（中间的空格跟着留下）。
        let mut end = start + 1;
        while end < chars.len() && (kept(end) || chars[end].is_whitespace()) {
            end += 1;
        }
        while chars[end - 1].is_whitespace() {
            end -= 1;
        }
        let (left, right_edge) = (spans[start].0, spans[end - 1].1);
        let share = (right_edge - left).max(f64::EPSILON);
        let mut piece = OcrLine {
            text: chars[start..end].iter().collect(),
            confidence: line.confidence,
            bounds: line.bounds,
        };
        piece.bounds.x = b.x + left * b.width;
        piece.bounds.width = share * b.width;
        let mut seen = visual;
        seen.x += left * visual.width;
        seen.width = share * visual.width;
        let spans = spans[start..end]
            .iter()
            .map(|&(l, r)| ((l - left) / share, (r - left) / share))
            .collect();
        pieces.push((piece, seen, spans));
        start = end;
    }
    pieces
}

fn blank(ch: &PdfPageTextChar<'_>) -> bool {
    ch.unicode_char().is_none_or(char::is_whitespace)
}

/// 框里取不到字（PDFium 按字形、PDFKit 按字的中心），页图上框里全黑。
fn verify_page(
    doc: &Pdf,
    objects: &PdfDocument<'_>,
    index: u32,
    areas: &[Rect],
) -> Result<(), Error> {
    let failed = || Error::RedactionFailed { page: index };
    // PDFium 的字形框（不看空白：它在字与字之间补的空格框可能贴着涂黑框）。
    for (_, bounds) in page_chars(objects, index)? {
        if areas
            .iter()
            .any(|area| intersection(*area, bounds).is_some())
        {
            return Err(failed());
        }
    }
    // PDFKit 的字框（含空格）中心不许落在框里：这是在「预览」里框选、复制时取到的字。
    doc.with_page(index, |page| {
        // SAFETY: 在所属线程读取 PDFKit 的字框。
        let count = unsafe { page.numberOfCharacters() };
        for i in 0..count {
            let b = from_cg_rect(unsafe { page.characterBoundsAtIndex(i as isize) });
            if b.width > 0.0 && b.height > 0.0 && areas.iter().any(|a| centered(*a, b)) {
                return Err(failed());
            }
        }
        Ok(())
    })?;
    let (draw, transform, bounds) = super::check::content_pixels(doc, index)?;
    let (width, height) = (draw.width as usize, draw.height as usize);
    for area in areas {
        let area = from_cg_rect(CGRectApplyAffineTransform(to_cg_rect(*area), transform));
        let [left, row, right, end] = pixel_area(area, bounds, width, height, false);
        // 比一个像素还窄的框没有完整落在框里的像素可查。
        if (row..end).any(|y| {
            left < right
                && draw.bytes[y * width + left..y * width + right]
                    .iter()
                    .any(|&v| v != 0)
        }) {
            return Err(failed());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::OcrBounds;

    fn line(text: &str, x: f64, y: f64, width: f64) -> Observation {
        let count = text.chars().count() as f64;
        let bounds = OcrBounds {
            x,
            y,
            width,
            height: 10.0,
        };
        let spans = (0..text.chars().count())
            .map(|i| (i as f64 / count, (i + 1) as f64 / count))
            .collect();
        let line = OcrLine {
            text: text.into(),
            confidence: 1.0,
            bounds,
        };
        (line, bounds, spans)
    }

    #[test]
    fn keeps_recognized_words_outside_the_box() {
        let area = Rect {
            x: 40.0,
            y: 100.0,
            width: 30.0,
            height: 10.0,
        };
        // 一个字 10pt 宽：横穿涂黑框的行切成框左、框右两段。
        let pieces = outside(line("abcd efgh", 0.0, 100.0, 90.0), area, true);
        let texts: Vec<_> = pieces.iter().map(|(l, _, _)| l.text.as_str()).collect();
        assert_eq!(texts, ["abcd", "gh"]);
        assert_eq!(
            (pieces[1].0.bounds.x, pieces[1].0.bounds.width),
            (70.0, 20.0)
        );
        let spans: Vec<_> = pieces[1]
            .2
            .iter()
            .map(|&(l, r)| ((l * 100.0).round(), (r * 100.0).round()))
            .collect();
        assert_eq!(spans, [(0.0, 50.0), (50.0, 100.0)]);
        // 只沾到下边的行：收到框上面。
        let above = outside(line("above", 0.0, 108.0, 50.0), area, true);
        assert_eq!(
            (above[0].0.bounds.y, above[0].0.bounds.height),
            (110.0, 8.0)
        );
        // 转过的页：沾到框就整行不要。
        assert!(outside(line("abcd", 0.0, 100.0, 90.0), area, false).is_empty());
    }
}
