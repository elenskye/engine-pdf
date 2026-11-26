use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{AnyThread, DefinedClass, define_class, msg_send};
use objc2_app_kit::{
    NSAttributedStringNSExtendedStringDrawing, NSAttributedStringNSStringDrawing, NSBezierPath,
    NSBezierPathElement, NSColor, NSColorSpace, NSFont, NSFontAttributeName,
    NSForegroundColorAttributeName, NSGraphicsContext, NSStringDrawingOptions,
};
use objc2_core_graphics::CGContext;
use objc2_foundation::{
    NSArray, NSAttributedString, NSDictionary, NSNumber, NSObject, NSPoint, NSRect, NSSize,
    NSString, NSValue, ns_string,
};
use objc2_pdf_kit::{
    PDFActionURL, PDFAnnotation, PDFAnnotationSubtypeCircle, PDFAnnotationSubtypeFreeText,
    PDFAnnotationSubtypeHighlight, PDFAnnotationSubtypeInk, PDFAnnotationSubtypeLine,
    PDFAnnotationSubtypeSquare, PDFAnnotationSubtypeStrikeOut, PDFAnnotationSubtypeText,
    PDFAnnotationSubtypeUnderline, PDFBorder, PDFDisplayBox, PDFDocument, PDFLineStyle,
    kPDFDestinationUnspecifiedValue,
};

use crate::Error;
use crate::edit::{
    Annotation, AnnotationKind, Color, Mark, MarkId, MarkKind, Point, Quad, Rect, Shape,
    StampFormat, Stroke, Target,
};

use super::{Pdf, edit::destination_page};

struct FreeTextAppearance {
    text: Retained<NSAttributedString>,
    border: Option<Stroke>,
}

define_class!(
    // SAFETY: PDFKit 支持自定义标注绘制。字体和文字由 ivar 保留，绘制只使用当前回调的上下文。
    #[unsafe(super(PDFAnnotation, NSObject))]
    #[name = "EnginePdfFreeText"]
    #[ivars = FreeTextAppearance]
    struct FreeText;

    impl FreeText {
        #[unsafe(method(drawWithBox:inContext:))]
        fn draw(&self, _display_box: PDFDisplayBox, context: &CGContext) {
            // SAFETY: PDFKit 在所属线程回调并提供有效标注和图形上下文。
            let bounds = unsafe { self.bounds() };
            let inset = |d: f64| NSRect::new(
                NSPoint::new(bounds.origin.x + d, bounds.origin.y + d),
                NSSize::new(bounds.size.width - 2.0 * d, bounds.size.height - 2.0 * d),
            );
            let style = self.ivars();
            let border_width = style.border.map_or(0.0, |border| border.width);
            let cg = Some(context);
            CGContext::save_g_state(cg);
            CGContext::clip_to_rect(cg, bounds);
            let text_rect = inset((border_width + 1.0).max(2.0));
            // PDF.js 按自己的字宽把框定得刚好放下文字：这里不自动换行，放不下就整体缩小。
            let measured = style.text.size();
            if text_rect.size.width > 0.0
                && text_rect.size.height > 0.0
                && measured.width > 0.0
                && measured.height > 0.0
            {
                let scale = (text_rect.size.width / measured.width)
                    .min(text_rect.size.height / measured.height)
                    .min(1.0);
                CGContext::save_g_state(cg);
                CGContext::translate_ctm(cg, text_rect.origin.x, text_rect.origin.y + text_rect.size.height);
                CGContext::scale_ctm(cg, scale, scale);
                NSGraphicsContext::saveGraphicsState_class();
                NSGraphicsContext::setCurrentContext(Some(&NSGraphicsContext::graphicsContextWithCGContext_flipped(
                    context, false,
                )));
                style.text.drawWithRect_options_context(
                    NSRect::new(
                        NSPoint::new(0.0, -measured.height),
                        NSSize::new(measured.width + 1.0, measured.height),
                    ),
                    NSStringDrawingOptions::UsesLineFragmentOrigin | NSStringDrawingOptions::UsesFontLeading,
                    None,
                );
                NSGraphicsContext::restoreGraphicsState_class();
                CGContext::restore_g_state(cg);
            }
            if let Some(border) = style.border.filter(|border| border.width > 0.0) {
                let color = border.color;
                CGContext::set_rgb_stroke_color(cg, color.red, color.green, color.blue, color.alpha);
                let stroke_rect = inset(border.width / 2.0);
                CGContext::stroke_rect_with_width(cg, stroke_rect, border.width);
            }
            CGContext::restore_g_state(cg);
        }
    }
);

