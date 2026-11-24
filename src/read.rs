use crate::{Cancel, Document, Error, Image, ImageFormat};

/// 没有可用文字层的页要不要在本机识别。v0.2.0 只有 `Off`，`Auto` 在 v0.3.0。
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Ocr {
    #[default]
    Off,
}

/// `read` / `read_each` 的选项。默认：不开 OCR、不出图，阈值见各字段。
#[derive(Clone, Copy, Debug)]
pub struct ReadOptions {
    pub ocr: Ocr,
    /// 要不要附页图，见 [`Render`]。
    pub render: Option<Render>,
    /// 文字层至少要有这么多个非空白字符才算能代表整页（默认 20，沿用 Peck、Pith）。
    pub text_min_chars: usize,
    /// 深色像素占比严格低于该比例时判为空白，取值 0–1。
    pub blank_max_dark_fraction: f64,
    /// 可疑字符占比严格高于该比例时判为乱码，取值 0–1。
    pub garbled_min_fraction: f64,
}

impl Default for ReadOptions {
    fn default() -> Self {
        Self {
            ocr: Ocr::Off,
            render: None,
            text_min_chars: 20,
            blank_max_dark_fraction: 0.002,
            garbled_min_fraction: 0.20,
        }
    }
}

/// 附页图的设置：格式、最长边、只给哪些页、最多几张。
#[derive(Clone, Copy, Debug)]
pub struct Render {
    format: ImageFormat,
    max_side: u32,
    only_no_text: bool,
    limit: usize,
}

impl Render {
    pub fn jpeg(max_side: u32) -> Self {
        Self::new(ImageFormat::Jpeg, max_side)
    }

    pub fn png(max_side: u32) -> Self {
        Self::new(ImageFormat::Png, max_side)
    }

    pub fn gray(max_side: u32) -> Self {
        Self::new(ImageFormat::Gray, max_side)
    }

    fn new(format: ImageFormat, max_side: u32) -> Self {
        Self {
            format,
            max_side,
            only_no_text: false,
            limit: usize::MAX,
        }
    }

    /// 仅给 `Content::NoText` 的页附图。
    pub fn only_no_text(mut self) -> Self {
        self.only_no_text = true;
        self
    }

    /// 本次读取最多附上 `limit` 张图。
    pub fn at_most(mut self, limit: usize) -> Self {
        self.limit = limit;
        self
    }
}

/// 读出来的一页。
#[derive(Debug)]
pub struct Page {
    /// 原文件的物理页码，从 1 起。引用、存库一律用它。
    pub index: u32,
    /// 页标签，显示用。见 [`Document::label`]。
    pub label: Option<String>,
    pub content: Content,
    pub image: Option<Image>,
}

/// 一页读出来是什么。
#[derive(Debug)]
pub enum Content {
    /// 文字层就能代表这一页：字数够、不是乱码；或者页面上除了这几个字别无他物（只有页码、一句标题的页）。
    Text(String),
    /// 空白页：没有文字层，页面几乎全白。
    Blank,
    /// 页面上有画面，但没取到能代表它的字（扫描页、图、只有几个字的图文页）。
    /// `partial` 是文字层里那几个字（可能为空；乱码时为空），App 可以和页图一起用。
    NoText {
        reason: NoTextReason,
        partial: String,
    },
    /// 这一页读失败，其他页照常。
    Failed(Error),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NoTextReason {
    /// 文字层太少或没有，且没开 OCR。
    OcrOff,
    /// 文字层是乱码（字体没带文字对照表），且没开 OCR。
    Garbled,
}

/// `read_each` 回调的返回值：接着读，或到此为止。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Flow {
    Continue,
    Stop,
}

/// `read_each` 读完（或停下）时的计数。
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Summary {
    pub processed: usize,
    pub text: usize,
    pub blank: usize,
    pub no_text: usize,
    pub failed: usize,
    pub images: usize,
}

