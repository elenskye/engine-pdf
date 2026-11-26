//! lopdf 这一层：读（带交叉引用表修复）、写、跨文件搬对象，以及把 CoreGraphics 画的东西做成 Form XObject。

use std::collections::{HashMap, HashSet};

use lopdf::{Dictionary, Document, LoadOptions, Object, ObjectId, Stream, dictionary};
use objc2_core_foundation::{CFMutableData, CGRect};
use objc2_core_graphics::{
    CGContext, CGDataConsumer, CGPDFContextBeginPage, CGPDFContextClose, CGPDFContextCreate,
    CGPDFContextEndPage,
};

use crate::{Cancel, Error};

/// 读入 PDF 对象。lopdf 读不了、PDFKit 能读的文件（如知网的 `xref 0 133` 写法、错位的偏移）
/// 先按对象扫描重建交叉引用表再读。
pub(super) fn load(bytes: &[u8]) -> Result<Document, Error> {
    load_with(bytes, LoadOptions::default)
}

pub(super) fn load_with(
    bytes: &[u8],
    options: impl Fn() -> LoadOptions,
) -> Result<Document, Error> {
    match Document::load_mem_with_options(bytes, options()) {
        Ok(mut document) => {
            fill_unsized_streams(&mut document, bytes);
            Ok(document)
        }
        Err(_) => repair(bytes)
            .and_then(|bytes| {
                let mut document = Document::load_mem_with_options(&bytes, options()).ok()?;
                fill_unsized_streams(&mut document, &bytes);
                Some(document)
            })
            .ok_or(Error::Damaged),
    }
}

/// lopdf 遇到没有 `/Length`（或长度读不出）的流只记下起点、内容留空，原样写回就丢了数据
/// （PDFKit 写的 CFF 字体就没有 `/Length`）。这里读到 `endstream` 为止补上。
fn fill_unsized_streams(document: &mut Document, bytes: &[u8]) {
    let body = bytes
        .windows(5)
        .position(|window| window == b"%PDF-")
        .map_or(bytes, |start| &bytes[start..]);
    let missing: Vec<_> = document
        .objects
        .iter()
        .filter_map(|(id, object)| match object {
            Object::Stream(stream)
                if stream.content.is_empty()
                    && stream.start_position.is_some()
                    && stream
                        .dict
                        .get(b"Length")
                        .and_then(|length| document.dereference(length))
                        .and_then(|(_, length)| length.as_i64())
                        .is_err() =>
            {
                Some(*id)
            }
            _ => None,
        })
        .collect();
    for id in missing {
        let Some(Object::Stream(stream)) = document.objects.get_mut(&id) else {
            continue;
        };
        let Some(start) = stream.start_position else {
            continue;
        };
        let Some(mut end) = find(body, b"endstream", start) else {
            continue;
        };
        // 流数据后面的一个换行不算内容。
        if body[..end].ends_with(b"\r\n") {
            end -= 2;
        } else if body[..end].ends_with(b"\n") || body[..end].ends_with(b"\r") {
            end -= 1;
        }
        if end >= start {
            stream.set_content(body[start..end].to_vec());
        }
    }
}

/// 读入不加密的 PDF，供改写。
pub(super) fn load_plain(bytes: &[u8]) -> Result<Document, Error> {
    let document = load(bytes).map_err(|_| Error::SaveFailed)?;
    if document.is_encrypted() || document.encryption_state.is_some() {
        return Err(Error::SaveFailed);
    }
    Ok(document)
}

pub(super) fn save(document: &mut Document, cancel: &Cancel) -> Result<Vec<u8>, Error> {
    cancel.check()?;
    let mut bytes = Vec::new();
    document
        .save_to(&mut bytes)
        .map_err(|_| Error::SaveFailed)?;
    cancel.check()?;
    Ok(bytes)
}

