# HANDOFF

更新于 2026-11-28。**v0.8.0 已打 tag，等站主推**（跟着 Pomelo 1.2.0 做完；v0.7.0 已发、已推）。Pomelo 现在用 `[patch]` 指本机这份，推上去后改钉 tag `v0.8.0`。

## 这一版（v0.8.0）

本机 Claude 在 Pomelo 1.2.0 的会话里直接改（总仓库 `decisions/0009`），接口变化见 `CHANGELOG.md`「v0.8.0」：

- **Mathpix 花钱的规矩和共用账本**（`decisions/0023`）：规矩和账本是 engine-core v0.4.1 的 `spend`（钉 git tag），`mathpix::spend` 只再导出；工具包照账本定要不要问，先弹卡片的 App 同意后 `prepared.approve()` 再 `run`。账本 `<底>/engine-pdf/mathpix-ledger.jsonl`。
- **转格式、Word 的页数、接着跑旧上传**（`decisions/0024`）：`mathpix::convert`（价目表没列，当不收费：不问、不记账；`echo` 也模拟）；`submit_file` 先按估计页数记账、`wait` 按服务报的补；`Submitted::uploaded` 接着跑引擎之前上传的。`echo` 里没字有图的页交回一句模拟的话。
- **页图上画框**：`Document::page_box` + `PageBox::on_image`。
- **工具层多导出**：`tools::Toolkit`、`Prepared`、`to_openai`、`to_anthropic`。

## 验证

- `./scripts/check.sh`：通过（三种配置的测试和 Clippy、Web 检查和测试）。新测试：转格式（假服务和脚本：格式没好时等、某种失败时别的照样交回）、`Submitted::uploaded` 只发 GET 不记账、Word 先记估计再补一次（存下再等不重复补）、`echo` 的照片页。
- **Mathpix 真跑**（在 Pomelo 1.2.0 Phase 6，共用账本里记成「Pomelo（测试）」，合计 $0.065）：柚子的 PDF 工具箱识别样本第 8 页的公式（$0.005，没到门槛直接跑）；Pub2MD 整份 10 页样本（$0.05）和两张图（各 $0.005），上传、等、下载、`.mmd.zip` 都对；转格式 Word、PDF、LaTeX、HTML 三十来次都转出来了，只有两次 Mathpix 报某个格式转不出来（隔一会儿同样的内容又能转，Pomelo 那边失败的格式隔 15 秒再要一次）。
- 在 Pomelo 里：知网论文的框落在字上（Phase 4）；Pub2MD 的模拟流程、按规矩停在上传前、退出后继续不重传、1.1 的半截任务接着跑都有测试。

## 已知问题

- 不开 `pdfium` 时，连字丢失和偏高的中文框照旧（Peck 不开）。
- 扫描页上盖的字超过 20 个（JSTOR 那种「This content downloaded from…」）时，`read`、`find` 都当文字页、不识别。
- Vision 偶尔丢字（样本扫描页把「北面的山脊」读成「北面山脊」），`find` 整句就找不到那一处。
- 进程里第一次用识别模型要等 16–33 秒（系统的，引擎管不了）；查找的缓存只在进程里。
- `/v3/converter` 偶尔报某个格式转不出来（`Processing`），过一会儿同样的内容能转；引擎不自己重试，App 决定。

## 等站主

1. 推 engine-pdf：`main` 和 tag `v0.8.0`（engine-core v0.4.1 已推）。推完 Pomelo 改钉 tag、去掉 `[patch]`。

## 下一步

- **Peck 1.4.0**：Peck 1.3.0 做完后在 Peck 仓库开工，钉 `v0.8.0`（`ROADMAP.md`「v0.4.0」；Mathpix 照新规矩）。
- **Pith（v0.9.0）**：大纲在 `ROADMAP.md`，开工前细化。

## Mathpix 真跑

发版验收用 Pith 同一份凭据，写入会话临时目录的 `600` 权限文件，通过 `MATHPIX_APP_ID` / `MATHPIX_APP_KEY` 传入。只对样本真跑，照 `CLAUDE.md`「五」「花钱的规矩」（真花的钱记进共用账本）。

```bash
cargo test --features samples --test mathpix_live live_mathpix_mixed_page_8 -- --ignored --exact --nocapture
```
