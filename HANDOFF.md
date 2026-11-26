# HANDOFF

更新于 2026-11-26。v0.5.0 已完成、Claude 审核过，已提交未打 tag。最近已发布的引擎是 `v0.4.0`。

## 本轮（Claude 审核 Codex 的 Phase 1–6）

- **写入改成 lopdf 在原文件的对象上写**（`decisions/0017`）。新增 `src/sys/write.rs`（页面搬运、标注增删、链接、目录、页标签、结构树、文件信息）和 `src/sys/objects.rs`（lopdf 读写、交叉引用表修复、补回没有 `/Length` 的流、跨文件搬对象、CoreGraphics 画的 Form XObject）。删掉 `live_form.rs`、`page_content.rs` 和逐页核对文字层的流程。`forms.rs`、`flatten.rs`、`password.rs` 改用 `objects.rs`。
- **修的问题**（38 份真实论文实测发现）：
  - 知网论文任何编辑都报 `SaveFailed`。
  - 带结构树的期刊论文加不上标注、调不了页（`EditTextChanged`）。
  - 原本就找不到目标的链接，任何保存都被删掉。
  - 编辑后页标签丢失。
  - `/Fields` 为空的 `/AcroForm` 挡住调页。
  - 「预览」里中文文本框乱码。
  - 文本框文字被截断：现在不自动换行，放不下就整体缩小。
- 页序不变、只有旋转时也在原文件上改，表单、结构树都保留。
- 小处：`compose::images` 改走 `sys::images`；`Save` 字段补注释；试用页没填权限密码时直接提示；`samples::raw_pdf_with_objects` 公开给测试用。
- 新测试：知网式交叉引用表、无 `/Length` 的流、调页后页标签、具名链接、结构树、空表单字典，原地编辑不动其余对象，中文文本框的嵌入字体。
- 文档：`decisions/0017`（新），`0009`、`0012`（标为被取代）、`0013`、`0014`，design §6、§7，`CLAUDE.md` 工程约定一条，README、ROADMAP、CHANGELOG。

## 验证

- `./scripts/check.sh` 通过：两种配置的 `cargo test`、`clippy -D warnings`，Web 类型检查和测试。付费的 Mathpix 1 项保持忽略。`cargo check --target aarch64-unknown-linux-gnu --all-targets --features samples,fake` 通过。
- 真实论文验收：38 份（`~/ResearchLab-test` 28 份 + `local/pdf` 10 份）逐份跑整份倒序、转一页、调页加删页、13 种标注加删、读回重建、密码、合并，1,778 项通过。没过的 6 项是「有/无结构树的论文合并」，属已知行为（design §6）。验收程序在会话临时目录，没进仓库。
- 浏览器（试用页 + PDF.js 6.5.438）：
  - 知网论文：画矩形、椭圆、直线、箭头、链接、下划线、便签、中文文本框，转一页、删一页，存出 11 页。转的页尺寸对、每页文字不变、8 个标注读回。
  - 带结构树的英文论文：高亮、绘图、文本框，21 页文字不变，原有 12 个链接保留。
  - `form.pdf`：四类控件填值并设打开密码，存出后不给密码打不开，给了能开，四个值都对。
- 「预览」原生窗口没开（以前受 ScreenCaptureKit `-3811` 阻塞）。外观用引擎（PDFKit）画页图核对过：中文文本框、全套标注。

## 已知问题

- 用压缩对象流的文件，保存后最多变成原来的 1.6 倍大。lopdf 的对象流写法也只能降到约 1.5 倍，值得时再做。
- 有/无结构树的文件合并时整份不带结构树，带结构树那份按版面取字。
- `Mark::Image` 的字节在 JSON 里是数字数组，签名图大时请求体偏大，可改成 base64（改合同，等接 App 时一起定）。

## 等站主

- 按 `ROADMAP.md`「用户验收清单」的 v0.5.0 三条，在试用页和「预览」里各用一遍（运行步骤见 README「本地试用页」）。看过没问题就让 Claude 打 tag `v0.5.0`（push 由你做）。

## 下一轮

- 站主验收后打 tag `v0.5.0`。
- 之后照 ROADMAP：Peck 接入（等 Peck 1.1.12），或 v0.6.0 PDFium 级（计划已写，等批）。

## Mathpix 真跑

发版验收用 Pith 同一份凭据，写入会话临时目录的 `600` 权限文件，通过 `MATHPIX_APP_ID` / `MATHPIX_APP_KEY` 传入。只对样本真跑，每版上限 $1。v0.5.0 没动 Mathpix，没有真跑。

```bash
cargo test --features samples --test mathpix_live live_mathpix_mixed_page_8 -- --ignored --exact --nocapture
```
