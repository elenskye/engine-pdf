//! 涂黑页上的标记结构（无障碍标签）。结构元素的 `ActualText`、`Alt`、`E` 可能抄着页上的原文，
//! 涂黑页上的元素连同上层元素一律去掉这几项；整页重画的页，结构树里指向旧内容的引用也去掉。

use std::collections::HashSet;

use lopdf::{Dictionary, Document, Object, ObjectId};

const ALTERNATES: [&[u8]; 3] = [b"ActualText", b"Alt", b"E"];

pub(super) struct Scope<'a> {
    /// 涂黑过的页。
    pub(super) pages: &'a HashSet<ObjectId>,
    /// 内容整页换掉的页：原来的标记内容都不在了。
    pub(super) cleared: &'a HashSet<ObjectId>,
    /// 涂黑时删掉的批注。
    pub(super) annotations: &'a HashSet<ObjectId>,
}

pub(super) fn forget(doc: &mut Document, scope: &Scope<'_>) {
    for &page in scope.cleared {
        if let Ok(page) = doc.get_dictionary_mut(page) {
            page.remove(b"StructParents");
        }
    }
    let root = doc
        .catalog()
        .ok()
        .and_then(|catalog| catalog.get(b"StructTreeRoot").ok())
        .and_then(|root| root.as_reference().ok());
    let Some(root) = root else {
        return;
    };
    let Some(mut kids) = doc
        .get_dictionary(root)
        .ok()
        .and_then(|root| root.get(b"K").ok())
        .cloned()
    else {
        return;
    };
    visit(doc, &mut kids, None, scope, &mut HashSet::new());
    if let Ok(root) = doc.get_dictionary_mut(root) {
        root.set("K", kids);
    }
}

struct Visit {
    keep: bool,
    touched: bool,
}

fn visit(
    doc: &mut Document,
    kid: &mut Object,
    page: Option<ObjectId>,
    scope: &Scope<'_>,
    seen: &mut HashSet<ObjectId>,
) -> Visit {
    match kid {
        // 元素自己的页上的标记内容编号。
        Object::Integer(_) => Visit {
            keep: !page.is_some_and(|page| scope.cleared.contains(&page)),
            touched: page.is_some_and(|page| scope.pages.contains(&page)),
        },
        Object::Array(items) => {
            let mut touched = false;
            items.retain_mut(|item| {
                let visit = visit(doc, item, page, scope, seen);
                touched |= visit.touched;
                visit.keep
            });
            Visit {
                keep: true,
                touched,
            }
        }
        Object::Reference(id) => {
            let id = *id;
            if !seen.insert(id) {
                return Visit {
                    keep: true,
                    touched: false,
                };
            }
            let Some(Object::Dictionary(mut dict)) = doc.get_object(id).ok().cloned() else {
                return Visit {
                    keep: true,
                    touched: false,
                };
            };
            let visit = element(doc, &mut dict, page, scope, seen);
            doc.objects.insert(id, Object::Dictionary(dict));
            visit
        }
        Object::Dictionary(dict) => element(doc, dict, page, scope, seen),
        _ => Visit {
            keep: true,
            touched: false,
        },
    }
}

fn element(
    doc: &mut Document,
    dict: &mut Dictionary,
    page: Option<ObjectId>,
    scope: &Scope<'_>,
    seen: &mut HashSet<ObjectId>,
) -> Visit {
    let page = dict
        .get(b"Pg")
        .ok()
        .and_then(|page| page.as_reference().ok())
        .or(page);
    let on_page = page.is_some_and(|page| scope.pages.contains(&page));
    let cleared = page.is_some_and(|page| scope.cleared.contains(&page));
    match dict.get(b"Type").ok().and_then(|kind| kind.as_name().ok()) {
        Some(b"MCR") => {
            return Visit {
                keep: !cleared,
                touched: on_page,
            };
        }
        Some(b"OBJR") => {
            let removed = dict
                .get(b"Obj")
                .ok()
                .and_then(|object| object.as_reference().ok())
                .is_some_and(|object| scope.annotations.contains(&object));
            return Visit {
                keep: !removed,
                touched: on_page || removed,
            };
        }
        _ => {}
    }
    let mut touched = false;
    if let Ok(kids) = dict.get_mut(b"K") {
        let mut kids = std::mem::replace(kids, Object::Null);
        touched = visit(doc, &mut kids, page, scope, seen).touched;
        dict.set("K", kids);
    }
    if touched {
        for key in ALTERNATES {
            dict.remove(key);
        }
    }
    Visit {
        keep: true,
        touched,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::dictionary;

    #[test]
    fn forgets_alternate_text_and_cleared_content() {
        let mut doc = Document::with_version("1.7");
        let page = doc.add_object(dictionary! { "Type" => "Page", "StructParents" => 0 });
        let other = doc.add_object(dictionary! { "Type" => "Page" });
        let note = doc.add_object(dictionary! { "Type" => "Annot" });
        let span = doc.add_object(dictionary! {
            "S" => "Span", "Pg" => page, "ActualText" => Object::string_literal("secret"),
            "K" => vec![0.into(), dictionary! { "Type" => "OBJR", "Obj" => note }.into()],
        });
        let kept = doc.add_object(dictionary! {
            "S" => "Figure", "Pg" => other, "Alt" => Object::string_literal("chart"), "K" => 3,
        });
        let paragraph = doc.add_object(dictionary! {
            "S" => "P", "Alt" => Object::string_literal("summary"), "K" => vec![span.into(), kept.into()],
        });
        let root = doc.add_object(dictionary! { "Type" => "StructTreeRoot", "K" => paragraph });
        let catalog = doc.add_object(dictionary! { "Type" => "Catalog", "StructTreeRoot" => root });
        doc.trailer.set("Root", catalog);

        let pages = HashSet::from([page]);
        let annotations = HashSet::from([note]);
        forget(
            &mut doc,
            &Scope {
                pages: &pages,
                cleared: &pages,
                annotations: &annotations,
            },
        );
        let get = |id| doc.get_dictionary(id).unwrap();
        assert!(!get(span).has(b"ActualText"));
        assert_eq!(get(span).get(b"K").unwrap().as_array().unwrap().len(), 0);
        assert!(!get(paragraph).has(b"Alt"));
        assert!(get(kept).has(b"Alt"));
        assert!(!get(page).has(b"StructParents"));
    }
}
