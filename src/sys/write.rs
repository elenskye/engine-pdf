//! 写出改动单（`decisions/0017`）：lopdf 从原文件的对象出发，只改动到的部分。
//!
//! - 页序不变（可以带旋转）：输出就是原文件，内容流、字体、页标签、结构树、表单、链接都原样保留。
//! - 调整页序：把所选页对象搬进新文件，内容流和资源不重写；页内链接、目录按新页序重映射，
//!   页标签按原页接上，只来自一份文件时连结构树（决定取字顺序）一起搬。
//! - 新标注的外观由 PDFKit 在同样几何的空白页上画好，再搬到输出页上。

use std::collections::{HashMap, HashSet};

use lopdf::{Dictionary, Document, Object, ObjectId, StringFormat, dictionary, text_string};
use objc2::rc::autoreleasepool;
use objc2_foundation::NSString;

use crate::edit::{
    InfoPatch, Mark, MarkId, MarkKind, OutlineItem, Pick, Plan, Point, Rect, Shape, Target,
};
use crate::{Cancel, Error};

use super::objects::{self, Importer, array, inherited, media_origin, number, value};
use super::{Pdf, annotation, stamp};

/// 页面上可编辑标注的顺序与 `MarkId::index` 一致：跳过弹窗和表单控件。
fn editable(document: &Document, annotation: &Object) -> bool {
    document
        .dereference(annotation)
        .ok()
        .and_then(|(_, annotation)| annotation.as_dict().ok())
        .and_then(|annotation| value(document, annotation, b"Subtype"))
        .and_then(|subtype| subtype.as_name().ok())
        .is_none_or(|subtype| subtype != b"Popup" && subtype != b"Widget")
}

pub(super) fn write(
    sources: &[&Pdf],
    picks: &[Pick],
    in_place: bool,
    plan: &Plan,
    cancel: &Cancel,
) -> Result<Vec<u8>, Error> {
    let mut used = vec![false; sources.len()];
    used[0] = true;
    for source in picks
        .iter()
        .map(|pick| pick.source)
        .chain(plan.remove.iter().map(|id| id.source))
    {
        *used
            .get_mut(source as usize)
            .ok_or(Error::InvalidSource { source })? = true;
    }
    let mut documents = Vec::with_capacity(sources.len());
    for (source, used) in sources.iter().zip(used) {
        cancel.check()?;
        documents.push(if used { Some(source.objects()?) } else { None });
    }
    remove_marks(sources, &mut documents, &plan.remove, cancel)?;

    let (mut output, pages) = if in_place {
        let mut output = documents[0].take().ok_or(Error::SaveFailed)?;
        let pages: Vec<_> = output.get_pages().into_values().collect();
        for (pick, &page) in picks.iter().zip(&pages) {
            if pick.rotate.rem_euclid(360) != 0 {
                rotate(&mut output, page, pick.rotate)?;
            }
        }
        (output, pages)
    } else {
        compose(sources, &documents, picks, plan.outline.is_none(), cancel)?
    };
    let mut positions: HashMap<_, Vec<_>> = HashMap::new();
    for (index, pick) in picks.iter().enumerate() {
        positions
            .entry((pick.source, pick.page))
            .or_default()
            .push(index);
    }
    add_marks(&mut output, &pages, sources, &positions, &plan.add, cancel)?;
    if let Some(items) = &plan.outline {
        write_outline(&mut output, items, &pages)?;
    }
    if let Some(patch) = &plan.info {
        patch_info(&mut output, patch)?;
    }
    output.prune_objects();
    objects::save(&mut output, cancel)
}

fn rotate(document: &mut Document, page: ObjectId, by: i32) -> Result<(), Error> {
    let dictionary = document
        .get_dictionary(page)
        .map_err(|_| Error::SaveFailed)?;
    let rotation = inherited(document, dictionary, b"Rotate")
        .and_then(|value| value.as_i64().ok())
        .unwrap_or(0);
    document
        .get_dictionary_mut(page)
        .map_err(|_| Error::SaveFailed)?
        .set("Rotate", (rotation + i64::from(by)).rem_euclid(360));
    Ok(())
}

fn page_annotations(document: &Document, page: ObjectId) -> Vec<Object> {
    document
        .get_dictionary(page)
        .ok()
        .and_then(|page| array(document, page, b"Annots"))
        .map(<[Object]>::to_vec)
        .unwrap_or_default()
}

fn set_annotations(
    document: &mut Document,
    page: ObjectId,
    annotations: Vec<Object>,
) -> Result<(), Error> {
    let page = document
        .get_dictionary_mut(page)
        .map_err(|_| Error::SaveFailed)?;
    if annotations.is_empty() {
        page.remove(b"Annots");
    } else {
        page.set("Annots", annotations);
    }
    Ok(())
}

