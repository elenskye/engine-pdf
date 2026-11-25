# HANDOFF

更新：2026-11-25 · v0.3.0 已发版（tag `v0.3.0`）· Claude Code 审核

## 现在在哪

v0.3.0（本机 OCR + Mathpix）完成：Codex 实施 Phase 1–5，Claude 审核、改进、实测、提交、打 tag。v0.4.0 接 Peck 的计划已写进 `ROADMAP.md`（站主定了：Peck 这边算小幅优化、一轮做完），等 Peck 1.1.10 提交后开工。

## 这轮审核改了什么

- **OCR 阅读顺序**：原来只认两栏，四栏杂志页（《经济学人》The World This Week）会一行左栏一行右栏地串着读。改成递归切块（`src/ocr/order.rs`）：先找栏缝、把通栏标题和页脚分出去，没有栏缝再按大空白上下切。四栏、两栏加插图、单栏都按阅读顺序排对；加了四栏的单元测试。
- **仿扫描件工具**：原来是 Swift 脚本 `examples/make_scan.swift`，改成引擎自己的 `samples::scan(&doc, page)`（App 的测试也能用）和 `examples/make_scan.rs`，删掉 Swift 脚本；加了「文字页 → 仿扫描 → OCR 读回」的测试。
- **Mathpix**：流程（提交、等待、下载、删除）从 `src/mathpix.rs` 拆到 `src/mathpix/job.rs`，`mathpix.rs` 只留类型；响应加 1 GB 上限，超了报「文件太大」（`TransportError::TooLarge`）；`probe --mathpix` 带上 `lines.json` 并打印每页映射到的原页码。
- **文档**：design §2.3、§2.5、§5.1、§6（实测结论）更新精简；CHANGELOG、ROADMAP、README 定稿。

## 测试

- `./scripts/check.sh`：通过（默认配置 + `samples,fake`，`cargo test` + `cargo clippy -D warnings`；单元 18、`document` 6、`mathpix` 7、`ocr` 4、`read` 4、文档测试 1，付费测试 1 个默认忽略）。`cargo fmt --check` 通过。
- `cargo check --target aarch64-unknown-linux-gnu --all-targets`（默认、`samples,fake`）：通过。
- **站主的真实扫描图**（`local/ocr/`，5 张：《经济学人》两栏、四栏页，繁体中文长文）：全部识别成功；改完后栏序全对；繁体整段几乎无错（一页约 3 处）。系统第一次加载模型 33 s，之后每页 0.4–0.6 s。
- **仿扫描件字错率**（整页比，含页眉、脚注、全半角）：英文约 3%，简体中文 9–11%。Vision 常漏「的」，换分辨率和设置都一样（design §6）。简体页不会被认成繁体。
- **禁网**：`sandbox-exec` 禁掉网络后（确认连不上 api.mathpix.com），`mixed` 10 页照常分类和识别，真实扫描页照常识别。
- **Mathpix 真跑**：Codex 跑第 8 页（$0.005）；Claude 跑第 3、5、8 页（空白、中文扫描、公式表格，$0.015）——三页都精确映射回原页码，公式是正确的 LaTeX，表格完整，远端已删。本版共花 $0.02（上限 $1）。

## 已知问题

- Vision 中文会漏「的」等字，逐字要求高的页用 Mathpix。
- 引擎关掉了 `log` 的 TRACE（`decisions/0006`），接进 App 后整个 App 都没有 TRACE 日志（DEBUG 及以上照常）。
- 系统第一次加载 OCR 模型要 16–33 s，App 要么启动时后台调 `ocr::warm_up()`，要么第一次识别时显示进度。

## 真跑

密钥沿用 Pith 真跑那份（站主同意，每版上限 $1）：在会话临时目录建 600 权限的文件，用 `MATHPIX_APP_ID` / `MATHPIX_APP_KEY` 交给程序，不打印。只对生成样本跑：

```bash
cargo test --features samples --test mathpix_live live_mathpix_mixed_page_8 -- --ignored --exact --nocapture
```

```bash
cargo run --release --example probe -- samples/out/mixed.pdf --mathpix 3,5,8 --text
```

## 等你

1. **push**：

   ```bash
   git push origin main
   ```

   ```bash
   git push origin v0.3.0
   ```

2. **看 v0.4.0（接 Peck）的计划**：`ROADMAP.md`「v0.4.0 接 Peck」。你在提问里定的都写进去了。
3. **Peck 1.1.10 做完、提交后**告诉我们一声，v0.4.0 才能开工（两边改的是同一个仓库）。

## 下一轮

Peck 1.1.10 提交后 → 让 Codex 在 Peck 仓库做 v0.4.0：「读 engine-pdf 的 AGENTS.md、HANDOFF.md、ROADMAP.md『v0.4.0 接 Peck』，再读 Peck 的 CLAUDE.md；照步骤 1–5 做，Peck 的改动守 Peck 的规矩，引擎的改动在 engine-pdf 做；做完两边都写 HANDOFF」。Codex 做完回到 Claude 审核、commit、打 tag。
