use serde_json::{Value, json};

use super::Usage;

/// 识别结果与原 PDF 物理页码的对应关系。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Mapping {
    /// 每页文字都有确定的原始页码。
    Exact,
    /// 原始页码无法完整确定，整份 MMD 附在第一张选页上。非 PDF 没有原始页码。
    Approximate,
}

/// 识别结果与本次识别的用量。
#[derive(Debug)]
pub struct Recognized {
    pub pages: Vec<RecognizedPage>,
    /// Mathpix 返回的整份 MMD，保留分页标记。
    pub mmd: String,
    /// 下载的 MMD 压缩包（含裁图），有请求才返回。
    pub mmd_zip: Option<Vec<u8>>,
    pub mapping: Mapping,
    pub usage: Usage,
}

/// 原 PDF 某一页的识别结果。
#[derive(Debug)]
pub struct RecognizedPage {
    /// 原文件的物理页码，从 1 起。
    pub index: u32,
    pub mmd: String,
    /// `lines.json` 中这一页的对象，`page` 字段已改成 `index`。
    pub lines: Option<Value>,
}

impl Recognized {
    /// 按 Mathpix 的 `{"pages": [...]}` 结构拼回可定位的行数据，供 Pith 等调用方使用。
    pub fn lines_json(&self) -> Option<Value> {
        let pages: Vec<_> = self
            .pages
            .iter()
            .filter_map(|page| page.lines.clone())
            .collect();
        (!pages.is_empty()).then(|| json!({ "pages": pages }))
    }
}

