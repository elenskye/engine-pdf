use std::{ptr, slice};

use objc2::{AnyThread, runtime::AnyObject};
use objc2_app_kit::{NSBitmapImageFileType, NSBitmapImageRep, NSImageCompressionFactor};
use objc2_core_foundation::{CGPoint, CGRect, CGSize};
use objc2_core_graphics::{
    CGBitmapContextCreate, CGBitmapContextCreateImage, CGBitmapContextGetBytesPerRow,
    CGBitmapContextGetData, CGColorSpace, CGContext, CGImageAlphaInfo,
};
use objc2_foundation::{NSDictionary, NSNumber, NSString};
use objc2_pdf_kit::{PDFDisplayBox, PDFPage};

use crate::{Error, Image, ImageFormat};

pub(super) fn render(
    page: &PDFPage,
    index: u32,
    format: ImageFormat,
    max_side: u32,
) -> Result<Image, Error> {
    let error = || Error::Render { page: index };
    // 画 CropBox：阅读器（Preview、PDF.js）显示的就是它，裁掉的边角不画。
    // SAFETY: 页面在文档所属线程使用，CropBox 是有效的显示框（没写时 PDFKit 用 MediaBox）。
    let (bounds, rotation) =
        unsafe { (page.boundsForBox(PDFDisplayBox::CropBox), page.rotation()) };
    let (mut w, mut h) = (bounds.size.width, bounds.size.height);
    if max_side == 0 || !w.is_finite() || !h.is_finite() || w <= 0.0 || h <= 0.0 {
        return Err(error());
    }
    if rotation.rem_euclid(180) == 90 {
        std::mem::swap(&mut w, &mut h);
    }
    let long_side = w.max(h);
    let width = (w / long_side * f64::from(max_side)).round().max(1.0) as u32;
    let height = (h / long_side * f64::from(max_side)).round().max(1.0) as u32;
    let (space, alpha) = match format {
        ImageFormat::Gray => (CGColorSpace::new_device_gray(), CGImageAlphaInfo::None),
        ImageFormat::Jpeg | ImageFormat::Png => (
            CGColorSpace::new_device_rgb(),
            CGImageAlphaInfo::NoneSkipLast,
        ),
    };
    let space = space.ok_or_else(error)?;
    // SAFETY: 空指针让 CoreGraphics 分配位图和选取行跨度。色彩空间和 alpha 布局匹配。
    let bitmap = unsafe {
        CGBitmapContextCreate(
            ptr::null_mut(),
            width as usize,
            height as usize,
            8,
            0,
            Some(&space),
            alpha.0,
        )
    }
    .ok_or_else(error)?;
    CGContext::set_gray_fill_color(Some(&bitmap), 1.0, 1.0);
    CGContext::fill_rect(
        Some(&bitmap),
        CGRect::new(CGPoint::ZERO, CGSize::new(width.into(), height.into())),
    );
    CGContext::scale_ctm(Some(&bitmap), f64::from(width) / w, f64::from(height) / h);
    // SAFETY: 位图和页面均在当前线程有效。PDFKit 绘制时应用 CropBox 原点、页面旋转和注释。
    unsafe { page.drawWithBox_toContext(PDFDisplayBox::CropBox, &bitmap) };

    let bytes = match format {
        ImageFormat::Gray => {
            let stride = CGBitmapContextGetBytesPerRow(Some(&bitmap));
            let len = stride
                .checked_mul(height as usize)
                .filter(|&len| len <= isize::MAX as usize)
                .ok_or_else(error)?;
            let data = CGBitmapContextGetData(Some(&bitmap)).cast::<u8>();
            if data.is_null() || stride < width as usize {
                return Err(error());
            }
            // SAFETY: 位图拥有从首行开始的 stride × height 字节。读取时位图仍存活且不会写入。
            let data = unsafe { slice::from_raw_parts(data, len) };
            data.chunks_exact(stride)
                .flat_map(|row| row[..width as usize].iter().copied())
                .collect()
        }
        ImageFormat::Jpeg | ImageFormat::Png => {
            let cg_image = CGBitmapContextCreateImage(Some(&bitmap)).ok_or_else(error)?;
            let image = NSBitmapImageRep::initWithCGImage(NSBitmapImageRep::alloc(), &cg_image);
            let (kind, props) = if matches!(format, ImageFormat::Jpeg) {
                let quality = NSNumber::numberWithDouble(0.85);
                // SAFETY: AppKit 提供该键，压缩质量为 0 到 1 的 NSNumber。
                let props = NSDictionary::<NSString, AnyObject>::from_slices(
                    &[unsafe { NSImageCompressionFactor }],
                    &[&quality],
                );
                (NSBitmapImageFileType::JPEG, props)
            } else {
                (NSBitmapImageFileType::PNG, NSDictionary::new())
            };
            // SAFETY: JPEG 字典只含有效的压缩质量，PNG 使用空字典。
            unsafe { image.representationUsingType_properties(kind, &props) }
                .ok_or_else(error)?
                .to_vec()
        }
    };
    Ok(Image {
        format,
        width,
        height,
        bytes,
    })
}
