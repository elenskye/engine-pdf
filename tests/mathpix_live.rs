//! v0.3.0 的付费验收。只发送生成样本的第 8 页。

#![cfg(all(target_os = "macos", feature = "samples"))]

use std::{env, fs, time::Duration};

use engine_pdf::{
    Cancel, Document,
    mathpix::{self, Keys, Mapping, Method, Options, Request},
    samples::Sample,
};

#[test]
#[ignore = "会上传生成样本的第 8 页，按一页计费。"]
fn live_mathpix_mixed_page_8() -> Result<(), Box<dyn std::error::Error>> {
    let keys = Keys::new(
        env::var("MATHPIX_APP_ID").map_err(|_| "请设置 MATHPIX_APP_ID。")?,
        env::var("MATHPIX_APP_KEY").map_err(|_| "请设置 MATHPIX_APP_KEY。")?,
    );
    if keys.app_id().trim().is_empty() || keys.app_key().trim().is_empty() {
        return Err("Mathpix 凭据不能为空。".into());
    }

    let document = Document::from_bytes(&Sample::Mixed.generate()?)?;
    let quote = mathpix::quote(&document, &[8])?;
    assert_eq!(quote.pages(), [8]);
    assert!(quote.usd() <= 0.005, "验收只上传一页。");
    let options = Options {
        lines: true,
        ..Options::default()
    };
    let result = match mathpix::run(&document, &quote, &keys, &options, &Cancel::new(), &|_| {}) {
        Ok(result) => result,
        Err(failure) => {
            eprintln!(
                "用量 {} 页 ${:.3}，估计 {}，远端删除 {:?}。",
                failure.usage.pages_billed,
                failure.usage.usd,
                failure.usage.estimated,
                failure.usage.remote_deleted,
            );
            return Err(failure.error.to_string().into());
        }
    };
    let usage = &result.usage;
    println!(
        "用量 {} 页 ${:.3}，估计 {}，远端删除 {:?}。",
        usage.pages_billed, usage.usd, usage.estimated, usage.remote_deleted,
    );
    assert_eq!(usage.remote_deleted, Some(true));
    let pdf_id = usage.pdf_id.as_deref().ok_or("缺少 Mathpix 任务 ID。")?;
    let reply = options.transport.send(Request {
        method: Method::Get,
        base: &options.base,
        path: &format!("/v3/pdf/{pdf_id}"),
        keys: &keys,
        content_type: None,
        body: &[],
        timeout: Duration::from_secs(30),
    })?;
    let status: serde_json::Value =
        serde_json::from_slice(&reply.body).map_err(|_| "读不懂删除后的任务状态。")?;
    assert_eq!(reply.status, 200, "查询删除状态失败。");
    assert!(
        status["deleted_at"]
            .as_str()
            .is_some_and(|value| !value.is_empty()),
        "Mathpix 尚未返回删除时间。"
    );

    assert_eq!(result.mapping, Mapping::Exact);
    assert_eq!(result.pages.len(), 1);
    let page = &result.pages[0];
    assert_eq!(page.index, 8);
    assert!(page.lines.as_ref().is_some_and(|lines| lines["page"] == 8));
    assert!(!page.mmd.trim().is_empty());
    assert_eq!(usage.pages_billed, 1);
    assert!(!usage.estimated);
    assert_eq!(usage.usd, quote.usd());
    println!("原 PDF 第 8 页，MMD 和行数据映射正确，已确认远端删除时间。");

    if let Some(output) = env::var_os("MATHPIX_LIVE_MMD_OUT") {
        assert!(
            !page.mmd.contains(keys.app_id()) && !page.mmd.contains(keys.app_key()),
            "回应含有凭据，未保存。"
        );
        fs::write(output, &page.mmd)?;
    }
    Ok(())
}
