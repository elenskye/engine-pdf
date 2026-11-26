//! 从照片或现有页面新建 PDF。

use std::path::Path;

use crate::edit::{self, InfoPatch, Mark, MarkKind, OutlineItem, Pick, Plan, Rect, Save, Target};
use crate::{Cancel, Document, Error};

/// 图片文件或已读入的原始图片字节。
#[derive(Clone, Copy, Debug)]
pub enum ImageSource<'a> {
    Path(&'a Path),
    Bytes(&'a [u8]),
}

/// 按输入顺序把照片打包为 PDF。多页 TIFF 每帧一页，逐页检查取消。
pub fn images(sources: &[ImageSource<'_>], cancel: &Cancel) -> Result<Vec<u8>, Error> {
    crate::sys::images(sources, cancel)
}

/// 拼装 PDF 时可一起写入的链接、目录和文件信息。
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Assembly {
    pub links: Vec<AssemblyLink>,
    pub outline: Option<Vec<OutlineItem>>,
    pub info: Option<InfoPatch>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AssemblyLink {
    /// 输出文件中放置链接的物理页码，从 1 起。
    pub page: u32,
    pub rect: Rect,
    pub target: Target,
}

/// 按输入顺序拼接各份 PDF 的全部页面，链接和目录使用输出物理页码。
pub fn assemble(parts: &[&[u8]], how: &Assembly, cancel: &Cancel) -> Result<Vec<u8>, Error> {
    if !cfg!(target_os = "macos") {
        return Err(Error::Unsupported);
    }
    cancel.check()?;
    let docs = parts
        .iter()
        .map(|bytes| {
            cancel.check()?;
            Document::from_bytes(bytes)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let (doc, others) = docs.split_first().ok_or(Error::EmptyAssembly)?;
    let pages: Vec<_> = docs
        .iter()
        .enumerate()
        .flat_map(|(source, doc)| {
            (1..=doc.page_count()).map(move |page| Pick {
                source: source as u32,
                page,
                rotate: 0,
            })
        })
        .collect();
    let add = how
        .links
        .iter()
        .map(|link| {
            let pick = link
                .page
                .checked_sub(1)
                .and_then(|position| pages.get(position as usize))
                .ok_or(Error::PageOutOfRange {
                    page: link.page,
                    total: pages.len() as u32,
                })?;
            Ok(Mark {
                source: pick.source,
                page: pick.page,
                rect: link.rect,
                author: None,
                contents: None,
                kind: MarkKind::Link {
                    target: link.target.clone(),
                },
            })
        })
        .collect::<Result<Vec<_>, Error>>()?;
    let plan = Plan {
        pages: (!others.is_empty()).then_some(pages),
        add,
        outline: how.outline.clone(),
        info: how.info.clone(),
        ..Plan::default()
    };
    let others: Vec<_> = others.iter().collect();
    edit::apply(doc, &others, &plan, &Save::default(), cancel)
}
