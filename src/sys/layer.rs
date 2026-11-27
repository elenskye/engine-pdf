//! PDFium 取的一页字和字形框（开了 `pdfium` 才有）：补连字的参照、PDFKit 读不出时的后备、
//! 收紧偏高的行框（`decisions/0021`、`0022`）。另有在两份取字结果之间对位的工具。

use std::ops::Range;

use crate::edit::Rect;

pub(super) struct Layer {
    pub(super) text: String,
    /// 每个字在 `text` 里的起点和字形框；空白没有框。
    glyphs: Vec<(usize, Option<Rect>)>,
}

impl Layer {
    #[cfg(feature = "pdfium")]
    pub(super) fn of(pdf: &pdfium_render::prelude::PdfDocument<'_>, page: u32) -> Option<Self> {
        let page = pdf.pages().get(i32::try_from(page - 1).ok()?).ok()?;
        let native = page.text().ok()?;
        let mut text = String::new();
        let mut glyphs = Vec::new();
        for glyph in native.chars().iter() {
            let Some(ch) = glyph.unicode_char().filter(|ch| *ch != '\0') else {
                continue;
            };
            let bounds = if ch.is_whitespace() {
                None
            } else {
                glyph
                    .tight_bounds()
                    .ok()
                    .map(super::pdfium::rect)
                    .filter(|rect| valid_rect(*rect))
            };
            glyphs.push((text.len(), bounds));
            text.push(ch);
        }
        Some(Self { text, glyphs })
    }

    pub(super) fn cost(&self) -> usize {
        self.text.len() + self.glyphs.len() * std::mem::size_of::<(usize, Option<Rect>)>()
    }

    /// 另一份取字结果里的区间换到本层：顺序相同按字对位，不同就找两边都唯一的原样片段。
    pub(super) fn map(&self, original: &str, range: Range<usize>) -> Option<Range<usize>> {
        align(original, &self.text)
            .and_then(|pairs| mapped_range(&pairs, &self.text, range.clone()))
            .or_else(|| unique_range(original, &self.text, range))
    }

    pub(super) fn locate(&self, original: &str, range: Range<usize>) -> Option<Vec<Rect>> {
        let rects = self.rects(self.map(original, range)?);
        (!rects.is_empty()).then_some(rects)
    }

    /// 区间里的字按行合成框。
    pub(super) fn rects(&self, range: Range<usize>) -> Vec<Rect> {
        let mut result = Vec::new();
        let mut line: Option<Rect> = None;
        for &(offset, bounds) in self.glyphs_in(range) {
            match bounds {
                Some(rect) => line = Some(line.map_or(rect, |line| union(line, rect))),
                None if self.text[offset..].starts_with(['\r', '\n']) => result.extend(line.take()),
                None => {}
            }
        }
        result.extend(line);
        result
    }

    /// 区间经过的每一行里所有字的上下沿（底、顶），用来判断 PDFKit 的行框是不是高出一截。
    pub(super) fn line_extents(&self, range: Range<usize>) -> Vec<(f64, f64)> {
        let breaks = |at: usize| self.text[..at].rfind(['\r', '\n']).map_or(0, |i| i + 1);
        let line_start = breaks(range.start);
        let line_end = self.text[range.end..]
            .find(['\r', '\n'])
            .map_or(self.text.len(), |i| range.end + i);
        self.rects(line_start..line_end)
            .into_iter()
            .map(|rect| (rect.y, rect.y + rect.height))
            .collect()
    }

    fn glyphs_in(&self, range: Range<usize>) -> impl Iterator<Item = &(usize, Option<Rect>)> {
        let start = self
            .glyphs
            .partition_point(|(offset, _)| *offset < range.start);
        self.glyphs[start..]
            .iter()
            .take_while(move |(offset, _)| *offset < range.end)
    }
}

fn union(a: Rect, b: Rect) -> Rect {
    let x = a.x.min(b.x);
    let y = a.y.min(b.y);
    Rect {
        x,
        y,
        width: (a.x + a.width).max(b.x + b.width) - x,
        height: (a.y + a.height).max(b.y + b.height) - y,
    }
}

pub(super) fn valid_rect(rect: Rect) -> bool {
    [rect.x, rect.y, rect.width, rect.height]
        .into_iter()
        .all(f64::is_finite)
        && rect.width > 0.0
        && rect.height > 0.0
}