/// 在各来源的对象上删掉指定的已有标注，连同它的弹窗。
fn remove_marks(
    sources: &[&Pdf],
    documents: &mut [Option<Document>],
    ids: &[MarkId],
    cancel: &Cancel,
) -> Result<(), Error> {
    let mut grouped: HashMap<_, HashSet<_>> = HashMap::new();
    for id in ids {
        grouped
            .entry((id.source, id.page))
            .or_default()
            .insert(id.index);
    }
    for ((source, page), indices) in grouped {
        cancel.check()?;
        let pdf = sources
            .get(source as usize)
            .ok_or(Error::InvalidSource { source })?;
        let expected = annotation::editable_types(pdf, page)?;
        let document = documents[source as usize]
            .as_mut()
            .ok_or(Error::SaveFailed)?;
        let page_id = *document
            .get_pages()
            .get(&page)
            .ok_or(Error::PageOutOfRange {
                page,
                total: pdf.page_count(),
            })?;
        let annotations = page_annotations(document, page_id);
        let positions: Vec<_> = (0..annotations.len())
            .filter(|&position| editable(document, &annotations[position]))
            .collect();
        // lopdf 和 PDFKit 数出的种类顺序要一致，才能确定删的是哪一个。
        let found: Vec<_> = positions
            .iter()
            .map(|&position| {
                document
                    .dereference(&annotations[position])
                    .ok()
                    .and_then(|(_, entry)| entry.as_dict().ok())
                    .and_then(|entry| value(document, entry, b"Subtype"))
                    .and_then(|subtype| subtype.as_name().ok())
                    .map(|subtype| String::from_utf8_lossy(subtype).into_owned())
                    .unwrap_or_default()
            })
            .collect();
        if found != expected
            || indices
                .iter()
                .any(|&index| index as usize >= positions.len())
        {
            return Err(Error::AnnotationNotFound);
        }
        let dropped: HashSet<_> = indices
            .iter()
            .map(|&index| positions[index as usize])
            .collect();
        // 删掉的标注和它的弹窗；指回被删标注的弹窗也删。
        let mut removed = HashSet::new();
        for &position in &dropped {
            if let Ok(id) = annotations[position].as_reference() {
                removed.insert(id);
            }
            if let Some(popup) = document
                .dereference(&annotations[position])
                .ok()
                .and_then(|(_, entry)| entry.as_dict().ok())
                .and_then(|entry| entry.get(b"Popup").ok())
                .and_then(|popup| popup.as_reference().ok())
            {
                removed.insert(popup);
            }
        }
        let parent = |entry: &Object| {
            document
                .dereference(entry)
                .ok()
                .and_then(|(_, entry)| entry.as_dict().ok())
                .and_then(|entry| entry.get(b"Parent").ok())
                .and_then(|parent| parent.as_reference().ok())
        };
        let kept: Vec<_> = annotations
            .iter()
            .enumerate()
            .filter(|(position, entry)| {
                !dropped.contains(position)
                    && entry
                        .as_reference()
                        .map_or(true, |id| !removed.contains(&id))
                    && parent(entry).is_none_or(|parent| !removed.contains(&parent))
            })
            .map(|(_, entry)| entry.clone())
            .collect();
        set_annotations(document, page_id, kept)?;
    }
    Ok(())
}

