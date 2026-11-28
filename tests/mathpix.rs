//! Mathpix 上传、识别、续跑和清理的本地验证。

#![cfg(all(target_os = "macos", feature = "samples", feature = "fake"))]

use std::{
    io::{Cursor, Read},
    sync::Arc,
    time::Duration,
};

use engine_pdf::{
    Cancel, Document, Error, ImageFormat,
    mathpix::{
        self, FakeTransport, Keys, Mapping, MathpixErrorKind, Method, Options, RecordedRequest,
        Response, Submitted, TransportError,
    },
    samples::Sample,
};

const ACCEPTED: &str = r#"{"pdf_id":"fixture-1"}"#;
const COMPLETE: &str = r#"{"status":"completed","percent_done":100,"num_pages":2}"#;

fn reply(status: u16, body: impl Into<Vec<u8>>) -> Result<Response, TransportError> {
    Ok(Response {
        status,
        body: body.into(),
        retry_after: None,
    })
}

fn uploaded_pdf(request: &RecordedRequest) -> &[u8] {
    assert_eq!(request.method, Method::Post);
    assert_eq!(request.path, "/v3/pdf");
    let (_, boundary) = request
        .content_type
        .as_deref()
        .unwrap()
        .split_once("; boundary=")
        .unwrap();
    let start = b"\r\n\r\n%PDF-";
    let pdf = request
        .body
        .windows(start.len())
        .position(|bytes| bytes == start)
        .unwrap()
        + 4;
    request.body[pdf..]
        .strip_suffix(format!("\r\n--{boundary}--\r\n").as_bytes())
        .unwrap()
}

