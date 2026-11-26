use std::ptr::{NonNull, null_mut};

use objc2::AnyThread;
use objc2::rc::{Retained, autoreleasepool};
use objc2_core_graphics::{CGPDFArray, CGPDFDictionary, CGPDFDocument};
use objc2_foundation::{NSCopying, NSData, NSString};
use objc2_pdf_kit::{PDFDocument, PDFDocumentPermissions, PDFPage};
use zeroize::Zeroizing;

use crate::{Cancel, Error, Image, ImageFormat, OcrBounds, OcrLine, OcrOptions};

// PDFDocument 自身 !Send / !Sync。
pub(crate) struct Pdf {
    document: Retained<PDFDocument>,
    bytes: Retained<NSData>,
    has_labels: bool,
    has_form: bool,
    password: Option<Zeroizing<String>>,
    owner_verified: bool,
}

impl Pdf {
    pub(crate) fn from_bytes(bytes: &[u8]) -> Result<Self, Error> {
        Self::read_bytes(bytes, None)
    }

    pub(crate) fn from_bytes_with_password(
        bytes: &[u8],
        password: &Zeroizing<String>,
    ) -> Result<Self, Error> {
        Self::read_bytes(bytes, Some(password))
    }

    fn read_bytes(bytes: &[u8], password: Option<&Zeroizing<String>>) -> Result<Self, Error> {
        // 和 Acrobat、PDFKit 一样，文件头可以出现在前 1024 字节里（前面偶尔带着下载时混进来的杂字节）。
        if !bytes[..bytes.len().min(1024)]
            .windows(5)
            .any(|w| w == b"%PDF-")
        {
            return Err(Error::NotPdf);
        }
        Self::from_data(NSData::with_bytes(bytes), password)
    }

    fn from_data(
        data: Retained<NSData>,
        password: Option<&Zeroizing<String>>,
    ) -> Result<Self, Error> {
        autoreleasepool(|_| {
            // SAFETY: NSData 拥有输入的副本。PDFKit 在当前线程创建并使用文档。
            let document = unsafe { PDFDocument::initWithData(PDFDocument::alloc(), &data) }
                .ok_or(Error::Damaged)?;
            // SAFETY: 文档已初始化，在创建它的线程检查权限及解锁。
            let (encrypted, locked) = unsafe { (document.isEncrypted(), document.isLocked()) };
            let password = password.filter(|_| encrypted);
            let mut owner_verified = false;
            if let Some(password) = password {
                // SAFETY: 文档在所属线程，密码字符串只用于本次 PDFKit 调用。
                let (unlocked, permissions) = unsafe {
                    let unlocked = document.unlockWithPassword(&NSString::from_str(password));
                    (unlocked, document.permissionsStatus())
                };
                owner_verified = permissions == PDFDocumentPermissions::Owner;
                // 仅设权限密码的文档本来就能打开，PDFKit 会对错误密码返回 true。
                if !unlocked || (!locked && !password.is_empty() && !owner_verified) {
                    return Err(Error::WrongPassword);
                }
            } else if locked {
                return Err(Error::Locked);
            }
            // SAFETY: 文档与其 Catalog 在查询期间有效，键是静态 C 字符串，出参可以为空。
            // `/Fields` 非空或带 XFA 才算有表单：很多论文带着空的 `/AcroForm`。
            let (has_labels, has_form) = unsafe { document.documentRef() }
                .map(|reference| unsafe {
                    let catalog = CGPDFDocument::catalog(Some(&reference));
                    let key = |key: &'static std::ffi::CStr| NonNull::from(key).cast();
                    let labels =
                        CGPDFDictionary::dictionary(catalog, key(c"PageLabels"), null_mut());
                    let mut form = null_mut();
                    let mut fields = null_mut();
                    let form = CGPDFDictionary::dictionary(catalog, key(c"AcroForm"), &mut form)
                        && (CGPDFDictionary::object(form, key(c"XFA"), null_mut())
                            || CGPDFDictionary::array(form, key(c"Fields"), &mut fields)
                                && CGPDFArray::count(fields) > 0);
                    (labels, form)
                })
                .unwrap_or_default();
            let pdf = Self {
                document,
                bytes: data,
                has_labels,
                has_form,
                password: password.cloned(),
                owner_verified,
            };
            // PDFKit 有时为坏文件返回零页文档。
            if pdf.page_count() == 0 {
                return Err(Error::Damaged);
            }
            Ok(pdf)
        })
    }

    pub(crate) fn page_count(&self) -> u32 {
        // SAFETY: 在文档所属的线程查询页数。
        unsafe { self.document.pageCount() as u32 }
    }

    pub(super) fn native(&self) -> &PDFDocument {
        &self.document
    }

    pub(super) fn has_form(&self) -> bool {
        self.has_form
    }

    pub(super) fn owner_verified(&self) -> bool {
        self.owner_verified
    }

    pub(super) fn objects(&self) -> Result<lopdf::Document, Error> {
        if self.owner_verified {
            // PDFKit 验证权限密码后导出明文。lopdf 0.42 的旧安全处理器会把权限密码当作打开密码解内容。
            return super::objects::load(&super::password::plaintext(self)?);
        }
        super::objects::load_with(self.original_bytes(), || {
            self.password
                .as_ref()
                .map_or_else(lopdf::LoadOptions::default, |password| {
                    lopdf::LoadOptions::with_password(password.as_str())
                })
        })
    }