/// 按页序把各来源的页对象搬进新文件。
fn compose(
    sources: &[&Pdf],
    documents: &[Option<Document>],
    picks: &[Pick],
    keep_outlines: bool,
    cancel: &Cancel,
) -> Result<(Document, Vec<ObjectId>), Error> {
    let version = documents
        .iter()
        .flatten()
        .map(|document| document.version.clone())
        .max()
        .unwrap_or_else(|| "1.7".into());
    let mut output = Document::with_version(version);
    let tree = output.new_object_id();
    let pages: Vec<_> = picks.iter().map(|_| output.new_object_id()).collect();
    let source_pages: Vec<_> = documents
        .iter()
        .map(|document| document.as_ref().map(Document::get_pages))
        .collect();
    let mut first: HashMap<(u32, u32), ObjectId> = HashMap::new();
    for (pick, &page) in picks.iter().zip(&pages) {
        first.entry((pick.source, pick.page)).or_insert(page);
    }
    let document = |source: u32| documents[source as usize].as_ref().ok_or(Error::SaveFailed);
    let source_page = |source: u32, page: u32| {
        source_pages[source as usize]
            .as_ref()
            .and_then(|pages| pages.get(&page).copied())
            .ok_or(Error::SaveFailed)
    };
    let seeds = |source: u32| -> Result<Vec<(ObjectId, ObjectId)>, Error> {
        first
            .iter()
            .filter(|((from, _), _)| *from == source)
            .map(|(&(from, page), &output)| Ok((source_page(from, page)?, output)))
            .collect()
    };
    // 每个来源一个搬运器：重复选的页共用内容流和资源。主文件总要搬文件信息。
    let mut importers: HashMap<u32, Importer> = HashMap::new();
    for source in std::iter::once(0).chain(picks.iter().map(|pick| pick.source)) {
        if let std::collections::hash_map::Entry::Vacant(entry) = importers.entry(source) {
            let mut importer = Importer::new(document(source)?);
            for (old, new) in seeds(source)? {
                importer.seed(old, new);
            }
            entry.insert(importer);
        }
    }

    // 带结构树的来源，结构树跟着搬，取字顺序不变。几份合在一起时 `/StructParents` 依次错开。
    let mut tagged: Vec<(u32, i64)> = Vec::new();
    let mut next_key = 0;
    for pick in picks {
        if tagged.iter().any(|(source, _)| *source == pick.source) {
            continue;
        }
        if let Some(root) = structure_root(document(pick.source)?) {
            let from = document(pick.source)?;
            let used = value(from, root, b"ParentTree")
                .and_then(|tree| tree.as_dict().ok())
                .and_then(|tree| number_tree(from, tree).last().map(|(key, _)| key + 1))
                .unwrap_or(0)
                .max(
                    value(from, root, b"ParentTreeNextKey")
                        .and_then(|key| key.as_i64().ok())
                        .unwrap_or(0),
                );
            tagged.push((pick.source, next_key));
            next_key += used;
        }
    }
    // PDFKit 按整份文件决定取字顺序：混进没有结构树的来源时，那些页在带结构树的文件里会换序，
    // 所以只要有一份没有结构树就都不带（和「预览」一样按版面取字）。
    if picks
        .iter()
        .any(|pick| !tagged.iter().any(|(source, _)| *source == pick.source))
    {
        tagged.clear();
    }
    let offset = |source: u32| {
        tagged
            .iter()
            .find(|(tagged, _)| *tagged == source)
            .map(|(_, offset)| *offset)
    };

    for (pick, &page_id) in picks.iter().zip(&pages) {
        cancel.check()?;
        let from = document(pick.source)?;
        let original_id = source_page(pick.source, pick.page)?;
        // 结构树只认每页的第一份副本。
        let shift = offset(pick.source).filter(|_| first[&(pick.source, pick.page)] == page_id);
        let structured = shift.is_some();
        let original = from
            .get_dictionary(original_id)
            .map_err(|_| Error::SaveFailed)?;
        let mut page = Dictionary::new();
        page.set("Type", "Page");
        for key in [b"MediaBox".as_slice(), b"CropBox", b"Resources"] {
            if let Some(value) = inherited(from, original, key) {
                page.set(key, value.clone());
            }
        }
        for key in [
            b"BleedBox".as_slice(),
            b"TrimBox",
            b"ArtBox",
            b"Contents",
            b"Group",
            b"UserUnit",
            b"Thumb",
            b"VP",
            b"Metadata",
        ] {
            if let Ok(value) = original.get(key) {
                page.set(key, value.clone());
            }
        }
        if let Some(shift) = shift {
            if let Some(key) =
                value(from, original, b"StructParents").and_then(|key| key.as_i64().ok())
            {
                page.set("StructParents", key + shift);
            }
            if let Ok(tabs) = original.get(b"Tabs") {
                page.set("Tabs", tabs.clone());
            }
        }
        let rotation = inherited(from, original, b"Rotate")
            .and_then(|value| value.as_i64().ok())
            .unwrap_or(0);
        let rotation = (rotation + i64::from(pick.rotate)).rem_euclid(360);
        if rotation != 0 {
            page.set("Rotate", rotation);
        }
        let mut page = Object::Dictionary(page);
        let importer = importers.get_mut(&pick.source).ok_or(Error::SaveFailed)?;
        importer.remap(&mut page, &mut output);
        importer.finish(&mut output, cancel)?;
        let mut page = page.as_dict().map_err(|_| Error::SaveFailed)?.clone();
        page.set("Parent", tree);

        // 每份副本各有自己的标注对象。
        let mut annotations = Importer::new(from);
        for (old, new) in seeds(pick.source)? {
            annotations.seed(old, new);
        }
        annotations.seed(original_id, page_id);
        let entries = page_annotations(from, original_id);
        let mut ids = Vec::new();
        for entry in &entries {
            if let Ok(old) = entry.as_reference() {
                let new = output.new_object_id();
                annotations.seed(old, new);
                ids.push(Some((old, new)));
            } else {
                ids.push(None);
            }
        }
        if structured {
            // 结构树里指向标注的 OBJR 用这份副本的标注。
            for &(old, new) in ids.iter().flatten() {
                importer_seed(&mut importers, pick.source, old, new)?;
            }
        }
        let mut kept = Vec::new();
        for (entry, id) in entries.iter().zip(ids) {
            let Ok((_, Object::Dictionary(annotation))) = from.dereference(entry) else {
                continue;
            };
            let mut annotation = annotation.clone();
            match (
                shift,
                value(from, &annotation, b"StructParent").and_then(|key| key.as_i64().ok()),
            ) {
                (Some(shift), Some(key)) => annotation.set("StructParent", key + shift),
                _ => {
                    annotation.remove(b"StructParent");
                }
            }
            let link = value(from, &annotation, b"Subtype")
                .and_then(|subtype| subtype.as_name().ok())
                == Some(b"Link");
            if link {
                resolve_named_destination(from, &mut annotation);
            }
            let mut object = Object::Dictionary(annotation);
            annotations.remap(&mut object, &mut output);
            let mut annotation = object.as_dict().map_err(|_| Error::SaveFailed)?.clone();
            // 目标页没有进输出的页内链接删掉。
            if link && lost_destination(&output, &annotation) {
                continue;
            }
            annotation.set("P", page_id);
            match id {
                Some((_, new)) => {
                    output.objects.insert(new, Object::Dictionary(annotation));
                    kept.push(Object::Reference(new));
                }
                None => kept.push(Object::Dictionary(annotation)),
            }
        }
        annotations.finish(&mut output, cancel)?;
        if !kept.is_empty() {
            page.set("Annots", kept);
        }
        output.objects.insert(page_id, Object::Dictionary(page));
    }
    output.objects.insert(
        tree,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => pages.iter().copied().map(Object::Reference).collect::<Vec<_>>(),
            "Count" => pages.len() as i64,
        }),
    );

    let mut catalog = dictionary! { "Type" => "Catalog", "Pages" => tree };
    {
        let main = document(0)?;
        let importer = importers.get_mut(&0).ok_or(Error::SaveFailed)?;
        let root = main.catalog().map_err(|_| Error::SaveFailed)?;
        for key in [
            b"Lang".as_slice(),
            b"ViewerPreferences",
            b"PageLayout",
            b"OCProperties",
            b"Metadata",
        ] {
            if let Ok(value) = root.get(key) {
                let mut value = value.clone();
                importer.remap(&mut value, &mut output);
                catalog.set(key, value);
            }
        }
        if let Some(info) = main
            .trailer
            .get(b"Info")
            .ok()
            .and_then(|info| main.dereference(info).ok())
            .and_then(|(_, info)| info.as_dict().ok())
        {
            let mut info = Object::Dictionary(info.clone());
            importer.remap(&mut info, &mut output);
            let info = output.add_object(info);
            output.trailer.set("Info", info);
        }
        importer.finish(&mut output, cancel)?;
    }
    if !tagged.is_empty() {
        let root = merge_structure(
            &mut output,
            &mut importers,
            documents,
            &tagged,
            next_key,
            cancel,
        )?;
        catalog.set("StructTreeRoot", root);
        catalog.set("MarkInfo", dictionary! { "Marked" => true });
    }
    if let Some(labels) = page_labels(documents, picks) {
        catalog.set("PageLabels", labels);
    }
    let catalog = output.add_object(catalog);
    output.trailer.set("Root", catalog);

    if keep_outlines {
        // 各参与来源的原目录按新页序接起来；目标页不在输出里的条目留作分组。
        let mut items = Vec::new();
        for (source, pdf) in sources.iter().enumerate() {
            if !picks.iter().any(|pick| pick.source as usize == source) {
                continue;
            }
            let position = |page: u32| {
                picks
                    .iter()
                    .position(|pick| pick.source as usize == source && pick.page == page)
                    .map(|index| index as u32 + 1)
            };
            items.extend(remap_outline(super::edit::outline(pdf), &position));
        }
        write_outline(&mut output, &items, &pages)?;
    }
    Ok((output, pages))
}

