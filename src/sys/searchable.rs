use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::fmt::Write;

use lopdf::content::{Content as PdfContent, Operation};
use lopdf::{
    Dictionary, Document as PdfObjects, Object, ObjectId, Stream, StringFormat, dictionary,
};
use objc2_core_graphics::CGAffineTransformInvert;
use objc2_pdf_kit::PDFDisplayBox;

use crate::ocr::{self, CharacterSpans};
use crate::{Cancel, Content, Document, Error, OcrOptions, OcrText, ReadOptions};

use super::{objects, pdf::OCR_TEXT_KEY};

// 引擎自制的占位字体（`scripts/ocr-font.py` 生成）。两个字形分别用于空白与普通字符，实际文字由 ToUnicode 提供。
const OCR_FONT: &[u8] = include_bytes!("ocr.ttf");

pub(crate) fn searchable(
    doc: &Document,
    pages: &[u32],
    opts: &OcrOptions,
    cancel: &Cancel,
    progress: &dyn Fn(u32),
) -> Result<Vec<u8>, Error> {
    cancel.check()?;
    let total = doc.page_count();
    if let Some(&page) = pages.iter().find(|&&page| page == 0 || page > total) {
        return Err(Error::PageOutOfRange { page, total });
    }
    if pages.is_empty() {
        return Ok(doc.original_bytes().to_vec());
    }
    let native = doc.native();
    // SAFETY: 只在文档所属线程读取其加密状态。
    if unsafe { native.native().isEncrypted() } {
        return Err(if native.owner_verified() {
            Error::PasswordActionRequired
        } else {
            Error::OwnerPasswordRequired
        });
    }

    let read = ReadOptions {
        render: None,
        text_min_chars: 1,
        ..ReadOptions::default()
    };
    let mut output = None;
    let mut seen = HashSet::new();
    for &index in pages {
        cancel.check()?;
        if !seen.insert(index) {
            continue;
        }
        let (_, text, embedded) = doc.page(index)?;
        let content = if embedded {
            Content::SearchableText(text)
        } else {
            crate::read::classify(doc, index, text, &read, cancel)?
        };
        let recognized = if matches!(content, Content::NoText { .. }) {
            cancel.check()?;
            let observations = native.ocr(index, opts, true);
            cancel.check()?;
            Some(ocr::assemble(observations?))
        } else {
            None
        };
        if let Some((result, spans)) = recognized.filter(|(text, _)| !text.lines.is_empty()) {
            let (pdf, pages, font) = match &mut output {
                Some(state) => state,
                slot @ None => {
                    let mut pdf = objects::load_plain(doc.original_bytes())?;
                    let pages = pdf.get_pages();
                    let font = add_font_file(&mut pdf);
                    slot.insert((pdf, pages, font))
                }
            };
            let page = *pages.get(&index).ok_or(Error::SaveFailed)?;
            let transform = native.with_page(index, |page| {
                // SAFETY: 页面仍归当前线程。OCR 与写入都以 CropBox 的显示坐标为基准。
                Ok(CGAffineTransformInvert(unsafe {
                    page.transformForBox(PDFDisplayBox::CropBox)
                }))
            })?;
            add_layer(
                pdf,
                page,
                *font,
                &result,
                &spans,
                [transform.a, transform.b, transform.c, transform.d],
            )?;
        }
        progress(index);
    }
    cancel.check()?;
    match output {
        Some((mut pdf, _, _)) => objects::save(&mut pdf, cancel),
        None => Ok(doc.original_bytes().to_vec()),
    }
}

pub(super) fn add_font_file(pdf: &mut PdfObjects) -> ObjectId {
    pdf.add_object(Stream::new(
        dictionary! { "Length1" => OCR_FONT.len() as i64 },
        OCR_FONT.to_vec(),
    ))
}