    pub(crate) fn original_bytes(&self) -> &[u8] {
        // SAFETY: from_bytes 创建的 NSData 是固定长度的只读副本，并与文档一起存活。
        unsafe { self.bytes.as_bytes_unchecked() }
    }

    pub(crate) fn label(&self, page: u32) -> Option<String> {
        if !self.has_labels {
            return None;
        }
        self.with_page(page, |page| Ok(label(page))).ok().flatten()
    }

    pub(crate) fn text(&self, page: u32) -> Result<String, Error> {
        self.with_page(page, |page| Ok(text(page)))
    }

    pub(crate) fn page(&self, page: u32) -> Result<(Option<String>, String), Error> {
        self.with_page(page, |page| {
            Ok((if self.has_labels { label(page) } else { None }, text(page)))
        })
    }

    pub(crate) fn render(
        &self,
        page: u32,
        format: ImageFormat,
        max_side: u32,
    ) -> Result<Image, Error> {
        self.with_page(page, |native_page| {
            super::render::render(native_page, page, format, max_side)
        })
    }

    pub(crate) fn ocr(
        &self,
        page: u32,
        options: &OcrOptions,
    ) -> Result<Vec<(OcrLine, OcrBounds)>, Error> {
        self.with_page(page, |native_page| {
            super::ocr::recognize_page(native_page, page, options)
        })
    }

    pub(crate) fn image_counts(&self, cancel: &Cancel) -> Result<Vec<(u32, usize)>, Error> {
        autoreleasepool(|_| {
            // SAFETY: PDFKit 文档已初始化并保持存活，原生引用在当前线程使用。
            let document = unsafe { self.document.documentRef() }.ok_or(Error::Damaged)?;
            super::image_counts::image_counts(&document, cancel)
        })
    }

    pub(crate) fn split(&self, pages: &[u32], cancel: &Cancel) -> Result<Vec<u8>, Error> {
        autoreleasepool(|_| {
            // SAFETY: 通过 PDFKit 构造空文档，只在当前线程使用。
            let output = unsafe { PDFDocument::new() };
            for (position, &page) in pages.iter().enumerate() {
                cancel.check()?;
                self.with_page(page, |source| {
                    let copy = source.copy();
                    // SAFETY: copy 是独立页面，position 等于已经插入的页数，两个对象同线程。
                    unsafe { output.insertPage_atIndex(&copy, position) };
                    Ok(())
                })?;
            }
            cancel.check()?;
            // SAFETY: 输出文档及保留的页面在当前线程使用，序列化后复制数据。
            let data = unsafe { output.dataRepresentation() }.ok_or(Error::Damaged)?;
            Ok(data.to_vec())
        })
    }

    pub(super) fn with_page<T>(
        &self,
        page: u32,
        action: impl FnOnce(&PDFPage) -> Result<T, Error>,
    ) -> Result<T, Error> {
        let total = self.page_count();
        if page == 0 || page > total {
            return Err(Error::PageOutOfRange { page, total });
        }
        autoreleasepool(|_| {
            // SAFETY: 物理页码已检查，PDFKit 用从 0 起的下标，文档和页面均在当前线程。
            let page =
                unsafe { self.document.pageAtIndex((page - 1) as usize) }.ok_or(Error::Damaged)?;
            action(&page)
        })
    }

    #[cfg(feature = "samples")]
    pub(crate) fn scan(&self, page: u32) -> Result<Vec<u8>, Error> {
        self.with_page(page, |native_page| {
            super::samples::scan_page(native_page, page)
        })
    }

    #[cfg(feature = "samples")]
    pub(super) fn with_sample_password(&self, password: &str) -> Result<Vec<u8>, Error> {
        use objc2_foundation::{NSDictionary, NSString};
        use objc2_pdf_kit::{PDFDocumentOwnerPasswordOption, PDFDocumentUserPasswordOption};

        autoreleasepool(|_| {
            let user = NSString::from_str(password);
            let owner = NSString::from_str("engine-pdf-fixture-owner");
            // SAFETY: PDFKit 导出的两个键都接受 NSString 值，文档在所属线程上序列化。
            let bytes = unsafe {
                let options = NSDictionary::<NSString, NSString>::from_slices(
                    &[
                        PDFDocumentUserPasswordOption,
                        PDFDocumentOwnerPasswordOption,
                    ],
                    &[user.as_ref(), owner.as_ref()],
                );
                self.document
                    .dataRepresentationWithOptions(options.cast_unchecked())
            };
            bytes.map(|data| data.to_vec()).ok_or(Error::Damaged)
        })
    }
}

fn label(page: &PDFPage) -> Option<String> {
    // SAFETY: 调用者在有效页面的当前线程和 autorelease pool 内读取标签。
    unsafe { page.label() }.map(|label| label.to_string())
}

fn text(page: &PDFPage) -> String {
    // SAFETY: 调用者在有效页面的当前线程和 autorelease pool 内读取文字层。
    unsafe { page.string() }
        .map(|text| text.to_string())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_non_pdf_and_truncated_header() {
        assert!(matches!(Pdf::from_bytes(b"just text"), Err(Error::NotPdf)));
        assert!(matches!(
            Pdf::from_bytes(b"%PDF-1.4\n"),
            Err(Error::Damaged)
        ));
        let mut late_header = vec![b' '; 1020];
        late_header.extend_from_slice(b"%PDF-");
        assert!(matches!(Pdf::from_bytes(&late_header), Err(Error::NotPdf)));
    }
}