#[test]
fn freezes_quote_uploads_only_selected_pages_and_retries_pending_zip() {
    let original = Document::from_bytes(&Sample::Mixed.generate().unwrap()).unwrap();
    let fake = Arc::new(FakeTransport::scripted([
        reply(200, ACCEPTED),
        reply(200, COMPLETE),
        reply(
            200,
            "English scan\n\\pagebreak\nFormula page\n\\pagebreak\n",
        ),
        Ok(Response {
            status: 202,
            body: Vec::new(),
            retry_after: Some(Duration::ZERO),
        }),
        reply(200, b"opaque ZIP payload"),
        reply(204, []),
    ]));
    let mut options = Options {
        transport: fake.clone(),
        price_per_page: 0.0125,
        mmd_zip: true,
        extra: serde_json::json!({
            "include_page_breaks": false,
            "metadata": {"improve_mathpix": true},
            "page_ranges": "1",
            "streaming": true,
        })
        .as_object()
        .unwrap()
        .clone(),
        poll_interval: Duration::from_millis(1),
        ..quiet()
    };
    let quote = mathpix::quote_with_options(&original, &[8, 2, 8], &options).unwrap();
    assert_eq!(quote.pages(), [2, 8]);
    assert_eq!(quote.price_per_page(), 0.0125);
    assert_eq!(quote.usd(), 0.025);
    assert_eq!(quote.service(), "Mathpix");
    assert_eq!(
        engine_pdf::tools::Quote::from(&quote),
        engine_pdf::tools::Quote {
            service: "Mathpix".into(),
            unit: "page".into(),
            units: 2.0,
            usd: 0.025,
        },
    );
    assert!(fake.requests().is_empty());

    let keys = Keys::new("test-app", "test-key-not-for-recording");
    let cancel = Cancel::new();
    let max_wait = options.max_wait;
    options.max_wait = Duration::ZERO;
    let expired = mathpix::run(&original, &quote, &keys, &options, &cancel, &|_| {}).unwrap_err();
    assert!(
        matches!(&expired.error, Error::Mathpix(error) if error.kind == MathpixErrorKind::Timeout)
    );
    assert_eq!(expired.usage.usd, 0.0);
    assert!(engine_pdf::tools::ToolError::from(expired).usage.is_none());
    assert!(fake.requests().is_empty());
    options.max_wait = max_wait;

    options.price_per_page = 0.75;
    let recognized = mathpix::run(&original, &quote, &keys, &options, &cancel, &|_| {}).unwrap();
    assert_eq!(recognized.mapping, Mapping::Exact);
    assert_eq!(recognized.pages[0].index, 2);
    assert_eq!(recognized.pages[1].index, 8);
    assert_eq!(recognized.pages[1].mmd, "Formula page");
    assert_eq!(
        recognized.mmd_zip.as_deref(),
        Some(&b"opaque ZIP payload"[..])
    );
    assert_eq!(recognized.usage.pages_billed, 2);
    assert_eq!(recognized.usage.usd, 0.025);
    assert!(!recognized.usage.estimated);
    assert_eq!(recognized.usage.remote_deleted, Some(true));
    assert_eq!(
        engine_pdf::tools::Usage::from(&recognized.usage),
        engine_pdf::tools::Usage {
            service: "Mathpix".into(),
            unit: "page".into(),
            units: 2.0,
            usd: 0.025,
            estimated: false,
        },
    );

    let requests = fake.requests();
    assert_eq!(
        requests
            .iter()
            .map(|request| (request.method, request.path.as_str()))
            .collect::<Vec<_>>(),
        [
            (Method::Post, "/v3/pdf"),
            (Method::Get, "/v3/pdf/fixture-1"),
            (Method::Get, "/v3/pdf/fixture-1.mmd"),
            (Method::Get, "/v3/pdf/fixture-1.mmd.zip"),
            (Method::Get, "/v3/pdf/fixture-1.mmd.zip"),
            (Method::Delete, "/v3/pdf/fixture-1"),
        ]
    );
    let body = &requests[0].body;
    assert!(
        body.windows(b"pages.pdf".len())
            .any(|part| part == b"pages.pdf")
    );
    assert!(
        !body
            .windows(b"test-key-not-for-recording".len())
            .any(|part| part == b"test-key-not-for-recording")
    );
    let options_start = body
        .windows(4)
        .position(|bytes| bytes == b"\r\n\r\n")
        .unwrap()
        + 4;
    let options_size = body[options_start..]
        .windows(2)
        .position(|bytes| bytes == b"\r\n")
        .unwrap();
    let upload_options: serde_json::Value =
        serde_json::from_slice(&body[options_start..options_start + options_size]).unwrap();
    assert_eq!(upload_options["include_page_breaks"], true);
    assert_eq!(upload_options["metadata"]["improve_mathpix"], false);
    assert_eq!(upload_options["conversion_formats"]["mmd.zip"], true);
    assert!(upload_options.get("page_ranges").is_none());
    assert!(upload_options.get("streaming").is_none());
    let uploaded = Document::from_bytes(uploaded_pdf(&requests[0])).unwrap();
    assert_eq!(uploaded.page_count(), 2);
    for (uploaded_page, original_page) in [(1, 2), (2, 8)] {
        let uploaded_image = uploaded
            .render(uploaded_page, ImageFormat::Gray, 512)
            .unwrap();
        let original_image = original
            .render(original_page, ImageFormat::Gray, 512)
            .unwrap();
        assert_eq!(
            (uploaded_image.width, uploaded_image.height),
            (original_image.width, original_image.height)
        );
        assert!(
            uploaded_image.bytes == original_image.bytes,
            "上传件第 {uploaded_page} 页和原文件第 {original_page} 页的灰度像素不同"
        );
    }
}

