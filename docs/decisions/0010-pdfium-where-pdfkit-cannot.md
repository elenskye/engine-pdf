# 0010 PDFium 只补 PDFKit 做不到的，先实验再定

2026-11-25 站主定（ASK）。部分改了 `0003` 里「不用 pdfium」那一条：取字、渲染、OCR 仍然只走 PDFKit 和 Vision。

查到的（本机 macOS SDK 头文件、Apple 开发者论坛）：

- PDFKit 存盘有 `PDFDocumentSaveTextFromOCROption`（macOS 13 起），存的时候系统自己识别扫描页、写进文字层；还有 `BurnInAnnotations`（烧平标注）、`AccessPermissions`（权限）。
- PDFKit 没有真正的涂黑：`PDFMarkupType` 里虽有 redact，但盖上的只是标注，底下的字照样能复制。PDFKit 也不能移动、删除页面内容里的文字和图片。

定的：

- **PDFium 只负责**移动与删除文字、图片，以及涂黑。
- **可搜索扫描件**在 v0.6 Phase 1 比两种做法：引擎自己的 OCR（阅读顺序已排好）+ 用 PDFium 写不可见文字层，对比 PDFKit 自带的写入。比字准、再读回的顺序、预览和 PDF.js 里选中和搜索高亮的位置、文件大小；结论补进本文。
- **feature `pdfium`，默认关**：只有要用的 App 才打开，Peck 这类不用的 App 不多带东西。
- **尽量静态链进 App 的二进制**，不另带文件（和 `0002`「随 App 的安装包发布、不另装」一致）；拿不到 macOS arm64 的静态库，再随 App 带 `libpdfium.dylib`（App 的打包要加资源、签名）。Phase 1 量过体积再定，结论补进本文，依赖登记 `THIRD_PARTY.md`（PDFium 和它带的 FreeType 等的许可证）。
- **涂黑的硬条件**：涂完用引擎再取字，框里一个字都取不到；页图里框内全黑；图片资源里也不留原像素。达不到就报错，不交文件。

没选的：三项都用 PDFium（可搜索扫描件要往 PDF 里嵌中文字体或无字形字体，PDFKit 也许已经够用）；暂不引入（移动、删除字和图做不了，涂黑只能整页转图）。
