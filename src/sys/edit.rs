use objc2::rc::autoreleasepool;
use objc2_foundation::{NSArray, NSDate, NSString};
use objc2_pdf_kit::{
    PDFActionURL, PDFDestination, PDFDisplayBox, PDFDocument, PDFDocumentAuthorAttribute,
    PDFDocumentCreationDateAttribute, PDFDocumentCreatorAttribute, PDFDocumentKeywordsAttribute,
    PDFDocumentModificationDateAttribute, PDFDocumentProducerAttribute,
    PDFDocumentSubjectAttribute, PDFDocumentTitleAttribute, PDFOutline,
    kPDFDestinationUnspecifiedValue,
};

use crate::edit::{Info, InfoPatch, OutlineItem, Permissions, Pick, Plan, Save, Target};
use crate::{Cancel, Error};

use super::{Pdf, forms, password};

pub(crate) fn info(pdf: &Pdf) -> Info {
    autoreleasepool(|_| {
        let doc = pdf.native();
        // SAFETY: PDFKit 文档已初始化，只在所属线程查询属性和权限。
        unsafe {
            let attributes = doc.documentAttributes();
            let string = |key: &NSString| {
                attributes
                    .as_ref()?
                    .objectForKey(key)?
                    .downcast::<NSString>()
                    .ok()
                    .map(|value| value.to_string())
            };
            let date = |key: &NSString| {
                attributes
                    .as_ref()?
                    .objectForKey(key)?
                    .downcast::<NSDate>()
                    .ok()
                    .map(|value| value.timeIntervalSince1970())
            };
            let keywords = attributes
                .as_ref()
                .and_then(|attrs| attrs.objectForKey(PDFDocumentKeywordsAttribute))
                .map(|value| match value.downcast::<NSArray>() {
                    Ok(array) => array
                        .iter()
                        .filter_map(|entry| entry.downcast::<NSString>().ok())
                        .map(|keyword| keyword.to_string())
                        .collect(),
                    Err(value) => value
                        .downcast::<NSString>()
                        .ok()
                        .map(|keyword| keyword.to_string())
                        .into_iter()
                        .collect(),
                })
                .unwrap_or_default();
            let encrypted = doc.isEncrypted();
            let permissions = if encrypted && !pdf.owner_verified() {
                password::declared_permissions(pdf)
            } else {
                None
            };
            Info {
                title: string(PDFDocumentTitleAttribute),
                author: string(PDFDocumentAuthorAttribute),
                subject: string(PDFDocumentSubjectAttribute),
                keywords,
                creator: string(PDFDocumentCreatorAttribute),
                producer: string(PDFDocumentProducerAttribute),
                created: date(PDFDocumentCreationDateAttribute),
                modified: date(PDFDocumentModificationDateAttribute),
                encrypted,
                permissions: permissions.unwrap_or_else(|| Permissions {
                    print: doc.allowsPrinting(),
                    copy: doc.allowsCopying(),
                    modify: doc.allowsDocumentChanges(),
                    annotate: doc.allowsCommenting(),
                }),
            }
        }
    })
}

pub(crate) fn outline(pdf: &Pdf) -> Vec<OutlineItem> {
    autoreleasepool(|_| {
        let doc = pdf.native();
        // SAFETY: 在文档所属线程读取目录，根节点和文档在遍历期间保持存活。
        unsafe { doc.outlineRoot() }
            .map(|root| read_outline(doc, &root))
            .unwrap_or_default()
    })
}

