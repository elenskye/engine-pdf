//! Mathpix 文档识别。先本地报价，App 确认后再提交。

#[cfg(feature = "fake")]
mod fake;
mod job;
mod mapping;
mod protocol;
mod transport;

use std::{fmt, sync::Arc, time::Duration};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use zeroize::Zeroizing;

use crate::{Document, Error};

#[cfg(feature = "fake")]
pub use fake::*;
pub use job::{delete, fetch, run, submit, submit_file, wait};
pub use mapping::{Mapping, Recognized, RecognizedPage};
pub use transport::{
    MAX_RESPONSE_BYTES, Method, Request, Response, Transport, TransportError, UreqTransport,
};

use protocol::valid_id;

pub const PRICE_PER_PAGE: f64 = 0.005;
const BASE: &str = "https://api.mathpix.com";
const PDF_PATH: &str = "/v3/pdf";

/// Mathpix 认证信息。丢弃时清除字符串，调试输出隐藏密钥。
#[derive(Clone)]
pub struct Keys {
    app_id: Zeroizing<String>,
    app_key: Zeroizing<String>,
}

impl Keys {
    pub fn new(app_id: impl Into<String>, app_key: impl Into<String>) -> Self {
        Self {
            app_id: Zeroizing::new(app_id.into()),
            app_key: Zeroizing::new(app_key.into()),
        }
    }

    /// 自定义传输设置请求头时使用。
    pub fn app_id(&self) -> &str {
        &self.app_id
    }

    /// 自定义传输设置请求头时使用。
    pub fn app_key(&self) -> &str {
        &self.app_key
    }
}

impl fmt::Debug for Keys {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Keys").finish_non_exhaustive()
    }
}

/// 本地报价，页码和单价在创建时确定。
#[derive(Clone, Debug)]
pub struct Quote {
    pages: Vec<u32>,
    price_per_page: f64,
}

impl Quote {
    pub fn service(&self) -> &'static str {
        "Mathpix"
    }

    /// 原始物理页码，升序去重。
    pub fn pages(&self) -> &[u32] {
        &self.pages
    }

    pub fn price_per_page(&self) -> f64 {
        self.price_per_page
    }

    pub fn usd(&self) -> f64 {
        self.pages.len() as f64 * self.price_per_page
    }

    pub fn pages_text(&self) -> String {
        self.pages
            .iter()
            .map(u32::to_string)
            .collect::<Vec<_>>()
            .join("、")
    }
}

/// 使用默认单价在本地报价。
pub fn quote(doc: &Document, pages: &[u32]) -> Result<Quote, Error> {
    quote_at_price(doc, pages, PRICE_PER_PAGE)
}

/// 使用设置中的单价在本地报价。
pub fn quote_with_options(doc: &Document, pages: &[u32], opts: &Options) -> Result<Quote, Error> {
    quote_at_price(doc, pages, opts.price_per_page)
}

fn quote_at_price(doc: &Document, pages: &[u32], price_per_page: f64) -> Result<Quote, Error> {
    if pages.is_empty() {
        return Err(Error::BadPageSpec("请至少选择一页。".into()));
    }
    validate_price(price_per_page)?;
    let total = doc.page_count();
    if let Some(&page) = pages.iter().find(|&&page| page == 0 || page > total) {
        return Err(Error::PageOutOfRange { page, total });
    }

    let mut pages = pages.to_vec();
    pages.sort_unstable();
    pages.dedup();
    Ok(Quote {
        pages,
        price_per_page,
    })
}

fn validate_price(price: f64) -> Result<(), Error> {
    if !price.is_finite() || price < 0.0 || !(price * f64::from(u32::MAX)).is_finite() {
        return Err(MathpixErrorKind::InvalidOptions.into());
    }
    Ok(())
}

/// 提交、轮询和下载设置。App 负责费用确认和预算。
#[derive(Clone)]
pub struct Options {
    pub price_per_page: f64,
    pub lines: bool,
    pub mmd_zip: bool,
    pub math_inline_delimiters: Option<[String; 2]>,
    pub math_display_delimiters: Option<[String; 2]>,
    /// 其他 Mathpix 字段。选页、分页标记、数据留存和请求方式由引擎控制。
    pub extra: Map<String, Value>,
    pub poll_interval: Duration,
    /// 每次等待阶段的上限，包含该阶段的网络请求。
    pub max_wait: Duration,
    pub request_timeout: Duration,
    /// 生产为 Mathpix；测试可用 127.0.0.1。
    pub base: String,
    pub transport: Arc<dyn Transport>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            price_per_page: PRICE_PER_PAGE,
            lines: false,
            mmd_zip: false,
            math_inline_delimiters: None,
            math_display_delimiters: None,
            extra: Map::new(),
            poll_interval: Duration::from_secs(2),
            max_wait: Duration::from_secs(600),
            request_timeout: Duration::from_secs(120),
            base: BASE.into(),
            transport: Arc::new(UreqTransport::default()),
        }
    }
}

