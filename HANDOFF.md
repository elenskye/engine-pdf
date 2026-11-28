# HANDOFF

更新于 2026-11-28。**v0.8.0 进行中**（跟着 Pomelo 1.2.0 做，还没打 tag；v0.7.0 已发、已推）。Pomelo 用 `[patch]` 指本机这份开发。

## 这一版（v0.8.0，到现在）

本机 Claude 在 Pomelo 1.2.0 Phase 4 的会话里直接改（总仓库 `decisions/0009`），接口变化记在 `CHANGELOG.md`「v0.8.0（开发中）」：

- **Mathpix 花钱的规矩和共用账本**（站主 2026-11-28 定，`decisions/0023`）：一次超过 $0.5 或算不出费用、或所有 App 最近 24 小时合计加上这一次超过 $5 才问。**2026-11-28 下午改用 engine-core v0.4.0 的 `spend`**（引擎都停着时，在总仓库的会话里做的）：`mathpix::spend` 只再导出；工具包照账本定要不要问（`prepare` 的 `needs_confirm()`、`why()`，`call` 经 `Ctx.approve` 问；先弹卡片的 App 同意后 `prepared.approve()` 再 `run`，没同意的 `run` 报 `denied`）；账本 `<底>/engine-pdf/mathpix-ledger.jsonl`（`ENGINE_LEDGER_DIR` 或 `Ledger::At(底)` 改底，`Options::book()` 取）；命令行和 App 一样照规矩。Pomelo 要跟着改几处，见 `CHANGELOG.md`「App 升级要改的」。Peck 1.4.0 原定的「每次弹确认」照新规矩改（`ROADMAP.md` 已注）。
- **钉 engine-core v0.4.1**（tag 还没推，先指本机 `../engine-core`；推上去后换回 git tag）。
- **清理**（总仓库 `decisions/0013`）：`scripts/clean.sh`（engine-core 的 `engine-clean`）和开工钩子；已发版本的中间产物挪进了 `local/runs/v0.5.2/`、`v0.6.0/`、`v0.7.0/`，两套核对程序挪到 `local/checks/edit-redact/`、`local/checks/find-bounds/`；试用页存的副本改写到 `local/runs/v<版本>/studio/`。`check.sh` 开头把 `ENGINE_LEDGER_DIR` 指到临时目录，`web/node_modules` 没有就先装。
- **页图上画框**：`Document::page_box` + `PageBox::on_image`（四种旋转、CropBox 偏移），App 把 `pdf_find` 的框画到 `render` 的页图上用。
- **工具层多导出**：`tools::Toolkit`、`Prepared`、`to_openai`、`to_anthropic`（App 不用再直接依赖 engine-core）。

## 验证

- `./scripts/check.sh`：通过（三种配置的测试和 Clippy、Web 检查和测试）。新测试：花钱规矩（单次、24 小时、各 App 合计、写坏的行跳过）、工具层按规矩问和拒绝零请求、真上传进账本而模拟的不进、四种旋转加 CropBox 偏移的框落在字上。
- 在 Pomelo 里：知网论文（CropBox 和 MediaBox 不同）找「政治效能感」，框正好落在标题那几个字上；Pomelo 的测试覆盖了按规矩出卡片、拒绝不上传。
- 没做：Mathpix 真跑（上传的代码只多了记账，Pomelo Phase 6 一起真跑）。

## 已知问题

- 不开 `pdfium` 时，连字丢失和偏高的中文框照旧（Peck 不开）。
- 扫描页上盖的字超过 20 个（JSTOR 那种「This content downloaded from…」）时，`read`、`find` 都当文字页、不识别；要识别得看页面上是不是整页大图，没做。
- 进程里第一次用识别模型要等 16–33 秒（系统的，引擎管不了）；查找的缓存只在进程里。

## 等站主

- 不急：v0.8.0 的提交在本机 `main` 上，打 tag 前推不推都行（Pomelo 现在用本机路径）。

## 下一步

- **Peck 1.4.0**：Peck 1.3.0 做完后在 Peck 仓库开工，钉 `v0.7.0`（`ROADMAP.md`「v0.4.0」）。
- **Pomelo 1.2.0**：Phase 1–4 做完（对照和验收在 Pomelo 的 `docs/roadmap.md`「1.2.0」）。Phase 5 时引擎补 `mathpix::convert` 和从 Pomelo 旧任务接着跑的 `Submitted`（Word 的 `submit_file` 记账要按服务报的页数补记），Phase 6 打 tag `v0.8.0`。
- **Pith（v0.9.0）**：大纲在 `ROADMAP.md`，开工前细化。

## Mathpix 真跑

发版验收用 Pith 同一份凭据，写入会话临时目录的 `600` 权限文件，通过 `MATHPIX_APP_ID` / `MATHPIX_APP_KEY` 传入。只对样本真跑，照 `CLAUDE.md`「五」「花钱的规矩」（真花的钱记进共用账本）。

```bash
cargo test --features samples --test mathpix_live live_mathpix_mixed_page_8 -- --ignored --exact --nocapture
```
