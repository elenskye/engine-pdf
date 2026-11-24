use objc2::AnyThread;
use objc2::rc::{Retained, autoreleasepool};
use objc2_foundation::{NSCopying, NSData};
use objc2_pdf_kit::{PDFDocument, PDFPage};

use crate::{Cancel, Error, Image, ImageFormat};

// PDFDocument 自身 !Send / !Sync。
pub(crate) struct Pdf(Retained<PDFDocument>);

impl Pdf {
    pub(crate) fn from_bytes(bytes: &[u8]) -> Result<Self, Error> {
        // 和 Acrobat、PDFKit 一样，文件头可以出现在前 1024 字节里（前面偶尔带着下载时混进来的杂字节）。
        if !bytes[..bytes.len().min(1024)]
            .windows(5)
            .any(|w| w == b"%PDF-")
        {
            return Err(Error::NotPdf);
        }
        autoreleasepool(|_| {
            let data = NSData::with_bytes(bytes);
            // SAFETY: NSData 拥有输入的副本。PDFKit 在当前线程创建并使用文档。
            let document = unsafe { PDFDocument::initWithData(PDFDocument::alloc(), &data) }
                .ok_or(Error::Damaged)?;
            // SAFETY: 文档已初始化，在创建它的线程查询状态。
            if unsafe { document.isLocked() } {
                return Err(Error::Locked);
            }
            let pdf = Self(document);
            // PDFKit 有时为坏文件返回零页文档。
            if pdf.page_count() == 0 {
                return Err(Error::Damaged);
            }
            Ok(pdf)
        })
    }

    pub(crate) fn page_count(&self) -> u32 {
        // SAFETY: 在文档所属的线程查询页数。
        unsafe { self.0.pageCount() as u32 }
    }

    pub(crate) fn label(&self, page: u32) -> Option<String> {
        self.with_page(page, |page| Ok(label(page))).ok().flatten()
    }

    pub(crate) fn text(&self, page: u32) -> Result<String, Error> {
        self.with_page(page, |page| Ok(text(page)))
    }

    pub(crate) fn page(&self, page: u32) -> Result<(Option<String>, String), Error> {
        self.with_page(page, |page| Ok((label(page), text(page))))
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

    pub(crate) fn image_counts(&self, cancel: &Cancel) -> Result<Vec<(u32, usize)>, Error> {
        autoreleasepool(|_| {
            // SAFETY: PDFKit 文档已初始化并保持存活，原生引用在当前线程使用。
            let document = unsafe { self.0.documentRef() }.ok_or(Error::Damaged)?;
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

    fn with_page<T>(
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
            let page = unsafe { self.0.pageAtIndex((page - 1) as usize) }.ok_or(Error::Damaged)?;
            action(&page)
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
                self.0
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
