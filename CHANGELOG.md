# CHANGELOG

每个发出去的版本（tag `v<版本>`）：公共接口变化、App 升级要改什么。

## v0.4.0（开发中）

- **默认每页都附页图**：`ReadOptions::default()` 的 `render` 从 `None` 改成 `Some(Render::jpeg(1568))`，空白页、读失败的页除外（`decisions/0007`）。只要文字、不给模型看的场景（建索引、存库）请显式写 `render: None`。
- 去掉 `Render::only_no_text()`（只给没取到字的页出图）。

## v0.3.0

本机 OCR 和 Mathpix。

- **本机 OCR**：`Document::ocr`、`ocr::warm_up`、`OcrOptions`（默认简体、繁体、英文）。结果有按阅读顺序拼好的文字（两栏、多栏、通栏标题都排对）、逐行置信度和框（原 PDF 坐标）。
- **逐页读取**：`ReadOptions { ocr: Ocr::auto(), .. }` 让文字层不够或乱码的页走 OCR。新增 `Content::Ocr`、`NoTextReason::NothingRecognized`、`Error::Ocr`、`Summary::ocr`。`Ocr`、`ReadOptions` 不再是 `Copy`，要复制时 `clone()`。
- **Mathpix**：`quote` / `quote_with_options` 本地报价（不联网），确认后 `run`；或分步 `submit`（`Submitted` 可序列化）/ `wait` / `fetch` / `delete`；Word 用 `submit_file`。成功、失败都带 `Usage`，结果映射回原页码（`Mapping::Exact` / `Approximate`）。错误是 `Error::Mathpix(MathpixError)`。
- **日志**：引擎在编译期关掉 `log` 的 TRACE（`ureq` 会在 TRACE 里记下密钥），用同一个 `log` 的 App 也跟着没有 TRACE，DEBUG 及以上照常（`decisions/0006`）。
- **测试工具**：feature `fake`（`FakeTransport::scripted` / `echo`，给 App 的模拟模式）；`samples::scan` 把任意一页做成仿扫描件；`probe` 加 `--ocr`、`--mathpix <页码>`；`examples/make_scan`。

## v0.2.0

首个可用版本：本机读取。

- `Document`：从路径或字节打开（`NotPdf`、`Damaged`、`Locked` 分开报；文件头前的杂字节容忍到 1024 字节），`page_count`、`label`、`text`、`render`（JPEG / PNG / 灰度，画 CropBox，带旋转）、`image_counts`、`split`。`Document` 不跨线程。
- `read` / `read_each`：逐页分类为 `Text`、`Blank`、`NoText { reason, partial }`、`Failed`；按传入顺序、允许重复，越界页交 `Failed` 后接着读；`Flow::Stop` 提前结束，交回 `Summary`。`ReadOptions` 可调三个阈值、要不要附图。
- `Cancel`（跨线程取消，每页检查）、`Error`（中文 `Display`）、`pages::parse`（Peck 的页码语法）。
- feature `samples`：生成测试 PDF（`Sample`）和手写边界页（`raw_pdf`）。`examples/probe` 人工验收用。
- 还没有 OCR 和 Mathpix（`Ocr` 只有 `Off`）。

## v0.1.0（未打 tag）

- 初始化仓库，包含设计稿 `docs/design.md`、选型 `docs/decisions/0001–0004` 和 Phase 计划 `ROADMAP.md`。
- 还没有可用的接口（只有 `VERSION`），App 不要引用这一版。