fn structure_root(document: &Document) -> Option<&Dictionary> {
    let catalog = document.catalog().ok()?;
    value(document, catalog, b"StructTreeRoot")?.as_dict().ok()
}

/// 把各来源的结构树挂到同一个根下：顶层元素接起来，`/ParentTree` 的键照页上的错开。
fn merge_structure(
    output: &mut Document,
    importers: &mut HashMap<u32, Importer>,
    documents: &[Option<Document>],
    tagged: &[(u32, i64)],
    next_key: i64,
    cancel: &Cancel,
) -> Result<ObjectId, Error> {
    let root_id = output.new_object_id();
    let mut kids = Vec::new();
    let mut numbers = Vec::new();
    let mut roles = Dictionary::new();
    let mut classes = Dictionary::new();
    for &(source, shift) in tagged {
        let from = documents[source as usize]
            .as_ref()
            .ok_or(Error::SaveFailed)?;
        let importer = importers.get_mut(&source).ok_or(Error::SaveFailed)?;
        let catalog = from.catalog().map_err(|_| Error::SaveFailed)?;
        if let Ok(old) = catalog
            .get(b"StructTreeRoot")
            .and_then(Object::as_reference)
        {
            importer.seed(old, root_id);
        }
        let root = structure_root(from).ok_or(Error::SaveFailed)?;
        let mut own_kids = match value(from, root, b"K") {
            Some(Object::Array(items)) => items.clone(),
            Some(_) => root.get(b"K").ok().cloned().into_iter().collect(),
            None => Vec::new(),
        };
        let mut own_numbers = Vec::new();
        if let Some(tree) = value(from, root, b"ParentTree").and_then(|tree| tree.as_dict().ok()) {
            for (key, entry) in number_tree(from, tree) {
                own_numbers.push(Object::Integer(key + shift));
                own_numbers.push(entry.clone());
            }
        }
        for item in own_kids.iter_mut().chain(&mut own_numbers) {
            importer.remap(item, output);
        }
        for (map, key) in [
            (&mut roles, b"RoleMap".as_slice()),
            (&mut classes, b"ClassMap"),
        ] {
            if let Some(entries) = value(from, root, key).and_then(|entries| entries.as_dict().ok())
            {
                for (name, entry) in entries.iter() {
                    if !map.has(name) {
                        let mut entry = entry.clone();
                        importer.remap(&mut entry, output);
                        map.set(name.clone(), entry);
                    }
                }
            }
        }
        importer.finish(output, cancel)?;
        kids.extend(own_kids);
        numbers.extend(own_numbers);
    }
    let mut root = dictionary! {
        "Type" => "StructTreeRoot",
        "K" => kids,
        "ParentTree" => dictionary! { "Nums" => numbers },
        "ParentTreeNextKey" => next_key,
    };
    if !roles.is_empty() {
        root.set("RoleMap", roles);
    }
    if !classes.is_empty() {
        root.set("ClassMap", classes);
    }
    output.objects.insert(root_id, Object::Dictionary(root));
    Ok(root_id)
}

fn importer_seed(
    importers: &mut HashMap<u32, Importer>,
    source: u32,
    old: ObjectId,
    new: ObjectId,
) -> Result<(), Error> {
    importers
        .get_mut(&source)
        .ok_or(Error::SaveFailed)?
        .seed(old, new);
    Ok(())
}

fn remap_outline(
    items: Vec<OutlineItem>,
    position: &impl Fn(u32) -> Option<u32>,
) -> Vec<OutlineItem> {
    items
        .into_iter()
        .map(|item| OutlineItem {
            title: item.title,
            target: match item.target {
                Target::Page { page, y } => {
                    position(page).map_or(Target::None, |page| Target::Page { page, y })
                }
                target => target,
            },
            children: remap_outline(item.children, position),
        })
        .collect()
}

/// `/Dest (name)` 或 GoTo 的 `/D (name)` 换成目标数组，换页时才能重映射。找不到的保持原样。
fn resolve_named_destination(document: &Document, annotation: &mut Dictionary) {
    let named = |object: &Object| match document.dereference(object).ok()?.1 {
        Object::Name(name) | Object::String(name, _) => named_destination(document, name),
        _ => None,
    };
    if let Some(destination) = annotation.get(b"Dest").ok().and_then(named) {
        annotation.set("Dest", destination);
        return;
    }
    let Some(action) = annotation
        .get(b"A")
        .ok()
        .and_then(|action| document.dereference(action).ok())
        .and_then(|(_, action)| action.as_dict().ok())
    else {
        return;
    };
    if action.get(b"S").and_then(Object::as_name).ok() != Some(b"GoTo") {
        return;
    }
    if let Some(destination) = action.get(b"D").ok().and_then(named) {
        let mut action = action.clone();
        action.set("D", destination);
        annotation.set("A", action);
    }
}

