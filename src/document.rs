use std::path::Path;

use crate::{Cancel, Error, sys};

/// 已打开的 PDF。必须在创建它的线程上使用。
///
/// ```compile_fail
/// fn requires_send<T: Send>() {}
/// requires_send::<engine_pdf::Document>();
/// ```
pub struct Document(sys::Pdf);

impl Document {
    /// 从指定路径读取 PDF。
    pub fn open(path: impl AsRef<Path>) -> Result<Self, Error> {
        if !cfg!(target_os = "macos") {
            return Err(Error::Unsupported);
        }
        Self::from_bytes(&std::fs::read(path)?)
    }

    /// 从 PDF 字节打开文档，文档持有自己的副本。
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, Error> {
        sys::Pdf::from_bytes(bytes).map(Self)
    }

    pub fn page_count(&self) -> u32 {
        self.0.page_count()
    }

    /// 取指定物理页的页标签（印在页上的页码，来自 `/PageLabels`，如 "iv"）。
    /// PDF 没定义页标签时 PDFKit 给物理页码的数字；页码越界时返回 `None`。
    pub fn label(&self, page: u32) -> Option<String> {
        self.0.label(page)
    }

    /// 取 PDFKit 返回的原始文字层。没有文字层时返回空字符串。
    pub fn text(&self, page: u32) -> Result<String, Error> {
        self.0.text(page)
    }

    /// 输出白底页图，最长边为 `max_side` 像素。画的是 CropBox（阅读器显示的范围），带页面旋转。
    pub fn render(&self, page: u32, format: ImageFormat, max_side: u32) -> Result<Image, Error> {
        self.0.render(page, format, max_side)
    }

    /// 统计具有图片 XObject 的物理页及数量，按页码升序返回。
    /// 与 Pomelo 一致，忽略任一边不足 32 像素的图像。
    pub fn image_counts(&self, cancel: &Cancel) -> Result<Vec<(u32, usize)>, Error> {
        cancel.check()?;
        self.0.image_counts(cancel)
    }

    /// 按给定顺序复制指定物理页，允许重复。任一页越界时返回错误。
    pub fn split(&self, pages: &[u32], cancel: &Cancel) -> Result<Vec<u8>, Error> {
        cancel.check()?;
        if pages.is_empty() {
            return Err(Error::BadPageSpec("请至少选择一页。".into()));
        }
        self.0.split(pages, cancel)
    }

    pub(crate) fn page(&self, page: u32) -> Result<(Option<String>, String), Error> {
        self.0.page(page)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ImageFormat {
    Jpeg,
    Png,
    /// 8 位灰度像素。从左上角开始逐行排列，每行宽度等于 `width`。
    Gray,
}

#[derive(Debug)]
pub struct Image {
    pub format: ImageFormat,
    pub width: u32,
    pub height: u32,
    pub bytes: Vec<u8>,
}