/// 按传入顺序收集结果，允许重复页码。页数多时用 [`read_each`]。
pub fn read(
    doc: &Document,
    pages: &[u32],
    opts: &ReadOptions,
    cancel: &Cancel,
) -> Result<Vec<Page>, Error> {
    let mut results = Vec::new();
    read_each(doc, pages, opts, cancel, |page| {
        results.push(page);
        Flow::Continue
    })?;
    Ok(results)
}

/// 按传入顺序逐页调用回调，允许重复页码。越界和其他单页错误以 `Content::Failed` 交付。
/// 回调可返回 [`Flow::Stop`] 提前结束并取回摘要。
pub fn read_each(
    doc: &Document,
    pages: &[u32],
    opts: &ReadOptions,
    cancel: &Cancel,
    mut on_page: impl FnMut(Page) -> Flow,
) -> Result<Summary, Error> {
    cancel.check()?;
    let mut summary = Summary::default();
    for &index in pages {
        cancel.check()?;
        let (label, mut content) = match doc.page(index) {
            Ok((label, text)) => (
                label,
                classify(doc, index, text, opts).unwrap_or_else(Content::Failed),
            ),
            Err(error) => (None, Content::Failed(error)),
        };

        let mut image = None;
        if let Some(render) = opts.render {
            if summary.images < render.limit
                && !matches!(content, Content::Failed(_))
                && (!render.only_no_text || matches!(content, Content::NoText { .. }))
            {
                match doc.render(index, render.format, render.max_side) {
                    Ok(rendered) => {
                        image = Some(rendered);
                        summary.images += 1;
                    }
                    Err(error) => content = Content::Failed(error),
                }
            }
        }

        summary.processed += 1;
        match &content {
            Content::Text(_) => summary.text += 1,
            Content::Blank => summary.blank += 1,
            Content::NoText { .. } => summary.no_text += 1,
            Content::Failed(_) => summary.failed += 1,
        }
        if on_page(Page {
            index,
            label,
            content,
            image,
        }) == Flow::Stop
        {
            break;
        }
    }
    Ok(summary)
}

fn classify(
    doc: &Document,
    index: u32,
    text: String,
    opts: &ReadOptions,
) -> Result<Content, Error> {
    let (total, garbled) = text_quality(&text, opts.garbled_min_fraction);
    if total >= opts.text_min_chars.max(1) && !garbled {
        return Ok(Content::Text(text));
    }

    let gray = doc.render(index, ImageFormat::Gray, 64)?;
    let dark = gray.bytes.iter().filter(|&&value| value < 240).count();
    if (dark as f64 / gray.bytes.len() as f64) < opts.blank_max_dark_fraction {
        // 页面几乎全白：有正常的字就是一张只有这几个字的页，没有就是空白页。
        return Ok(if total > 0 && !garbled {
            Content::Text(text)
        } else {
            Content::Blank
        });
    }

    match opts.ocr {
        Ocr::Off => Ok(if garbled {
            Content::NoText {
                reason: NoTextReason::Garbled,
                partial: String::new(),
            }
        } else {
            Content::NoText {
                reason: NoTextReason::OcrOff,
                partial: text,
            }
        }),
    }
}

// 字符范围沿用 Pith 的乱码检查。
fn text_quality(text: &str, garbled_min_fraction: f64) -> (usize, bool) {
    let (mut total, mut bad) = (0, 0);
    for c in text.chars().filter(|c| !c.is_whitespace()) {
        total += 1;
        if c == '\u{fffd}' || c.is_control() || ('\u{e000}'..='\u{f8ff}').contains(&c) {
            bad += 1;
        }
    }
    (
        total,
        total > 0 && (bad as f64 / total as f64) > garbled_min_fraction,
    )
}

#[cfg(test)]
mod tests {
    use super::text_quality;

    #[test]
    fn text_quality_uses_pith_character_classes_and_strict_threshold() {
        assert_eq!(text_quality("字 a b c \u{fffd}\n", 0.20), (5, false));
        assert_eq!(text_quality("字 \u{e000} \u{1}", 0.20), (3, true));
        assert_eq!(text_quality("\t\n", 0.20), (0, false));
    }
}