/// 按 `N G obj` 扫描全部对象，在文件尾追加一份新的交叉引用表和 trailer。
fn repair(bytes: &[u8]) -> Option<Vec<u8>> {
    let start = bytes.windows(5).position(|window| window == b"%PDF-")?;
    let body = &bytes[start..];
    let mut offsets = HashMap::new();
    let mut index = 0;
    while index < body.len() {
        let Some((number, generation, end)) = object_header(body, index) else {
            index += 1;
            continue;
        };
        offsets.insert(number, (index, generation));
        // 跳过流数据，二进制里偶尔会出现像对象头的字节。
        let close = find(body, b"endobj", end);
        let stream = close.and_then(|close| find(&body[..close], b"stream", end));
        index = match stream {
            Some(stream) => find(body, b"endstream", stream).map_or(body.len(), |close| close + 9),
            None => end,
        };
    }
    let trailer = rfind(body, b"trailer")?;
    let dictionary = balanced_dictionary(body, trailer + 7)?;
    let dictionary = strip_keys(dictionary, &[b"/Prev", b"/XRefStm"]);
    let size = offsets.keys().max()? + 1;

    let mut output = body.to_vec();
    output.push(b'\n');
    let xref = output.len();
    output.extend_from_slice(format!("xref\n0 {size}\n").as_bytes());
    for number in 0..size {
        let entry = match offsets.get(&number) {
            Some((offset, generation)) => format!("{offset:010} {generation:05} n\r\n"),
            None => "0000000000 65535 f\r\n".to_owned(),
        };
        output.extend_from_slice(entry.as_bytes());
    }
    output.extend_from_slice(b"trailer\n");
    output.extend_from_slice(&dictionary);
    output.extend_from_slice(format!("\nstartxref\n{xref}\n%%EOF\n").as_bytes());
    Some(output)
}

fn object_header(body: &[u8], index: usize) -> Option<(u32, u16, usize)> {
    if index > 0
        && !matches!(
            body[index - 1],
            b'\n' | b'\r' | b' ' | b'\t' | b'>' | b'\x0c'
        )
    {
        return None;
    }
    let digits = |from: usize| {
        let end = body[from..]
            .iter()
            .position(|byte| !byte.is_ascii_digit())
            .map_or(body.len(), |length| from + length);
        (end > from).then_some(end)
    };
    let spaces = |from: usize| {
        let end = body[from..]
            .iter()
            .position(|byte| !matches!(byte, b' ' | b'\t' | b'\r' | b'\n'))
            .map_or(body.len(), |length| from + length);
        (end > from).then_some(end)
    };
    let number_end = digits(index)?;
    let generation_start = spaces(number_end)?;
    let generation_end = digits(generation_start)?;
    let keyword = spaces(generation_end)?;
    if !body[keyword..].starts_with(b"obj")
        || body
            .get(keyword + 3)
            .is_some_and(|byte| byte.is_ascii_alphanumeric())
    {
        return None;
    }
    let number = std::str::from_utf8(&body[index..number_end])
        .ok()?
        .parse()
        .ok()?;
    let generation = std::str::from_utf8(&body[generation_start..generation_end])
        .ok()?
        .parse()
        .ok()?;
    Some((number, generation, keyword + 3))
}

fn find(body: &[u8], pattern: &[u8], from: usize) -> Option<usize> {
    body.get(from..)?
        .windows(pattern.len())
        .position(|window| window == pattern)
        .map(|position| from + position)
}

fn rfind(body: &[u8], pattern: &[u8]) -> Option<usize> {
    body.windows(pattern.len())
        .rposition(|window| window == pattern)
}

fn balanced_dictionary(body: &[u8], from: usize) -> Option<&[u8]> {
    let start = find(body, b"<<", from)?;
    let mut depth = 0;
    let mut index = start;
    while index + 1 < body.len() {
        match &body[index..index + 2] {
            b"<<" => {
                depth += 1;
                index += 2;
            }
            b">>" => {
                depth -= 1;
                index += 2;
                if depth == 0 {
                    return Some(&body[start..index]);
                }
            }
            _ => index += 1,
        }
    }
    None
}

/// 去掉 `/Prev 123` 这类整数项，免得新表又指回读不了的旧表。
fn strip_keys(dictionary: &[u8], keys: &[&[u8]]) -> Vec<u8> {
    let mut output = dictionary.to_vec();
    for key in keys {
        while let Some(start) = find(&output, key, 0) {
            let mut end = start + key.len();
            while end < output.len() && matches!(output[end], b' ' | b'\t' | b'\r' | b'\n') {
                end += 1;
            }
            while end < output.len() && output[end].is_ascii_digit() {
                end += 1;
            }
            output.drain(start..end);
        }
    }
    output
}

/// 把一份文件里的对象连同它引用到的对象搬进另一份文件。
///
/// 引用到页、页树、Catalog 的对象不搬：事先用 [`Importer::seed`] 对应好的页换成输出里的页，
/// 其余换成 `null`，免得把整份文件拖进来。
pub(super) struct Importer<'a> {
    from: &'a Document,
    map: HashMap<ObjectId, ObjectId>,
    pending: Vec<(ObjectId, ObjectId)>,
}

