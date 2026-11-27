use std::ptr::null_mut;

use objc2::rc::{Retained, autoreleasepool};
use objc2::{AnyThread, msg_send};
use objc2_core_foundation::{CGPoint, CGRect, CGSize};
use objc2_core_graphics::{
    CGAffineTransformInvert, CGBitmapContextCreateImage, CGContext, CGImage,
    CGRectApplyAffineTransform,
};
use objc2_foundation::{NSArray, NSDictionary, NSError, NSRange, NSString};
use objc2_pdf_kit::{PDFDisplayBox, PDFPage};
use objc2_vision::{
    VNImageRequestHandler, VNRecognizeTextRequest, VNRecognizedTextObservation,
    VNRectangleObservation, VNRequest, VNRequestTextRecognitionLevel,
};

use crate::ocr::Observation;
use crate::{Error, ImageFormat, OcrBounds, OcrLine, OcrOptions};

use super::render::{draw_page, white_bitmap};

pub(super) enum Positions {
    None,
    Characters,
    Search,
}

pub(super) fn recognize_page(
    page: &PDFPage,
    index: u32,
    options: &OcrOptions,
    positions: Positions,
) -> Result<Vec<Observation>, Error> {
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
        let mut spans = Vec::new();
        if !matches!(positions, Positions::None) && visual.size.width > 0.0 {
            let mut utf16 = 0;
            let mut previous_end: f64 = 0.0;
            for (position, ch) in text.chars().enumerate() {
                let length = ch.len_utf16();
                let range = NSRange::new(utf16, length);
                utf16 += length;
                if ch.is_whitespace() {
                    continue;
                }
                // SAFETY: UTF-16 范围来自 candidate 自己的字符串，错误出参可为空。
                let rect: Option<Retained<VNRectangleObservation>> = unsafe {
                    msg_send![&*candidate, boundingBoxForRange: range, error: null_mut::<*mut NSError>()]
                };
                let Some(rect) = rect else {
                    continue;
                };
                // SAFETY: rect 是 Vision 为该字符串范围返回的观察对象。
                let rect = unsafe { rect.boundingBox() };
                if rect.size.width <= 0.0 {
                    continue;
                }
                let start = (rect.origin.x - visual.origin.x) / visual.size.width;
                let end = start + rect.size.width / visual.size.width;
                // Vision 有时只给词框。查找可接受相邻字符共用同一框，写入文字层仍要求独立字框。
                if spans.last().is_some_and(|&(left, right)| {
                    let shared_word = matches!(positions, Positions::Search)
                        && position == spans.len()
                        && (start, end) == (left, right);
                    start <= left && !shared_word
                }) {
                    spans.clear();
                    break;
                }
                let missing = position - spans.len();
                let step = (start - previous_end).max(0.0001) / missing.max(1) as f64;
                for _ in 0..missing {
                    spans.push((previous_end, previous_end + step));
                    previous_end += step;
                }
                spans.push((start, end));
                previous_end = end;
            }
            if !spans.is_empty() {
                let missing = text.chars().count() - spans.len();
                let step = (1.0 - previous_end).max(0.0001) / missing.max(1) as f64;
                for _ in 0..missing {
                    spans.push((previous_end, previous_end + step));
                    previous_end += step;
                }
            }
        }
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
            spans,
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
