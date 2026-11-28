//! 转格式：把 Mathpix Markdown 交给 `/v3/converter` 转成 Word、PDF、LaTeX、HTML 等，不重新识别。
//! Mathpix 的价目表里没有这一项，当不收费：不报价、不问、不记账（`decisions/0024`）。
//! MMD 里的图由 Mathpix 照链接去取，本机路径的图 App 先换成链接或拿掉。

use std::time::Instant;

use serde_json::{Map, Value, json};

use super::job::{pause, remaining};
use super::protocol::{json_response, response_error, transport_error, valid_id};
use super::transport::valid_base;
use super::{CONVERTER_PATH, Keys, MathpixErrorKind, Method, Options, Request};
use crate::{Cancel, Error};

/// 能要的格式（Mathpix 的写法，也是文件的后缀；zip 里带图）。
pub const CONVERT_FORMATS: [&str; 7] = [
    "docx",
    "pdf",
    "latex.pdf",
    "tex.zip",
    "html.zip",
    "md.zip",
    "pptx",
];
/// Mathpix 收的请求体上限。
pub const MAX_CONVERT_BYTES: usize = 10_000_000;

/// 一种格式转出来的文件，或它为什么没转成。
#[derive(Debug)]
pub struct Converted {
    pub format: String,
    pub result: Result<Vec<u8>, Error>,
}

/// 交上去、等每种格式转好（最多 `opts.max_wait`）、逐个下载。整次交不上去报 `Err`；某种格式没转成的
/// 写在它自己的 `result` 里，别的照样交回。
pub fn convert(
    mmd: &str,
    formats: &[&str],
    keys: &Keys,
    opts: &Options,
    cancel: &Cancel,
) -> Result<Vec<Converted>, Error> {
    cancel.check()?;
    let mut wanted: Vec<&str> = vec![];
    for format in formats {
        if !CONVERT_FORMATS.contains(format) {
            return Err(MathpixErrorKind::InvalidOptions.into());
        }
        if !wanted.contains(format) {
            wanted.push(format);
        }
    }
    if wanted.is_empty()
        || mmd.trim().is_empty()
        || !valid_base(&opts.base)
        || opts.request_timeout.is_zero()
        || opts.max_wait.is_zero()
    {
        return Err(MathpixErrorKind::InvalidOptions.into());
    }
    let body = json!({
        "mmd": mmd,
        "formats": wanted.iter().map(|f| (f.to_string(), Value::Bool(true))).collect::<Map<_, _>>(),
        "metadata": { "improve_mathpix": false },
    })
    .to_string();
    if body.len() > MAX_CONVERT_BYTES {
        return Err(MathpixErrorKind::TooLarge.into());
    }
    let start = Instant::now();
    let response = opts
        .transport
        .send(Request {
            method: Method::Post,
            base: &opts.base,
            path: CONVERTER_PATH,
            keys,
            content_type: Some("application/json"),
            body: body.as_bytes(),
            timeout: opts.request_timeout,
        })
        .map_err(transport_error)?;
    let reply = json_response(&response, keys)?;
    let id = reply["conversion_id"]
        .as_str()
        .filter(|id| valid_id(id) && *id != keys.app_id() && *id != keys.app_key())
        .ok_or(MathpixErrorKind::InvalidResponse)?
        .to_owned();
    let path = format!("{CONVERTER_PATH}/{id}");

    // 等每种格式不再是 processing（超时的那几种报 Timeout）
    let states = loop {
        cancel.check()?;
        let timeout = match remaining(start, opts.max_wait) {
            Ok(left) => left.min(opts.request_timeout),
            Err(_) => break None,
        };
        let response = opts
            .transport
            .send(get(opts, keys, &path, timeout))
            .map_err(transport_error)?;
        let body = json_response(&response, keys)?;
        let states: Vec<(String, String)> = wanted
            .iter()
            .map(|f| {
                let state = body["conversion_status"][*f]["status"]
                    .as_str()
                    .unwrap_or("processing");
                (f.to_string(), state.to_owned())
            })
            .collect();
        if states.iter().all(|(_, s)| s != "processing") {
            break Some(states);
        }
        if pause(start, opts.max_wait, opts.poll_interval, cancel).is_err() {
            cancel.check()?;
            break None;
        }
    };

    let mut out = vec![];
    for format in &wanted {
        let state = states
            .as_ref()
            .and_then(|s| s.iter().find(|(f, _)| f == format))
            .map_or("processing", |(_, s)| s.as_str());
        let result = match state {
            "completed" => download(&path, format, keys, opts, start, cancel),
            "processing" => Err(MathpixErrorKind::Timeout.into()),
            _ => Err(MathpixErrorKind::Processing.into()),
        };
        if let Err(Error::Cancelled) = &result {
            return Err(Error::Cancelled);
        }
        out.push(Converted {
            format: format.to_string(),
            result,
        });
    }
    Ok(out)
}

fn get<'a>(
    opts: &'a Options,
    keys: &'a Keys,
    path: &'a str,
    timeout: std::time::Duration,
) -> Request<'a> {
    Request {
        method: Method::Get,
        base: &opts.base,
        path,
        keys,
        content_type: None,
        body: &[],
        timeout,
    }
}

/// 一种格式的文件；还没好（202、404，或交回的是 JSON）就隔一会儿再要。
fn download(
    path: &str,
    format: &str,
    keys: &Keys,
    opts: &Options,
    start: Instant,
    cancel: &Cancel,
) -> Result<Vec<u8>, Error> {
    let path = format!("{path}.{format}");
    loop {
        cancel.check()?;
        let timeout = remaining(start, opts.max_wait)?.min(opts.request_timeout);
        let reply = opts
            .transport
            .send(get(opts, keys, &path, timeout))
            .map_err(transport_error)?;
        cancel.check()?;
        let json: Option<Value> =
            if reply.body.iter().find(|b| !b.is_ascii_whitespace()) == Some(&b'{') {
                serde_json::from_slice(&reply.body).ok()
            } else {
                None
            };
        let pending = matches!(reply.status, 202 | 404) || json.is_some();
        if !pending {
            if let Some(error) = response_error(reply.status, None, keys) {
                return Err(error);
            }
            if reply.body.is_empty() {
                return Err(MathpixErrorKind::ResultMissing.into());
            }
            return Ok(reply.body);
        }
        if let Some(error) = json
            .as_ref()
            .and_then(|body| response_error(200, Some(body), keys))
        {
            return Err(error);
        }
        pause(
            start,
            opts.max_wait,
            reply.retry_after.unwrap_or(opts.poll_interval),
            cancel,
        )?;
    }
}