fn read_outline(doc: &PDFDocument, parent: &PDFOutline) -> Vec<OutlineItem> {
    // SAFETY: 目录属于当前线程的 doc，childAtIndex 的下标取自实际子节点数。
    unsafe {
        (0..parent.numberOfChildren())
            .filter_map(|index| parent.childAtIndex(index))
            .map(|item| {
                let target = item
                    .destination()
                    .and_then(|destination| {
                        let page = destination_page(doc, &destination)?;
                        let mut y = destination.point().y;
                        if !y.is_finite() || y == kPDFDestinationUnspecifiedValue {
                            let bounds = destination.page()?.boundsForBox(PDFDisplayBox::CropBox);
                            y = bounds.origin.y + bounds.size.height;
                        }
                        Some(Target::Page { page, y })
                    })
                    .or_else(|| {
                        let action = item.action()?.downcast::<PDFActionURL>().ok()?;
                        let url = action.URL()?;
                        if url.isFileURL() {
                            return None;
                        }
                        Some(Target::Url {
                            url: url.absoluteString()?.to_string(),
                        })
                    })
                    .unwrap_or(Target::None);
                OutlineItem {
                    title: item
                        .label()
                        .map(|title| title.to_string())
                        .unwrap_or_default(),
                    target,
                    children: read_outline(doc, &item),
                }
            })
            .collect()
    }
}

pub(crate) fn apply(
    doc: &Pdf,
    others: &[&Pdf],
    plan: &Plan,
    save: &Save,
    cancel: &Cancel,
) -> Result<Vec<u8>, Error> {
    cancel.check()?;
    if save.remove_password && save.lock.is_some() {
        return Err(Error::InvalidPasswordOptions);
    }
    if !cfg!(feature = "pdfium") && (!plan.objects.is_empty() || !plan.redact.is_empty()) {
        return Err(Error::PdfiumUnavailable);
    }
    let changes_structure = plan.pages.is_some()
        || !plan.add.is_empty()
        || !plan.remove.is_empty()
        || plan.outline.is_some()
        || plan
            .info
            .as_ref()
            .is_some_and(|patch| *patch != InfoPatch::default());
    if !changes_structure
        && plan.objects.is_empty()
        && plan.redact.is_empty()
        && plan.fill.is_empty()
        && !save.flatten
        && !save.remove_password
        && save.lock.is_none()
    {
        return Ok(doc.original_bytes().to_vec());
    }
    let default_picks;
    let picks = match &plan.pages {
        Some(picks) => picks,
        None => {
            default_picks = (1..=doc.page_count())
                .map(|page| Pick {
                    source: 0,
                    page,
                    rotate: 0,
                })
                .collect();
            &default_picks
        }
    };
    if picks.is_empty() {
        return Err(Error::BadPageSpec("请至少选择一页。".into()));
    }

    let sources: Vec<_> = std::iter::once(doc).chain(others.iter().copied()).collect();
    let mut selected = vec![false; sources.len()];
    for pick in picks {
        cancel.check()?;
        let source = sources
            .get(pick.source as usize)
            .ok_or(Error::InvalidSource {
                source: pick.source,
            })?;
        let total = source.page_count();
        if pick.page == 0 || pick.page > total {
            return Err(Error::PageOutOfRange {
                page: pick.page,
                total,
            });
        }
        if pick.rotate % 90 != 0 {
            return Err(Error::InvalidRotation {
                rotate: pick.rotate,
            });
        }
        selected[pick.source as usize] = true;
    }
    if !selected[0] && !plan.fill.is_empty() {
        return Err(Error::FieldNotFound);
    }
    if !selected[0] && (!plan.objects.is_empty() || !plan.redact.is_empty()) {
        return Err(Error::InvalidSource { source: 0 });
    }
    // 页序与原文件相同（可以带旋转）时直接改原文件；连旋转也没有时表单可以保持可填。
    let in_place = picks.len() == doc.page_count() as usize
        && picks
            .iter()
            .zip(1..)
            .all(|(pick, page)| pick.source == 0 && pick.page == page);
    let keeps_layout = in_place && picks.iter().all(|pick| pick.rotate.rem_euclid(360) == 0);
    let mut prepared = Vec::with_capacity(sources.len());
    for (index, (source, selected)) in sources.iter().zip(selected).enumerate() {
        cancel.check()?;
        if !selected {
            prepared.push(None);
            continue;
        }
        if source.has_form() && !keeps_layout && !save.flatten {
            return Err(Error::FormEditUnavailable);
        }
        let mut bytes = None;
        // SAFETY: 文档在所属线程，只读取加密状态。
        if unsafe { source.native().isEncrypted() } {
            if !source.owner_verified() {
                return Err(Error::OwnerPasswordRequired);
            }
            if !save.remove_password && save.lock.is_none() {
                return Err(Error::PasswordActionRequired);
            }
            let clear = password::plaintext(source)?;
            let opened = Pdf::from_bytes(&clear)?;
            for page in 1..=source.page_count() {
                cancel.check()?;
                if opened.text(page)? != source.text(page)? {
                    return Err(Error::EditTextChanged { page });
                }
            }
            bytes = Some(clear);
        }
        #[cfg(feature = "pdfium")]
        if index == 0 && !plan.objects.is_empty() {
            let input = bytes.as_deref().unwrap_or_else(|| source.original_bytes());
            bytes = Some(super::pdfium::edit_objects(
                input,
                &plan.objects,
                source.page_count(),
                cancel,
            )?);
        }
        if index == 0 && !plan.fill.is_empty() {
            let input = bytes.as_deref().unwrap_or_else(|| source.original_bytes());
            bytes = Some(forms::fill(input, &plan.fill, cancel)?);
        }
        // 换页序会丢掉表单，先把控件烧进页面；页序不变时表单原样保留。
        if !keeps_layout && save.flatten && source.has_form() {
            let input = bytes.as_deref().unwrap_or_else(|| source.original_bytes());
            bytes = Some(super::flatten::flatten(input, true, cancel)?);
        }
        prepared.push(bytes.as_deref().map(Pdf::from_bytes).transpose()?);
    }
    let sources: Vec<_> = sources
        .into_iter()
        .zip(&prepared)
        .map(|(source, prepared)| prepared.as_ref().unwrap_or(source))
        .collect();
    let mut bytes = if changes_structure {
        super::write::write(&sources, picks, in_place, plan, cancel)?
    } else {
        sources[0].original_bytes().to_vec()
    };
    if save.flatten {
        bytes = super::flatten::flatten(&bytes, false, cancel)?;
    }
    #[cfg(feature = "pdfium")]
    if !plan.redact.is_empty() {
        bytes = super::redact::apply(&bytes, &plan.redact, picks, doc.page_count(), cancel)?;
    }
    if let Some(lock) = &save.lock {
        bytes = password::encrypt(&bytes, lock, cancel)?;
    }
    cancel.check()?;
    Ok(bytes)
}