fn named_destination(document: &Document, name: &[u8]) -> Option<Object> {
    let catalog = document.catalog().ok()?;
    let explicit = |object: &Object| -> Option<Object> {
        match document.dereference(object).ok()?.1 {
            array @ Object::Array(_) => Some(array.clone()),
            Object::Dictionary(dictionary) => Some(value(document, dictionary, b"D")?.clone()),
            _ => None,
        }
    };
    if let Some(found) = value(document, catalog, b"Dests")
        .and_then(|dests| dests.as_dict().ok())
        .and_then(|dests| dests.get(name).ok())
        .and_then(explicit)
    {
        return Some(found);
    }
    let tree = value(document, catalog, b"Names")?.as_dict().ok()?;
    let tree = value(document, tree, b"Dests")?.as_dict().ok()?;
    let mut pending = vec![tree];
    let mut seen = 0;
    while let Some(node) = pending.pop() {
        seen += 1;
        if seen > 100_000 {
            return None;
        }
        if let Some(names) = array(document, node, b"Names") {
            for pair in names.chunks_exact(2) {
                if document.dereference(&pair[0]).ok()?.1.as_str().ok() == Some(name) {
                    return explicit(&pair[1]);
                }
            }
        }
        for kid in array(document, node, b"Kids").unwrap_or_default() {
            if let Ok((_, Object::Dictionary(kid))) = document.dereference(kid) {
                pending.push(kid);
            }
        }
    }
    None
}

fn lost_destination(document: &Document, annotation: &Dictionary) -> bool {
    let target = annotation.get(b"Dest").ok().or_else(|| {
        let action = document
            .dereference(annotation.get(b"A").ok()?)
            .ok()?
            .1
            .as_dict()
            .ok()?;
        (action.get(b"S").and_then(Object::as_name).ok() == Some(b"GoTo"))
            .then(|| action.get(b"D").ok())
            .flatten()
    });
    target
        .and_then(|target| document.dereference(target).ok())
        .and_then(|(_, target)| target.as_array().ok())
        .and_then(|target| target.first())
        .is_some_and(|page| matches!(page, Object::Null))
}

/// 每个输出页沿用原页的页标签；同一来源里连着的页合成一段。没有来源定义页标签时不写。
fn page_labels(documents: &[Option<Document>], picks: &[Pick]) -> Option<Object> {
    let ranges: Vec<_> = documents
        .iter()
        .map(|document| document.as_ref().map(label_ranges).unwrap_or_default())
        .collect();
    if !picks
        .iter()
        .any(|pick| !ranges[pick.source as usize].is_empty())
    {
        return None;
    }
    let mut numbers = Vec::new();
    let mut previous: Option<(u32, u32, usize)> = None;
    for (index, pick) in picks.iter().enumerate() {
        let ranges = &ranges[pick.source as usize];
        let at = pick.page - 1;
        let range = ranges.iter().rposition(|(start, _)| *start <= at);
        if let (Some((source, page, previous_range)), Some(range)) = (previous, range)
            && source == pick.source
            && page + 1 == pick.page
            && previous_range == range
        {
            previous = Some((pick.source, pick.page, range));
            continue;
        }
        let mut label = match range {
            Some(range) => ranges[range].1.clone(),
            // 没有页标签的来源照物理页码标阿拉伯数字。
            None => dictionary! { "S" => "D" },
        };
        let (start, first) = range.map_or((0, 1), |range| {
            let (start, label) = &ranges[range];
            (
                *start,
                label.get(b"St").and_then(Object::as_i64).unwrap_or(1),
            )
        });
        let offset = i64::from(at - start.min(at));
        let start_number = if range.is_some() {
            first + offset
        } else {
            i64::from(pick.page)
        };
        if start_number != 1 {
            label.set("St", start_number);
        } else {
            label.remove(b"St");
        }
        numbers.push(Object::Integer(index as i64));
        numbers.push(Object::Dictionary(label));
        previous = range.map(|range| (pick.source, pick.page, range));
        if range.is_none() {
            previous = None;
        }
    }
    Some(Object::Dictionary(dictionary! { "Nums" => numbers }))
}

/// 读 `/PageLabels` 数字树：每段从第几页（0 起）开始、样式字典。
fn label_ranges(document: &Document) -> Vec<(u32, Dictionary)> {
    let Some(tree) = document
        .catalog()
        .ok()
        .and_then(|catalog| value(document, catalog, b"PageLabels"))
        .and_then(|tree| tree.as_dict().ok())
    else {
        return Vec::new();
    };
    number_tree(document, tree)
        .into_iter()
        .filter_map(|(start, label)| {
            let start = u32::try_from(start).ok()?;
            let label = document.dereference(label).ok()?.1.as_dict().ok()?;
            // 前缀可能是间接字符串，样式只认这三项。
            let mut clean = Dictionary::new();
            for key in [b"S".as_slice(), b"P", b"St"] {
                if let Some(value) = value(document, label, key) {
                    clean.set(key, value.clone());
                }
            }
            Some((start, clean))
        })
        .collect()
}

/// 读数字树（`/Nums`、`/Kids`），按键排好。
fn number_tree<'a>(document: &'a Document, tree: &'a Dictionary) -> Vec<(i64, &'a Object)> {
    let mut entries = Vec::new();
    let mut pending = vec![tree];
    let mut visited = 0;
    while let Some(node) = pending.pop() {
        visited += 1;
        if visited > 100_000 {
            break;
        }
        for pair in array(document, node, b"Nums")
            .unwrap_or_default()
            .chunks_exact(2)
        {
            if let Some(key) = document
                .dereference(&pair[0])
                .ok()
                .and_then(|(_, key)| key.as_i64().ok())
            {
                entries.push((key, &pair[1]));
            }
        }
        for kid in array(document, node, b"Kids").unwrap_or_default() {
            if let Ok((_, Object::Dictionary(kid))) = document.dereference(kid) {
                pending.push(kid);
            }
        }
    }
    entries.sort_by_key(|(key, _)| *key);
    entries
}