pub(crate) fn annotations(pdf: &Pdf, page: u32) -> Result<Vec<Annotation>, Error> {
    pdf.with_page(page, |native_page| {
        // SAFETY: 文档和原生页在创建线程的 autorelease pool 内读取。
        unsafe {
            Ok(native_page
                .annotations()
                .iter()
                .filter(|annotation| editable(annotation))
                .enumerate()
                .map(|(index, annotation)| {
                    let subtype = annotation
                        .r#type()
                        .map(|name| name.to_string())
                        .unwrap_or_default();
                    let kind = match subtype.as_str() {
                        "Highlight" => AnnotationKind::Highlight,
                        "Underline" => AnnotationKind::Underline,
                        "StrikeOut" => AnnotationKind::Strikeout,
                        "Text" => AnnotationKind::Note,
                        "FreeText" => AnnotationKind::TextBox,
                        "Ink" => AnnotationKind::Ink,
                        "Square" => AnnotationKind::Rectangle,
                        "Circle" => AnnotationKind::Ellipse,
                        "Line" => {
                            let arrow = [annotation.startLineStyle(), annotation.endLineStyle()]
                                .into_iter()
                                .any(has_arrow);
                            if arrow {
                                AnnotationKind::Arrow
                            } else {
                                AnnotationKind::Line
                            }
                        }
                        "Stamp" => AnnotationKind::Image,
                        "Link" => AnnotationKind::Link {
                            target: read_link(pdf.native(), &annotation),
                        },
                        _ => AnnotationKind::Other { subtype },
                    };
                    let bounds = annotation.bounds();
                    let rect = Rect {
                        x: bounds.origin.x,
                        y: bounds.origin.y,
                        width: bounds.size.width,
                        height: bounds.size.height,
                    };
                    let color =
                        saved_color(&annotation, ns_string!("/EnginePDFColor")).or_else(|| {
                            match kind {
                                AnnotationKind::TextBox => annotation.fontColor(),
                                AnnotationKind::Image | AnnotationKind::Link { .. } => None,
                                _ => Some(annotation.color()),
                            }
                            .as_deref()
                            .and_then(read_color)
                            .map(|mut color| {
                                if let Some(alpha) = annotation
                                    .valueForAnnotationKey(ns_string!("/CA"))
                                    .and_then(|opacity| opacity.downcast::<NSNumber>().ok())
                                    .map(|opacity| opacity.doubleValue())
                                    .filter(|value| (0.0..=1.0).contains(value))
                                {
                                    color.alpha = alpha;
                                }
                                color
                            })
                        });
                    let contents = if matches!(&kind, AnnotationKind::TextBox) {
                        annotation
                            .valueForAnnotationKey(ns_string!("/Subj"))
                            .and_then(|subject| subject.downcast::<NSString>().ok())
                    } else {
                        annotation.contents()
                    };
                    let mark = read_mark(&annotation, &kind, rect, color);
                    Annotation {
                        id: MarkId {
                            source: 0,
                            page,
                            index: index as u32,
                        },
                        rect,
                        author: annotation.userName().map(|author| author.to_string()),
                        contents: contents.map(|contents| contents.to_string()),
                        color,
                        kind,
                        mark,
                    }
                })
                .collect())
        }
    })
}

