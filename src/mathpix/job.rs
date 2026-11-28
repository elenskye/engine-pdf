//! 一次识别的流程：提交、等待、下载、删除；`run` 把四步连起来，不管成败都删掉远端的。

use std::{
    borrow::Cow,
    thread,
    time::{Duration, Instant},
};

use serde_json::Value;

use super::protocol::{
    self, check_response, download, json_response, multipart, request, response_error, valid_id,
};
use super::transport::valid_base;
use super::{
    Failure, Keys, MathpixErrorKind, Method, Options, PDF_PATH, Progress, Quote, Recognized,
    Request, Submitted, SubmittedOptions, mapping, validate_price,
};
use crate::{Cancel, Document, Error};

/// 提交报价中的物理页。返回后 App 可以立刻保存任务以供续跑。
pub fn submit(
    doc: &Document,
    quote: &Quote,
    keys: &Keys,
    opts: &Options,
    cancel: &Cancel,
) -> Result<Submitted, Failure> {
    cancel.check()?;
    let total = doc.page_count();
    if let Some(&page) = quote.pages.last().filter(|&&page| page > total) {
        return Err(Error::PageOutOfRange { page, total }.into());
    }
    let pages_sent = quote.pages.len() as u32;
    let bytes = if pages_sent == total {
        Cow::Borrowed(doc.original_bytes())
    } else {
        let bytes = doc.split(&quote.pages, cancel)?;
        if Document::from_bytes(&bytes)?.page_count() != pages_sent {
            return Err(Error::Damaged.into());
        }
        Cow::Owned(bytes)
    };

    upload(
        (&bytes, "application/pdf", "pages.pdf"),
        quote.pages.clone(),
        pages_sent,
        quote.price_per_page,
        keys,
        opts,
        cancel,
    )
}

/// 提交 Word 文档。App 已知预计页数时可传入，用于服务未报页数时记账。
pub fn submit_file(
    bytes: &[u8],
    mime: &str,
    estimated_pages: Option<u32>,
    keys: &Keys,
    opts: &Options,
    cancel: &Cancel,
) -> Result<Submitted, Failure> {
    cancel.check()?;
    validate_price(opts.price_per_page)?;
    let filename = match mime {
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document" => {
            "document.docx"
        }
        "application/msword" => "document.doc",
        _ => return Err(Error::from(MathpixErrorKind::InvalidOptions).into()),
    };
    upload(
        (bytes, mime, filename),
        Vec::new(),
        estimated_pages.unwrap_or_default(),
        opts.price_per_page,
        keys,
        opts,
        cancel,
    )
}

fn upload(
    (bytes, mime, filename): (&[u8], &str, &str),
    page_map: Vec<u32>,
    pages_sent: u32,
    price_per_page: f64,
    keys: &Keys,
    opts: &Options,
    cancel: &Cancel,
) -> Result<Submitted, Failure> {
    if !valid_base(&opts.base) || bytes.is_empty() || opts.request_timeout.is_zero() {
        return Err(Error::from(MathpixErrorKind::InvalidOptions).into());
    }
    let (content_type, body) = multipart(bytes, mime, filename, opts);
    cancel.check()?;
    let response = opts
        .transport
        .send(Request {
            method: Method::Post,
            base: &opts.base,
            path: PDF_PATH,
            keys,
            content_type: Some(&content_type),
            body: &body,
            timeout: opts.request_timeout,
        })
        .map_err(protocol::transport_error)?;
    let body: Option<Value> = serde_json::from_slice(&response.body).ok();
    let error = response_error(response.status, body.as_ref(), keys);
    let pdf_id = body
        .as_ref()
        .and_then(|body| body["pdf_id"].as_str())
        .filter(|id| {
            valid_id(id)
                && ![keys.app_key(), keys.app_id()]
                    .into_iter()
                    .any(|key| !key.is_empty() && *id == key)
        });
    let Some(pdf_id) = pdf_id else {
        return Err(error
            .unwrap_or_else(|| MathpixErrorKind::InvalidResponse.into())
            .into());
    };
    let submitted = Submitted {
        pdf_id: pdf_id.into(),
        page_map,
        pages_sent,
        options: SubmittedOptions {
            price_per_page,
            lines: opts.lines,
            mmd_zip: opts.mmd_zip,
            base: opts.base.clone(),
            simulated: opts.transport.is_simulated(),
        },
        num_pages: None,
    };
    // 拿到任务就算花了（之后失败也收费）：记进所有 App 共用的账本，记不上不挡识别
    if !opts.transport.is_simulated() {
        let _ = opts.ledger.record(&super::spend::entry(
            &opts.app,
            pages_sent,
            f64::from(pages_sent) * price_per_page,
        ));
    }
    if let Some(error) = error {
        let mut failure = submitted.failure(error);
        failure.usage.remote_deleted = Some(delete(&submitted, keys, opts));
        return Err(failure);
    }
    Ok(submitted)
}