fn check_rect(rect: &Rect) -> Result<(), Error> {
    if [
        rect.x,
        rect.y,
        rect.width,
        rect.height,
        rect.x + rect.width,
        rect.y + rect.height,
    ]
    .into_iter()
    .all(f64::is_finite)
        && rect.width > 0.0
        && rect.height > 0.0
    {
        Ok(())
    } else {
        Err(Error::InvalidGeometry)
    }
}

fn add_marks(
    output: &mut Document,
    pages: &[ObjectId],
    sources: &[&Pdf],
    positions: &HashMap<(u32, u32), Vec<usize>>,
    marks: &[Mark],
    cancel: &Cancel,
) -> Result<(), Error> {
    let mut by_page: HashMap<usize, Vec<&Mark>> = HashMap::new();
    for mark in marks {
        cancel.check()?;
        let source = sources
            .get(mark.source as usize)
            .ok_or(Error::InvalidSource {
                source: mark.source,
            })?;
        if mark.page == 0 || mark.page > source.page_count() {
            return Err(Error::PageOutOfRange {
                page: mark.page,
                total: source.page_count(),
            });
        }
        check_rect(&mark.rect)?;
        for &index in positions
            .get(&(mark.source, mark.page))
            .into_iter()
            .flatten()
        {
            by_page.entry(index).or_default().push(mark);
        }
    }
    let mut indices: Vec<_> = by_page.keys().copied().collect();
    indices.sort_unstable();
    for index in indices {
        cancel.check()?;
        let page = pages[index];
        let marks = &by_page[&index];
        let mut annotations = page_annotations(output, page);
        let drawn: Vec<&Mark> = marks
            .iter()
            .copied()
            .filter(|mark| !matches!(mark.kind, MarkKind::Link { .. }))
            .collect();
        // 链接直接写字典；其余在 PDFKit 画好外观后按原顺序放回。
        let mut drawn_ids = if drawn.is_empty() {
            Vec::new()
        } else {
            draw_marks(output, page, &drawn, cancel)?
        }
        .into_iter();
        for mark in marks {
            match &mark.kind {
                MarkKind::Link { target } => {
                    let link = link(page, pages, mark, target)?;
                    annotations.push(Object::Reference(output.add_object(link)));
                }
                _ => {
                    for id in drawn_ids.by_ref() {
                        annotations.push(Object::Reference(id));
                        // 便签的弹窗跟在主标注后面，一起放回。
                        if !output
                            .get_dictionary(id)
                            .ok()
                            .and_then(|entry| entry.get(b"Subtype").ok())
                            .and_then(|subtype| subtype.as_name().ok())
                            .is_some_and(|subtype| subtype == b"Popup")
                        {
                            break;
                        }
                    }
                }
            }
        }
        annotations.extend(drawn_ids.map(Object::Reference));
        set_annotations(output, page, annotations)?;
    }
    Ok(())
}

fn link(
    page: ObjectId,
    pages: &[ObjectId],
    mark: &Mark,
    target: &Target,
) -> Result<Dictionary, Error> {
    let rect = mark.rect;
    let mut link = dictionary! {
        "Type" => "Annot",
        "Subtype" => "Link",
        "Rect" => [rect.x, rect.y, rect.x + rect.width, rect.y + rect.height].map(|value| Object::Real(value as f32)).to_vec(),
        "Border" => vec![0.into(), 0.into(), 0.into()],
        "F" => 4,
        "P" => page,
    };
    link.set("A", action(pages, target)?.ok_or(Error::InvalidTarget)?);
    if let Some(author) = &mark.author {
        link.set("T", text_string(author));
    }
    if let Some(contents) = &mark.contents {
        link.set("Contents", text_string(contents));
    }
    Ok(link)
}

fn action(pages: &[ObjectId], target: &Target) -> Result<Option<Dictionary>, Error> {
    match target {
        Target::Page { page, y } => {
            let page = page
                .checked_sub(1)
                .and_then(|index| pages.get(index as usize))
                .ok_or(Error::InvalidTarget)?;
            if !y.is_finite() {
                return Err(Error::InvalidTarget);
            }
            Ok(Some(dictionary! {
                "S" => "GoTo",
                "D" => vec![Object::Reference(*page), "XYZ".into(), Object::Null, Object::Real(*y as f32), Object::Null],
            }))
        }
        Target::Url { url } => {
            let (scheme, rest) = url.split_once(':').ok_or(Error::InvalidTarget)?;
            let valid = !rest.is_empty()
                && scheme.starts_with(|c: char| c.is_ascii_alphabetic())
                && scheme
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || "+-.".contains(c))
                && !["file", "javascript"].contains(&scheme.to_ascii_lowercase().as_str());
            if !valid || url.chars().any(char::is_control) {
                return Err(Error::InvalidTarget);
            }
            Ok(Some(dictionary! {
                "S" => "URI",
                "URI" => Object::String(url.as_bytes().to_vec(), StringFormat::Literal),
            }))
        }
        Target::None => Ok(None),
    }
}

