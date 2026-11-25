use objc2::AnyThread;
use objc2::rc::{Retained, autoreleasepool};
use objc2_core_foundation::{CGPoint, CGRect, CGSize};
use objc2_core_graphics::{
    CGAffineTransformInvert, CGBitmapContextCreateImage, CGContext, CGImage,
    CGRectApplyAffineTransform,
};
use objc2_foundation::{NSArray, NSDictionary, NSString};
use objc2_pdf_kit::{PDFDisplayBox, PDFPage};
use objc2_vision::{
    VNImageRequestHandler, VNRecognizeTextRequest, VNRecognizedTextObservation, VNRequest,
    VNRequestTextRecognitionLevel,
};

use crate::{Error, ImageFormat, OcrBounds, OcrLine, OcrOptions};

use super::render::{draw_page, white_bitmap};

pub(super) fn recognize_page(
    page: &PDFPage,
    index: u32,
    options: &OcrOptions,
) -> Result<Vec<(OcrLine, OcrBounds)>, Error> {
    let (bitmap, display_size) = draw_page(page, index, ImageFormat::Gray, 2400)?;
    let image = CGBitmapContextCreateImage(Some(&bitmap)).ok_or(Error::Render { page: index })?;
    let observations = recognize(&image, index, options)?;
    // SAFETY: 页面在文档所属线程使用，CropBox 与绘图使用的显示框相同。
    let display_to_pdf =
        CGAffineTransformInvert(unsafe { page.transformForBox(PDFDisplayBox::CropBox) });

    let mut lines = Vec::new();
    for observation in observations.iter() {
        let Some(candidate) = observation.topCandidates(1).firstObject() else {
            continue;
        };
        let text = candidate.string().to_string();
        if text.trim().is_empty() {
            continue;
        }
        // SAFETY: Vision 已经完成请求，observation 是请求结果里的有效对象。
        let visual = unsafe { observation.boundingBox() };
        let display = CGRect::new(
            CGPoint::new(
                visual.origin.x * display_size.width,
                visual.origin.y * display_size.height,
            ),
            CGSize::new(
                visual.size.width * display_size.width,
                visual.size.height * display_size.height,
            ),
        );
        let pdf = CGRectApplyAffineTransform(display, display_to_pdf);
        lines.push((
            OcrLine {
                text,
                confidence: candidate.confidence(),
                bounds: OcrBounds {
                    x: pdf.origin.x,
                    y: pdf.origin.y,
                    width: pdf.size.width,
                    height: pdf.size.height,
                },
            },
            OcrBounds {
                x: visual.origin.x,
                y: visual.origin.y,
                width: visual.size.width,
                height: visual.size.height,
            },
        ));
    }
    Ok(lines)
}

pub(crate) fn warm_up(options: &OcrOptions) -> Result<(), Error> {
    autoreleasepool(|_| {
        let error = || Error::Ocr { page: 1 };
        let bitmap = white_bitmap(150, 64, ImageFormat::Gray).ok_or_else(error)?;
        CGContext::set_gray_fill_color(Some(&bitmap), 0.0, 1.0);
        // 画一行字，让 Vision 实际走到文字识别，加载模型。
        let glyphs = [
            " ###   ###  #### ",
            "#   # #   # #   #",
            "#   # #     #   #",
            "#   # #     #### ",
            "#   # #     # #  ",
            "#   # #   # #  # ",
            " ###   ###  #   #",
        ];
        for (row, glyphs) in glyphs.iter().rev().enumerate() {
            for (column, pixel) in glyphs.bytes().enumerate() {
                if pixel == b'#' {
                    CGContext::fill_rect(
                        Some(&bitmap),
                        CGRect::new(
                            CGPoint::new(16.0 + column as f64 * 6.0, 16.0 + row as f64 * 5.0),
                            CGSize::new(6.0, 5.0),
                        ),
                    );
                }
            }
        }
        let image = CGBitmapContextCreateImage(Some(&bitmap)).ok_or_else(error)?;
        recognize(&image, 1, options)?;
        Ok(())
    })
}

fn recognize(
    image: &CGImage,
    page: u32,
    options: &OcrOptions,
) -> Result<Retained<NSArray<VNRecognizedTextObservation>>, Error> {
    let request = VNRecognizeTextRequest::new();
    let languages: Retained<NSArray<NSString>> = options
        .languages
        .iter()
        .map(|language| NSString::from_str(language))
        .collect();
    request.setRecognitionLevel(VNRequestTextRecognitionLevel::Accurate);
    request.setUsesLanguageCorrection(true);
    request.setRecognitionLanguages(&languages);
    if options.languages.len() > 1 && objc2::available!(macos = 13.0) {
        request.setAutomaticallyDetectsLanguage(true);
    }
    // SAFETY: 图像在同步请求结束前有效且不写入，图像选项为空。
    let handler = unsafe {
        VNImageRequestHandler::initWithCGImage_options(
            VNImageRequestHandler::alloc(),
            image,
            &NSDictionary::new(),
        )
    };
    let requests = NSArray::<VNRequest>::from_slice(&[&request]);
    handler
        .performRequests_error(&requests)
        .map_err(|_| Error::Ocr { page })?;
    request.results().ok_or(Error::Ocr { page })
}
