use std::{fmt, io};

/// 引擎的错误。`Display` 是给人看的一句中文，不带路径、密钥和服务原文。
#[derive(Debug)]
pub enum Error {
    NotPdf,
    Damaged,
    Locked,
    WrongPassword,
    OwnerPasswordRequired,
    PasswordActionRequired,
    InvalidPasswordOptions,
    PageOutOfRange {
        page: u32,
        total: u32,
    },
    /// 引擎生成的页码错误提示。
    BadPageSpec(String),
    Render {
        page: u32,
    },
    Ocr {
        page: u32,
    },
    EmptyImages,
    ImageInput {
        index: usize,
    },
    EmptyAssembly,
    InvalidSource {
        source: u32,
    },
    InvalidRotation {
        rotate: i32,
    },
    InvalidTarget,
    InvalidGeometry,
    InvalidStampImage,
    AnnotationNotFound,
    UnsupportedForm,
    FieldNotFound,
    FieldReadOnly,
    InvalidFieldValue,
    PdfiumUnavailable,
    ObjectNotFound {
        page: u32,
        id: u32,
    },
    ObjectNotEditable {
        page: u32,
        id: u32,
    },
    ObjectEditFailed {
        page: u32,
    },
    RedactionFailed {
        page: u32,
    },
    RedactionSharedImage {
        page: u32,
    },
    SaveFailed,
    EditTextChanged {
        page: u32,
    },
    FormEditUnavailable,
    Mathpix(crate::mathpix::MathpixError),
    Cancelled,
    Unsupported,
    Io(io::Error),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotPdf => f.write_str("这不是 PDF 文件。"),
            Self::Damaged => f.write_str("这份 PDF 已损坏，打不开。"),
            Self::Locked => f.write_str("这份 PDF 加了密码，打不开。"),
            Self::WrongPassword => f.write_str("PDF 密码不正确。"),
            Self::OwnerPasswordRequired => f.write_str("请用权限密码打开 PDF 后再编辑或更改密码。"),
            Self::PasswordActionRequired => f.write_str("请指定输出使用新密码或去掉现有密码。"),
            Self::InvalidPasswordOptions => f.write_str("PDF 密码设置无效。"),
            Self::PageOutOfRange { page, total } => {
                write!(f, "这份 PDF 只有 {total} 页，没有第 {page} 页。")
            }
            Self::BadPageSpec(message) => f.write_str(message),
            Self::Render { page } => write!(f, "第 {page} 页画不出来。"),
            Self::Ocr { page } => write!(f, "第 {page} 页识别失败。"),
            Self::EmptyImages => f.write_str("请至少选择一张图片。"),
            Self::ImageInput { index } => write!(f, "第 {index} 张图片读不了或放不进 PDF。"),
            Self::EmptyAssembly => f.write_str("请至少选择一份 PDF。"),
            Self::InvalidSource { source } => write!(f, "找不到第 {source} 个附加来源。"),
            Self::InvalidRotation { rotate } => write!(f, "旋转角度 {rotate}° 必须是 90° 的倍数。"),
            Self::InvalidTarget => f.write_str("链接或目录的目标无效。"),
            Self::InvalidGeometry => f.write_str("页面坐标或尺寸无效。"),
            Self::InvalidStampImage => f.write_str("图章图片格式无效或无法解码。"),
            Self::AnnotationNotFound => f.write_str("找不到指定的 PDF 标注。"),
            Self::UnsupportedForm => f.write_str("这类 PDF 表单暂不支持。"),
            Self::FieldNotFound => f.write_str("找不到指定的 PDF 表单字段。"),
            Self::FieldReadOnly => f.write_str("指定的 PDF 表单字段只读。"),
            Self::InvalidFieldValue => f.write_str("指定的 PDF 表单值无效。"),
            Self::PdfiumUnavailable => f.write_str("当前引擎未启用 PDFium 编辑。"),
            Self::ObjectNotFound { page, id } => write!(f, "找不到第 {page} 页的对象 {id}。"),
            Self::ObjectNotEditable { page, id } => write!(f, "第 {page} 页的对象 {id} 无法编辑。"),
            Self::ObjectEditFailed { page } => write!(f, "第 {page} 页的对象编辑失败。"),
            Self::RedactionFailed { page } => {
                write!(f, "无法确认第 {page} 页已彻底涂黑，已停止保存。")
            }
            Self::RedactionSharedImage { page } => write!(
                f,
                "第 {page} 页要涂黑的图像仍有其他引用，请把相关位置一并涂黑。"
            ),
            Self::SaveFailed => f.write_str("PDF 保存失败。"),
            Self::EditTextChanged { page } => {
                write!(f, "第 {page} 个输出页的文字层发生变化，已停止保存。")
            }
            Self::FormEditUnavailable => f.write_str("请先烧平可填写表单，再调整页面顺序或旋转。"),
            Self::Mathpix(error) => error.fmt(f),
            Self::Cancelled => f.write_str("已停止。"),
            Self::Unsupported => f.write_str("只有 Mac 上能读 PDF。"),
            Self::Io(_) => f.write_str("读不了这个文件。"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Mathpix(error) => Some(error),
            _ => None,
        }
    }
}

impl From<io::Error> for Error {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_omits_io_details() {
        assert_eq!(
            Error::from(io::Error::other("/private/test.pdf")).to_string(),
            "读不了这个文件。"
        );
    }
}
