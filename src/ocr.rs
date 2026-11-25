//! Vision 本机 OCR。

mod order;
pub(crate) use order::assemble;

/// 本机识别设置。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OcrOptions {
    /// Vision 识别使用的语言，按优先顺序列出 BCP 47 语言标记。
    pub languages: Vec<String>,
}

impl Default for OcrOptions {
    fn default() -> Self {
        Self {
            languages: ["zh-Hans", "zh-Hant", "en-US"].map(String::from).into(),
        }
    }
}

/// 识别一页的原因。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OcrReason {
    /// 调用方直接要求识别这一页。
    Requested,
    /// 文字层为空或字太少，无法代表整页。
    NoTextLayer,
    /// 文字层乱码。
    Garbled,
}

/// 一页的识别结果。
#[derive(Debug)]
pub struct OcrText {
    /// 按阅读顺序拼好的文字，栏和块之间空一行。
    pub text: String,
    pub why: OcrReason,
    /// 各行置信度的算术平均值，范围 0–1；未识别到字时为 0。
    pub confidence: f32,
    /// 按阅读顺序排列的识别行。
    pub lines: Vec<OcrLine>,
}

/// Vision 返回的一行文字。
#[derive(Debug)]
pub struct OcrLine {
    pub text: String,
    /// Vision 返回的置信度，范围 0–1。
    pub confidence: f32,
    pub bounds: OcrBounds,
}

/// 原 PDF 的页面坐标，左下原点，单位 pt，包含 CropBox 的偏移。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OcrBounds {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// 用合成样本加载本机 OCR 模型，可以在 App 的后台任务里调用。
/// 首次加载可能需要数秒；识别失败时错误指向样本的第 1 页。
pub fn warm_up() -> Result<(), crate::Error> {
    crate::sys::warm_up(&OcrOptions::default())
}
