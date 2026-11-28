use std::path::Path;

use zeroize::Zeroizing;

use crate::{Cancel, Error, OcrOptions, OcrText, ocr, sys};

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

    /// 用打开密码或权限密码读取指定路径的 PDF。
    pub fn open_with_password(
        path: impl AsRef<Path>,
        password: &Zeroizing<String>,
    ) -> Result<Self, Error> {
        if !cfg!(target_os = "macos") {
            return Err(Error::Unsupported);
        }
        Self::from_bytes_with_password(&std::fs::read(path)?, password)
    }

    /// 用打开密码或权限密码从字节打开 PDF。
    pub fn from_bytes_with_password(
        bytes: &[u8],
        password: &Zeroizing<String>,
    ) -> Result<Self, Error> {
        sys::Pdf::from_bytes_with_password(bytes, password).map(Self)
    }

    pub fn page_count(&self) -> u32 {
        self.0.page_count()
    }

    /// 读取文件信息及当前打开权限。
    pub fn info(&self) -> crate::edit::Info {
        crate::sys::info(&self.0)
    }

    /// 读取目录书签，保留层级和可识别的阅读目标。
    pub fn outline(&self) -> Vec<crate::edit::OutlineItem> {
        crate::sys::outline(&self.0)
    }

    /// 读取指定原始物理页的标注。表单控件由表单接口读取。
    pub fn annotations(&self, page: u32) -> Result<Vec<crate::edit::Annotation>, Error> {
        crate::sys::annotations(&self.0, page)
    }

    /// 列出该页的页面对象（PDFium 读取）。可见的文字和图片可通过 `edit::Plan::objects` 移动或删除；
    /// 改动只写在这几个对象的指令上，页面其余内容原样保留（`decisions/0019`）。
    #[cfg(feature = "pdfium")]
    pub fn objects(&self, page: u32) -> Result<Vec<crate::edit::PageObject>, Error> {
        crate::sys::page_objects(&self.0, page)
    }

    /// 读取逻辑表单字段及其各个控件位置。
    pub fn fields(&self, cancel: &Cancel) -> Result<Vec<crate::edit::Field>, Error> {
        cancel.check()?;
        crate::sys::fields(&self.0, cancel)
    }

    /// 取指定物理页的页标签（印在页上的页码，来自 `/PageLabels`，如 "iv"）。
    /// PDF 未定义 `/PageLabels` 或页码越界时返回 `None`。
    pub fn label(&self, page: u32) -> Option<String> {
        self.0.label(page)
    }

    /// 取 PDFKit 返回的原始文字层。没有文字层时返回空字符串。
    pub fn text(&self, page: u32) -> Result<String, Error> {
        self.0.text(page)
    }

    /// 在本机识别指定物理页，使用最长边 2400 像素的灰度页图。
    /// 没有识别出字时，结果的文字和行列表为空。
    pub fn ocr(&self, page: u32, opts: &OcrOptions) -> Result<OcrText, Error> {
        let (result, _) = ocr::assemble(self.0.ocr(page, opts, false)?);
        Ok(result)
    }

    /// 输出白底页图，最长边为 `max_side` 像素。画的是 CropBox（阅读器显示的范围），带页面旋转。
    pub fn render(&self, page: u32, format: ImageFormat, max_side: u32) -> Result<Image, Error> {
        self.0.render(page, format, max_side)
    }

    /// 第 `page` 页阅读器显示的范围（CropBox，PDF 坐标）和页面旋转，和 `render` 画的一样。
    /// App 把查找的框画到页图上时用 `PageBox::on_image`。
    pub fn page_box(&self, page: u32) -> Result<PageBox, Error> {
        let (crop, rotation) = self.0.page_box(page)?;
        Ok(PageBox {
            crop,
            rotation: rotation.rem_euclid(360),
        })
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

    pub(crate) fn page(&self, page: u32) -> Result<(Option<String>, String, bool), Error> {
        self.0.page(page)
    }

    #[cfg(feature = "samples")]
    pub(crate) fn scan(&self, page: u32) -> Result<Vec<u8>, Error> {
        self.0.scan(page)
    }

    pub(crate) fn original_bytes(&self) -> &[u8] {
        self.0.original_bytes()
    }

    pub(crate) fn native(&self) -> &sys::Pdf {
        &self.0
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

/// 一页显示的范围：CropBox（PDF 坐标：pt、左下原点）和顺时针旋转（0、90、180、270）。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PageBox {
    pub crop: crate::edit::Rect,
    pub rotation: i32,
}

impl PageBox {
    /// PDF 坐标里的框（查找、OCR 交回的那种）在 `render` 画出的页图上的位置：左上原点，
    /// 按页图宽高的比例（0–1），超出页面的部分截掉。
    pub fn on_image(&self, rect: &crate::edit::Rect) -> crate::edit::Rect {
        let c = &self.crop;
        if c.width <= 0.0 || c.height <= 0.0 {
            return crate::edit::Rect {
                x: 0.0,
                y: 0.0,
                width: 0.0,
                height: 0.0,
            };
        }
        let clamp = |v: f64| v.clamp(0.0, 1.0);
        // 不转时：从左量、从上量的比例
        let (u0, u1) = (
            clamp((rect.x - c.x) / c.width),
            clamp((rect.x + rect.width - c.x) / c.width),
        );
        let (t0, t1) = (
            clamp((c.y + c.height - rect.y - rect.height) / c.height),
            clamp((c.y + c.height - rect.y) / c.height),
        );
        let (left, top, right, bottom) = match self.rotation {
            90 => (1.0 - t1, u0, 1.0 - t0, u1),
            180 => (1.0 - u1, 1.0 - t1, 1.0 - u0, 1.0 - t0),
            270 => (t0, 1.0 - u1, t1, 1.0 - u0),
            _ => (u0, t0, u1, t1),
        };
        crate::edit::Rect {
            x: left,
            y: top,
            width: right - left,
            height: bottom - top,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::PageBox;
    use crate::edit::Rect;

    fn r(x: f64, y: f64, width: f64, height: f64) -> Rect {
        Rect {
            x,
            y,
            width,
            height,
        }
    }

    #[test]
    fn boxes_land_on_the_page_image_as_it_is_turned() {
        // a 200 × 100 crop starting at (10, 20); the box sits near the top left
        let page = |rotation| PageBox {
            crop: r(10.0, 20.0, 200.0, 100.0),
            rotation,
        };
        let hit = r(30.0, 100.0, 40.0, 10.0);
        let close = |a: Rect, b: Rect| {
            [
                (a.x, b.x),
                (a.y, b.y),
                (a.width, b.width),
                (a.height, b.height),
            ]
            .iter()
            .all(|(p, q)| (p - q).abs() < 1e-9)
        };
        assert!(close(page(0).on_image(&hit), r(0.1, 0.1, 0.2, 0.1)));
        assert!(close(page(90).on_image(&hit), r(0.8, 0.1, 0.1, 0.2)));
        assert!(close(page(180).on_image(&hit), r(0.7, 0.8, 0.2, 0.1)));
        assert!(close(page(270).on_image(&hit), r(0.1, 0.7, 0.1, 0.2)));
        // outside the crop: cut off at the edge
        assert!(close(
            page(0).on_image(&r(-50.0, 100.0, 100.0, 10.0)),
            r(0.0, 0.1, 0.2, 0.1)
        ));
    }
}
