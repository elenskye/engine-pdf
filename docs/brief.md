# engine-pdf · 第一轮设计简报

## 目标

给 Peck、Pith、Pomelo 共用一套 Mac PDF 引擎。三个 App 可以把 PDF 和指定页码交进来，拿回文字、页图或识别结果。PDF 处理的代码只维护一份，三个 App 各自决定怎么展示和使用结果。

这轮请 Claude Code 做架构设计，交一份 `design.md`。需要站主拍板的取舍，用 ASK 直接问。设计确认后由 Codex 实施，先接 Peck，验收后再接 Pith 和 Pomelo。

## 已定的范围

- **Mac 优先。** 用系统 PDFKit 打开 PDF、按页读文字和渲染页图。普通扫描件用 Vision 在本机识别。
- **复杂公式和表格用 Mathpix。** 由 App 在上传前展示服务、原文件页码和预计费用。站主确认后，引擎在本地拆出指定页，只上传这些页。结果映射回原文件页码，取消确认时直接结束。
- **独立 Rust 库。** 库放在 `~/Documents/GitHub/engine-pdf/`。三个 App 在构建时各自引用固定版本，随自己的安装包发布，可以独立升级和回退。
- **App 按需调用。** 这轮共用的是打开、抽字、渲染和识别。界面、阅读交互、文件权限、业务数据和费用确认由各 App 管。Pith 继续用现有 PDF.js 阅读器，Pomelo 可以直接把识别结果用在知识库和 Pub2MD。

## 先看现有实现

以下三个仓库供这轮只读参考。先核对代码里实际用到的能力，再定共用接口。

| App | 在哪里看 | 第一批接入点 |
| --- | --- | --- |
| Peck | `~/Documents/GitHub/personal-cli-studio/peck/` 的 `src-tauri/src/harness/tools/pdf.rs`、`src-tauri/src/gateway/mathpix.rs` | 从现有 `read_pdf`、`ocr_pdf` 入口接入。参考已有的按页取字、附图和选页后上传。 |
| Pith | `~/Documents/GitHub/pith/` 的 `src-tauri/src/research/content.rs`、`src-tauri/src/concept/mathpix.rs`、`src/reader/controller.ts` | 先接论文后台取字和识别。检查返回的页码能否用于引用定位。 |
| Pomelo | `~/Documents/GitHub/pomelo/` 的 `src-tauri/src/youzi/pdf.rs`、`src-tauri/src/youzi/files.rs`、`src-tauri/src/youzi/assistants/pub2md/` | 先看知识库导入和 Pub2MD 原页图。核对哪些 PDFKit 调用可替换，哪些与 App 自己的流程有关。 |

## 设计稿要回答什么

1. **库怎么用。** 给出打开、取字、渲染、OCR 和 Mathpix 所需的最小接口。说明页码、图片怎么交回，以及 App 怎么分辨文字层、OCR、空白和读取失败。给一段 Peck 调用示例即可。
2. **原生实现怎么复用。** 说明怎样从 Rust 调 PDFKit 和 Vision、准备复用哪段现有代码，以及长文档如何按页处理和取消。
3. **云端调用的交界在哪。** 说明 App 的密钥、确认和记账怎样接到共用库。写清指定页的拆分、上传、结果下载、原页码映射，以及取消或失败时怎么收尾。
4. **三个 App 怎么迁移。** 写清 Peck 最先改哪个入口、原来的行为怎么核对。再列 Pith 和 Pomelo 的接入点、每个 App 会替换掉的重复实现和退回旧版的方式。
5. **怎么验收。** 用一组可提交的公开或自制样本跑完下面的场景。列清哪些能自动验证、哪些要看实际输出。

## 首轮验收场景

- 用 Peck 读指定页，能取到文字或页图，回答能回指原文件页码。用中文和双栏样本检查缺字和阅读顺序。
- 一份 PDF 里混有文字页、扫描页和空白页。断网时普通扫描能被识别，结果能区分原文文字、OCR 结果和空白页。
- 只选第 8 页识别公式或表格。上传确认显示第 8 页和预计费用，取消时没有发出请求。确认后上传件只有这一页，结果仍标成原文件的第 8 页。
- 文件损坏或需要密码时，App 能给出明确提示。接入后的 App 从自己的安装包调用引擎。

设计稿以能交给 Codex 开始实施为准。收尾用几句话讲清楚第一步要改什么、为什么从那里开始，以及还有哪些效果需要用样本验证。
