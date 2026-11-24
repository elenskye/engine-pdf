# CHANGELOG

每个发出去的版本（tag `v<版本>`）：公共接口变化、App 升级要改什么。

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
