use std::{fmt, time::Duration};

use super::Keys;

#[cfg(target_os = "macos")]
const _: () = assert!(
    !matches!(log::STATIC_MAX_LEVEL, log::LevelFilter::Trace),
    "Mathpix HTTP 客户端要求禁用 log 的 TRACE 级别",
);

/// 同步发送 Mathpix 请求，返回 HTTP 状态、响应体及可选的重试间隔。
pub trait Transport: Send + Sync {
    fn send(&self, request: Request<'_>) -> Result<Response, TransportError>;

    /// 完全模拟请求的传输实现可返回 true，模拟结果的费用记为零。
    fn is_simulated(&self) -> bool {
        false
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Method {
    Get,
    Post,
    Delete,
}

/// 单次传输请求。密钥和上传件只在调用期间借用。
pub struct Request<'a> {
    pub method: Method,
    pub base: &'a str,
    pub path: &'a str,
    pub keys: &'a Keys,
    pub content_type: Option<&'a str>,
    pub body: &'a [u8],
    pub timeout: Duration,
}

/// HTTP 响应。包括非成功状态，交给调用方分类。
pub struct Response {
    pub status: u16,
    pub body: Vec<u8>,
    pub retry_after: Option<Duration>,
}

impl fmt::Debug for Response {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Response")
            .field("status", &self.status)
            .field("body_len", &self.body.len())
            .field("retry_after", &self.retry_after)
            .finish()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TransportError {
    Network,
    Timeout,
    InvalidRequest,
    /// 响应体超过 [`MAX_RESPONSE_BYTES`]。
    TooLarge,
    Unsupported,
}

/// 默认客户端最多读这么多字节的响应（结果压缩包带裁图，整本书也够用），防止异常响应占满内存。
pub const MAX_RESPONSE_BYTES: u64 = 1 << 30;

impl fmt::Display for TransportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Network => "连不上 Mathpix。",
            Self::Timeout => "Mathpix 请求超时。",
            Self::InvalidRequest => "Mathpix 请求参数无效。",
            Self::TooLarge => "Mathpix 的回应太大。",
            Self::Unsupported => "只有 Mac 支持 Mathpix HTTP 客户端。",
        })
    }
}

impl std::error::Error for TransportError {}

/// 默认 HTTP 客户端，使用系统 TLS，直接连接 Mathpix。
#[cfg_attr(not(target_os = "macos"), derive(Default))]
pub struct UreqTransport {
    #[cfg(target_os = "macos")]
    agent: ureq::Agent,
}

pub(super) fn valid_base(base: &str) -> bool {
    if base == "https://api.mathpix.com" {
        return true;
    }
    match base.strip_prefix("http://127.0.0.1") {
        Some("") => true,
        Some(rest) => rest.strip_prefix(':').is_some_and(|port| {
            port.bytes().all(|byte| byte.is_ascii_digit())
                && port.parse::<u16>().is_ok_and(|port| port > 0)
        }),
        None => false,
    }
}

#[cfg(target_os = "macos")]
impl Default for UreqTransport {
    fn default() -> Self {
        use ureq::tls::{RootCerts, TlsConfig, TlsProvider};

        let config = ureq::Agent::config_builder()
            .tls_config(
                TlsConfig::builder()
                    .provider(TlsProvider::NativeTls)
                    .root_certs(RootCerts::PlatformVerifier)
                    .build(),
            )
            .proxy(None)
            .max_redirects(0)
            .http_status_as_error(false)
            .build();
        Self {
            agent: ureq::Agent::new_with_config(config),
        }
    }
}

#[cfg(target_os = "macos")]
impl Transport for UreqTransport {
    fn send(&self, request: Request<'_>) -> Result<Response, TransportError> {
        use ureq::http::{self, header::HeaderValue};

        if !valid_base(request.base) || !request.path.starts_with('/') || request.timeout.is_zero()
        {
            return Err(TransportError::InvalidRequest);
        }

        let method = match request.method {
            Method::Get => http::Method::GET,
            Method::Post => http::Method::POST,
            Method::Delete => http::Method::DELETE,
        };
        let mut app_id = HeaderValue::from_str(request.keys.app_id())
            .map_err(|_| TransportError::InvalidRequest)?;
        let mut app_key = HeaderValue::from_str(request.keys.app_key())
            .map_err(|_| TransportError::InvalidRequest)?;
        app_id.set_sensitive(true);
        app_key.set_sensitive(true);
        let mut builder = http::Request::builder()
            .method(method)
            .uri(format!("{}{}", request.base, request.path))
            .header("app_id", app_id)
            .header("app_key", app_key);
        if let Some(content_type) = request.content_type {
            builder = builder.header(http::header::CONTENT_TYPE, content_type);
        }
        let outgoing = builder
            .body(())
            .map_err(|_| TransportError::InvalidRequest)?;
        let outgoing = self
            .agent
            .configure_request(outgoing)
            .timeout_global(Some(request.timeout))
            .build();
        let mut response = match request.method {
            Method::Post => self.agent.run(outgoing.map(|()| request.body)),
            _ => self.agent.run(outgoing),
        }?;

        let retry_after = response
            .headers()
            .get(http::header::RETRY_AFTER)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse().ok())
            .map(Duration::from_secs);
        Ok(Response {
            status: response.status().as_u16(),
            body: response
                .body_mut()
                .with_config()
                .limit(MAX_RESPONSE_BYTES)
                .read_to_vec()?,
            retry_after,
        })
    }
}

#[cfg(target_os = "macos")]
impl From<ureq::Error> for TransportError {
    fn from(error: ureq::Error) -> Self {
        match error {
            ureq::Error::Timeout(_) => Self::Timeout,
            ureq::Error::Io(error) if error.kind() == std::io::ErrorKind::TimedOut => Self::Timeout,
            ureq::Error::Http(_) | ureq::Error::BadUri(_) => Self::InvalidRequest,
            ureq::Error::BodyExceedsLimit(_) => Self::TooLarge,
            _ => Self::Network,
        }
    }
}

#[cfg(not(target_os = "macos"))]
impl Transport for UreqTransport {
    fn send(&self, _: Request<'_>) -> Result<Response, TransportError> {
        Err(TransportError::Unsupported)
    }
}

#[cfg(test)]
mod tests {
    use super::valid_base;

    #[test]
    fn restricts_credentials_to_mathpix_and_loopback() {
        for base in [
            "https://api.mathpix.com",
            "http://127.0.0.1",
            "http://127.0.0.1:12345",
        ] {
            assert!(valid_base(base), "{base}");
        }
        for base in [
            "http://api.mathpix.com",
            "https://api.mathpix.com.evil.test",
            "https://api.mathpix.com@other.test",
            "https://api.mathpix.com/v3",
            "https://api.mathpix.com?next=other.test",
            "http://127.0.0.10",
            "http://127.0.0.1:80@other.test",
            "http://127.0.0.1:+80",
            "http://127.0.0.1:0",
            "http://127.0.0.1:65536",
        ] {
            assert!(!valid_base(base), "{base}");
        }
    }
}
