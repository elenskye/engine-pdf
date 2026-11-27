use std::collections::BTreeSet;
use std::fmt::Write;

use engine_core::find::{Match, Query, Source};
use serde_json::{Value, json};

use crate::mathpix;

use super::{Ctx, Output, Quote, ToolError, decode, languages, open_main, report};

pub(super) fn find(
    _: &mathpix::Options,
    args: &Value,
    ctx: &Ctx<'_>,
    _: Option<&Quote>,
) -> Result<Output, ToolError> {
    let doc = open_main(args, ctx)?;
    let query: Query = decode(args, "请提供要查找的词或语句。")?;
    let total = doc.page_count();
    let selected = crate::pages::parse(args["pages"].as_str(), total, total as usize)?;
    let options = crate::find::Options {
        ocr: (args["ocr"] == true).then(|| languages(args)),
    };
    let found = crate::find::find(
        &doc,
        &selected,
        &query,
        &options,
        ctx.cancel,
        &|completed, page| {
            report(
                ctx,
                completed,
                selected.len(),
                format!("已查找第 {page} 页。"),
            );
        },
    )?;

    let mut text = match (found.total, found.hits.first().map(|hit| hit.how)) {
        (0, _) => "没有找到。".into(),
        (total, Some(Match::Words)) => {
            // 按词时每个词各算一处，模型更需要知道是哪几页。
            let pages = found
                .hits
                .iter()
                .map(|hit| hit.location.page)
                .collect::<BTreeSet<_>>();
            format!(
                "整句没有找到。改按词找：第 {} 页{}同时出现这些词，共 {total} 处。",
                pages
                    .into_iter()
                    .map(|page| page.to_string())
                    .collect::<Vec<_>>()
                    .join("、"),
                if found.truncated { "等" } else { "" }
            )
        }
        (total, _) => format!("找到 {total} 处。"),
    };
    for hit in &found.hits {
        let at = &hit.location;
        let label = at
            .label
            .as_ref()
            .map_or_else(String::new, |label| format!("（{label}）"));
        let source = if hit.source == Source::Ocr {
            "〔本机识别〕"
        } else {
            ""
        };
        let _ = write!(
            text,
            "\n- 第 {} 页{label}{source}　{}**{}**{}",
            at.page,
            one_line(&hit.before),
            one_line(&hit.matched),
            one_line(&hit.after)
        );
    }
    if found.truncated {
        let _ = write!(
            text,
            "\n\n只列了前 {} 处，可缩小页范围或提高 max_hits。",
            found.hits.len()
        );
    }
    Ok(Output {
        text,
        data: json!(found),
        ..Output::default()
    })
}

/// 把原文的换行排成一行给模型看：中文之间的换行直接去掉，其余换成空格。
fn one_line(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch != '\r' && ch != '\n' {
            out.push(ch);
            continue;
        }
        while chars
            .next_if(|&next| next == '\r' || next == '\n')
            .is_some()
        {}
        let joined = out.chars().next_back().is_some_and(crate::find::wide)
            && chars.peek().is_some_and(|&c| crate::find::wide(c));
        if !joined {
            out.push(' ');
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::one_line;

    #[test]
    fn joins_lines_without_spaces_inside_chinese() {
        assert_eq!(one_line("内外政\n治效能感"), "内外政治效能感");
        assert_eq!(one_line("quan-\r\ntile sketch"), "quan- tile sketch");
        assert_eq!(one_line("2010\n年"), "2010 年");
        assert_eq!(one_line("末尾\n"), "末尾 ");
    }
}