pub(super) fn recognize(
    mmd: String,
    lines_json: Option<Value>,
    mmd_zip: Option<Vec<u8>>,
    page_map: &[u32],
    usage: Usage,
) -> Recognized {
    let mut pages: Vec<_> = page_map
        .iter()
        .map(|&index| RecognizedPage {
            index,
            mmd: String::new(),
            lines: None,
        })
        .collect();

    let records =
        lines_json.and_then(|mut value| value.get_mut("pages")?.as_array_mut().map(std::mem::take));
    let records_match_count = if let Some(records) = records {
        let count = records.len();
        let mut seen = vec![false; pages.len()];
        for mut record in records {
            let Some(position) = record["page"]
                .as_u64()
                .and_then(|number| usize::try_from(number).ok())
                .and_then(|number| number.checked_sub(1))
            else {
                continue;
            };
            let Some(page) = pages.get_mut(position) else {
                continue;
            };
            if std::mem::replace(&mut seen[position], true) {
                // 同一上传页收到多条记录时，无法确定哪条有效。
                page.lines = None;
            } else if record["lines"].is_array() {
                record["page"] = json!(page.index);
                page.lines = Some(record);
            }
        }
        count == pages.len()
    } else {
        false
    };

    // 官方格式在每页末尾都有独立的分页标记。最后一个标记后的换行不另算一页。
    let mut sections = Vec::new();
    let (mut start, mut offset) = (0, 0);
    for line in mmd.split_inclusive('\n') {
        if line.trim_end() == "\\pagebreak" {
            sections.push(&mmd[start..offset]);
            start = offset + line.len();
        }
        offset += line.len();
    }
    let tail = &mmd[start..];
    if sections.is_empty() || !tail.trim().is_empty() {
        sections.push(tail);
    }

    let per_page = if sections.len() == pages.len() {
        Some(
            sections
                .iter()
                .map(|section| section.trim_matches(['\r', '\n']).to_owned())
                .collect::<Vec<_>>(),
        )
    } else if records_match_count && !pages.is_empty() {
        pages
            .iter()
            .map(|page| {
                let lines = page.lines.as_ref()?.get("lines")?.as_array()?;
                let mut fragments = Vec::new();
                for line in lines {
                    if line["conversion_output"].as_bool() == Some(false) {
                        continue;
                    }
                    let fragment = line["text_display"]
                        .as_str()
                        .or_else(|| line["text"].as_str())?;
                    if !fragment.trim().is_empty() {
                        fragments.push(fragment);
                    }
                }
                Some(fragments.join("\n\n"))
            })
            .collect::<Option<Vec<_>>>()
            .filter(|rebuilt| {
                rebuilt.iter().any(|page| !page.is_empty())
                    || sections.iter().all(|page| page.trim().is_empty())
            })
    } else {
        None
    };

    let mapping = if let Some(per_page) = per_page {
        for (page, text) in pages.iter_mut().zip(per_page) {
            page.mmd = text;
        }
        Mapping::Exact
    } else {
        if let Some(first) = pages.first_mut() {
            first.mmd.clone_from(&mmd);
        }
        Mapping::Approximate
    };

    Recognized {
        pages,
        mmd,
        mmd_zip,
        mapping,
        usage,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // 按 Mathpix 官方格式写的合成夹具，模拟末页分页符、无文字的页，以及被合并的空页。
    const MMD: &str = include_str!("../../tests/fixtures/mathpix/pages.mmd");
    const MERGED: &str = include_str!("../../tests/fixtures/mathpix/merged-blank.mmd");
    const LINES: &str = include_str!("../../tests/fixtures/mathpix/pages.lines.json");
    const ALPHA: &str = r"Alpha carries `\pagebreak` in its example.";
    const PAGE_MAP: &[u32] = &[2, 8, 11];

    #[test]
    fn splits_mmd_and_remaps_lines_to_original_pages() {
        let mmd = MMD.replace('\n', "\r\n");
        let result = recognize(
            mmd.clone(),
            Some(serde_json::from_str(LINES).unwrap()),
            None,
            PAGE_MAP,
            usage(),
        );

        assert_eq!(result.mapping, Mapping::Exact);
        assert_eq!(result.mmd, mmd);
        assert_eq!(
            result
                .pages
                .iter()
                .map(|page| (page.index, page.mmd.as_str()))
                .collect::<Vec<_>>(),
            [(2, ALPHA), (8, ""), (11, "$$x^2$$")]
        );
        let lines = result.lines_json().unwrap();
        assert_eq!(lines["pages"][0]["page"], 2);
        assert_eq!(lines["pages"][1]["page"], 8);
        assert_eq!(lines["pages"][1]["lines"], json!([]));
        assert_eq!(lines["pages"][2]["page"], 11);
        assert_eq!(lines["pages"][2]["image-id"], "sample-3");
        assert_eq!(lines["pages"][2]["lines"][0]["region"]["top_left_x"], 90);
    }

    #[test]
    fn restores_merged_blank_from_complete_lines() {
        let result = recognize(
            MERGED.into(),
            Some(serde_json::from_str(LINES).unwrap()),
            None,
            PAGE_MAP,
            usage(),
        );
        assert_eq!(result.mapping, Mapping::Exact);
        assert_eq!(result.pages[0].mmd, ALPHA);
        assert_eq!(result.pages[1].mmd, "");
        assert_eq!(result.pages[2].mmd, "$$x^2$$");
    }

    #[test]
    fn keeps_all_mmd_on_first_page_when_page_boundaries_are_unknown() {
        let result = recognize(MERGED.into(), None, None, PAGE_MAP, usage());
        assert_eq!(result.mapping, Mapping::Approximate);
        assert_eq!(result.pages[0].index, 2);
        assert_eq!(result.pages[0].mmd, MERGED);
        assert!(result.pages[1..].iter().all(|page| page.mmd.is_empty()));
        assert!(result.lines_json().is_none());

        let mut lines: Value = serde_json::from_str(LINES).unwrap();
        let records = lines["pages"].as_array_mut().unwrap();
        records.pop();
        records.push(records[0].clone());
        let result = recognize(MERGED.into(), Some(lines), None, PAGE_MAP, usage());
        assert_eq!(result.mapping, Mapping::Approximate);
        assert!(result.pages[1].lines.is_none());
        assert!(result.pages[2].lines.is_none());
        assert_eq!(result.lines_json().unwrap()["pages"][0]["page"], 2);
    }

    #[test]
    fn non_pdf_does_not_invent_original_page_numbers() {
        let result = recognize(
            MMD.into(),
            Some(serde_json::from_str(LINES).unwrap()),
            Some(vec![1, 2, 3]),
            &[],
            usage(),
        );
        assert_eq!(result.mapping, Mapping::Approximate);
        assert!(result.pages.is_empty());
        assert!(result.lines_json().is_none());
        assert_eq!(result.mmd, MMD);
        assert_eq!(result.mmd_zip, Some(vec![1, 2, 3]));
    }

    fn usage() -> Usage {
        Usage {
            pdf_id: Some("sample".into()),
            pages_billed: 3,
            usd: 0.015,
            estimated: false,
            remote_deleted: None,
        }
    }
}
