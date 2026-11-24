use std::ffi::{CStr, c_char, c_void};
use std::ptr::{NonNull, from_mut, null, null_mut};

use objc2::rc::autoreleasepool;
use objc2_core_graphics::{
    CGPDFDictionary, CGPDFDocument, CGPDFInteger, CGPDFObject, CGPDFObjectRef, CGPDFObjectType,
    CGPDFPage, CGPDFStream, CGPDFStreamRef,
};

use crate::{Cancel, Error};

// 沿用 Pomelo 的过滤尺寸、页面树深度和 Form 嵌套深度。
const MIN_IMAGE_SIDE: CGPDFInteger = 32;
const MAX_PAGE_TREE_LEVELS: usize = 16;
const MAX_FORM_DEPTH: u32 = 3;

pub(super) fn image_counts(
    document: &CGPDFDocument,
    cancel: &Cancel,
) -> Result<Vec<(u32, usize)>, Error> {
    let document = Some(document);
    let total =
        u32::try_from(CGPDFDocument::number_of_pages(document)).map_err(|_| Error::Damaged)?;
    let mut pages = Vec::new();
    for index in 1..=total {
        cancel.check()?;
        let count = autoreleasepool(|_| {
            let page = CGPDFDocument::page(document, index as usize)?;
            let mut dict = NonNull::new(CGPDFPage::dictionary(Some(&page)))?;
            for _ in 0..MAX_PAGE_TREE_LEVELS {
                // SAFETY: dict 和其子字典均由当前文档持有，页面在这个 pool 内有效。
                unsafe {
                    if let Some(resources) = dictionary_entry(dict, c"Resources") {
                        let mut count = Counting {
                            images: 0,
                            depth: 0,
                        };
                        count_xobjects(resources, &mut count);
                        return Some(count.images);
                    }
                    dict = dictionary_entry(dict, c"Parent")?;
                }
            }
            None
        });
        if let Some(count) = count.filter(|&count| count > 0) {
            pages.push((index, count));
        }
    }
    Ok(pages)
}

struct Counting {
    images: usize,
    depth: u32,
}

fn key(name: &CStr) -> NonNull<c_char> {
    NonNull::new(name.as_ptr().cast_mut()).unwrap()
}

// SAFETY: dict 必须是仍由当前 CGPDFDocument 持有的有效字典。
unsafe fn dictionary_entry(
    dict: NonNull<CGPDFDictionary>,
    name: &CStr,
) -> Option<NonNull<CGPDFDictionary>> {
    let mut value = null_mut();
    // SAFETY: 键在调用期间有效且以 NUL 结尾，value 是可写出参。
    if unsafe { CGPDFDictionary::dictionary(dict.as_ptr(), key(name), &mut value) } {
        return NonNull::new(value);
    }
    None
}

// SAFETY: resources 必须是仍由当前 CGPDFDocument 持有的有效字典。
unsafe fn count_xobjects(resources: NonNull<CGPDFDictionary>, count: &mut Counting) {
    // SAFETY: 字典由文档持有。CoreGraphics 同步调用回调，不保留栈上计数器的指针。
    unsafe {
        if let Some(xobjects) = dictionary_entry(resources, c"XObject") {
            CGPDFDictionary::apply_function(
                xobjects.as_ptr(),
                Some(count_xobject),
                from_mut(count).cast(),
            );
        }
    }
}

unsafe extern "C-unwind" fn count_xobject(
    _: NonNull<c_char>,
    object: CGPDFObjectRef,
    info: *mut c_void,
) {
    // SAFETY: 回调同步执行，object 由当前文档持有，info 指向调用者独占的计数器。
    unsafe {
        let count = &mut *info.cast::<Counting>();
        let mut stream: CGPDFStreamRef = null_mut();
        if !CGPDFObject::value(
            object,
            CGPDFObjectType::Stream,
            from_mut(&mut stream).cast(),
        ) {
            return;
        }
        let Some(dict) = NonNull::new(CGPDFStream::dictionary(stream)) else {
            return;
        };
        let mut subtype = null();
        if !CGPDFDictionary::name(dict.as_ptr(), key(c"Subtype"), &mut subtype) {
            return;
        }
        // CGPDFDictionaryGetName 成功时返回文档持有的 NUL 结尾名字。
        match CStr::from_ptr(subtype).to_bytes() {
            b"Image" => {
                let (mut width, mut height) = (0, 0);
                if CGPDFDictionary::integer(dict.as_ptr(), key(c"Width"), &mut width)
                    && CGPDFDictionary::integer(dict.as_ptr(), key(c"Height"), &mut height)
                    && width >= MIN_IMAGE_SIDE
                    && height >= MIN_IMAGE_SIDE
                {
                    count.images += 1;
                }
            }
            b"Form" if count.depth < MAX_FORM_DEPTH => {
                if let Some(resources) = dictionary_entry(dict, c"Resources") {
                    count.depth += 1;
                    count_xobjects(resources, count);
                    count.depth -= 1;
                }
            }
            _ => {}
        }
    }
}
