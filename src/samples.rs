//! 现场生成测试用 PDF。需开启 `samples` feature。

use crate::Error;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Sample {
    TextEn,
    TextZh,
    TwoColumnEn,
    TwoColumnZh,
    Mixed,
    ScannedTwoColumn,
    Labels,
    Locked,
    Damaged,
    NotPdf,
}

/// `locked.pdf` 的用户密码，仅用于测试样本。
pub const LOCKED_PASSWORD: &str = "engine-pdf-fixture";

impl Sample {
    pub const ALL: [Self; 10] = [
        Self::TextEn,
        Self::TextZh,
        Self::TwoColumnEn,
        Self::TwoColumnZh,
        Self::Mixed,
        Self::ScannedTwoColumn,
        Self::Labels,
        Self::Locked,
        Self::Damaged,
        Self::NotPdf,
    ];

    pub fn filename(self) -> &'static str {
        match self {
            Self::TextEn => "text_en.pdf",
            Self::TextZh => "text_zh.pdf",
            Self::TwoColumnEn => "two_column_en.pdf",
            Self::TwoColumnZh => "two_column_zh.pdf",
            Self::Mixed => "mixed.pdf",
            Self::ScannedTwoColumn => "scanned_two_column.pdf",
            Self::Labels => "labels.pdf",
            Self::Locked => "locked.pdf",
            Self::Damaged => "damaged.pdf",
            Self::NotPdf => "not_pdf.txt",
        }
    }

    /// 生成样本字节。非 macOS 返回 `Error::Unsupported`。
    pub fn generate(self) -> Result<Vec<u8>, Error> {
        crate::sys::generate(self)
    }

    /// 指定物理页的源文字。扫描文字同样有转录，两栏按左栏、右栏排列。
    pub fn source_text(self, page: u32) -> Option<String> {
        let index = page.checked_sub(1)? as usize;
        if self == Self::Labels {
            return Some(LABEL_TEXT.get(index)?.to_string());
        }
        match *self.pages().get(index)? {
            Page::Text(lines) | Page::Scan(lines) => Some(lines.join("\n")),
            Page::Columns { left, right, .. } => Some([left, right].concat().join("\n")),
            Page::Blank | Page::Image | Page::Formula => None,
        }
    }

    pub(crate) fn pages(self) -> &'static [Page] {
        match self {
            Self::TextEn | Self::Locked => &TEXT_EN,
            Self::TextZh => &TEXT_ZH,
            Self::TwoColumnEn => &[Page::Columns {
                left: LEFT_EN,
                right: RIGHT_EN,
            }],
            Self::TwoColumnZh | Self::ScannedTwoColumn => &[Page::Columns {
                left: LEFT_ZH,
                right: RIGHT_ZH,
            }],
            Self::Mixed => &MIXED,
            Self::Labels | Self::Damaged | Self::NotPdf => &[],
        }
    }
}

/// 仿扫描件：把 `doc` 的第 `page` 页画成灰度图（最长边 2400 像素），做成一份只有图、没有文字层的单页 PDF。
/// 原页的文字层（[`Document::text`]）就是 OCR 的标准答案。非 macOS 返回 `Error::Unsupported`。
pub fn scan(doc: &crate::Document, page: u32) -> Result<Vec<u8>, Error> {
    doc.scan(page)
}

/// 手写一份最小的 PDF，给生成器画不出来的边界情况用（页标签、旋转、CropBox、只有几个字的页）。
///
/// - `catalog_extra`：加进 Catalog 字典的条目，如 `/PageLabels << … >>`；不需要就给 `""`。
/// - `pages`：每页 `(页字典的额外条目, 内容流)`。页字典要自己写 `/MediaBox`；内容流里可以用 `/F1`（Helvetica）。
pub fn raw_pdf(catalog_extra: &str, pages: &[(&str, &str)]) -> Vec<u8> {
    use std::fmt::Write;

    let kids = (0..pages.len())
        .map(|i| format!("{} 0 R", 4 + i * 2))
        .collect::<Vec<_>>()
        .join(" ");
    let mut objects = vec![
        format!("<< /Type /Catalog /Pages 2 0 R {catalog_extra} >>"),
        format!("<< /Type /Pages /Kids [{kids}] /Count {} >>", pages.len()),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".into(),
    ];
    for (index, (page, stream)) in pages.iter().enumerate() {
        objects.push(format!(
            "<< /Type /Page /Parent 2 0 R {page} /Contents {} 0 R /Resources << /Font << /F1 3 0 R >> >> >>",
            5 + index * 2
        ));
        objects.push(format!(
            "<< /Length {} >>\nstream\n{stream}\nendstream",
            stream.len()
        ));
    }
    let mut pdf = String::from("%PDF-1.4\n");
    let mut offsets = Vec::new();
    for (index, object) in objects.iter().enumerate() {
        offsets.push(pdf.len());
        writeln!(pdf, "{} 0 obj\n{object}\nendobj", index + 1).unwrap();
    }
    let xref = pdf.len();
    let size = offsets.len() + 1;
    write!(pdf, "xref\n0 {size}\n0000000000 65535 f \n").unwrap();
    for offset in offsets {
        writeln!(pdf, "{offset:010} 00000 n ").unwrap();
    }
    writeln!(
        pdf,
        "trailer\n<< /Size {size} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF"
    )
    .unwrap();
    pdf.into_bytes()
}

