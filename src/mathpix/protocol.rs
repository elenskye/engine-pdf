use std::time::Duration;

use serde_json::{Map, Value, json};

use crate::{Cancel, Error};

use super::transport::valid_base;
use super::{
    Keys, MathpixError, MathpixErrorKind, Method, Options, Request, Response, TransportError,
};

pub(super) fn multipart(
    bytes: &[u8],
    mime: &str,
    filename: &str,
    opts: &Options,
) -> (String, Vec<u8>) {
    let mut options = opts.extra.clone();
    // 上传件的页已经按报价固定，响应要使用可轮询的普通文档模式。
    for key in ["url", "page_ranges", "streaming"] {
        options.remove(key);
    }
    options.insert("include_page_breaks".into(), Value::Bool(true));
    let mut metadata = match options.remove("metadata") {
        Some(Value::Object(metadata)) => metadata,
        _ => Map::new(),
    };
    metadata.insert("improve_mathpix".into(), Value::Bool(false));
    options.insert("metadata".into(), Value::Object(metadata));
    if let Some(delimiters) = &opts.math_inline_delimiters {
        options.insert("math_inline_delimiters".into(), json!(delimiters));
    }
    if let Some(delimiters) = &opts.math_display_delimiters {
        options.insert("math_display_delimiters".into(), json!(delimiters));
    }
    if opts.mmd_zip {
        let formats = options
            .entry("conversion_formats")
            .or_insert_with(|| Value::Object(Map::new()));
        if !formats.is_object() {
            *formats = Value::Object(Map::new());
        }
        formats["mmd.zip"] = Value::Bool(true);
    }
    let options = Value::Object(options).to_string();

    // 只要边界与任一部分中的分隔行不冲突，就无需生成随机数。
    let boundary = (0_u64..)
        .map(|n| format!("engine-pdf-{n}"))
        .find(|boundary| {
            let marker = format!("\r\n--{boundary}");
            !bytes.starts_with(&marker.as_bytes()[2..])
                && !bytes
                    .windows(marker.len())
                    .any(|window| window == marker.as_bytes())
        })
        .expect("有限长度的上传件存在可用的 multipart 分隔符");
    let mut body = Vec::with_capacity(bytes.len() + options.len() + 256);
    body.extend_from_slice(format!(
        "--{boundary}\r\nContent-Disposition: form-data; name=\"options_json\"\r\n\r\n{options}\r\n\
         --{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{filename}\"\r\n\
         Content-Type: {mime}\r\n\r\n"
    ).as_bytes());
    body.extend_from_slice(bytes);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    (format!("multipart/form-data; boundary={boundary}"), body)
}

pub(super) fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

pub(super) fn request(
    opts: &Options,
    keys: &Keys,
    method: Method,
    path: &str,
    timeout: Duration,
) -> Result<Response, Error> {
    if !valid_base(&opts.base) || timeout.is_zero() {
        return Err(MathpixErrorKind::InvalidOptions.into());
    }
    opts.transport
        .send(Request {
            method,
            base: &opts.base,
            path,
            keys,
            content_type: None,
            body: &[],
            timeout,
        })
        .map_err(transport_error)
}

pub(super) fn download(
    opts: &Options,
    keys: &Keys,
    path: &str,
    timeout: Duration,
    cancel: &Cancel,
) -> Result<Response, Error> {
    cancel.check()?;
    let response = request(opts, keys, Method::Get, path, timeout);
    cancel.check()?;
    response
}

pub(super) fn transport_error(error: TransportError) -> Error {
    match error {
        TransportError::Unsupported => return Error::Unsupported,
        TransportError::Network => MathpixErrorKind::Network,
        TransportError::Timeout => MathpixErrorKind::Timeout,
        TransportError::InvalidRequest => MathpixErrorKind::InvalidOptions,
        TransportError::TooLarge => MathpixErrorKind::TooLarge,
    }
    .into()
}

pub(super) fn json_response(response: &Response, keys: &Keys) -> Result<Value, Error> {
    let body = serde_json::from_slice(&response.body).ok();
    if let Some(error) = response_error(response.status, body.as_ref(), keys) {
        return Err(error);
    }
    body.ok_or_else(|| MathpixErrorKind::InvalidResponse.into())
}

pub(super) fn check_response(response: &Response, keys: &Keys) -> Result<(), Error> {
    let body = if response
        .body
        .iter()
        .find(|byte| !byte.is_ascii_whitespace())
        == Some(&b'{')
    {
        serde_json::from_slice(&response.body).ok()
    } else {
        None
    };
    match response_error(response.status, body.as_ref(), keys) {
        Some(error) => Err(error),
        None => Ok(()),
    }
}

pub(super) fn response_error(status: u16, body: Option<&Value>, keys: &Keys) -> Option<Error> {
    use MathpixErrorKind::*;

    let id = body.and_then(|body| body["error_info"]["id"].as_str());
    let message = body.and_then(|body| {
        body["error_info"]["message"]
            .as_str()
            .or_else(|| body["error"].as_str())
    });
    let kind = match (status, id) {
        (401 | 403, _) | (_, Some("http_unauthorized")) => Auth,
        (402, _) | (_, Some("quota_exceeded" | "http_payment_required")) => Quota,
        (413, _)
        | (
            _,
            Some(
                "pdf_page_limit_exceeded"
                | "pdf_too_many_pages"
                | "image_max_size"
                | "sys_request_too_large",
            ),
        ) => TooLarge,
        (429, _) | (_, Some("http_max_requests" | "rate_limit_exceeded")) => RateLimited,
        (408 | 504, _) => Timeout,
        (500..=599, _) => Service,
        (202 | 404, _) | (_, Some("pdf_unknown_id")) => ResultMissing,
        (_, Some("sys_exception")) => Service,
        (200..=299, _)
            if id.is_none()
                && message.is_none()
                && body
                    .and_then(|value| value["error_info"].as_object())
                    .is_none() =>
        {
            return None;
        }
        _ => Processing,
    };
    let detail = message.or(id).filter(|text| !text.is_empty()).map(|text| {
        let mut text = text.to_owned();
        let mut secrets = [keys.app_id(), keys.app_key()];
        secrets.sort_by_key(|value| std::cmp::Reverse(value.len()));
        for secret in secrets.into_iter().filter(|value| !value.is_empty()) {
            text = text.replace(secret, "[已隐藏]");
        }
        text.chars()
            .take(160)
            .map(|ch| if ch.is_control() { ' ' } else { ch })
            .collect()
    });
    Some(Error::Mathpix(MathpixError { kind, detail }))
}
