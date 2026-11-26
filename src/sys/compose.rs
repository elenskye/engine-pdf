use std::borrow::Cow;

use objc2::{AnyThread, rc::autoreleasepool};
use objc2_app_kit::NSImage;
use objc2_foundation::NSData;
use objc2_pdf_kit::{PDFDocument, PDFPage};

use crate::{Cancel, Error, compose::ImageSource};

pub(crate) fn images(sources: &[ImageSource<'_>], cancel: &Cancel) -> Result<Vec<u8>, Error> {
    cancel.check()?;
    if sources.is_empty() {
        return Err(Error::EmptyImages);
    }

    autoreleasepool(|_| {
        // SAFETY: 新文档只在创建它的线程上填充、保存。
        let output = unsafe { PDFDocument::new() };
        let mut position = 0;
        for (i, source) in sources.iter().enumerate() {
            cancel.check()?;
            let failure = || Error::ImageInput { index: i + 1 };
            autoreleasepool(|_| {
                let bytes = match source {
                    ImageSource::Path(path) => {
                        Cow::Owned(std::fs::read(path).map_err(|_| failure())?)
                    }
                    ImageSource::Bytes(bytes) => Cow::Borrowed(*bytes),
                };
                let data = NSData::with_bytes(&bytes);
                let image = NSImage::initWithData(NSImage::alloc(), &data)
                    .filter(|image| image.isValid())
                    .ok_or_else(failure)?;
                // 只有 TIFF 的多个 representation 是独立页。其他格式可能用它保存同一图片的不同大小。
                let is_tiff = matches!(
                    bytes.get(..4),
                    Some(b"II\x2a\x00" | b"MM\x00\x2a" | b"II\x2b\x00" | b"MM\x00\x2b")
                );
                let frames = is_tiff
                    .then(|| image.representations())
                    .filter(|frames| frames.count() > 1);
                let count = frames.as_ref().map_or(1, |frames| frames.count());
                for index in 0..count {
                    cancel.check()?;
                    autoreleasepool(|_| -> Result<(), Error> {
                        let frame = frames.as_ref().map(|frames| {
                            let rep = frames.objectAtIndex(index);
                            let frame = NSImage::initWithSize(NSImage::alloc(), rep.size());
                            frame.addRepresentation(&rep);
                            frame
                        });
                        let image = frame.as_deref().unwrap_or(&image);
                        // SAFETY: AppKit 已解码图片。页面和输出文档均在当前线程，position 为尾部索引。
                        unsafe {
                            let page = PDFPage::initWithImage(PDFPage::alloc(), image)
                                .ok_or_else(failure)?;
                            output.insertPage_atIndex(&page, position);
                        }
                        position += 1;
                        Ok(())
                    })?;
                }
                Ok::<_, Error>(())
            })?;
        }
        cancel.check()?;
        // SAFETY: 文档及其页面还在所属线程上，返回的数据会复制到 Rust。
        let bytes = unsafe { output.dataRepresentation() }.map(|data| data.to_vec());
        cancel.check()?;
        bytes.ok_or(Error::SaveFailed)
    })
}