impl<'a> Importer<'a> {
    pub(super) fn new(from: &'a Document) -> Self {
        Self {
            from,
            map: HashMap::new(),
            pending: Vec::new(),
        }
    }

    pub(super) fn seed(&mut self, old: ObjectId, new: ObjectId) {
        self.map.insert(old, new);
    }

    /// 改写 `object` 里的引用，引用到的对象排进待搬列表；调 [`Importer::finish`] 才真正搬。
    pub(super) fn remap(&mut self, object: &mut Object, into: &mut Document) {
        match object {
            Object::Reference(old) => {
                if let Some(&new) = self.map.get(old) {
                    *old = new;
                } else if self.structural(*old) {
                    *object = Object::Null;
                } else {
                    let new = into.new_object_id();
                    self.map.insert(*old, new);
                    self.pending.push((*old, new));
                    *old = new;
                }
            }
            Object::Array(items) => {
                for item in items {
                    self.remap(item, into);
                }
            }
            Object::Dictionary(dictionary)
            | Object::Stream(Stream {
                dict: dictionary, ..
            }) => {
                for (_, value) in dictionary.iter_mut() {
                    self.remap(value, into);
                }
            }
            _ => {}
        }
    }

    pub(super) fn finish(&mut self, into: &mut Document, cancel: &Cancel) -> Result<(), Error> {
        while let Some((old, new)) = self.pending.pop() {
            cancel.check()?;
            let mut object = self.from.objects.get(&old).cloned().unwrap_or(Object::Null);
            self.remap(&mut object, into);
            into.objects.insert(new, object);
        }
        Ok(())
    }

    fn structural(&self, id: ObjectId) -> bool {
        self.from
            .objects
            .get(&id)
            .and_then(|object| object.type_name().ok())
            .is_some_and(|kind| matches!(kind, b"Page" | b"Pages" | b"Catalog"))
    }
}

/// 用 CoreGraphics 在 `media` 大小的单页 PDF 上画，交回画出内容的 Form XObject（已搬进 `into`）。
pub(super) fn drawn_form(
    into: &mut Document,
    media: CGRect,
    cancel: &Cancel,
    draw: impl FnOnce(&CGContext),
) -> Result<ObjectId, Error> {
    let data = CFMutableData::new(None, 0).ok_or(Error::SaveFailed)?;
    let consumer = CGDataConsumer::with_cf_data(Some(&data)).ok_or(Error::SaveFailed)?;
    // SAFETY: consumer 与 media 在调用期间有效，PDF context 只在当前线程绘制。
    let context =
        unsafe { CGPDFContextCreate(Some(&consumer), &media, None) }.ok_or(Error::SaveFailed)?;
    // SAFETY: context 刚创建且没有打开的页面，沿用 media 作页面大小。
    unsafe { CGPDFContextBeginPage(Some(&context), None) };
    draw(&context);
    CGPDFContextEndPage(Some(&context));
    CGPDFContextClose(Some(&context));
    cancel.check()?;

    let source = Document::load_mem(&data.to_vec()).map_err(|_| Error::SaveFailed)?;
    let page_id = *source.get_pages().get(&1).ok_or(Error::SaveFailed)?;
    let page = source
        .get_dictionary(page_id)
        .map_err(|_| Error::SaveFailed)?;
    let media = inherited(&source, page, b"MediaBox").ok_or(Error::SaveFailed)?;
    let resources = inherited(&source, page, b"Resources")
        .cloned()
        .unwrap_or_else(|| Object::Dictionary(Dictionary::new()));
    let contents = source
        .get_page_content(page_id)
        .map_err(|_| Error::SaveFailed)?;
    let mut form = Stream::new(
        dictionary! {
            "Type" => "XObject",
            "Subtype" => "Form",
            "BBox" => media.clone(),
            "Resources" => resources,
        },
        contents,
    );
    form.compress().map_err(|_| Error::SaveFailed)?;
    let mut form = Object::Stream(form);
    let mut importer = Importer::new(&source);
    importer.remap(&mut form, into);
    importer.finish(into, cancel)?;
    if source.version > into.version {
        into.version.clone_from(&source.version);
    }
    Ok(into.add_object(form))
}

/// 取字典里的值，跟随间接引用；`null` 当作没有。
pub(super) fn value<'a>(
    document: &'a Document,
    dictionary: &'a Dictionary,
    key: &[u8],
) -> Option<&'a Object> {
    let (_, value) = document.dereference(dictionary.get(key).ok()?).ok()?;
    (!matches!(value, Object::Null)).then_some(value)
}

