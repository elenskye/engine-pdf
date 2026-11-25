//! engine-pdf：Peck、Pith、Pomelo 共用的 Mac PDF 引擎。
//!
//! 接口按 `ROADMAP.md` 的 Phase 实现，设计见 `docs/design.md`。

mod cancel;
mod document;
mod error;
pub mod mathpix;
pub mod ocr;
pub mod pages;
mod read;
#[cfg(feature = "samples")]
pub mod samples;
mod sys;

pub use cancel::Cancel;
pub use document::{Document, Image, ImageFormat};
pub use error::Error;
pub use ocr::{OcrBounds, OcrLine, OcrOptions, OcrReason, OcrText};
pub use read::{
    Content, Flow, NoTextReason, Ocr, Page, ReadOptions, Render, Summary, read, read_each,
};

/// 引擎版本（App 记日志、核对钉住的 tag 用）。
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    #[test]
    fn version_matches_manifest() {
        assert_eq!(super::VERSION, "0.3.0");
    }
}
