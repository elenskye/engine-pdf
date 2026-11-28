#![cfg(all(target_os = "macos", feature = "samples", feature = "fake"))]

use std::{collections::BTreeMap, io::Write, sync::Arc};

use engine_core::{call::NamedTempFile, cli};
use engine_pdf::{
    Cancel, Document, ImageFormat,
    mathpix::{FakeTransport, Method, Options},
    samples::Sample,
    tools::{
        self, Approve, AttachmentContent, Ctx, Files, Keys, Override, Overrides, Prices, Quote,
        ToolError, ToolSpec,
    },
};
use serde_json::{Value, json};
use zeroize::Zeroizing;

struct Host {
    files: BTreeMap<String, Vec<u8>>,
    secrets: BTreeMap<String, String>,
    yes: bool,
}

impl Files for Host {
    fn read(&self, path: &str) -> Result<Vec<u8>, ToolError> {
        self.files
            .get(path)
            .cloned()
            .ok_or_else(|| ToolError::of("file"))
    }
}

impl Keys for Host {
    fn get(&self, name: &str) -> Option<Zeroizing<String>> {
        self.secrets.get(name).cloned().map(Zeroizing::new)
    }
}

impl Approve for Host {
    fn ask(&self, _: &ToolSpec, _: Option<&Quote>) -> bool {
        self.yes
    }
}

impl Host {
    fn ctx<'a>(&'a self, cancel: &'a Cancel, prices: &'a Prices) -> Ctx<'a> {
        Ctx {
            files: self,
            keys: self,
            approve: self,
            cancel,
            prices,
            progress: &|_| {},
        }
    }
}

#[test]
fn runs_every_manifest_example_with_generated_files_and_mock_mathpix() {
    let cancel = Cancel::new();
    let prices = Prices::default();
    let mut host = Host {
        files: [
            ("mixed.pdf", Sample::Mixed),
            ("locked.pdf", Sample::Locked),
            ("not_pdf.txt", Sample::NotPdf),
            ("outline.pdf", Sample::Outline),
            ("form.pdf", Sample::Form),
            ("annotated.pdf", Sample::Annotated),
            ("text_en.pdf", Sample::TextEn),
            ("text_zh.pdf", Sample::TextZh),
        ]
        .into_iter()
        .map(|(path, sample)| (format!("samples/out/{path}"), sample.generate().unwrap()))
        .collect(),
        secrets: [
            ("mathpix_app_id".into(), "fixture-app".into()),
            ("mathpix_app_key".into(), "fixture-key".into()),
        ]
        .into(),
        yes: true,
    };
    let doc = Document::from_bytes(&host.files["samples/out/mixed.pdf"]).unwrap();
    let jpeg = doc.render(1, ImageFormat::Jpeg, 256).unwrap();
    host.files
        .insert("samples/out/read-page.jpg".into(), jpeg.bytes);
    let fake = Arc::new(FakeTransport::echo());
    let kit = tools::toolkit_with_mathpix(Options {
        transport: fake.clone(),
        ..quiet()
    });
    let ctx = host.ctx(&cancel, &prices);
    assert_eq!(tools::manifest().len(), 17);
    assert_eq!(
        kit.manifest()
            .iter()
            .map(|spec| spec.examples.len())
            .sum::<usize>(),
        26
    );
    let kit = if cfg!(feature = "pdfium") {
        kit
    } else {
        let unavailable = ["pdf_objects", "pdf_redact"];
        for name in unavailable {
            let spec = kit
                .manifest()
                .iter()
                .find(|spec| spec.name == name)
                .unwrap();
            let err = kit.call(name, &spec.examples[0].args, &ctx).unwrap_err();
            assert_eq!(err.kind, "pdfium_unavailable");
        }
        let overrides = unavailable
            .into_iter()
            .map(|name| {
                (
                    name.to_owned(),
                    Override {
                        disabled: true,
                        ..Override::default()
                    },
                )
            })
            .collect::<Overrides>();
        kit.with_overrides(&overrides).unwrap()
    };
    kit.check_examples(&ctx).unwrap();
    assert_eq!(
        fake.requests()
            .iter()
            .filter(|request| request.method == Method::Post)
            .count(),
        1
    );
}