pub(super) fn destination_page(doc: &PDFDocument, destination: &PDFDestination) -> Option<u32> {
    // SAFETY: 来源和目标都在当前线程查询。indexForPage 找不到页时返回 NSNotFound。
    unsafe {
        let page = destination.page()?;
        let index = doc.indexForPage(&page);
        (index < doc.pageCount()).then(|| index as u32 + 1)
    }
}

#[cfg(all(test, feature = "samples"))]
mod tests {
    use objc2::runtime::AnyObject;
    use objc2_foundation::ns_string;
    use objc2_pdf_kit::PDFActionGoTo;

    use super::*;
    use crate::edit::{Mark, MarkKind, Rect};
    use crate::samples::Sample;

    #[test]
    fn rebuilt_links_keep_coordinates_and_use_output_destinations() {
        let source = Pdf::from_bytes(&Sample::Links.generate().unwrap()).unwrap();
        let plan = Plan {
            pages: Some(vec![
                Pick {
                    source: 0,
                    page: 2,
                    rotate: 0,
                },
                Pick {
                    source: 0,
                    page: 1,
                    rotate: 0,
                },
                Pick {
                    source: 0,
                    page: 2,
                    rotate: 0,
                },
            ]),
            add: vec![Mark {
                source: 0,
                page: 1,
                rect: Rect {
                    x: 50.0,
                    y: 200.0,
                    width: 80.0,
                    height: 14.0,
                },
                author: None,
                contents: None,
                kind: MarkKind::Link {
                    target: Target::Page { page: 3, y: 145.0 },
                },
            }],
            ..Plan::default()
        };
        let cancel = Cancel::new();
        let bytes = apply(&source, &[], &plan, &Save::default(), &cancel).unwrap();
        let output = Pdf::from_bytes(&bytes).unwrap();
        // SAFETY: 测试只读当前线程上文档的标注和链接目标。
        let verify = |pdf: &Pdf, page, expected| {
            pdf.with_page(page, |page| {
                let mut destinations = Vec::new();
                let mut urls = Vec::new();
                for annotation in unsafe { page.annotations() }.to_vec() {
                    let action = unsafe { annotation.action() }.unwrap();
                    match action.downcast::<PDFActionGoTo>() {
                        Ok(action) => {
                            let destination = unsafe { action.destination() };
                            destinations.push((
                                destination_page(pdf.native(), &destination).unwrap(),
                                unsafe { destination.point() }.y,
                            ));
                        }
                        Err(action) => {
                            let url = action.downcast::<PDFActionURL>().unwrap();
                            urls.push(
                                unsafe { url.URL() }
                                    .unwrap()
                                    .absoluteString()
                                    .unwrap()
                                    .to_string(),
                            );
                        }
                    }
                }
                assert_eq!(destinations, expected);
                assert_eq!(urls, ["https://example.com/link"]);
                Ok(())
            })
            .unwrap();
        };
        verify(&output, 2, &[(1, 310.0), (3, 145.0)][..]);
        verify(&source, 1, &[(2, 310.0)][..]);

        let plan = Plan {
            pages: Some(vec![Pick {
                source: 0,
                page: 1,
                rotate: 0,
            }]),
            ..Plan::default()
        };
        let bytes = apply(&source, &[], &plan, &Save::default(), &cancel).unwrap();
        verify(&Pdf::from_bytes(&bytes).unwrap(), 1, &[][..]);
    }