#[test]
fn echo_uploads_original_whole_pdf_and_preserves_blank_last_page() {
    let cancel = Cancel::new();
    let fixture = Document::from_bytes(&Sample::Mixed.generate().unwrap()).unwrap();
    let bytes = fixture.split(&[1, 3], &cancel).unwrap();
    let document = Document::from_bytes(&bytes).unwrap();
    let fake = Arc::new(FakeTransport::echo());
    let options = Options {
        transport: fake.clone(),
        lines: true,
        mmd_zip: true,
        ..quiet()
    };
    let keys = Keys::new("mock-id", "mock-key");
    let quote = mathpix::quote(&document, &[2, 1]).unwrap();
    let recognized = mathpix::run(&document, &quote, &keys, &options, &cancel, &|_| {}).unwrap();
    assert!(uploaded_pdf(&fake.requests()[0]) == bytes);
    assert_eq!(recognized.mapping, Mapping::Exact);
    assert_eq!(recognized.pages.len(), 2);
    assert_eq!(
        recognized.pages[0].mmd,
        document.text(1).unwrap().trim_matches(['\r', '\n'])
    );
    assert_eq!(recognized.pages[1].mmd, "");
    assert_eq!(recognized.mmd.matches("\\pagebreak").count(), 2);
    let lines = recognized.lines_json().unwrap();
    assert_eq!(lines["pages"][0]["page"], 1);
    assert_eq!(lines["pages"][1]["page"], 2);
    assert!(!lines["pages"][0]["lines"].as_array().unwrap().is_empty());
    assert!(lines["pages"][1]["lines"].as_array().unwrap().is_empty());
    assert_eq!(recognized.usage.pages_billed, 0);
    assert_eq!(recognized.usage.usd, 0.0);
    assert_eq!(recognized.usage.remote_deleted, Some(true));

    let mut zip = zip::ZipArchive::new(Cursor::new(recognized.mmd_zip.unwrap())).unwrap();
    let mut mmd = String::new();
    zip.by_name("extraction/extraction.mmd")
        .unwrap()
        .read_to_string(&mut mmd)
        .unwrap();
    assert_eq!(mmd, recognized.mmd);
}

