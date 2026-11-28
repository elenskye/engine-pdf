# 0023 Mathpix 花钱的规矩和共用账本

2026-11-28 站主定（Pomelo 1.2.0 Phase 4 的 ASK）。改了 v0.7.0「`pdf_mathpix` 每次确认」。

## 定的

- **什么时候问用户**：一次超过 $0.5、或算不出这一次要花多少，先问；所有用 engine-pdf 的 App 合起来最近 24 小时花的，加上这一次超过 $5，也先问（窗口滚动，每过 24 小时自然刷新）。其余情况模型直接用，不问。不同意就一个请求都不发。
- **规矩写在引擎里，所有 App 一样**：单次和合计都由引擎算（`mathpix::spend`），`pdf_mathpix` 在上传前自己判断、要问时经 `Ctx.approve` 问；清单的 `confirm` 改成 `never`，免得问两次。`pdf_mathpix_quote` 的结果带 `ask`、`why`，模型和 App 都能先知道这一次要不要问。App 想先弹卡片再执行（`prepare` → 卡片 → `run`），用 `Options.ledger.approval(Some(usd))` 拿同一个判断，`Approve` 交回用户的选择。
- **账本在引擎里，所有 App 共用**：`~/Library/Application Support/engine-pdf/mathpix-ledger.jsonl`（`ENGINE_PDF_LEDGER` 可改），每次真正上传成功（拿到任务编号就算，之后失败也收费）追加一行：时间、App（`Options.app`）、页数、金额。模拟的传输不记。App 要报告花了多少就读 `Ledger::entries`。几个 App 同时写各追加一整行，不会写坏。
- **命令行照旧**：`engine-pdf call pdf_mathpix` 不加 `--yes` 只报价（总仓库 GUIDE「一」），用 `tools::terminal_toolkit`；另外照样记账、照样按规矩判断。
- **开发和测试不碰共用账本**：App 的开发版、测试用 `Ledger::At`（自己的文件）或 `Ledger::Off`；引擎自己的测试一律 `Off` 或临时文件。真跑的验收照常记进共用账本（真花了钱）。

## 没选的

- 每次都问：模型用一页公式识别也要等人点，站主嫌烦。
- 账本放在各 App 自己的数据里：合计要跨 App 算，各记各的就合不起来。
- 用 SQLite 记账：引擎不带数据库；一行一次的追加写够用，读的时候跳过写坏的行。