/// 在和输出页同样大小、裁切、旋转的空白页上用 PDFKit 画出新标注，搬到输出页，交回新对象（含弹窗）。
fn draw_marks(
    output: &mut Document,
    page: ObjectId,
    marks: &[&Mark],
    cancel: &Cancel,
) -> Result<Vec<ObjectId>, Error> {
    let dictionary = output.get_dictionary(page).map_err(|_| Error::SaveFailed)?;
    let (x0, y0) = media_origin(output, dictionary).ok_or(Error::SaveFailed)?;
    let page_box = |key: &[u8]| -> Option<Vec<f32>> {
        let values = inherited(output, dictionary, key)?.as_array().ok()?;
        let values: Option<Vec<_>> = values.iter().map(|value| number(output, value)).collect();
        values.filter(|values| values.len() == 4)
    };
    let media = page_box(b"MediaBox").ok_or(Error::SaveFailed)?;
    let right = media[0].max(media[2]);
    let crop = page_box(b"CropBox");
    let rotate = inherited(output, dictionary, b"Rotate")
        .and_then(|value| value.as_i64().ok())
        .unwrap_or(0);
    // 空白页原点放在 0，标注一起平移，搬回时再移回去。
    let shift = |values: &[f32]| -> Vec<Object> {
        values
            .iter()
            .enumerate()
            .map(|(index, value)| Object::Real(value - if index % 2 == 0 { x0 } else { y0 }))
            .collect()
    };
    let mut scratch = Document::with_version("1.7");
    let tree = scratch.new_object_id();
    let mut blank = dictionary! { "Type" => "Page", "Parent" => tree, "MediaBox" => shift(&media), "Rotate" => rotate };
    if let Some(crop) = &crop {
        blank.set("CropBox", shift(crop));
    }
    let blank = scratch.add_object(blank);
    scratch.objects.insert(
        tree,
        Object::Dictionary(
            dictionary! { "Type" => "Pages", "Kids" => vec![blank.into()], "Count" => 1 },
        ),
    );
    let catalog = scratch.add_object(dictionary! { "Type" => "Catalog", "Pages" => tree });
    scratch.trailer.set("Root", catalog);
    let scratch = objects::save(&mut scratch, cancel)?;

    let (dx, dy) = (f64::from(x0), f64::from(y0));
    let pdf = Pdf::from_bytes(&scratch).map_err(|_| Error::SaveFailed)?;
    let drawn = pdf.with_page(1, |native| {
        for mark in marks {
            let mark = shifted(mark, -dx, -dy);
            let annotation = match &mark.kind {
                MarkKind::Image { format, bytes } => {
                    let rect = mark.rect;
                    let bounds = objc2_foundation::NSRect::new(
                        objc2_foundation::NSPoint::new(rect.x, rect.y),
                        objc2_foundation::NSSize::new(rect.width, rect.height),
                    );
                    stamp::new(bounds, *format, bytes).ok_or(Error::InvalidStampImage)?
                }
                _ => annotation::create(&mark)?,
            };
            autoreleasepool(|_| {
                // SAFETY: 标注、字符串和页面都在当前线程。显式清空缺省作者，避免写入系统帐户名。
                unsafe {
                    let author = mark.author.as_deref().map(NSString::from_str);
                    annotation.setUserName(author.as_deref());
                    if !matches!(mark.kind, MarkKind::TextBox { .. }) {
                        annotation.setContents(
                            mark.contents.as_deref().map(NSString::from_str).as_deref(),
                        );
                    }
                    native.addAnnotation(&annotation);
                }
            });
        }
        // SAFETY: 空白文档在当前线程，交回 PDFKit 写出的数据副本。
        unsafe { pdf.native().dataRepresentation() }
            .map(|data| data.to_vec())
            .ok_or(Error::SaveFailed)
    })?;
    cancel.check()?;

    let drawn = objects::load_plain(&drawn)?;
    let drawn_page = *drawn.get_pages().get(&1).ok_or(Error::SaveFailed)?;
    let written = drawn
        .get_dictionary(drawn_page)
        .map_err(|_| Error::SaveFailed)?;
    // PDFKit 若改了空白页的原点，标注坐标跟着它走，这里一并补回。
    let (wx, wy) = media_origin(&drawn, written).ok_or(Error::SaveFailed)?;
    let (dx, dy) = (x0 - wx, y0 - wy);
    let mut importer = Importer::new(&drawn);
    importer.seed(drawn_page, page);
    let mut entries = Object::Array(page_annotations(&drawn, drawn_page));
    importer.remap(&mut entries, output);
    importer.finish(output, cancel)?;
    let ids: Vec<_> = entries
        .as_array()
        .map_err(|_| Error::SaveFailed)?
        .iter()
        .filter_map(|entry| entry.as_reference().ok())
        .collect();
    let mut with_popups = Vec::with_capacity(ids.len());
    for id in ids {
        let annotation = output
            .get_dictionary_mut(id)
            .map_err(|_| Error::SaveFailed)?;
        annotation.set("P", page);
        if dx != 0.0 || dy != 0.0 {
            translate(annotation, dx, dy);
        }
        with_popups.push(id);
        // 便签带一个收起的弹窗，「预览」点图标才会展开批注文字（它自己存的便签也这样）。
        let is_note = annotation.get(b"Subtype").and_then(Object::as_name).ok() == Some(b"Text");
        if is_note && !annotation.has(b"Popup") {
            let rect = annotation
                .get(b"Rect")
                .and_then(Object::as_array)
                .ok()
                .and_then(|rect| {
                    rect.iter()
                        .map(|value| value.as_float().ok())
                        .collect::<Option<Vec<_>>>()
                })
                .filter(|rect| rect.len() == 4)
                .ok_or(Error::SaveFailed)?;
            let (left, top) = if rect[2] + 200.0 <= right {
                (rect[2], rect[3])
            } else {
                (rect[0] - 200.0, rect[3])
            };
            let popup = output.add_object(dictionary! {
                "Type" => "Annot",
                "Subtype" => "Popup",
                "Rect" => [left, top - 120.0, left + 200.0, top].map(Object::Real).to_vec(),
                "Parent" => id,
                "Open" => false,
                "P" => page,
            });
            output
                .get_dictionary_mut(id)
                .map_err(|_| Error::SaveFailed)?
                .set("Popup", popup);
            with_popups.push(popup);
        }
    }
    Ok(with_popups)
}

/// 平移标注字典里的页面坐标；外观流按 `Rect` 定位，不用改。
fn translate(annotation: &mut Dictionary, dx: f32, dy: f32) {
    let shift = |values: &mut Vec<Object>| {
        for (index, value) in values.iter_mut().enumerate() {
            if let Ok(number) = value.as_float() {
                *value = Object::Real(number + if index % 2 == 0 { dx } else { dy });
            }
        }
    };
    for key in [b"Rect".as_slice(), b"QuadPoints", b"L", b"Vertices", b"CL"] {
        if let Ok(Object::Array(values)) = annotation.get_mut(key) {
            shift(values);
        }
    }
    if let Ok(Object::Array(paths)) = annotation.get_mut(b"InkList") {
        for path in paths {
            if let Object::Array(values) = path {
                shift(values);
            }
        }
    }
}

