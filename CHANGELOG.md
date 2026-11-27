# CHANGELOG

记录各版本的公共接口变化和 App 升级事项。

## v0.6.0

- **可搜索扫描件**：`searchable(doc, pages, opts, cancel, progress)` 用 Vision 本机识别选中的扫描页，lopdf 写入不可见文字层（字和扫描图上的字对齐）。选页用原始物理页码；已有文字层的页跳过。加密文件先用权限密码打开，经 `edit::apply` 去密码。不需要 `pdfium`。
- **读回 OCR**：`read`、`read_each` 新增 `Content::SearchableText(String)`，按识别时的阅读顺序交回，换页序、合并后照样认得；`Summary::ocr` 计入这些页。穷尽匹配 `Content` 的要补分支。
- **对象编辑和涂黑**（feature `pdfium`，默认关）：`Document::objects(page)` 列出页面对象；`Plan.objects` 移动、删除可见的文字和图片，`Plan.redact` 在普通编辑之后涂黑。对象编号只由文件内容决定，同一份文件每次列出都一样。改动只写在那几条指令上，别处原样保留（`decisions/0019`）。Rust 直接初始化完整 `Plan` 的要补这两个字段，原来的 Plan JSON 照用。关掉 `pdfium` 时返回 `Error::PdfiumUnavailable`。
- **新错误**：`PdfiumUnavailable`、`ObjectNotFound`、`ObjectNotEditable`（字宽拿不准、删掉后会挪动同一行后面的字等）、`ObjectEditFailed`（对不上对象清单，或改完页面别处会变）、`RedactionFailed`、`RedactionSharedImage`。穷尽匹配 `Error` 的要补分支。
- **Web 套件和试用页**：`EditorTools` 支持对象点选、框选、拖动、删除，涂黑先画草稿、经 `confirmRedactions` 确认。App 用 `setPageObjects` 交对象清单。试用页加「生成可搜索副本」，开 `pdfium` 后加「选对象」「涂黑」。
- **分发**：开 `pdfium` 时静态链接预编译 PDFium（macOS arm64，二进制约 +7.3 MiB），随 App 附上 `THIRD_PARTY.md` 所列通知。

## v0.5.2

- 设密码的输出改由 lopdf 加密（同样是 128 位 RC4），PDFKit 只核对。以前用 PDFKit 加密，带表单的文件即使没有打开密码，在 Edge 里控件也全不显示。接口不变。
- 已知、不处理（站主定）：设了打开密码的表单，Edge 新版阅读器（Adobe 引擎）不显示控件；Chrome、「预览」、PDF.js、PDFium 都正常，网上有 Acrobat 自己生成的文件同样的报告。

## v0.5.1

v0.5.0 没单独打 tag，内容都在本版（下面 v0.5.0 一节照样适用）。本版是在「预览」里验收后的修正：

- 新加的便签带一个收起的弹窗（`/Popup`），「预览」里点便签图标才会展开批注文字。接口不变。
- 试用页新增目录项指向所选页的顶端（以前是页底）。App 自己做目录编辑时，`Target::Page.y` 也请用页顶（裁切框上沿），不要给 0。

## v0.5.0

页面与结构、预览级编辑（`ROADMAP.md` v0.5.0）。

- **页标签行为变了**：源文件没有 `/PageLabels` 时，`Document::label` 和逐页读取的 `label` 返回 `None`（以前给物理页码的数字，和 PDF.js 不一致）。App 显示时回退到物理页码。
- **读**：`Document::info`、`outline`、`annotations(page)`、`fields(&Cancel)`；`open_with_password` / `from_bytes_with_password` 收 `Zeroizing<String>`。
- **改**：`edit::apply(doc, others, &Plan, &Save, &Cancel)` 交回新字节，原文件不动。`Plan` 管页序（合并、插入、删除、旋转、重排、抽页）、加删标注（10 种）、填表、目录、文件信息；`Save` 管烧平、设密码、去密码。`Plan` 的 JSON 是 camelCase、`kind` 标签，和 `web/src/plan.ts` 一一对应。
- **写法**：lopdf 在原文件的对象上写（`decisions/0017`）。页序不变时其余内容原样保留；调页序时页标签、链接、目录、结构树跟着页走。
- **新建**：`compose::images` 照片打包，`compose::assemble` 拼 PDF（带链接、目录、信息）。
- **Web 套件**：`web/`（`@elenskye/engine-pdf-web`），PDF.js 6.5 上的无样式编辑逻辑，App 从同一 tag 构建离线包（`decisions/0016`）。试用页 `examples/studio`。
- **`Error` 新增种类**：密码（`WrongPassword` 等 4 种）、改动单（`InvalidSource`、`InvalidRotation`、`InvalidTarget`、`InvalidGeometry` 等）、表单（`FieldNotFound` 等）、新建（`EmptyImages`、`ImageInput`、`EmptyAssembly`）。`Error` 没标 `non_exhaustive`，App 里穷尽的 `match` 要补分支。
- 新依赖 `lopdf`（MIT，仅 macOS）。

## v0.4.0

先单独发版（2026-11-25 站主定），让 v0.5.0 开工；Peck 接入时再发现的引擎问题走之后的版本。

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
