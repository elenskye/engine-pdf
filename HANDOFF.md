# HANDOFF

更新：2026-11-24 · v0.2.0 已发版（tag `v0.2.0`）· Claude Code 审核

## 现在在哪

v0.2.0（本机读取）完成：Codex 实施 Phase 1–5，Claude 审核、改进、实测、提交、打 tag。下一步是 v0.3.0（Vision 本机 OCR + Mathpix），Phase 计划等站主答了决策点再写进 `ROADMAP.md`。

## 这轮审核改了什么

- **短文字不丢**：原来只有几个字的页（标题页、只有页码的页、一句话的幻灯片）被判成 `NoText` / `Blank`，那几个字直接丢掉。现在页面上除了这几个字别无他物的算 `Text`；有图形的算 `NoText`，字放在 `partial` 里交回。`Content::NoText` 从元组改成 `{ reason, partial }`。
- **页图画 CropBox**：原来画 MediaBox（Peck、Pomelo 的老做法），知网等文件两者不同，页图会比阅读器里看到的多出边。
- **文件头**：前面带杂字节的 PDF（PDFKit、Pith 都能开）原来被判成「不是 PDF」，现在容忍到 1024 字节。
- **去掉还没用到的**：`Error::Ocr`、`Error::Mathpix`、`MathpixError` 留到 v0.3.0 按 design §3 再加。
- **结构**：手写最小 PDF 的代码原来在样本生成器和测试里各一份，合成 `samples::raw_pdf`（App 的测试也能用）；集成测试按主题重排成 `tests/document.rs`、`tests/read.rs`（原来叫 phase34 / phase5）；`probe` 加 `--text`；公开接口补了文档注释。
- **文档**：决策 0005–0007（页码错误、页序、灰度格式）是接口细节，并进 design §1 后删掉；design 的实测表、README、CHANGELOG、ROADMAP、CLAUDE.md（检查命令、测试命名）精简更新。

## 测试

- `./scripts/check.sh`：通过（两种配置的 `cargo test` + `cargo clippy -D warnings`；单元 10、`document` 6、`read` 4、文档测试 1）。
- `cargo check --target aarch64-unknown-linux-gnu --all-targets`（开 / 不开 `samples`）：通过。
- **真实文件**（`~/ResearchLab-test/zotero-copy` 28 篇 + Pith `local/` 2 篇，中英文、单双栏）：全部打开，535 页全是 `Text`、没有失败，每份 30–220 ms。**和 Peck 现在的 JXA 读法逐字比对 535 页完全一致**。中文知网、英文双栏的顺序看过，正常。
- 页图：横向旋转页摆正、中文页清晰（人看过）；真实论文 1568px JPEG 约 40 ms/页。
- 坏文件：截掉一半、截掉 10% → `Damaged`；只坏了尾部目录 → PDFKit 修好照常读；中间写进 4KB 乱码 → 能读（坏的那页内容可能不对，测不出来）。只设权限密码的 PDF 照常读。

## 已知问题

- PDFKit 原样交出连字（`ﬀ`）、全角数字；个别字体的连字没有文字对照（XGBoost 论文里 `https` 读成 `hps`）。整理归 App，取不到的只能靠 OCR。
- 这批真实文件里没有扫描件，`NoText` / `Blank` 只在生成样本上验过；v0.3.0 需要真实扫描件（见「等你」）。

## 等你

1. **push**：

   ```bash
   git push origin main
   ```

   ```bash
   git push origin v0.2.0
   ```

2. **回答 v0.3.0 的决策点**（本轮最后的提问）。
3. （v0.3.0 用）**一两份真实扫描 PDF**：中文扫描书或论文的几页就够，放进 `local/`（不进 Git）。没有的话只用生成样本验 OCR。

## 下一轮

站主答完决策点 → Claude 把 v0.3.0 的 Phase 表写进 `ROADMAP.md` → Codex 照 `AGENTS.md` 做 v0.3.0 Phase 1。