#[test]
fn resumes_a_serialized_submission_and_preserves_reported_usage() {
    let document = Document::from_bytes(&Sample::Mixed.generate().unwrap()).unwrap();
    let fake = Arc::new(FakeTransport::scripted([
        reply(200, ACCEPTED),
        reply(200, r#"{"status":"completed","num_pages":3}"#),
        reply(200, "First\n\\pagebreak\nSecond\n\\pagebreak\n"),
        reply(
            200,
            r#"{"pages":[{"page":1,"lines":[{"text":"First"}]},{"page":2,"lines":[{"text":"Second"}]}]}"#,
        ),
        reply(204, []),
    ]));
    let options = Options {
        transport: fake.clone(),
        lines: true,
        ..quiet()
    };
    let keys = Keys::new("mock-id", "mock-key-never-serialized");
    let cancel = Cancel::new();
    let quote = mathpix::quote(&document, &[4, 7]).unwrap();
    let submitted = mathpix::submit(&document, &quote, &keys, &options, &cancel).unwrap();
    let saved = serde_json::to_string(&submitted).unwrap();
    assert!(!saved.contains("mock-key-never-serialized"));
    let mut resumed: Submitted = serde_json::from_str(&saved).unwrap();
    mathpix::wait(&mut resumed, &keys, &options, &cancel, &|_| {}).unwrap();
    let after_wait = serde_json::to_string(&resumed).unwrap();
    let resumed: Submitted = serde_json::from_str(&after_wait).unwrap();
    let recognized = mathpix::fetch(&resumed, &keys, &options, &cancel).unwrap();
    assert_eq!(recognized.mapping, Mapping::Exact);
    let lines = recognized.lines_json().unwrap();
    assert_eq!(lines["pages"][0]["page"], 4);
    assert_eq!(lines["pages"][1]["page"], 7);
    assert_eq!(recognized.usage.pages_billed, 3);
    assert!(!recognized.usage.estimated);
    assert_eq!(recognized.usage.remote_deleted, None);
    assert!(mathpix::delete(&resumed, &keys, &options));
    assert_eq!(
        fake.requests()
            .iter()
            .filter(|request| request.method == Method::Post)
            .count(),
        1
    );
}

#[test]
fn run_returns_usage_and_deletes_after_processing_errors_and_timeouts() {
    let document = Document::from_bytes(&Sample::Mixed.generate().unwrap()).unwrap();
    let cancel = Cancel::new();
    let keys = Keys::new("fixture-id", "must-not-appear-in-errors");
    let quote = mathpix::quote(&document, &[2]).unwrap();
    for (reply_after_upload, kind, delete_status) in [
        (
            reply(
                200,
                r#"{"status":"error","error_info":{"id":"error_pdf","message":"failed must-not-appear-in-errors"}}"#,
            ),
            MathpixErrorKind::Processing,
            204,
        ),
        (Err(TransportError::Timeout), MathpixErrorKind::Timeout, 503),
    ] {
        let fake = Arc::new(FakeTransport::scripted([
            reply(200, ACCEPTED),
            reply_after_upload,
            reply(delete_status, []),
        ]));
        let options = Options {
            transport: fake.clone(),
            ..quiet()
        };
        let failure =
            mathpix::run(&document, &quote, &keys, &options, &cancel, &|_| {}).unwrap_err();
        assert!(matches!(&failure.error, Error::Mathpix(error) if error.kind == kind));
        assert!(!format!("{:?}", failure.error).contains("must-not-appear-in-errors"));
        assert_eq!(failure.usage.pdf_id.as_deref(), Some("fixture-1"));
        assert_eq!(failure.usage.pages_billed, 1);
        assert_eq!(failure.usage.usd, 0.005);
        assert!(failure.usage.estimated);
        assert_eq!(failure.usage.remote_deleted, Some(delete_status == 204));
        let usage = engine_pdf::tools::ToolError::from(failure).usage.unwrap();
        assert_eq!(
            (usage.units, usage.usd, usage.estimated),
            (1.0, 0.005, true)
        );
        assert_eq!(fake.requests().last().unwrap().method, Method::Delete);
    }
}

#[test]
fn a_rejected_upload_is_never_retried_and_a_returned_job_id_is_cleaned_up() {
    let doc = Document::from_bytes(&Sample::Mixed.generate().unwrap()).unwrap();
    let quote = mathpix::quote(&doc, &[8]).unwrap();
    let cancel = Cancel::new();
    let keys = Keys::new("fixture-id", "fixture-key");
    for (status, body, kind, remote_deleted) in [
        (
            401,
            r#"{"error_info":{"id":"http_unauthorized","message":"invalid fixture-key"}}"#,
            MathpixErrorKind::Auth,
            None,
        ),
        (
            200,
            r#"{"pdf_id":"fixture-1","error_info":{"id":"pdf_encrypted"}}"#,
            MathpixErrorKind::Processing,
            Some(true),
        ),
    ] {
        let fake = Arc::new(FakeTransport::scripted([
            reply(status, body),
            reply(204, []),
        ]));
        let opts = Options {
            transport: fake.clone(),
            ..quiet()
        };
        let failure = mathpix::run(&doc, &quote, &keys, &opts, &cancel, &|_| {}).unwrap_err();
        assert!(matches!(&failure.error, Error::Mathpix(error) if error.kind == kind));
        assert!(!format!("{failure:?}").contains("fixture-key"));
        assert_eq!(failure.usage.remote_deleted, remote_deleted);
        assert_eq!(
            failure.usage.pages_billed,
            u32::from(remote_deleted.is_some())
        );
        let calls = fake.requests();
        assert_eq!(calls.len(), 1 + usize::from(remote_deleted.is_some()));
        assert_eq!(calls[0].method, Method::Post);
        if remote_deleted.is_some() {
            assert_eq!(calls[1].method, Method::Delete);
        }
    }
}

#[test]
fn checks_cancellation_before_upload_and_deletes_after_upload() {
    let document = Document::from_bytes(&Sample::Mixed.generate().unwrap()).unwrap();
    let fake = Arc::new(FakeTransport::scripted([
        reply(200, ACCEPTED),
        reply(200, r#"{"status":"processing","percent_done":25}"#),
        reply(204, []),
    ]));
    let options = Options {
        transport: fake.clone(),
        ..quiet()
    };
    let keys = Keys::new("mock-id", "mock-key");
    let quote = mathpix::quote(&document, &[2]).unwrap();

    let cancelled = Cancel::new();
    cancelled.cancel();
    let early = mathpix::run(&document, &quote, &keys, &options, &cancelled, &|_| {}).unwrap_err();
    assert!(matches!(early.error, Error::Cancelled));
    assert!(fake.requests().is_empty());
    assert_eq!(early.usage.pdf_id, None);
    assert_eq!(early.usage.usd, 0.0);

    let cancel = Cancel::new();
    let late = mathpix::run(&document, &quote, &keys, &options, &cancel, &|_| {
        cancel.cancel()
    })
    .unwrap_err();
    assert!(matches!(late.error, Error::Cancelled));
    assert_eq!(late.usage.pdf_id.as_deref(), Some("fixture-1"));
    assert_eq!(late.usage.pages_billed, 1);
    assert_eq!(late.usage.usd, 0.005);
    assert!(late.usage.estimated);
    assert_eq!(late.usage.remote_deleted, Some(true));
    assert_eq!(
        fake.requests()
            .iter()
            .map(|request| request.method)
            .collect::<Vec<_>>(),
        [Method::Post, Method::Get, Method::Delete]
    );
}

#[test]
fn echo_file_submission_has_no_original_page_mapping() {
    let fake = Arc::new(FakeTransport::echo());
    let options = Options {
        transport: fake.clone(),
        ..quiet()
    };
    let keys = Keys::new("mock-id", "mock-key");
    let cancel = Cancel::new();
    let mut submitted = mathpix::submit_file(
        b"mock Word payload",
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        Some(4),
        &keys,
        &options,
        &cancel,
    )
    .unwrap();
    mathpix::wait(&mut submitted, &keys, &options, &cancel, &|_| {}).unwrap();
    let recognized = mathpix::fetch(&submitted, &keys, &options, &cancel).unwrap();
    assert_eq!(recognized.mapping, Mapping::Approximate);
    assert!(recognized.pages.is_empty());
    assert!(recognized.mmd.contains("模拟识别"));
    assert_eq!(recognized.usage.usd, 0.0);
    assert!(mathpix::delete(&submitted, &keys, &options));
}

/// 测试不碰所有 App 共用的账本。
fn quiet() -> Options {
    Options {
        ledger: engine_pdf::mathpix::spend::Ledger::Off,
        ..Options::default()
    }
}

#[test]
fn every_real_upload_goes_into_the_shared_ledger_and_simulated_ones_do_not() {
    use mathpix::spend::Ledger;
    let dir = std::env::temp_dir().join(format!("engine-pdf-ledger-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let ledger = Ledger::At(dir.join("ledger.jsonl"));
    let doc = Document::from_bytes(&Sample::Mixed.generate().unwrap()).unwrap();
    let keys = Keys::new("fixture-app", "fixture-key");
    let scripted = Arc::new(FakeTransport::scripted([
        reply(200, ACCEPTED),
        reply(200, COMPLETE),
        reply(200, "One\n\\pagebreak\nTwo\n\\pagebreak\n"),
        reply(204, []),
    ]));
    let options = Options {
        transport: scripted,
        price_per_page: 0.0125,
        poll_interval: Duration::from_millis(1),
        ledger: ledger.clone(),
        app: "Pomelo".into(),
        ..quiet()
    };
    let quote = mathpix::quote_with_options(&doc, &[2, 8], &options).unwrap();
    mathpix::run(&doc, &quote, &keys, &options, &Cancel::new(), &|_| {}).unwrap();
    let entries = ledger.entries(std::time::UNIX_EPOCH);
    assert_eq!(entries.len(), 1);
    assert_eq!(
        (
            entries[0].app.as_str(),
            entries[0].service.as_str(),
            entries[0].pages,
            entries[0].usd
        ),
        ("Pomelo", "Mathpix", 2, 0.025)
    );
    // the echo (simulated) one: nothing recorded
    let echo = Options {
        transport: Arc::new(FakeTransport::echo()),
        ..options
    };
    let quote = mathpix::quote_with_options(&doc, &[2], &echo).unwrap();
    mathpix::run(&doc, &quote, &keys, &echo, &Cancel::new(), &|_| {}).unwrap();
    assert_eq!(ledger.entries(std::time::UNIX_EPOCH).len(), 1);
}