fn shifted(mark: &Mark, dx: f64, dy: f64) -> Mark {
    let point = |point: Point| Point {
        x: point.x + dx,
        y: point.y + dy,
    };
    let mut mark = mark.clone();
    mark.rect.x += dx;
    mark.rect.y += dy;
    match &mut mark.kind {
        MarkKind::Highlight { quads, .. }
        | MarkKind::Underline { quads, .. }
        | MarkKind::Strikeout { quads, .. } => {
            for quad in quads {
                quad.top_left = point(quad.top_left);
                quad.top_right = point(quad.top_right);
                quad.bottom_left = point(quad.bottom_left);
                quad.bottom_right = point(quad.bottom_right);
            }
        }
        MarkKind::Ink { strokes, .. } => {
            for stroke in strokes {
                for value in stroke {
                    *value = point(*value);
                }
            }
        }
        MarkKind::Shape {
            shape: Shape::Line { start, end } | Shape::Arrow { start, end },
            ..
        } => {
            *start = point(*start);
            *end = point(*end);
        }
        _ => {}
    }
    mark
}

/// 整棵换掉目录；空数组删除目录。目标页码是输出物理页码。
fn write_outline(
    output: &mut Document,
    items: &[OutlineItem],
    pages: &[ObjectId],
) -> Result<(), Error> {
    let root = output.new_object_id();
    let (first, last, count) = outline_level(output, items, root, pages)?;
    let catalog = output.catalog_mut().map_err(|_| Error::SaveFailed)?;
    match (first, last) {
        (Some(first), Some(last)) => {
            catalog.set("Outlines", root);
            output.objects.insert(
                root,
                Object::Dictionary(dictionary! { "Type" => "Outlines", "First" => first, "Last" => last, "Count" => count }),
            );
        }
        _ => {
            catalog.remove(b"Outlines");
        }
    }
    Ok(())
}

/// 写一层目录，交回首项、末项和本层条数。下级默认收起。
fn outline_level(
    output: &mut Document,
    items: &[OutlineItem],
    parent: ObjectId,
    pages: &[ObjectId],
) -> Result<(Option<ObjectId>, Option<ObjectId>, i64), Error> {
    let ids: Vec<_> = items.iter().map(|_| output.new_object_id()).collect();
    for (index, item) in items.iter().enumerate() {
        let mut node = dictionary! { "Title" => text_string(&item.title), "Parent" => parent };
        if index > 0 {
            node.set("Prev", ids[index - 1]);
        }
        if let Some(&next) = ids.get(index + 1) {
            node.set("Next", next);
        }
        match action(pages, &item.target)? {
            Some(action) if action.get(b"S").and_then(Object::as_name).ok() == Some(b"GoTo") => {
                node.set(
                    "Dest",
                    action.get(b"D").map_err(|_| Error::SaveFailed)?.clone(),
                );
            }
            Some(action) => node.set("A", action),
            None => {}
        }
        let (first, last, count) = outline_level(output, &item.children, ids[index], pages)?;
        if let (Some(first), Some(last)) = (first, last) {
            node.set("First", first);
            node.set("Last", last);
            node.set("Count", -count);
        }
        output.objects.insert(ids[index], Object::Dictionary(node));
    }
    Ok((
        ids.first().copied(),
        ids.last().copied(),
        items.len() as i64,
    ))
}

fn patch_info(output: &mut Document, patch: &InfoPatch) -> Result<(), Error> {
    if *patch == InfoPatch::default() {
        return Ok(());
    }
    let existing = output
        .trailer
        .get(b"Info")
        .ok()
        .and_then(|info| output.dereference(info).ok())
        .and_then(|(_, info)| info.as_dict().ok())
        .cloned()
        .unwrap_or_default();
    let mut info = existing;
    for (key, value) in [
        (b"Title".as_slice(), &patch.title),
        (b"Author", &patch.author),
        (b"Subject", &patch.subject),
        (b"Creator", &patch.creator),
    ] {
        if let Some(value) = value {
            info.set(key, text_string(value));
        }
    }
    if let Some(keywords) = &patch.keywords {
        // PDFKit 按「, 」拆回数组。
        info.set("Keywords", text_string(&keywords.join(", ")));
    }
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |time| time.as_secs());
    info.set("ModDate", Object::string_literal(pdf_date(now)));
    let info = output.add_object(info);
    output.trailer.set("Info", info);
    // 旧的 XMP 元数据会和新标题打架，阅读器常优先读它。
    if patch.title.is_some()
        || patch.author.is_some()
        || patch.subject.is_some()
        || patch.keywords.is_some()
    {
        output
            .catalog_mut()
            .map_err(|_| Error::SaveFailed)?
            .remove(b"Metadata");
    }
    Ok(())
}

/// Unix 秒换成 PDF 日期 `D:YYYYMMDDHHmmSSZ`。
fn pdf_date(seconds: u64) -> String {
    let days = (seconds / 86_400) as i64;
    let rest = seconds % 86_400;
    // Howard Hinnant 的 civil_from_days。
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "D:{year:04}{month:02}{day:02}{:02}{:02}{:02}Z",
        rest / 3_600,
        rest / 60 % 60,
        rest % 60
    )
}

#[cfg(test)]
mod tests {
    #[test]
    fn formats_pdf_dates() {
        assert_eq!(super::pdf_date(0), "D:19700101000000Z");
        assert_eq!(super::pdf_date(1_700_000_000), "D:20231114221320Z");
    }
}
