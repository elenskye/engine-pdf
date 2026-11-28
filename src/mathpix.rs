//! Mathpix 文档识别。先本地报价，照花钱的规矩（`spend`）要问时问过再提交。

mod convert;
#[cfg(feature = "fake")]
mod fake;
mod job;
mod mapping;
mod protocol;
pub mod spend;
mod transport;

use std::{fmt, sync::Arc, time::Duration};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use zeroize::Zeroizing;

use crate::{Document, Error};

pub use convert::{CONVERT_FORMATS, Converted, MAX_CONVERT_BYTES, convert};
#[cfg(feature = "fake")]
pub use fake::*;
pub use job::{delete, fetch, run, submit, submit_file, wait};
pub use mapping::{Mapping, Recognized, RecognizedPage};
pub use transport::{
    MAX_RESPONSE_BYTES, Method, Request, Response, Transport, TransportError, UreqTransport,
};

use protocol::valid_id;

pub const PRICE_PER_PAGE: f64 = 0.005;
pub(crate) const SERVICE: &str = "Mathpix";
pub(crate) const UNIT: &str = "page";
const BASE: &str = "https://api.mathpix.com";
const PDF_PATH: &str = "/v3/pdf";
const CONVERTER_PATH: &str = "/v3/converter";

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

    pub(crate) fn from_zeroizing(app_id: Zeroizing<String>, app_key: Zeroizing<String>) -> Self {
        Self { app_id, app_key }
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
        SERVICE
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

impl From<&Quote> for engine_core::call::Quote {
    fn from(quote: &Quote) -> Self {
        Self {
            service: quote.service().into(),
            unit: UNIT.into(),
            units: quote.pages().len() as f64,
            usd: quote.usd(),
        }
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

pub(crate) fn quote_at_price(
    doc: &Document,
    pages: &[u32],
    price_per_page: f64,
) -> Result<Quote, Error> {
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

/// 提交、轮询和下载设置。要不要先问用户照 `spend` 的规矩（工具层自己问），每次上传记进 `ledger`。
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
    /// 花钱的账本放在哪：默认所有 App 共用的那份；开发版、测试用 `At(底)` 或 `Off`。模拟的传输不记。
    /// 工具包（`tools::toolkit_with_mathpix`）照它定要不要问，`job` 照它记账。
    pub ledger: spend::Ledger,
    /// 记进账本的 App 名字（Peck、Pith、Pomelo）。
    pub app: String,
}

/// 账本里本引擎的短名（`<底>/engine-pdf/…`）。
pub(crate) const ENGINE: &str = "pdf";

impl Options {
    /// Mathpix 这一份账：问不问（`approval`）、记一笔（`record`）、App 报告花了多少（`entries`）都用它。
    pub fn book(&self) -> spend::Book {
        self.ledger.book(ENGINE, SERVICE)
    }
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
            ledger: spend::Ledger::Shared,
            app: String::new(),
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
    /// 账已经按服务报的页数补过（Word 上传时只记了估计；PDF 上传时就记准了）。
    #[serde(default)]
    settled: bool,
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
    /// 用引擎之前（如 Pomelo 1.2 以前）就上传了的整份文件接着等、下载：`pages` 是那份 PDF 的页数，
    /// Word 为 `None`。那次上传不在账本里，这里也不补记；按 `opts` 的价格和下载设置。
    pub fn uploaded(pdf_id: &str, pages: Option<u32>, opts: &Options) -> Result<Self, Error> {
        validate_price(opts.price_per_page)?;
        if !valid_id(pdf_id) || pages == Some(0) {
            return Err(MathpixErrorKind::InvalidOptions.into());
        }
        Ok(Self {
            pdf_id: pdf_id.into(),
            page_map: pages.map_or_else(Vec::new, |n| (1..=n).collect()),
            pages_sent: pages.unwrap_or_default(),
            options: SubmittedOptions {
                price_per_page: opts.price_per_page,
                lines: opts.lines,
                mmd_zip: opts.mmd_zip,
                base: opts.base.clone(),
                simulated: opts.transport.is_simulated(),
            },
            num_pages: None,
            settled: true,
        })
    }

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

impl From<&Usage> for engine_core::call::Usage {
    fn from(usage: &Usage) -> Self {
        Self {
            service: SERVICE.into(),
            unit: UNIT.into(),
            units: f64::from(usage.pages_billed),
            usd: usage.usd,
            estimated: usage.estimated,
        }
    }
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

impl From<engine_core::Cancelled> for Failure {
    fn from(error: engine_core::Cancelled) -> Self {
        Error::from(error).into()
    }
}

impl From<Failure> for engine_core::call::ToolError {
    fn from(Failure { error, usage }: Failure) -> Self {
        Self::from(error).with_usage((usage != Usage::default()).then(|| (&usage).into()))
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