pub(crate) enum Page {
    Text(&'static [&'static str]),
    Scan(&'static [&'static str]),
    Columns {
        left: &'static [&'static str],
        right: &'static [&'static str],
    },
    Blank,
    Image,
    Formula,
}

const EN_1: &[&str] = &[
    "Page one begins at the harbor before sunrise.",
    "Three small boats return with the morning tide.",
    "The keeper records 42 lamps, 7 ropes, and 12 crates.",
    "By noon, the fog has lifted from the eastern shore.",
];
const EN_2: &[&str] = &[
    "Page two follows the river through a quiet valley.",
    "A wooden bridge connects the farms to the market.",
    "At 09:30, the first train crosses the northern ridge.",
    "The station clock runs exactly two minutes slow.",
];
const EN_3: &[&str] = &[
    "Page three ends in a library beside the main square.",
    "A reader copies four lines from an old field journal.",
    "The entry is dated October 15, 2026.",
    "A blue envelope holds the original map.",
];

const ZH_1: &[&str] = &[
    "第一页记录清晨的港口，三艘小船顺着涨潮归来。",
    "管理员在表格里写下 42 盏灯、7 根绳和 12 个木箱。",
    "观测站编号为 PDF-01，记录时间是 08:30。",
    "到了中午，东岸的薄雾渐渐散去，远处可以看见灯塔。",
];
const ZH_2: &[&str] = &[
    "第二页沿着河流穿过安静的山谷，岸边分布着几个村落。",
    "一座木桥连接农田和市场，桥下的河水十分清澈。",
    "早上 09:30，第一班列车驶过北面的山脊。",
    "站里的时钟慢了 2 分钟，记录编号是 Train-02。",
];
const ZH_3: &[&str] = &[
    "第三页来到广场旁的图书馆，窗台上摆着几盆绿植。",
    "读者从旧日记中抄下四行文字，日期是 2026 年 10 月 15 日。",
    "蓝色信封里收着旧地图，上面标有 A1、B2、C3。",
    "管理员说：“请把地图放回原处。”",
];

const TEXT_EN: [Page; 3] = [Page::Text(EN_1), Page::Text(EN_2), Page::Text(EN_3)];
const TEXT_ZH: [Page; 3] = [Page::Text(ZH_1), Page::Text(ZH_2), Page::Text(ZH_3)];

const LEFT_EN: &[&str] = &[
    "01 The survey starts by the harbor.",
    "Two lamps mark the safe channel.",
    "",
    "02 The crew checks the tide table.",
    "Each reading goes into the logbook.",
];
const RIGHT_EN: &[&str] = &[
    "03 The survey continues upstream.",
    "A bridge marks the second station.",
    "",
    "04 The crew returns before sunset.",
    "The final report lists four readings.",
];
const LEFT_ZH: &[&str] = &[
    "01 调查从港口开始，先登记船只。",
    "两座灯塔标出航道，渔船缓缓靠岸。",
    "",
    "02 船员翻看潮汐表，核对涨潮时间。",
    "每一条观测数据都登记在航行日志。",
];
const RIGHT_ZH: &[&str] = &[
    "03 调查向上游继续，河道逐渐变窄。",
    "旧木桥附近设有第二个水位观测站。",
    "",
    "04 船员在日落前返回，整理样本。",
    "最终报告列出四组数据和观测日期。",
];

const MIXED: [Page; 10] = [
    Page::Text(EN_1),
    Page::Scan(EN_2),
    Page::Blank,
    Page::Text(ZH_1),
    Page::Scan(ZH_2),
    Page::Image,
    Page::Text(EN_3),
    Page::Formula,
    Page::Blank,
    Page::Text(ZH_3),
];

pub(crate) const LABEL_TEXT: [&str; 6] = [
    "Preface: first page",
    "Preface: second page",
    "Preface: third page",
    "Body: first page",
    "Body: second page",
    "Body: third page",
];

#[cfg(all(test, not(target_os = "macos")))]
#[test]
fn sample_generation_requires_macos() {
    assert!(matches!(Sample::TextEn.generate(), Err(Error::Unsupported)));
}