#[test]
fn mathpix_preparation_cannot_upload_and_zero_price_is_rejected() {
    let bytes = Sample::Mixed.generate().unwrap();
    let fake = Arc::new(FakeTransport::echo());
    // the terminal: a paid tool only quotes without --yes
    let kit = tools::terminal_toolkit(Options {
        transport: fake.clone(),
        ..quiet()
    });
    let mut pdf = NamedTempFile::new().unwrap();
    pdf.write_all(&bytes).unwrap();
    let args = json!({"file":pdf.path(),"pages":[4,1,4]});
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let code = cli::run_with(
        &kit,
        ["call", "pdf_mathpix", &args.to_string()],
        &mut stdout,
        &mut stderr,
    );
    let reply: Value = serde_json::from_slice(&stdout).unwrap();
    assert_eq!(code, 3);
    assert_eq!(reply["tool"], "pdf_mathpix");
    assert_eq!(reply["quote"]["units"].as_f64(), Some(2.0));
    assert_eq!(reply["quote"]["usd"], 0.01);
    assert!(fake.requests().is_empty());

    let mut host = Host {
        files: [(pdf.path().to_str().unwrap().to_owned(), bytes)].into(),
        secrets: [
            ("mathpix_app_id".into(), "fixture-app".into()),
            ("mathpix_app_key".into(), "fixture-key".into()),
        ]
        .into(),
        yes: false,
    };
    let cancel = Cancel::new();
    let mut prices = Prices::default();
    prices.set("Mathpix", "page", 0.0);
    let ctx = host.ctx(&cancel, &prices);
    for name in ["pdf_mathpix_quote", "pdf_mathpix"] {
        assert_eq!(kit.call(name, &args, &ctx).unwrap_err().kind, "bad_args");
    }
    prices.set("Mathpix", "page", 0.0125);
    let ctx = host.ctx(&cancel, &prices);
    let quote = kit.call("pdf_mathpix_quote", &args, &ctx).unwrap();
    assert_eq!(quote.data["pages"], json!([1, 4]));
    assert_eq!(quote.data["usd"], 0.025);
    assert_eq!(
        kit.call("pdf_mathpix", &args, &ctx).unwrap_err().kind,
        "denied"
    );
    assert!(fake.requests().is_empty());

    host.yes = true;
    let ctx = host.ctx(&cancel, &prices);
    let mut args = args;
    args["lines"] = true.into();
    args["mmd_zip"] = true.into();
    let output = kit.call("pdf_mathpix", &args, &ctx).unwrap();
    assert_eq!(output.data["remote_deleted"], true);
    assert_eq!(output.usage.unwrap().usd, 0.0);
    let lines = output
        .attachments
        .iter()
        .find(|item| item.filename == "mathpix-lines.json")
        .unwrap();
    let AttachmentContent::Bytes(bytes) = &lines.content else {
        panic!("期望行数据字节")
    };
    let lines: Value = serde_json::from_slice(bytes).unwrap();
    assert_eq!(lines["pages"][1]["page"], 4);
    assert_eq!(
        fake.requests()
            .iter()
            .filter(|request| request.method == Method::Post)
            .count(),
        1
    );
}

#[test]
fn app_overrides_affect_manifest_and_execution() {
    let cancel = Cancel::new();
    let prices = Prices::default();
    let host = Host {
        files: [("allowed".into(), Sample::Mixed.generate().unwrap())].into(),
        secrets: BTreeMap::new(),
        yes: false,
    };
    let overrides: Overrides = serde_json::from_value(json!({
        "pdf_render": {
            "description":"应用的页图说明。",
            "defaults":{"format":"png","max_side":128},
            "limits":{"max_side":{"maximum":256}},
            "confirm":{"kind":"always"}
        },
        "pdf_info": {"disabled":true}
    }))
    .unwrap();
    let kit = tools::toolkit_with_mathpix(quiet())
        .with_overrides(&overrides)
        .unwrap();
    let ctx = host.ctx(&cancel, &prices);
    let args = json!({"file":"allowed","page":1});
    let spec = kit
        .manifest()
        .iter()
        .find(|spec| spec.name == "pdf_render")
        .unwrap();
    assert_eq!(spec.description, "应用的页图说明。");
    assert!(!kit.manifest().iter().any(|spec| spec.name == "pdf_info"));
    assert_eq!(
        kit.call("pdf_info", &args, &ctx).unwrap_err().kind,
        "bad_args"
    );
    assert_eq!(
        kit.call("pdf_render", &args, &ctx).unwrap_err().kind,
        "denied"
    );
    assert_eq!(
        kit.call(
            "pdf_render",
            &json!({"file":"allowed","page":1,"max_side":257}),
            &ctx
        )
        .unwrap_err()
        .kind,
        "bad_args"
    );
    let mut host = host;
    host.yes = true;
    let output = kit
        .call("pdf_render", &args, &host.ctx(&cancel, &prices))
        .unwrap();
    assert_eq!(output.data["format"], "png");
    assert_eq!(output.data["height"], 128);
    assert_eq!(output.attachments[0].mime_type, "image/png");
}

