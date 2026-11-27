//! PDFKit 在部分期刊的字体上把 fi、fl、ff、ffi、ffl 连字取成 `f`、空格或干脆丢掉
//! （`five` → `fve`，`significant` → `signi cant`，「预览」里也搜不到）。PDFium 取得对。
//! 这里拿 PDFium 同一页的字当参照，只把这些词换回来，其余一字不动、顺序照 PDFKit（`decisions/0022`）。

use std::collections::{HashMap, HashSet};
use std::ops::Range;

/// 补过连字的一页文字，记着每处替换，好把区间换回 PDFKit 原文（选区、字框按原文的下标取）。
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Repaired {
    pub(crate) text: String,
    /// 按位置排好：PDFKit 原文的区间 → 补过的文字里的区间。
    edits: Vec<(Range<usize>, Range<usize>)>,
}

impl Repaired {
    /// 补过的文字里的区间换成 PDFKit 原文的区间；落在换过的词里就取整个词。
    pub(crate) fn original_range(&self, range: Range<usize>) -> Range<usize> {
        let mut start = None;
        let mut shift = 0isize;
        for (original, repaired) in &self.edits {
            if start.is_none() {
                if range.start < repaired.start {
                    start = Some(range.start.saturating_add_signed(shift));
                } else if range.start < repaired.end {
                    start = Some(original.start);
                }
            }
            if range.end <= repaired.start {
                break;
            }
            if range.end <= repaired.end {
                return start.unwrap_or(original.start)..original.end;
            }
            shift = original.end as isize - repaired.end as isize;
        }
        let end = range.end.saturating_add_signed(shift);
        start.unwrap_or_else(|| range.start.saturating_add_signed(shift))..end
    }
}

/// 这一页能不能说明这份文件的字体丢不丢连字：参照里至少有几处连字才算数。
/// 交回 `Some(true)` 是 PDFKit 丢了，`Some(false)` 是没丢，`None` 是这页说明不了。
pub(crate) fn lost(kit: &str, reference: &str) -> Option<bool> {
    let expected = ligature_count(&expand(reference));
    (expected >= 3).then(|| ligature_count(&expand(kit)) < expected)
}

/// 参照里的连字比 PDFKit 原文多时，把 PDFKit 原文里取坏的词换成参照里的写法；没有要补的交回 `None`。
pub(crate) fn repair(kit: &str, reference: &str) -> Option<Repaired> {
    let reference = expand(reference);
    if ligature_count(&reference) <= ligature_count(&expand(kit)) {
        return None;
    }
    let words = tokens(&reference)
        .map(|(_, word)| word.to_lowercase())
        .collect::<Vec<_>>();
    let known: HashSet<&str> = words.iter().map(String::as_str).collect();
    let phrases: HashSet<String> = words
        .windows(2)
        .map(|pair| pair.join(" "))
        .chain(words.windows(3).map(|triple| triple.join(" ")))
        .collect();

    // 参照里每个带连字的词，PDFKit 可能取成的样子 → 原词。同一个样子对着两个词的不用。
    let mut broken: HashMap<String, Option<String>> = HashMap::new();
    for word in known.iter().filter(|word| ligature_count(word) > 0) {
        for form in broken_forms(word) {
            let taken = if form.contains(' ') {
                phrases.contains(&form)
            } else {
                known.contains(form.as_str())
            };
            if !taken && form.chars().filter(|c| c.is_alphabetic()).count() >= 2 {
                broken
                    .entry(form)
                    .and_modify(|seen| {
                        if seen.as_deref() != Some(*word) {
                            *seen = None;
                        }
                    })
                    .or_insert_with(|| Some((*word).to_owned()));
            }
        }
    }

    let kit_tokens = tokens(kit).collect::<Vec<_>>();
    let mut edits = Vec::new();
    let mut index = 0;
    while index < kit_tokens.len() {
        // 先试三个词（ffi 拆成两处空格）、两个词（中间正好一个空格），再试一个词。
        let found = (1..=3).rev().find_map(|span| {
            let group = kit_tokens.get(index..index + span)?;
            let spaced = group
                .windows(2)
                .all(|pair| &kit[pair[0].0.end..pair[1].0.start] == " ");
            if !spaced {
                return None;
            }
            let form = group
                .iter()
                .map(|(_, word)| word.to_lowercase())
                .collect::<Vec<_>>()
                .join(" ");
            let word = broken.get(&form)?.as_deref()?;
            let original = group[0].0.start..group[span - 1].0.end;
            Some((span, original, cased(word, &group[0].1)))
        });
        match found {
            Some((span, original, word)) => {
                edits.push((original, word));
                index += span;
            }
            None => index += 1,
        }
    }
    if edits.is_empty() {
        return None;
    }

    let mut text = String::with_capacity(kit.len() + edits.len() * 2);
    let mut mapped = Vec::with_capacity(edits.len());
    let mut copied = 0;
    for (original, word) in edits {
        text.push_str(&kit[copied..original.start]);
        let start = text.len();
        text.push_str(&word);
        mapped.push((original.clone(), start..text.len()));
        copied = original.end;
    }
    text.push_str(&kit[copied..]);
    Some(Repaired {
        text,
        edits: mapped,
    })
}

const LIGATURES: [&str; 5] = ["ffi", "ffl", "ff", "fi", "fl"];

