use std::ptr;

use objc2::{AnyThread, DefinedClass, define_class, msg_send, rc::Retained};
use objc2_app_kit::{NSBitmapImageFileType, NSBitmapImageRep};
use objc2_core_graphics::{
    CGBitmapContextCreate, CGBitmapContextCreateImage, CGColorSpace, CGContext, CGImage,
    CGImageAlphaInfo,
};
use objc2_foundation::{NSCopying, NSData, NSDictionary, NSObject, NSRect};
use objc2_pdf_kit::{PDFAnnotation, PDFAnnotationSubtypeStamp, PDFDisplayBox};

use crate::edit::StampFormat;

define_class!(
    // SAFETY: PDFKit 支持覆盖 PDFAnnotation 的绘制方法。图像在初始化时保留，析构无需额外操作。
    #[unsafe(super(PDFAnnotation, NSObject))]
    #[name = "EnginePdfRasterStamp"]
    #[ivars = Retained<CGImage>]
    struct RasterStamp;

    impl RasterStamp {
        #[unsafe(method(drawWithBox:inContext:))]
        fn draw(&self, _display_box: PDFDisplayBox, context: &CGContext) {
            // SAFETY: PDFKit 在当前文档的绘制回调中提供有效的上下文和标注。
            let bounds = unsafe { self.bounds() };
            CGContext::draw_image(Some(context), bounds, Some(self.ivars()));
        }
    }
);

/// 让 PDFKit 从图像绘制标准 Stamp 外观流，保留透明像素。
pub(super) fn new(
    rect: NSRect,
    format: StampFormat,
    bytes: &[u8],
) -> Option<Retained<PDFAnnotation>> {
    let header: &[u8] = match format {
        StampFormat::Png => b"\x89PNG\r\n\x1a\n",
        StampFormat::Jpeg => b"\xff\xd8\xff",
    };
    if !bytes.starts_with(header) {
        return None;
    }
    let data = NSData::with_bytes(bytes);
    let bitmap = NSBitmapImageRep::initWithData(NSBitmapImageRep::alloc(), &data)?;
    let image = bitmap.CGImage()?;
    let stamp = RasterStamp::alloc().set_ivars(image);
    // SAFETY: 标注的 ivar 已初始化。调用所属父类的构造方法，不传附加属性。
    let stamp: Option<Retained<RasterStamp>> = unsafe {
        msg_send![super(stamp), initWithBounds: rect, forType: PDFAnnotationSubtypeStamp,
            withProperties: None::<&NSDictionary>]
    };
    stamp.map(Retained::into_super)
}

/// 将图章的现有外观画到透明 PNG，供应用重建标注。
pub(super) fn appearance(annotation: &PDFAnnotation) -> Option<Vec<u8>> {
    // 分离页对象，避免 PDFKit 把原页的旋转和裁切偏移应用到图章局部坐标。
    let annotation = annotation.copy();
    // SAFETY: 标注副本在当前线程使用，持有独立的外观数据引用。
    let bounds = unsafe { annotation.bounds() };
    let (x, y) = (bounds.origin.x, bounds.origin.y);
    let (w, h) = (bounds.size.width, bounds.size.height);
    if !x.is_finite() || !y.is_finite() || !w.is_finite() || !h.is_finite() || w <= 0.0 || h <= 0.0
    {
        return None;
    }
    let scale = (1568.0 / w.max(h)).min(2.0);
    let width = (w * scale).round().max(1.0) as usize;
    let height = (h * scale).round().max(1.0) as usize;
    let space = CGColorSpace::new_device_rgb()?;
    // SAFETY: 空数据指针让 CoreGraphics 管理位图，色彩空间与预乘 alpha 布局匹配。
    let bitmap = unsafe {
        CGBitmapContextCreate(
            ptr::null_mut(),
            width,
            height,
            8,
            0,
            Some(&space),
            CGImageAlphaInfo::PremultipliedLast.0,
        )
    }?;
    CGContext::scale_ctm(Some(&bitmap), width as f64 / w, height as f64 / h);
    CGContext::translate_ctm(Some(&bitmap), -x, -y);
    // SAFETY: 标注与上下文都在当前线程使用，绘制时资源保持存活。
    unsafe { annotation.drawWithBox_inContext(PDFDisplayBox::CropBox, &bitmap) };
    let image = CGBitmapContextCreateImage(Some(&bitmap))?;
    let rep = NSBitmapImageRep::initWithCGImage(NSBitmapImageRep::alloc(), &image);
    // SAFETY: PNG 编码不需要初始化参数，空字典符合接口约定。
    unsafe {
        rep.representationUsingType_properties(NSBitmapImageFileType::PNG, &NSDictionary::new())
    }
    .map(|data| data.to_vec())
}