pub(super) fn array<'a>(
    document: &'a Document,
    dictionary: &'a Dictionary,
    key: &[u8],
) -> Option<&'a [Object]> {
    value(document, dictionary, key)?
        .as_array()
        .ok()
        .map(Vec::as_slice)
}

pub(super) fn number(document: &Document, value: &Object) -> Option<f32> {
    document.dereference(value).ok()?.1.as_float().ok()
}

/// 页对象上可继承的项（`Resources`、`MediaBox`、`CropBox`、`Rotate`），沿页树向上找，已跟随引用。
pub(super) fn inherited<'a>(
    document: &'a Document,
    mut node: &'a Dictionary,
    key: &[u8],
) -> Option<&'a Object> {
    let mut seen = HashSet::new();
    loop {
        if let Some(value) = value(document, node, key) {
            return Some(value);
        }
        let parent = node.get(b"Parent").ok()?.as_reference().ok()?;
        if !seen.insert(parent) {
            return None;
        }
        node = document.get_dictionary(parent).ok()?;
    }
}

/// 页面 `MediaBox` 的左下角。
pub(super) fn media_origin(document: &Document, page: &Dictionary) -> Option<(f32, f32)> {
    let [x0, y0, x1, y1] = inherited(document, page, b"MediaBox")?
        .as_array()
        .ok()?
        .as_slice()
    else {
        return None;
    };
    let [x0, y0, x1, y1] = [x0, y0, x1, y1].map(|value| number(document, value));
    Some((x0?.min(x1?), y0?.min(y1?)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repairs_a_cnki_style_xref_header() {
        let objects = [
            "<< /Type /Catalog /Pages 2 0 R >>",
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 100 100] >>",
        ];
        let mut pdf = b"%PDF-1.6\n".to_vec();
        let mut offsets = Vec::new();
        for (index, object) in objects.iter().enumerate() {
            offsets.push(pdf.len());
            pdf.extend_from_slice(format!("{} 0 obj\n{object}\nendobj\n", index + 1).as_bytes());
        }
        let xref = pdf.len();
        // 知网的写法：子段头和 xref 在同一行，文件尾另有附加数据。
        pdf.extend_from_slice(b"xref 0 4\r\n0000000000 65535 f\r\n");
        for offset in offsets {
            pdf.extend_from_slice(format!("{offset:010} 00000 n\r\n").as_bytes());
        }
        pdf.extend_from_slice(
            format!("trailer\n<< /Size 4 /Root 1 0 R /Prev 9 >>\nstartxref\n{xref}\n%%EOF\nWebFastLoad <x/>")
                .as_bytes(),
        );
        assert!(Document::load_mem(&pdf).is_err());
        let document = load(&pdf).unwrap();
        assert_eq!(document.get_pages().len(), 1);
    }

    #[test]
    fn recovers_streams_without_length() {
        // PDFKit 写 CFF 字体时就不带 `/Length`；lopdf 会把内容读成空。
        let data = b"\x01\x00\x04binary font data\n";
        let mut pdf = b"%PDF-1.7\n".to_vec();
        let mut offsets = Vec::new();
        for object in [
            b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
            b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),
            b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 10 10] /Resources << /X 4 0 R >> >>"
                .to_vec(),
            [
                b"<< /Subtype /CIDFontType0C >>\nstream\n".as_slice(),
                data,
                b"endstream",
            ]
            .concat(),
        ] {
            offsets.push(pdf.len());
            pdf.extend_from_slice(format!("{} 0 obj\n", offsets.len()).as_bytes());
            pdf.extend_from_slice(&object);
            pdf.extend_from_slice(b"\nendobj\n");
        }
        let xref = pdf.len();
        pdf.extend_from_slice(b"xref\n0 5\n0000000000 65535 f\r\n");
        for offset in offsets {
            pdf.extend_from_slice(format!("{offset:010} 00000 n\r\n").as_bytes());
        }
        pdf.extend_from_slice(
            format!("trailer\n<< /Size 5 /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n").as_bytes(),
        );
        let raw = Document::load_mem(&pdf).unwrap();
        assert!(
            raw.get_object((4, 0))
                .unwrap()
                .as_stream()
                .unwrap()
                .content
                .is_empty()
        );
        let document = load(&pdf).unwrap();
        let stream = document.get_object((4, 0)).unwrap().as_stream().unwrap();
        assert_eq!(stream.content, &data[..data.len() - 1]);
    }
}