/// 连字本身（U+FB00–FB04）展开；软连字符去掉；PDFium 的行尾断词（U+FFFE 加换行）拼回一个词。
fn expand(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '\u{fffe}' => while chars.next_if(|c| matches!(c, '\r' | '\n')).is_some() {},
            '\u{fb00}' => out.push_str("ff"),
            '\u{fb01}' => out.push_str("fi"),
            '\u{fb02}' => out.push_str("fl"),
            '\u{fb03}' => out.push_str("ffi"),
            '\u{fb04}' => out.push_str("ffl"),
            '\u{ad}' => {}
            ch => out.push(ch),
        }
    }
    out
}

fn ligature_count(text: &str) -> usize {
    let lower = text.to_lowercase();
    let mut count = 0;
    let mut rest = lower.as_str();
    while let Some(at) = rest.find('f') {
        let tail = &rest[at..];
        match LIGATURES
            .iter()
            .find(|ligature| tail.starts_with(**ligature))
        {
            Some(ligature) => {
                count += 1;
                rest = &tail[ligature.len()..];
            }
            None => rest = &tail[1..],
        }
    }
    count
}

/// 一个词里的每处连字都按同一种方式取坏：留前几个字母，后面接空格或什么都不接。
fn broken_forms(word: &str) -> Vec<String> {
    let mut forms = Vec::new();
    for keep in 0..3 {
        for gap in ["", " "] {
            let mut form = String::with_capacity(word.len() + 2);
            let mut rest = word;
            while !rest.is_empty() {
                match LIGATURES
                    .iter()
                    .find(|ligature| rest.starts_with(**ligature))
                {
                    Some(ligature) => {
                        form.push_str(&ligature[..keep.min(ligature.len() - 1)]);
                        form.push_str(gap);
                        rest = &rest[ligature.len()..];
                    }
                    None => {
                        let ch = rest.chars().next().unwrap();
                        form.push(ch);
                        rest = &rest[ch.len_utf8()..];
                    }
                }
            }
            let form = form
                .split(' ')
                .filter(|part| !part.is_empty())
                .collect::<Vec<_>>();
            if (1..=3).contains(&form.len()) {
                forms.push(form.join(" "));
            }
        }
    }
    forms.retain(|form| form != word);
    forms.sort();
    forms.dedup();
    forms
}

/// 字母连成的词和它在原文里的区间；软连字符算词的一部分，比较时去掉。
fn tokens(text: &str) -> impl Iterator<Item = (Range<usize>, String)> + '_ {
    let mut chars = text.char_indices().peekable();
    std::iter::from_fn(move || {
        let (start, first) = chars.find(|&(_, ch)| ch.is_alphabetic())?;
        let mut word = String::from(first);
        let mut end = start + first.len_utf8();
        while let Some((at, ch)) = chars.next_if(|&(_, ch)| ch.is_alphabetic() || ch == '\u{ad}') {
            if ch != '\u{ad}' {
                word.push(ch);
            }
            end = at + ch.len_utf8();
        }
        Some((start..end, word))
    })
}

/// 照 PDFKit 那几个字的大小写写回原词。
fn cased(word: &str, sample: &str) -> String {
    let letters = sample
        .chars()
        .filter(|c| c.is_alphabetic())
        .collect::<Vec<_>>();
    if letters.len() > 1 && letters.iter().all(|c| c.is_uppercase()) {
        return word.to_uppercase();
    }
    let mut out = String::with_capacity(word.len());
    for (i, ch) in word.chars().enumerate() {
        if i == 0 && letters.first().is_some_and(|c| c.is_uppercase()) {
            out.extend(ch.to_uppercase());
        } else {
            out.push(ch);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{repair, tokens};

    #[test]
    fn puts_lost_ligatures_back_in_kit_order() {
        // Phoenix 2024：连字取成空格；Arpino 2022：fi 取成 f；ffi 取成「f 加空格」。
        let kit = "who perceive signi cant discrimina-\ntion, given a fxed level. Racial Ef cacy";
        let reference = "Racial Efficacy who perceive significant discrimina\u{fffe}\r\ntion, given a ﬁxed level.";
        let repaired = repair(kit, reference).unwrap();
        assert_eq!(
            repaired.text,
            "who perceive significant discrimina-\ntion, given a fixed level. Racial Efficacy"
        );
        let at = repaired.text.find("significant").unwrap();
        let original = repaired.original_range(at..at + "significant".len());
        assert_eq!(&kit[original], "signi cant");
        let at = repaired.text.find("level").unwrap();
        assert_eq!(&kit[repaired.original_range(at..at + 5)], "level");
        let at = repaired.text.find("ficant disc").unwrap();
        assert_eq!(
            &kit[repaired.original_range(at..at + 11)],
            "signi cant disc"
        );
    }

    #[test]
    fn leaves_correct_text_and_real_words_alone() {
        assert_eq!(repair("the first five", "the first five"), None);
        // 参照里本来就有的写法（of set）不当成取坏的 offset。
        let kit = "an of set and an of set";
        assert_eq!(repair(kit, "an offset and an of set"), None);
        // 中文、没有连字的页不动。
        assert_eq!(repair("中文没有连字", "中文没有连字"), None);
    }

    #[test]
    fn keeps_soft_hyphens_inside_kit_words() {
        let words = tokens("dis\u{ad}so\u{ad}lu\u{ad}tion fve")
            .map(|(_, word)| word)
            .collect::<Vec<_>>();
        assert_eq!(words, ["dissolution", "fve"]);
        let repaired = repair("a fxed lev\u{ad}el", "a fixed level").unwrap();
        assert_eq!(repaired.text, "a fixed lev\u{ad}el");
    }
}
