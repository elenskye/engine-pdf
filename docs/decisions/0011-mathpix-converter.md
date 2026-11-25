# 0011 Mathpix 的转换（/v3/converter）搬进引擎

2026-11-25，Claude 定（`docs/brief-pomelo.md` 留给计划定）。改了 `0004` 的最后一条。

- v0.7.0 接 Pomelo 时，`/v3/converter`（MMD 转 docx、tex.zip、html.zip 等）作 `mathpix::convert`，走同一个 `Transport`、同一套密钥、错误种类和日志规矩，`FakeTransport` 也能模拟。
- 原因：转换只占约 60 行，留在 Pomelo 的话，Pomelo 为它还得留下整套 reqwest 的 Mathpix 客户端（鉴权、错误、超时）；搬进来以后 Pomelo 的 `pub2md/mathpix.rs` 能整份删掉，Mathpix 只剩引擎这一处。
- 转换不收确认、不报价（Mathpix 按转换计不计费，接入时查清楚写进 design §3）；转出的文件 Mathpix 那边留不久，引擎拿到就交回，不存盘。