/// 对齐取字顺序相同、空白不同的两份文字。每对记录非空白字符的 UTF-8 开头。
pub(super) fn align(left: &str, right: &str) -> Option<Vec<(usize, usize)>> {
    let mut rhs = right.char_indices().filter(|(_, c)| !c.is_whitespace());
    let pairs = left
        .char_indices()
        .filter(|(_, c)| !c.is_whitespace())
        .map(|(left, ch)| {
            let (right, other) = rhs.next()?;
            (ch == other).then_some((left, right))
        })
        .collect::<Option<Vec<_>>>()?;
    rhs.next().is_none().then_some(pairs)
}

pub(super) fn mapped_range(
    alignment: &[(usize, usize)],
    target: &str,
    range: Range<usize>,
) -> Option<Range<usize>> {
    let start = alignment.partition_point(|&(offset, _)| offset < range.start);
    let end = alignment.partition_point(|&(offset, _)| offset < range.end);
    if end <= start {
        return None;
    }
    let first = alignment[start].1;
    let last = alignment[end - 1].1;
    Some(first..last + target[last..].chars().next()?.len_utf8())
}

/// 阅读顺序不同时，用两边都只出现一次的片段对位（比较时不看空白，两边换行写法不同）。
pub(super) fn unique_range(
    original: &str,
    target: &str,
    range: Range<usize>,
) -> Option<Range<usize>> {
    let fragment = original[range]
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect::<Vec<_>>();
    if fragment.is_empty() {
        return None;
    }
    let solid = |text: &str| {
        text.char_indices()
            .filter(|(_, c)| !c.is_whitespace())
            .collect::<Vec<_>>()
    };
    let occurrences = |chars: &[(usize, char)]| {
        chars
            .windows(fragment.len())
            .enumerate()
            .filter(|(_, window)| window.iter().map(|(_, c)| *c).eq(fragment.iter().copied()))
            .map(|(at, _)| at)
            .take(2)
            .collect::<Vec<_>>()
    };
    let target_chars = solid(target);
    let [at] = occurrences(&target_chars)[..] else {
        return None;
    };
    if occurrences(&solid(original)).len() != 1 {
        return None;
    }
    let (start, _) = target_chars[at];
    let (last, ch) = target_chars[at + fragment.len() - 1];
    Some(start..last + ch.len_utf8())
}

#[cfg(test)]
mod tests {
    use super::{Layer, Rect};

    fn glyph(x: f64, y: f64, h: f64) -> Option<Rect> {
        Some(Rect {
            x,
            y,
            width: 10.0,
            height: h,
        })
    }

    #[test]
    fn maps_unique_fragments_across_different_line_breaks() {
        let original = "其他\n种族差异\n导致政治效能感";
        let target = "导致政治效能感\r\n其他 种族差异\r\n导致政治效";
        let at = original.find('种').unwrap();
        let end = original.find("能感").unwrap();
        // 「种族差异导致政治效」在目标里只出现一次，空白写法不同也能对上。
        let mapped = super::unique_range(original, target, at..end).unwrap();
        assert_eq!(&target[mapped], "种族差异\r\n导致政治效");
        // 「导致政治效」在目标里有两处，不对位。
        let at = original.find('导').unwrap();
        assert_eq!(super::unique_range(original, target, at..end), None);
    }

    #[test]
    fn groups_glyphs_by_line_and_measures_whole_lines() {
        // 两行：「甲乙」在上，「丙丁」在下；第二行的「丁」偏高一点。
        let text = "甲乙\r\n丙丁".to_owned();
        let offsets = text.char_indices().map(|(at, _)| at).collect::<Vec<_>>();
        let boxes = [
            glyph(0.0, 100.0, 10.0),
            glyph(10.0, 100.0, 10.0),
            None,
            None,
            glyph(0.0, 85.0, 10.0),
            glyph(10.0, 85.0, 11.0),
        ];
        let layer = Layer {
            glyphs: offsets.into_iter().zip(boxes).collect(),
            text,
        };
        let second = layer.text.find('丙').unwrap();
        assert_eq!(layer.rects(0..layer.text.len()).len(), 2);
        assert_eq!(layer.line_extents(second..second + 3), [(85.0, 96.0)]);
        let first = layer.text.find('乙').unwrap();
        assert_eq!(
            layer.line_extents(first..second + 3),
            [(100.0, 110.0), (85.0, 96.0)]
        );
    }
}