/// 页上可编辑标注的种类（PDF 名称），顺序即 `MarkId::index`。
pub(super) fn editable_types(pdf: &Pdf, page: u32) -> Result<Vec<String>, Error> {
    pdf.with_page(page, |native| {
        // SAFETY: 页面和标注在所属线程的 autorelease pool 内只读。
        Ok(unsafe { native.annotations() }
            .iter()
            .filter(|annotation| editable(annotation))
            .map(|annotation| {
                // SAFETY: 同上。
                unsafe { annotation.r#type() }
                    .map(|name| name.to_string())
                    .unwrap_or_default()
            })
            .collect())
    })
}

pub(super) fn editable(annotation: &PDFAnnotation) -> bool {
    // SAFETY: 标注及类型属于当前文档线程；Popup 随主标注，Widget 交给表单接口。
    unsafe { annotation.r#type() }
        .as_deref()
        .is_none_or(|kind| kind != ns_string!("Popup") && kind != ns_string!("Widget"))
}

fn read_mark(
    annotation: &PDFAnnotation,
    kind: &AnnotationKind,
    rect: Rect,
    color: Option<Color>,
) -> Option<MarkKind> {
    let point = |local: NSPoint| Point {
        x: local.x + rect.x,
        y: local.y + rect.y,
    };
    // SAFETY: 原生标注及其颜色、边框、点列属于当前文档线程，读取时不改动对象。
    unsafe {
        let width = || annotation.border().map_or(1.0, |border| border.lineWidth());
        let stroke = || {
            Some(Stroke {
                width: width(),
                color: color?,
            })
        };
        Some(match kind {
            AnnotationKind::Highlight | AnnotationKind::Underline | AnnotationKind::Strikeout => {
                let points: Vec<_> = annotation
                    .quadrilateralPoints()?
                    .iter()
                    .map(|value| point(value.pointValue()))
                    .collect();
                let quads: Vec<_> = points
                    .chunks_exact(4)
                    .map(|quad| Quad {
                        top_left: quad[0],
                        top_right: quad[1],
                        bottom_left: quad[2],
                        bottom_right: quad[3],
                    })
                    .collect();
                if quads.is_empty() {
                    return None;
                }
                match kind {
                    AnnotationKind::Highlight => MarkKind::Highlight {
                        quads,
                        color: color?,
                    },
                    AnnotationKind::Underline => MarkKind::Underline {
                        quads,
                        color: color?,
                    },
                    _ => MarkKind::Strikeout {
                        quads,
                        color: color?,
                    },
                }
            }
            AnnotationKind::Note => MarkKind::Note { color: color? },
            AnnotationKind::TextBox => {
                let border = annotation
                    .border()
                    .filter(|border| border.lineWidth() > 0.0)
                    .map(|border| Stroke {
                        width: border.lineWidth(),
                        color: saved_color(annotation, ns_string!("/EnginePDFBorderColor"))
                            .unwrap_or(Color {
                                red: 0.0,
                                green: 0.0,
                                blue: 0.0,
                                alpha: 1.0,
                            }),
                    });
                MarkKind::TextBox {
                    text: annotation
                        .contents()
                        .map(|text| text.to_string())
                        .unwrap_or_default(),
                    font_size: annotation.font()?.pointSize(),
                    color: color?,
                    border,
                }
            }
            AnnotationKind::Ink => {
                let mut strokes = Vec::new();
                for path in annotation.paths()?.iter() {
                    let path = path.bezierPathByFlatteningPath();
                    let mut points = Vec::new();
                    for index in 0..path.elementCount() {
                        let mut element_points = [NSPoint::ZERO; 3];
                        match path
                            .elementAtIndex_associatedPoints(index, element_points.as_mut_ptr())
                        {
                            NSBezierPathElement::MoveTo => {
                                if points.len() > 1 {
                                    strokes.push(std::mem::take(&mut points));
                                }
                                points.clear();
                                points.push(point(element_points[0]));
                            }
                            NSBezierPathElement::LineTo => points.push(point(element_points[0])),
                            NSBezierPathElement::ClosePath => {
                                if let Some(&first) = points.first() {
                                    points.push(first);
                                }
                            }
                            _ => {}
                        }
                    }
                    if points.len() > 1 {
                        strokes.push(points);
                    }
                }
                if strokes.is_empty() {
                    return None;
                }
                MarkKind::Ink {
                    strokes,
                    stroke: stroke()?,
                }
            }
            AnnotationKind::Rectangle
            | AnnotationKind::Ellipse
            | AnnotationKind::Line
            | AnnotationKind::Arrow => {
                let shape = match kind {
                    AnnotationKind::Rectangle => Shape::Rectangle,
                    AnnotationKind::Ellipse => Shape::Ellipse,
                    AnnotationKind::Line => Shape::Line {
                        start: point(annotation.startPoint()),
                        end: point(annotation.endPoint()),
                    },
                    _ => {
                        let mut start = point(annotation.startPoint());
                        let mut end = point(annotation.endPoint());
                        if has_arrow(annotation.startLineStyle())
                            && !has_arrow(annotation.endLineStyle())
                        {
                            std::mem::swap(&mut start, &mut end);
                        }
                        Shape::Arrow { start, end }
                    }
                };
                let fill = saved_color(annotation, ns_string!("/EnginePDFFillColor"))
                    .or_else(|| annotation.interiorColor().as_deref().and_then(read_color));
                MarkKind::Shape {
                    shape,
                    stroke: stroke()?,
                    fill,
                }
            }
            AnnotationKind::Image => MarkKind::Image {
                format: StampFormat::Png,
                bytes: super::stamp::appearance(annotation)?,
            },
            AnnotationKind::Link { target } if *target != Target::None => MarkKind::Link {
                target: target.clone(),
            },
            AnnotationKind::Link { .. } | AnnotationKind::Other { .. } => return None,
        })
    }
}

fn has_arrow(style: PDFLineStyle) -> bool {
    matches!(style, PDFLineStyle::OpenArrow | PDFLineStyle::ClosedArrow)
}

fn read_color(color: &NSColor) -> Option<Color> {
    let color = color.colorUsingColorSpace(&NSColorSpace::sRGBColorSpace())?;
    Some(Color {
        red: color.redComponent(),
        green: color.greenComponent(),
        blue: color.blueComponent(),
        alpha: color.alphaComponent(),
    })
}

fn saved_color(annotation: &PDFAnnotation, key: &NSString) -> Option<Color> {
    // SAFETY: 标注、字符串和原生数组只在所属线程读取。
    let array = unsafe { annotation.valueForAnnotationKey(key) }?
        .downcast::<NSArray>()
        .ok()?;
    if array.count() != 4 {
        return None;
    }
    let mut components = [0.0; 4];
    for (component, value) in components.iter_mut().zip(array.iter()) {
        *component = value.downcast::<NSNumber>().ok()?.doubleValue();
        if !(0.0..=1.0).contains(component) {
            return None;
        }
    }
    let [red, green, blue, alpha] = components;
    Some(Color {
        red,
        green,
        blue,
        alpha,
    })
}

fn save_color(annotation: &PDFAnnotation, key: &NSString, color: Color) {
    let components =
        [color.red, color.green, color.blue, color.alpha].map(NSNumber::numberWithDouble);
    let components = NSArray::from_retained_slice(&components);
    // SAFETY: 颜色已验证。附加数组只携带读回参数，标准外观由 PDFKit 绘制并保存。
    unsafe { annotation.setValue_forAnnotationKey(&components, key) };
}

fn read_link(doc: &PDFDocument, annotation: &PDFAnnotation) -> Target {
    // SAFETY: 注释与目标都在文档所属线程只读。这里只读取页内位置或网址文字。
    unsafe {
        if let Some((destination, page)) = annotation.destination().and_then(|destination| {
            let page = destination_page(doc, &destination)?;
            Some((destination, page))
        }) {
            let mut y = destination.point().y;
            if !y.is_finite() || y == kPDFDestinationUnspecifiedValue {
                let Some(target_page) = destination.page() else {
                    return Target::None;
                };
                let bounds = target_page.boundsForBox(PDFDisplayBox::CropBox);
                y = bounds.origin.y + bounds.size.height;
            }
            return Target::Page { page, y };
        }
        if let Some(url) = annotation
            .action()
            .and_then(|action| action.downcast::<PDFActionURL>().ok())
            .and_then(|action| action.URL())
            .filter(|url| !url.isFileURL())
            .and_then(|url| url.absoluteString())
        {
            return Target::Url {
                url: url.to_string(),
            };
        }
        Target::None
    }
}

pub(super) fn create(mark: &Mark) -> Result<Retained<PDFAnnotation>, Error> {
    let rect = mark.rect;
    if ![
        rect.x,
        rect.y,
        rect.width,
        rect.height,
        rect.x + rect.width,
        rect.y + rect.height,
    ]
    .into_iter()
    .all(f64::is_finite)
        || rect.width <= 0.0
        || rect.height <= 0.0
    {
        return Err(Error::InvalidGeometry);
    }
    let bounds = NSRect::new(
        NSPoint::new(rect.x, rect.y),
        NSSize::new(rect.width, rect.height),
    );
    // SAFETY: 调用方在输出文档所属线程和 autorelease pool 内创建标注。各 setter
    // 依照标注种类使用，几何与颜色在传入原生 API 前检查。
    unsafe {
        let subtype = match &mark.kind {
            MarkKind::Highlight { .. } => PDFAnnotationSubtypeHighlight,
            MarkKind::Underline { .. } => PDFAnnotationSubtypeUnderline,
            MarkKind::Strikeout { .. } => PDFAnnotationSubtypeStrikeOut,
            MarkKind::Note { .. } => PDFAnnotationSubtypeText,
            MarkKind::TextBox { .. } => PDFAnnotationSubtypeFreeText,
            MarkKind::Ink { .. } => PDFAnnotationSubtypeInk,
            MarkKind::Shape { shape, .. } => match shape {
                Shape::Rectangle => PDFAnnotationSubtypeSquare,
                Shape::Ellipse => PDFAnnotationSubtypeCircle,
                Shape::Line { .. } | Shape::Arrow { .. } => PDFAnnotationSubtypeLine,
            },
            MarkKind::Image { .. } | MarkKind::Link { .. } => return Err(Error::Unsupported),
        };
        let annotation = match &mark.kind {
            MarkKind::TextBox {
                text,
                font_size,
                color,
                border,
            } => create_text_box(bounds, text, *font_size, *color, *border)?,
            _ => PDFAnnotation::initWithBounds_forType_withProperties(
                PDFAnnotation::alloc(),
                bounds,
                subtype,
                None,
            ),
        };
        annotation.setShouldDisplay(true);
        annotation.setShouldPrint(true);

        match &mark.kind {
            MarkKind::Highlight { quads, color }
            | MarkKind::Underline { quads, color }
            | MarkKind::Strikeout { quads, color } => {
                if quads.is_empty() {
                    return Err(Error::InvalidGeometry);
                }
                let mut points = Vec::with_capacity(quads.len() * 4);
                for quad in quads {
                    for point in [
                        quad.top_left,
                        quad.top_right,
                        quad.bottom_left,
                        quad.bottom_right,
                    ] {
                        points.push(NSValue::valueWithPoint(local_point(point, rect)?));
                    }
                }
                annotation.setQuadrilateralPoints(Some(&NSArray::from_retained_slice(&points)));
                annotation.setColor(&*native_color(*color)?);
                if color.alpha < 1.0 {
                    save_color(&annotation, ns_string!("/EnginePDFColor"), *color);
                }
            }
            MarkKind::Note { color } => {
                annotation.setColor(&*native_color(*color)?);
                if color.alpha < 1.0 {
                    save_color(&annotation, ns_string!("/EnginePDFColor"), *color);
                }
            }
            MarkKind::TextBox { .. } => {
                if let Some(contents) = &mark.contents {
                    annotation.setValue_forAnnotationKey(
                        &NSString::from_str(contents),
                        ns_string!("/Subj"),
                    );
                }
            }
            MarkKind::Ink { strokes, stroke } => {
                if strokes.is_empty() || stroke.width <= 0.0 {
                    return Err(Error::InvalidGeometry);
                }
                set_stroke(&annotation, *stroke)?;
                for points in strokes {
                    let [first, second, rest @ ..] = points.as_slice() else {
                        return Err(Error::InvalidGeometry);
                    };
                    let path = NSBezierPath::bezierPath();
                    path.moveToPoint(local_point(*first, rect)?);
                    path.lineToPoint(local_point(*second, rect)?);
                    for point in rest {
                        path.lineToPoint(local_point(*point, rect)?);
                    }
                    annotation.addBezierPath(&path);
                }
            }
            MarkKind::Shape {
                shape,
                stroke,
                fill,
            } => {
                set_stroke(&annotation, *stroke)?;
                let fill_color = fill.map(native_color).transpose()?;
                if let Some(fill) = fill_color.as_deref() {
                    annotation.setInteriorColor(Some(fill));
                }
                if let Some(fill) = fill.filter(|color| color.alpha < 1.0) {
                    save_color(&annotation, ns_string!("/EnginePDFFillColor"), fill);
                }
                match *shape {
                    Shape::Line { start, end } | Shape::Arrow { start, end } => {
                        annotation.setStartPoint(local_point(start, rect)?);
                        annotation.setEndPoint(local_point(end, rect)?);
                        if matches!(shape, Shape::Arrow { .. }) {
                            let style = if fill.is_some() {
                                PDFLineStyle::ClosedArrow
                            } else {
                                PDFLineStyle::OpenArrow
                            };
                            annotation.setEndLineStyle(style);
                        }
                    }
                    Shape::Rectangle | Shape::Ellipse => {}
                }
            }
            MarkKind::Image { .. } | MarkKind::Link { .. } => unreachable!(),
        }
        Ok(annotation)
    }
}

fn create_text_box(
    rect: NSRect,
    text: &str,
    font_size: f64,
    color: Color,
    border: Option<Stroke>,
) -> Result<Retained<PDFAnnotation>, Error> {
    if !font_size.is_finite() || font_size <= 0.0 {
        return Err(Error::InvalidGeometry);
    }
    // 与 PDF.js 的无衬线编辑框使用接近的字宽，保留原有换行。PDFKit（「预览」）按 `/DA` 的字体
    // 重画文本框、不用外观流：Helvetica 没有汉字会画成乱码，含中日文时改用苹方。
    let name = if text.chars().any(is_cjk) {
        ns_string!("PingFangSC-Regular")
    } else {
        ns_string!("Helvetica")
    };
    let font = NSFont::fontWithName_size(name, font_size)
        .unwrap_or_else(|| NSFont::systemFontOfSize(font_size));
    let font_color = native_color(color)?;
    let text = NSString::from_str(text);
    let native_border = border
        .map(|border| {
            let _ = native_color(border.color)?;
            native_border(border.width)
        })
        .transpose()?;
    // SAFETY: 字体和颜色属于当前线程。标准 FreeText 属性用于编辑和读回，绘制回调生成可移植外观。
    unsafe {
        let attributes = NSDictionary::<NSString, AnyObject>::from_slices(
            &[NSFontAttributeName, NSForegroundColorAttributeName],
            &[&font, &font_color],
        );
        let styled = NSAttributedString::initWithString_attributes(
            NSAttributedString::alloc(),
            &text,
            Some(&attributes),
        );
        let annotation = FreeText::alloc().set_ivars(FreeTextAppearance {
            text: styled,
            border,
        });
        let annotation: Option<Retained<FreeText>> = msg_send![super(annotation), initWithBounds: rect,
            forType: PDFAnnotationSubtypeFreeText, withProperties: None::<&NSDictionary>];
        let annotation = annotation.ok_or(Error::SaveFailed)?.into_super();
        annotation.setContents(Some(&text));
        annotation.setFont(Some(&font));
        annotation.setFontColor(Some(&font_color));
        annotation.setColor(&NSColor::clearColor());
        annotation.setBorder(native_border.as_deref());
        if color.alpha < 1.0 {
            save_color(&annotation, ns_string!("/EnginePDFColor"), color);
        }
        if let Some(border) = border {
            save_color(
                &annotation,
                ns_string!("/EnginePDFBorderColor"),
                border.color,
            );
        }
        Ok(annotation)
    }
}

fn is_cjk(c: char) -> bool {
    matches!(c,
        '\u{2E80}'..='\u{9FFF}' | '\u{F900}'..='\u{FAFF}' | '\u{FE30}'..='\u{FE4F}'
        | '\u{FF00}'..='\u{FFEF}' | '\u{20000}'..='\u{3FFFF}')
}

fn native_color(color: Color) -> Result<Retained<NSColor>, Error> {
    if ![color.red, color.green, color.blue, color.alpha]
        .into_iter()
        .all(|component| (0.0..=1.0).contains(&component))
    {
        return Err(Error::InvalidGeometry);
    }
    Ok(NSColor::colorWithSRGBRed_green_blue_alpha(
        color.red,
        color.green,
        color.blue,
        color.alpha,
    ))
}

fn local_point(point: Point, rect: Rect) -> Result<NSPoint, Error> {
    let (x, y) = (point.x - rect.x, point.y - rect.y);
    if !x.is_finite() || !y.is_finite() {
        return Err(Error::InvalidGeometry);
    }
    Ok(NSPoint::new(x, y))
}

fn native_border(width: f64) -> Result<Retained<PDFBorder>, Error> {
    if !width.is_finite() || width < 0.0 {
        return Err(Error::InvalidGeometry);
    }
    // SAFETY: 宽度已检查，边框由当前线程创建后添加到同线程的标注。
    unsafe {
        let border = PDFBorder::new();
        border.setLineWidth(width);
        Ok(border)
    }
}

fn set_stroke(annotation: &PDFAnnotation, stroke: Stroke) -> Result<(), Error> {
    let color = native_color(stroke.color)?;
    let border = native_border(stroke.width)?;
    // SAFETY: 颜色、边框和标注均在当前线程使用。
    unsafe {
        annotation.setColor(&color);
        annotation.setBorder(Some(&border));
    }
    if stroke.color.alpha < 1.0 {
        save_color(annotation, ns_string!("/EnginePDFColor"), stroke.color);
    }
    Ok(())
}