    #[test]
    fn owner_encryption_keeps_no_op_bytes_and_blocks_editing() {
        use objc2_foundation::{NSDictionary, NSNumber};
        use objc2_pdf_kit::{PDFDocumentAccessPermissionsOption, PDFDocumentOwnerPasswordOption};

        let original = Pdf::from_bytes(&Sample::TextEn.generate().unwrap()).unwrap();
        let bytes = autoreleasepool(|_| {
            let permissions = NSNumber::numberWithUnsignedInt(0);
            // SAFETY: 只对生成样本设置公开的密码和权限选项，PDFKit 对象都在同一线程。
            unsafe {
                let options = NSDictionary::<NSString, AnyObject>::from_slices(
                    &[
                        PDFDocumentOwnerPasswordOption,
                        PDFDocumentAccessPermissionsOption,
                    ],
                    &[ns_string!("engine-pdf-fixture-owner"), &permissions],
                );
                original
                    .native()
                    .dataRepresentationWithOptions(options.cast_unchecked())
                    .unwrap()
                    .to_vec()
            }
        });
        let encrypted = Pdf::from_bytes(&bytes).unwrap();
        assert!(info(&encrypted).encrypted);
        assert!(!info(&encrypted).permissions.copy);
        let cancel = Cancel::new();
        assert_eq!(
            apply(&encrypted, &[], &Plan::default(), &Save::default(), &cancel).unwrap(),
            bytes
        );
        let plan = Plan {
            info: Some(InfoPatch {
                title: Some("Example".into()),
                ..InfoPatch::default()
            }),
            ..Plan::default()
        };
        assert!(matches!(
            apply(&encrypted, &[], &plan, &Save::default(), &cancel),
            Err(Error::OwnerPasswordRequired)
        ));
    }
}
