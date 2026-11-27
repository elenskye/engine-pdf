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

impl From<engine_core::Cancelled> for Error {
    fn from(_: engine_core::Cancelled) -> Self {
        Self::Cancelled
    }
}

impl From<Error> for engine_core::call::ToolError {
    fn from(error: Error) -> Self {
        use crate::mathpix::MathpixErrorKind as Mathpix;

        let (kind, hint, retry) = match &error {
            Error::NotPdf => ("not_pdf", "请换一份 PDF。", false),
            Error::Damaged => ("damaged_pdf", "请用原始文件或重新导出 PDF。", false),
            Error::Locked => ("locked", "请让用户提供 PDF 的打开密码。", false),
            Error::WrongPassword => ("wrong_password", "请让用户重新提供 PDF 密码。", false),
            Error::OwnerPasswordRequired => (
                "owner_password_required",
                "请让用户提供 PDF 的权限密码，再执行本次操作。",
                false,
            ),
            Error::PasswordActionRequired => (
                "password_action_required",
                "请让用户明确选择输出的加密方式。",
                false,
            ),
            Error::PageOutOfRange { .. } | Error::BadPageSpec(_) => {
                ("bad_args", "请按原文件的物理页码选择页面。", false)
            }
            Error::Render { .. } => ("render_failed", "请尝试读取文字或换一个页码。", false),
            Error::Ocr { .. } => ("ocr_failed", "请检查页图，或缩小识别范围。", false),
            Error::ImageInput { .. } | Error::InvalidStampImage => {
                ("invalid_image", "请换一张可打开的图片，再提交改动。", false)
            }
            Error::AnnotationNotFound | Error::FieldNotFound | Error::ObjectNotFound { .. } => (
                "item_not_found",
                "请从同一份 PDF 重新列出标注、字段或页面对象，再选取要改的项目。",
                false,
            ),
            Error::PdfiumUnavailable => (
                "pdfium_unavailable",
                "请使用启用了 PDFium 的 Mac 构建。",
                false,
            ),
            Error::ObjectNotEditable { .. } | Error::ObjectEditFailed { .. } => (
                "object_edit_failed",
                "请缩小改动范围，或改用 PDF 编辑器处理这一页。",
                false,
            ),
            Error::RedactionFailed { .. } | Error::RedactionSharedImage { .. } => (
                "redaction_failed",
                "请核对区域和其他出现位置，调整范围后重新确认。",
                false,
            ),
            Error::SaveFailed | Error::EditTextChanged { .. } => (
                "save_failed",
                "请缩小改动范围，或换一份 PDF 重新操作。",
                false,
            ),
            Error::FormEditUnavailable => (
                "form_edit_unavailable",
                "请让用户决定是否烧平表单，再调整页面。",
                false,
            ),
            Error::UnsupportedForm | Error::FieldReadOnly | Error::Unsupported => {
                ("unsupported", "请换一种处理方式。", false)
            }
            Error::InvalidPasswordOptions
            | Error::EmptyImages
            | Error::EmptyAssembly
            | Error::InvalidSource { .. }
            | Error::InvalidRotation { .. }
            | Error::InvalidTarget
            | Error::InvalidGeometry
            | Error::InvalidFieldValue => ("bad_args", "请按工具说明改参数后重试。", false),
            Error::Cancelled => ("cancelled", "需要时可重新调用。", true),
            Error::Io(_) => ("file", "请检查文件和访问权限，或换一份文件。", false),
            Error::Mathpix(error) => match error.kind {
                Mathpix::Network => ("network", "请检查连接。已有任务时先恢复原任务。", true),
                Mathpix::Timeout => ("timeout", "已有任务时先查任务状态，再决定是否重试。", true),
                Mathpix::RateLimited => ("rate_limited", "请稍等片刻再试。", true),
                Mathpix::Auth => ("auth", "请让用户检查 Mathpix 密钥。", false),
                Mathpix::Quota => ("quota", "请让用户检查 Mathpix 余额和额度。", false),
                Mathpix::TooLarge => ("too_large", "请减少选中的物理页数。", false),
                Mathpix::InvalidOptions => ("bad_args", "请检查调用设置。", false),
                Mathpix::Service | Mathpix::InvalidResponse => {
                    ("service", "已有任务时先查任务状态，再决定是否重试。", true)
                }
                Mathpix::Processing => {
                    ("mathpix_processing", "请检查页图，或改用本机识别。", false)
                }
                Mathpix::ResultMissing => (
                    "mathpix_result_missing",
                    "请先检查原任务的识别结果是否已生成。",
                    true,
                ),
            },
        };
        let message = match &error {
            Error::BadPageSpec(_) => "指定的 PDF 页码范围无效。".into(),
            _ => error.to_string(),
        };
        Self::new(kind, message, hint, retry)
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

    #[test]
    fn tool_error_omits_untrusted_details_and_keeps_remediation() {
        use crate::mathpix::{MathpixError, MathpixErrorKind};
        use engine_core::call::ToolError;

        let bad_pages = ToolError::from(Error::BadPageSpec("/private/test.pdf".into()));
        assert_eq!(bad_pages.kind, "bad_args");
        assert!(!bad_pages.retry);
        assert!(
            !serde_json::to_string(&bad_pages)
                .unwrap()
                .contains("/private")
        );

        let timeout = ToolError::from(Error::Mathpix(MathpixError {
            kind: MathpixErrorKind::Timeout,
            detail: Some("private cloud response".into()),
        }));
        assert_eq!(timeout.kind, "timeout");
        assert!(timeout.retry);
        assert!(timeout.hint.contains("任务状态"));
        assert_eq!(timeout.message, "等待 Mathpix 超时。");
    }
}
