use std::{
    collections::{HashMap, VecDeque},
    io::{Cursor, Write},
    sync::Mutex,
    time::Duration,
};

use serde_json::json;

use super::{Method, Request, Response, Transport, TransportError};
use crate::Document;

/// 模拟服务收到的请求。鉴权信息不记录。
#[derive(Clone, Debug)]
pub struct RecordedRequest {
    pub method: Method,
    pub base: String,
    pub path: String,
    pub content_type: Option<String>,
    pub body: Vec<u8>,
    pub timeout: Duration,
}

/// 本机 Mathpix 测试替身，可回放脚本或用 PDF 文字层模拟识别。
pub struct FakeTransport(Mutex<State>);

struct State {
    requests: Vec<RecordedRequest>,
    mode: Mode,
}

enum Mode {
    Scripted(VecDeque<Result<Response, TransportError>>),
    Echo(Echo),
}

#[derive(Default)]
struct Echo {
    next_id: u64,
    jobs: HashMap<String, Vec<String>>,
}

impl FakeTransport {
    /// 按请求顺序交付脚本里的响应。用完后收到的请求返回 `InvalidRequest`。
    pub fn scripted(responses: impl IntoIterator<Item = Result<Response, TransportError>>) -> Self {
        Self(Mutex::new(State {
            requests: Vec::new(),
            mode: Mode::Scripted(responses.into_iter().collect()),
        }))
    }

    /// 用上传件的文字层模拟识别，费用为零。分步调用须沿用同一个替身。
    /// 其他文件格式返回一页模拟文字。
    pub fn echo() -> Self {
        Self(Mutex::new(State {
            requests: Vec::new(),
            mode: Mode::Echo(Echo::default()),
        }))
    }

    /// 按发送顺序返回请求记录。
    pub fn requests(&self) -> Vec<RecordedRequest> {
        self.0.lock().unwrap().requests.clone()
    }
}

impl Transport for FakeTransport {
    fn send(&self, request: Request<'_>) -> Result<Response, TransportError> {
        let mut state = self.0.lock().map_err(|_| TransportError::InvalidRequest)?;
        state.requests.push(RecordedRequest {
            method: request.method,
            base: request.base.to_owned(),
            path: request.path.to_owned(),
            content_type: request.content_type.map(str::to_owned),
            body: request.body.to_vec(),
            timeout: request.timeout,
        });
        match &mut state.mode {
            Mode::Scripted(responses) => responses
                .pop_front()
                .unwrap_or(Err(TransportError::InvalidRequest)),
            Mode::Echo(echo) => echo.send(request),
        }
    }

    fn is_simulated(&self) -> bool {
        matches!(self.0.lock().unwrap().mode, Mode::Echo(_))
    }
}

impl Echo {
    fn send(&mut self, request: Request<'_>) -> Result<Response, TransportError> {
        if request.method == Method::Post && request.path == "/v3/pdf" {
            let (mime, file) = uploaded_file(&request).ok_or(TransportError::InvalidRequest)?;
            let pages = if mime == b"application/pdf" {
                let doc = Document::from_bytes(file).map_err(|_| TransportError::InvalidRequest)?;
                (1..=doc.page_count())
                    .map(|page| doc.text(page))
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|_| TransportError::InvalidRequest)?
            } else {
                vec!["（模拟识别的文档）".into()]
            };
            self.next_id += 1;
            let id = format!("echo-{}", self.next_id);
            self.jobs.insert(id.clone(), pages);
            return Ok(response(200, json!({"pdf_id": id}).to_string()));
        }

        let Some(path) = request.path.strip_prefix("/v3/pdf/") else {
            return Ok(response(404, []));
        };
        let (id, suffix) = path.split_once('.').unwrap_or((path, ""));
        let Some(pages) = self.jobs.get(id) else {
            return Ok(response(404, []));
        };
        Ok(match (request.method, suffix) {
            (Method::Get, "") => response(
                200,
                json!({"status": "completed", "percent_done": 100, "num_pages": pages.len()})
                    .to_string(),
            ),
            (Method::Get, "mmd") => response(200, mmd(pages)),
            (Method::Get, "mmd.zip") => {
                let mut archive = zip::ZipWriter::new(Cursor::new(Vec::new()));
                archive
                    .start_file(
                        "extraction/extraction.mmd",
                        zip::write::SimpleFileOptions::default()
                            .compression_method(zip::CompressionMethod::Stored),
                    )
                    .map_err(|_| TransportError::InvalidRequest)?;
                archive
                    .write_all(mmd(pages).as_bytes())
                    .map_err(|_| TransportError::InvalidRequest)?;
                let data = archive
                    .finish()
                    .map_err(|_| TransportError::InvalidRequest)?
                    .into_inner();
                response(200, data)
            }
            (Method::Get, "lines.json") => {
                let pages = pages
                    .iter()
                    .enumerate()
                    .map(|(page_index, page)| {
                        let mut lines = Vec::new();
                        for (paragraph, text) in page.split("\n\n").enumerate() {
                            let parent_id = format!("{}-{}", page_index + 1, paragraph + 1);
                            for text in text.lines().map(str::trim).filter(|line| !line.is_empty())
                            {
                                lines.push(json!({
                                    "line": lines.len() + 1,
                                    "type": "text",
                                    "text": text,
                                    "text_display": text,
                                    "parent_id": parent_id,
                                    "conversion_output": true
                                }));
                            }
                        }
                        json!({"page": page_index + 1, "lines": lines})
                    })
                    .collect::<Vec<_>>();
                response(200, json!({"pages": pages}).to_string())
            }
            (Method::Delete, "") => {
                self.jobs.remove(id);
                response(204, [])
            }
            _ => response(404, []),
        })
    }
}

fn mmd(pages: &[String]) -> String {
    let mut result = String::new();
    for page in pages {
        result.push_str(page);
        result.push_str("\n\n\\pagebreak\n\n");
    }
    result
}

// 只拆引擎自己构造的上传件。最后一个 multipart 部分是文件。
fn uploaded_file<'a>(request: &Request<'a>) -> Option<(&'a [u8], &'a [u8])> {
    let boundary = request.content_type?.split_once("; boundary=")?.1;
    let first = format!("\r\n--{boundary}\r\nContent-Disposition: form-data; name=\"file\";");
    let last = format!("\r\n--{boundary}--\r\n");
    let start = request
        .body
        .windows(first.len())
        .position(|bytes| bytes == first.as_bytes())?;
    let part = request.body[start + first.len()..].strip_suffix(last.as_bytes())?;
    let headers_end = part.windows(4).position(|bytes| bytes == b"\r\n\r\n")?;
    let headers = &part[..headers_end];
    let mime = b"\r\nContent-Type: ";
    let mime_start = headers
        .windows(mime.len())
        .position(|bytes| bytes == mime)?
        + mime.len();
    Some((&headers[mime_start..], &part[headers_end + 4..]))
}

fn response(status: u16, body: impl Into<Vec<u8>>) -> Response {
    Response {
        status,
        body: body.into(),
        retry_after: None,
    }
}