/// 调用方可保存的续跑状态，包含页码映射、账单基准和下载设置。
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Submitted {
    pdf_id: String,
    page_map: Vec<u32>,
    pages_sent: u32,
    options: SubmittedOptions,
    #[serde(default)]
    num_pages: Option<u32>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct SubmittedOptions {
    price_per_page: f64,
    lines: bool,
    mmd_zip: bool,
    base: String,
    simulated: bool,
}

impl Submitted {
    pub fn pdf_id(&self) -> &str {
        &self.pdf_id
    }

    pub fn page_map(&self) -> &[u32] {
        &self.page_map
    }

    /// PDF 的实际送出页数，或 App 提供的 Word 分页估计。Word 未知时为 0。
    pub fn pages_sent(&self) -> u32 {
        self.pages_sent
    }

    /// 成功等待后服务报告的页数。
    pub fn num_pages(&self) -> Option<u32> {
        self.num_pages
    }

    fn usage(&self, success: bool) -> Usage {
        let reported = if success { self.num_pages } else { None };
        let estimated_pages = if self.pages_sent == 0 {
            self.num_pages.unwrap_or_default()
        } else {
            self.pages_sent
        };
        let pages_billed = if self.options.simulated {
            0
        } else {
            reported.unwrap_or(estimated_pages)
        };
        Usage {
            pdf_id: Some(self.pdf_id.clone()),
            pages_billed,
            usd: f64::from(pages_billed) * self.options.price_per_page,
            estimated: !self.options.simulated && reported.is_none(),
            remote_deleted: None,
        }
    }

    fn failure(&self, error: Error) -> Failure {
        Failure {
            error,
            usage: self.usage(false),
        }
    }

    fn status_path(&self, opts: &Options) -> Result<String, Error> {
        if !valid_id(&self.pdf_id)
            || self.options.base != opts.base
            || self.options.simulated != opts.transport.is_simulated()
        {
            return Err(MathpixErrorKind::InvalidOptions.into());
        }
        validate_price(self.options.price_per_page)?;
        Ok(format!("{PDF_PATH}/{}", self.pdf_id))
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Usage {
    pub pdf_id: Option<String>,
    pub pages_billed: u32,
    pub usd: f64,
    pub estimated: bool,
    pub remote_deleted: Option<bool>,
}

#[derive(Debug)]
pub struct Failure {
    pub error: Error,
    pub usage: Usage,
}

impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.error.fmt(f)
    }
}

impl std::error::Error for Failure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.error)
    }
}

impl From<Error> for Failure {
    fn from(error: Error) -> Self {
        Self {
            error,
            usage: Usage::default(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MathpixError {
    pub kind: MathpixErrorKind,
    /// 去掉密钥并截短的服务说明，供 App 排查问题。
    pub detail: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MathpixErrorKind {
    Network,
    Auth,
    Quota,
    TooLarge,
    RateLimited,
    Service,
    Timeout,
    InvalidOptions,
    InvalidResponse,
    Processing,
    ResultMissing,
}

impl From<MathpixErrorKind> for Error {
    fn from(kind: MathpixErrorKind) -> Self {
        Self::Mathpix(MathpixError { kind, detail: None })
    }
}

impl fmt::Display for MathpixError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use MathpixErrorKind::*;
        f.write_str(match self.kind {
            Network => "连不上 Mathpix。",
            Auth => "Mathpix 认证失败。",
            Quota => "Mathpix 的余额或页数额度不足。",
            TooLarge => "文件大小或页数超过 Mathpix 的限制。",
            RateLimited => "Mathpix 请求过于频繁。",
            Service => "Mathpix 服务出错了。",
            Timeout => "等待 Mathpix 超时。",
            InvalidOptions => "Mathpix 的调用设置无效。",
            InvalidResponse => "读不懂 Mathpix 的回应。",
            Processing => "Mathpix 没能识别这份文件。",
            ResultMissing => "Mathpix 的识别结果暂不可用。",
        })
    }
}

impl std::error::Error for MathpixError {}

/// 轮询中的进度。下载以服务状态为准，识别完成比例可能先到 100。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Progress {
    pub percent_done: Option<f64>,
    pub num_pages: Option<u32>,
    pub num_pages_completed: Option<u32>,
}
