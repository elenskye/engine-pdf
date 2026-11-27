//! 只在本机提供试用页。PDF 和密码经请求体传入，文件写入由引擎生成字节后完成。

use std::fs::{self, OpenOptions};
use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::{Ipv4Addr, TcpListener, TcpStream};
#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use engine_pdf::edit::{self, Lock, Permissions, Plan, Save};
use engine_pdf::{Cancel, Document, Error, OcrOptions, pages, searchable};
use serde::Deserialize;
use zeroize::Zeroizing;

const LIMIT: usize = 256 * 1024 * 1024;
const STUDIO: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/examples/studio");
const WEB: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/web");
const OUTPUT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/local/studio");

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
struct Request {
    lengths: Vec<usize>,
    passwords: Vec<Option<Zeroizing<String>>>,
    page: u32,
    page_spec: Option<String>,
    plan: Plan,
    save: SaveRequest,
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
struct SaveRequest {
    flatten: bool,
    remove_password: bool,
    lock: Option<LockRequest>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LockRequest {
    owner: Zeroizing<String>,
    user: Option<Zeroizing<String>>,
    permissions: Permissions,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    if !cfg!(target_os = "macos") {
        return Err(Error::Unsupported.into());
    }
    let mut args = std::env::args_os().skip(1);
    let source = args.next().map(PathBuf::from);
    if args.next().is_some() {
        return Err("用法：cargo run --release --example studio -- [PDF]".into());
    }
    if !Path::new(WEB).join("dist/index.js").is_file()
        || !Path::new(WEB)
            .join("node_modules/pdfjs-dist/build/pdf.mjs")
            .is_file()
    {
        return Err(
            "先运行 pnpm --dir web install --frozen-lockfile 和 pnpm --dir web build。".into(),
        );
    }
    let initial = source.map(fs::read).transpose()?;
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
    let authority = listener.local_addr()?.to_string();
    println!("试用页：http://{authority}/");
    for stream in listener.incoming() {
        let mut stream = stream?;
        stream.set_read_timeout(Some(Duration::from_secs(10)))?;
        stream.set_write_timeout(Some(Duration::from_secs(10)))?;
        let _ = serve(&mut stream, &authority, initial.as_deref());
    }
    Ok(())
}

fn serve(stream: &mut TcpStream, authority: &str, initial: Option<&[u8]>) -> io::Result<()> {
    let mut input = BufReader::new(stream);
    let mut line = String::new();
    let mut host = false;
    let mut origin = false;
    let mut frame = false;
    let mut length = None;
    let mut path = String::new();
    let mut post = false;
    let mut lines = (&mut input).take(16 * 1024);
    for index in 0.. {
        line.clear();
        if lines.read_line(&mut line)? == 0 {
            return Ok(());
        }
        let line = line.trim_end_matches(['\r', '\n']);
        if index == 0 {
            let mut parts = line.split_ascii_whitespace();
            match parts.next() {
                Some("GET") => {}
                Some("POST") => post = true,
                _ => break,
            }
            path = parts.next().unwrap_or_default().to_owned();
        } else if line.is_empty() {
            break;
        } else if let Some((key, value)) = line.split_once(':') {
            let value = value.trim();
            match key.to_ascii_lowercase().as_str() {
                "host" => host = value == authority,
                "origin" => origin = value.strip_prefix("http://") == Some(authority),
                "content-type" => frame = value == "application/x-engine-pdf",
                "content-length" => length = value.parse::<usize>().ok(),
                _ => {}
            }
        }
    }
    if !host || (post && (!origin || !frame)) {
        return reply(input.get_mut(), "403 Forbidden", "text/plain", b"", None);
    }
    if !post {
        if path == "/api/source" {
            return match initial {
                Some(bytes) => reply(input.get_mut(), "200 OK", "application/pdf", bytes, None),
                None => reply(
                    input.get_mut(),
                    "204 No Content",
                    "application/pdf",
                    b"",
                    None,
                ),
            };
        }
        return static_file(input.get_mut(), &path);
    }
    let Some(length) = length.filter(|length| *length <= LIMIT) else {
        return reply(
            input.get_mut(),
            "413 Payload Too Large",
            "text/plain",
            b"",
            None,
        );
    };
    let mut body = Zeroizing::new(vec![0; length]);
    input.read_exact(&mut body)?;
    match handle(&path, &body) {
        Ok((bytes, output)) => reply(
            input.get_mut(),
            "200 OK",
            if output.is_some() {
                "application/pdf"
            } else {
                "application/json"
            },
            &bytes,
            output.as_deref(),
        ),
        Err(message) => reply(
            input.get_mut(),
            "422 Unprocessable Content",
            "text/plain; charset=utf-8",
            message.as_bytes(),
            None,
        ),
    }
}

fn open_pdf(bytes: &[u8], password: Option<&Zeroizing<String>>) -> Result<Document, String> {
    match password {
        Some(password) => Document::from_bytes_with_password(bytes, password),
        None => Document::from_bytes(bytes),
    }
    .map_err(|error| error.to_string())
}

fn handle(path: &str, body: &[u8]) -> Result<(Vec<u8>, Option<String>), String> {
    let format = || "请求格式有误。".to_owned();
    let length = u32::from_le_bytes(body.get(..4).ok_or_else(format)?.try_into().unwrap()) as usize;
    let (head, mut bytes) = body[4..].split_at_checked(length).ok_or_else(format)?;
    let request: Request = serde_json::from_slice(head).map_err(|_| format())?;
    if request.lengths.is_empty() || request.passwords.len() != request.lengths.len() {
        return Err(format());
    }
    let mut files = Vec::with_capacity(request.lengths.len());
    for (length, password) in request.lengths.into_iter().zip(request.passwords) {
        let (source, rest) = bytes.split_at_checked(length).ok_or_else(format)?;
        bytes = rest;
        files.push((source, password));
    }
    if !bytes.is_empty() {
        return Err(format());
    }
    let ((source, password), others) = files.split_first().ok_or_else(format)?;
    let doc = open_pdf(source, password.as_ref())?;
    let cancel = Cancel::new();
    match path {
        "/api/inspect" => {
            let (fields, form_error) = match doc.fields(&cancel) {
                Ok(fields) => (fields, None),
                Err(Error::UnsupportedForm) => {
                    (Vec::new(), Some(Error::UnsupportedForm.to_string()))
                }
                Err(error) => return Err(error.to_string()),
            };
            let meta = serde_json::json!({
                "pageCount": doc.page_count(), "info": doc.info(), "outline": doc.outline(),
                "fields": fields, "formError": form_error, "pdfium": cfg!(feature = "pdfium"),
            });
            Ok((serde_json::to_vec(&meta).map_err(|_| format())?, None))
        }
        "/api/annotations" => {
            let annotations = doc
                .annotations(request.page)
                .map_err(|error| error.to_string())?;
            Ok((
                serde_json::to_vec(&annotations).map_err(|_| format())?,
                None,
            ))
        }
        // 对象编号只由文件字节决定：同一份字节每次列出的编号都一样，保存时再打开也对得上。
        "/api/objects" if others.is_empty() => {
            #[cfg(feature = "pdfium")]
            {
                let objects = doc
                    .objects(request.page)
                    .map_err(|error| error.to_string())?;
                Ok((
                    serde_json::to_vec(&serde_json::json!({ "objects": objects }))
                        .map_err(|_| format())?,
                    None,
                ))
            }
            #[cfg(not(feature = "pdfium"))]
            Err(Error::PdfiumUnavailable.to_string())
        }
        "/api/searchable" if others.is_empty() => {
            let spec = request
                .page_spec
                .as_deref()
                .filter(|spec| !spec.trim().is_empty())
                .ok_or_else(|| "请至少选择一页。".to_owned())?;
            let total = doc.page_count();
            let selected = pages::parse(Some(spec), total, total as usize)
                .map_err(|error| error.to_string())?;
            if selected.is_empty() {
                return Err("请至少选择一页。".into());
            }
            let bytes = searchable(&doc, &selected, &OcrOptions::default(), &cancel, &|_| {})
                .map_err(|error| error.to_string())?;
            write_copy(bytes, "searchable")
        }
        "/api/save" => {
            let save = Save {
                flatten: request.save.flatten,
                remove_password: request.save.remove_password,
                lock: request.save.lock.map(|lock| Lock {
                    owner: lock.owner,
                    user: lock.user,
                    permissions: lock.permissions,
                }),
            };
            let documents = others
                .iter()
                .map(|(bytes, password)| open_pdf(bytes, password.as_ref()))
                .collect::<Result<Vec<_>, _>>()?;
            let others: Vec<_> = documents.iter().collect();
            let bytes = edit::apply(&doc, &others, &request.plan, &save, &cancel)
                .map_err(|error| error.to_string())?;
            write_copy(bytes, "edited")
        }
        _ => Err("找不到此操作。".into()),
    }
}

fn write_copy(bytes: Vec<u8>, prefix: &str) -> Result<(Vec<u8>, Option<String>), String> {
    let time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let name = format!("{prefix}-{time}.pdf");
    fs::create_dir_all(OUTPUT)
        .and_then(|()| {
            let path = Path::new(OUTPUT).join(&name);
            let mut options = OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            options.mode(0o600);
            let mut file = options.open(&path)?;
            if let Err(error) = file.write_all(&bytes) {
                drop(file);
                let _ = fs::remove_file(path);
                return Err(error);
            }
            Ok(())
        })
        .map_err(|_| "无法写入试用页输出目录。".to_owned())?;
    Ok((bytes, Some(name)))
}

fn static_file(stream: &mut TcpStream, path: &str) -> io::Result<()> {
    let relative = path.trim_start_matches('/');
    if relative.split('/').any(|part| part == "." || part == "..") || relative.contains(['\\', '%'])
    {
        return reply(stream, "404 Not Found", "text/plain", b"", None);
    }
    let file = match relative {
        "" => Path::new(STUDIO).join("index.html"),
        "studio.mjs" | "studio.css" => Path::new(STUDIO).join(relative),
        path if path.starts_with("dist/") => Path::new(WEB).join(path),
        path if path.starts_with("pdfjs/") => Path::new(WEB)
            .join("node_modules/pdfjs-dist")
            .join(&path[6..]),
        _ => return reply(stream, "404 Not Found", "text/plain", b"", None),
    };
    let content = match file.extension().and_then(|extension| extension.to_str()) {
        Some("mjs" | "js") => "text/javascript; charset=utf-8",
        Some("html") => "text/html; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("wasm") => "application/wasm",
        _ => "application/octet-stream",
    };
    match fs::read(file) {
        Ok(bytes) => reply(stream, "200 OK", content, &bytes, None),
        Err(_) => reply(stream, "404 Not Found", "text/plain", b"", None),
    }
}

fn reply(
    stream: &mut TcpStream,
    status: &str,
    content: &str,
    bytes: &[u8],
    output: Option<&str>,
) -> io::Result<()> {
    write!(
        stream,
        "HTTP/1.1 {status}\r\nConnection: close\r\nCache-Control: no-store\r\nContent-Type: {content}\r\nContent-Length: {}\r\nX-Content-Type-Options: nosniff\r\n",
        bytes.len()
    )?;
    if let Some(output) = output {
        write!(stream, "X-Output-File: {output}\r\n")?;
    }
    stream.write_all(b"\r\n")?;
    stream.write_all(bytes)
}