pub(super) fn add_layer(
    pdf: &mut PdfObjects,
    page_id: ObjectId,
    font_file: ObjectId,
    ocr: &OcrText,
    spans: &[CharacterSpans],
    [a, b, c, d]: [f64; 4],
) -> Result<(), Error> {
    let chars: BTreeSet<_> = ocr
        .lines
        .iter()
        .flat_map(|line| line.text.chars())
        .collect();
    let codes = chars
        .into_iter()
        .zip(1_usize..)
        .map(|(ch, id)| Ok((ch, u16::try_from(id).map_err(|_| Error::SaveFailed)?)))
        .collect::<Result<BTreeMap<_, _>, Error>>()?;
    let font = add_font(pdf, font_file, &codes)?;

    let page = pdf.get_dictionary(page_id).map_err(|_| Error::SaveFailed)?;
    let mut resources = objects::inherited(pdf, page, b"Resources")
        .and_then(|value| value.as_dict().ok())
        .cloned()
        .unwrap_or_default();
    let mut fonts = objects::value(pdf, &resources, b"Font")
        .and_then(|value| value.as_dict().ok())
        .cloned()
        .unwrap_or_default();
    let mut name = format!("EnginePDFOCR{}", font.0).into_bytes();
    while fonts.has(&name) {
        name.push(b'_');
    }
    fonts.set(name.clone(), font);
    resources.set("Font", fonts);
    let original = page.get(b"Contents").ok();
    let mut contents = objects::array(pdf, page, b"Contents")
        .map(<[Object]>::to_vec)
        .unwrap_or_else(|| original.into_iter().cloned().collect());

    let mut ops = vec![
        Operation::new("q", vec![]),
        Operation::new("BT", vec![]),
        Operation::new("Tf", vec![Object::Name(name), 1.into()]),
        Operation::new("Tr", vec![3.into()]),
    ];
    for (line, characters) in ocr.lines.iter().zip(spans) {
        let bounds = line.bounds;
        let upright = a.abs() > 0.5;
        let (width, height) = if upright {
            (bounds.width, bounds.height)
        } else {
            (bounds.height, bounds.width)
        };
        if width <= 0.0 || height <= 0.0 || line.text.is_empty() {
            continue;
        }
        let count = line.text.chars().count();
        let x = if a < -0.5 || c < -0.5 {
            bounds.x + bounds.width
        } else {
            bounds.x
        };
        let y = if b < -0.5 || d < -0.5 {
            bounds.y + bounds.height
        } else {
            bounds.y
        };
        let mut write_text = |left: f64, advance: f64, encoded| {
            let matrix = [
                a * advance,
                b * advance,
                c * height,
                d * height,
                x + a * left + c * 0.15 * height,
                y + b * left + d * 0.15 * height,
            ];
            ops.push(Operation::new(
                "Tm",
                matrix.map(|value| Object::Real(value as f32)).into(),
            ));
            ops.push(Operation::new(
                "Tj",
                vec![Object::String(encoded, StringFormat::Hexadecimal)],
            ));
        };
        if characters.is_empty() {
            let encoded = line
                .text
                .chars()
                .flat_map(|ch| codes[&ch].to_be_bytes())
                .collect();
            write_text(0.0, width / count as f64, encoded);
        } else {
            for (ch, &(left, right)) in line.text.chars().zip(characters) {
                write_text(
                    left * width,
                    (right - left) * width,
                    codes[&ch].to_be_bytes().into(),
                );
            }
        }
    }
    ops.extend([Operation::new("ET", vec![]), Operation::new("Q", vec![])]);
    let encoded = (PdfContent { operations: ops })
        .encode()
        .map_err(|_| Error::SaveFailed)?;
    // 先画不可见层，恢复图形状态后再运行原内容流，原内容末尾的 CTM 或裁切不会影响 OCR。
    contents.insert(0, add_stream(pdf, encoded)?);
    let text = add_stream(pdf, ocr.text.as_bytes().to_vec())?;
    let page = pdf
        .get_object_mut(page_id)
        .and_then(Object::as_dict_mut)
        .map_err(|_| Error::SaveFailed)?;
    page.set("Resources", resources);
    page.set("Contents", contents);
    page.set(OCR_TEXT_KEY.to_bytes(), text);
    Ok(())
}

fn add_font(
    pdf: &mut PdfObjects,
    font_file: ObjectId,
    codes: &BTreeMap<char, u16>,
) -> Result<ObjectId, Error> {
    let name = Object::Name(b"EnginePDFOCR".to_vec());
    let mut cmap = String::from(concat!(
        "/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n",
        "/CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n",
        "/CMapName /Adobe-Identity-UCS def\n/CMapType 2 def\n",
        "1 begincodespacerange\n<0000> <FFFF>\nendcodespacerange\n",
    ));
    let mut glyphs = vec![0, 0];
    let entries: Vec<_> = codes.iter().collect();
    for chunk in entries.chunks(100) {
        writeln!(cmap, "{} beginbfchar", chunk.len()).unwrap();
        for &(&ch, &code) in chunk {
            write!(cmap, "<{code:04X}> <").unwrap();
            for unit in ch.encode_utf16(&mut [0; 2]) {
                write!(cmap, "{unit:04X}").unwrap();
            }
            cmap.push_str(">\n");
            glyphs.extend([0, if ch.is_whitespace() { 1 } else { 2 }]);
        }
        cmap.push_str("endbfchar\n");
    }
    cmap.push_str("endcmap\nCMapName currentdict /CMap defineresource pop\nend\nend\n");
    let to_unicode = add_stream(pdf, cmap.into_bytes())?;
    let cid_map = add_stream(pdf, glyphs)?;
    let descriptor = pdf.add_object(dictionary! {
        "Type" => "FontDescriptor",
        "FontName" => name.clone(),
        "Flags" => 4,
        "FontBBox" => vec![0.into(), (-150).into(), 1000.into(), 850.into()],
        "Ascent" => 850, "Descent" => -150, "CapHeight" => 850,
        "ItalicAngle" => 0, "StemV" => 80, "FontFile2" => font_file,
    });
    let descendant = pdf.add_object(dictionary! {
        "Type" => "Font", "Subtype" => "CIDFontType2", "BaseFont" => name.clone(),
        "CIDSystemInfo" => dictionary! {
            "Registry" => Object::string_literal("Adobe"),
            "Ordering" => Object::string_literal("Identity"),
            "Supplement" => 0,
        },
        "FontDescriptor" => descriptor, "DW" => 1000, "CIDToGIDMap" => cid_map,
    });
    Ok(pdf.add_object(dictionary! {
        "Type" => "Font", "Subtype" => "Type0", "BaseFont" => name,
        "Encoding" => "Identity-H", "DescendantFonts" => vec![Object::Reference(descendant)],
        "ToUnicode" => to_unicode,
    }))
}

fn add_stream(pdf: &mut PdfObjects, bytes: Vec<u8>) -> Result<Object, Error> {
    let mut stream = Stream::new(Dictionary::new(), bytes);
    stream.compress().map_err(|_| Error::SaveFailed)?;
    Ok(Object::Reference(pdf.add_object(stream)))
}