/// 等到服务状态为完成，并把服务报告的页数存回任务。分步调用由 App 负责删除。
pub fn wait(
    submitted: &mut Submitted,
    keys: &Keys,
    opts: &Options,
    cancel: &Cancel,
    progress: &dyn Fn(Progress),
) -> Result<(), Failure> {
    let outcome = (|| -> Result<(), Error> {
        let path = submitted.status_path(opts)?;
        let start = Instant::now();
        loop {
            cancel.check()?;
            let timeout = remaining(start, opts.max_wait)?.min(opts.request_timeout);
            let response = request(opts, keys, Method::Get, &path, timeout)?;
            cancel.check()?;
            let body = json_response(&response, keys)?;
            let report = Progress {
                percent_done: body["percent_done"].as_f64(),
                num_pages: body["num_pages"]
                    .as_u64()
                    .and_then(|n| u32::try_from(n).ok()),
                num_pages_completed: body["num_pages_completed"]
                    .as_u64()
                    .and_then(|n| u32::try_from(n).ok()),
            };
            if let Some(pages) = report.num_pages {
                submitted.num_pages = Some(pages);
            }
            match body["status"].as_str() {
                Some("error") => return Err(MathpixErrorKind::Processing.into()),
                Some("completed") => {
                    progress(report);
                    cancel.check()?;
                    return Ok(());
                }
                Some("received" | "loaded" | "split" | "processing") => {
                    progress(report);
                    pause(start, opts.max_wait, opts.poll_interval, cancel)?;
                }
                _ => return Err(MathpixErrorKind::InvalidResponse.into()),
            }
        }
    })();
    outcome.map_err(|error| submitted.failure(error))
}

/// 下载已完成任务的 MMD 和提交时选定的附加结果。ZIP 未就绪时等待并重试。
pub fn fetch(
    submitted: &Submitted,
    keys: &Keys,
    opts: &Options,
    cancel: &Cancel,
) -> Result<Recognized, Failure> {
    let outcome = (|| -> Result<Recognized, Error> {
        cancel.check()?;
        let path = submitted.status_path(opts)?;
        let start = Instant::now();
        let timeout = remaining(start, opts.max_wait)?.min(opts.request_timeout);
        let mmd = download(opts, keys, &format!("{path}.mmd"), timeout, cancel)?;
        check_response(&mmd, keys)?;
        let mmd = String::from_utf8(mmd.body).map_err(|_| MathpixErrorKind::InvalidResponse)?;

        let lines = if submitted.options.lines {
            let timeout = remaining(start, opts.max_wait)?.min(opts.request_timeout);
            let reply = download(opts, keys, &format!("{path}.lines.json"), timeout, cancel)?;
            if reply.status == 404 {
                None
            } else {
                Some(json_response(&reply, keys)?)
            }
        } else {
            None
        };
        let mut mmd_zip = None;
        if submitted.options.mmd_zip {
            let path = format!("{path}.mmd.zip");
            loop {
                let timeout = remaining(start, opts.max_wait)?.min(opts.request_timeout);
                let reply = download(opts, keys, &path, timeout, cancel)?;
                if reply.status != 202 {
                    check_response(&reply, keys)?;
                    mmd_zip = Some(reply.body);
                    break;
                }
                if let Some(error) = serde_json::from_slice(&reply.body)
                    .ok()
                    .and_then(|body| response_error(200, Some(&body), keys))
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
        cancel.check()?;
        Ok(mapping::recognize(
            mmd,
            lines,
            mmd_zip,
            &submitted.page_map,
            submitted.usage(true),
        ))
    })();
    outcome.map_err(|error| submitted.failure(error))
}

/// 删除已提交的任务和结果。返回服务是否确认删除。
pub fn delete(submitted: &Submitted, keys: &Keys, opts: &Options) -> bool {
    submitted
        .status_path(opts)
        .and_then(|path| {
            let reply = request(opts, keys, Method::Delete, &path, opts.request_timeout)?;
            check_response(&reply, keys)
        })
        .is_ok()
}

/// 提交、等待、下载并清理。只要拿到任务 ID，成功、失败或取消都会尝试删除。
pub fn run(
    doc: &Document,
    quote: &Quote,
    keys: &Keys,
    opts: &Options,
    cancel: &Cancel,
    progress: &dyn Fn(Progress),
) -> Result<Recognized, Failure> {
    cancel.check()?;
    if opts.max_wait.is_zero() {
        return Err(Error::from(MathpixErrorKind::Timeout).into());
    }
    let mut submitted = submit(doc, quote, keys, opts, cancel)?;
    let mut outcome = wait(&mut submitted, keys, opts, cancel, progress)
        .and_then(|()| fetch(&submitted, keys, opts, cancel));
    let deleted = delete(&submitted, keys, opts);
    let usage = match &mut outcome {
        Ok(result) => &mut result.usage,
        Err(failure) => &mut failure.usage,
    };
    usage.remote_deleted = Some(deleted);
    outcome
}

fn remaining(start: Instant, max_wait: Duration) -> Result<Duration, Error> {
    max_wait
        .checked_sub(start.elapsed())
        .filter(|left| !left.is_zero())
        .ok_or_else(|| MathpixErrorKind::Timeout.into())
}

fn pause(
    start: Instant,
    max_wait: Duration,
    duration: Duration,
    cancel: &Cancel,
) -> Result<(), Error> {
    let since = Instant::now();
    loop {
        cancel.check()?;
        let duration_left = duration.saturating_sub(since.elapsed());
        if duration_left.is_zero() {
            return Ok(());
        }
        let timeout = remaining(start, max_wait)?;
        thread::sleep(timeout.min(duration_left).min(Duration::from_millis(100)));
    }
}
