use std::{fmt, io};

/// 引擎的错误。`Display` 是给人看的一句中文，不带路径、密钥和服务原文。
#[derive(Debug)]
pub enum Error {
    NotPdf,
    Damaged,
    Locked,
    PageOutOfRange {
        page: u32,
        total: u32,
    },
    /// 引擎生成的页码错误提示。
    BadPageSpec(String),
    Render {
        page: u32,
    },
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
            Self::PageOutOfRange { page, total } => {
                write!(f, "这份 PDF 只有 {total} 页，没有第 {page} 页。")
            }
            Self::BadPageSpec(message) => f.write_str(message),
            Self::Render { page } => write!(f, "第 {page} 页画不出来。"),
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