#[test]
fn host_passwords_and_read_continuation_are_scoped_to_the_call() {
    let cancel = Cancel::new();
    let prices = Prices::default();
    let host = Host {
        files: [
            ("sealed".into(), Sample::Locked.generate().unwrap()),
            ("multi".into(), Sample::Mixed.generate().unwrap()),
        ]
        .into(),
        secrets: [(
            "pdf_password:sealed".into(),
            engine_pdf::samples::LOCKED_PASSWORD.into(),
        )]
        .into(),
        yes: true,
    };
    let ctx = host.ctx(&cancel, &prices);
    let info = tools::call("pdf_info", &json!({"file":"sealed"}), &ctx).unwrap();
    assert_eq!(info.data["info"]["encrypted"], true);
    let first = tools::call(
        "pdf_read",
        &json!({"file":"multi","images":false,"password":"never reflect"}),
        &ctx,
    )
    .unwrap();
    let next = first.next.unwrap();
    assert_eq!(next["start_page"], 6);
    assert!(next.get("password").is_none());
    let last = tools::call("pdf_read", &next, &ctx).unwrap();
    let pages = last.data["pages"].as_array().unwrap();
    assert_eq!(pages.len(), 5);
    assert_eq!(pages[0]["index"], 6);
    assert_eq!(pages[4]["index"], 10);
    assert!(last.next.is_none());
}

/// 测试不碰所有 App 共用的账本。
fn quiet() -> Options {
    Options {
        ledger: engine_pdf::mathpix::spend::Ledger::Off,
        ..Options::default()
    }
}

#[test]
fn the_spend_rule_asks_above_half_a_dollar_or_past_five_a_day() {
    use engine_pdf::mathpix::spend::{Entry, Ledger};
    let dir = std::env::temp_dir().join(format!("engine-pdf-tools-spend-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let ledger = Ledger::At(dir.join("ledger.jsonl"));
    let bytes = Sample::Mixed.generate().unwrap();
    let fake = Arc::new(FakeTransport::echo());
    let kit = tools::toolkit_with_mathpix(Options {
        transport: fake.clone(),
        ledger: ledger.clone(),
        ..quiet()
    });
    let mut host = Host {
        files: [("mixed".to_owned(), bytes)].into(),
        secrets: [
            ("mathpix_app_id".into(), "fixture-app".into()),
            ("mathpix_app_key".into(), "fixture-key".into()),
        ]
        .into(),
        yes: false,
    };
    let cancel = Cancel::new();
    let prices = Prices::default();
    let posts = || {
        fake.requests()
            .iter()
            .filter(|r| r.method == Method::Post)
            .count()
    };
    let args = json!({"file":"mixed","pages":[2]});
    // $0.005: no one asked, it runs (simulated: nothing goes in the ledger)
    let quote = kit
        .call("pdf_mathpix_quote", &args, &host.ctx(&cancel, &prices))
        .unwrap();
    assert_eq!(
        (quote.data["ask"].clone(), quote.data["why"].clone()),
        (json!(false), json!(""))
    );
    assert!(quote.text.contains("不用先问用户"), "{}", quote.text);
    kit.call("pdf_mathpix", &args, &host.ctx(&cancel, &prices))
        .unwrap();
    assert_eq!(posts(), 1);
    assert!(ledger.entries(std::time::UNIX_EPOCH).is_empty());
    // over $0.5 at once: asked; no means not a single request
    let mut dear = Prices::default();
    dear.set("Mathpix", "page", 0.3);
    let two = json!({"file":"mixed","pages":[1,2]});
    let quote = kit
        .call("pdf_mathpix_quote", &two, &host.ctx(&cancel, &dear))
        .unwrap();
    assert_eq!(quote.data["ask"], true);
    assert_eq!(
        kit.call("pdf_mathpix", &two, &host.ctx(&cancel, &dear))
            .unwrap_err()
            .kind,
        "denied"
    );
    assert_eq!(posts(), 1);
    // every app's last day near $5: a small one is asked too
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    ledger
        .record(&Entry {
            at: now - 60,
            app: "Peck".into(),
            service: "Mathpix".into(),
            pages: 999,
            usd: 4.998,
        })
        .unwrap();
    let quote = kit
        .call("pdf_mathpix_quote", &args, &host.ctx(&cancel, &prices))
        .unwrap();
    assert!(
        quote.data["why"]
            .as_str()
            .unwrap()
            .contains("最近 24 小时已花"),
        "{}",
        quote.data
    );
    assert_eq!(
        kit.call("pdf_mathpix", &args, &host.ctx(&cancel, &prices))
            .unwrap_err()
            .kind,
        "denied"
    );
    assert_eq!(posts(), 1);
    host.yes = true;
    kit.call("pdf_mathpix", &args, &host.ctx(&cancel, &prices))
        .unwrap();
    assert_eq!(posts(), 2);
}
