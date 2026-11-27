# 0010 PDFium 只补 PDFKit 做不到的，先实验再定

> 2026-11-27：`0022` 改了「取字只走 PDFKit」：开了 `pdfium` 时用 PDFium 同一页的字当参照，补 PDFKit 取丢的连字、收紧偏高的中文行框。

2026-11-25 站主定（ASK）。部分改了 `0003` 里「不用 pdfium」那一条：取字、渲染、OCR 仍然只走 PDFKit 和 Vision。

查到的（本机 macOS SDK 头文件、Apple 开发者论坛）：

- PDFKit 存盘有 `PDFDocumentSaveTextFromOCROption`（macOS 13 起），存的时候系统自己识别扫描页、写进文字层；还有 `BurnInAnnotations`（烧平标注）、`AccessPermissions`（权限）。
- PDFKit 没有真正的涂黑：`PDFMarkupType` 里虽有 redact，但盖上的只是标注，底下的字照样能复制。PDFKit 也不能移动、删除页面内容里的文字和图片。

定的：

- **PDFium 只负责**移动与删除文字、图片，以及涂黑。（2026-11-27 改为只读：列对象、取字形框、解图；写入在原内容流上，见 `0019`。）
- **可搜索扫描件**在 v0.6 Phase 1 比两种做法：引擎自己的 OCR（阅读顺序已排好）+ 用 PDFium 写不可见文字层，对比 PDFKit 自带的写入。比字准、再读回的顺序、预览和 PDF.js 里选中和搜索高亮的位置、文件大小；结论补进本文。
- **feature `pdfium`，默认关**：只有要用的 App 才打开，Peck 这类不用的 App 不多带东西。
- **只用现成的预编译库，不从源码编**（2026-11-26 站主定）：从源码编 PDFium 占磁盘、费时间，和 Peck 的 Tauri 构建同时跑时 18 GB 内存吃紧；预编译库都不行时先问站主。
- **尽量静态链进 App 的二进制**，不另带文件（和 `0002`「随 App 的安装包发布、不另装」一致）；拿不到 macOS arm64 的静态库，再随 App 带 `libpdfium.dylib`（App 的打包要加资源、签名）。Phase 1 量过体积再定，结论补进本文，依赖登记 `THIRD_PARTY.md`（PDFium 和它带的 FreeType 等的许可证）。
- **涂黑的硬条件**：涂完用引擎再取字，框里一个字都取不到；页图里框内全黑；图片资源里也不留原像素。达不到就报错，不交文件。

没选的：三项都用 PDFium（可搜索扫描件要往 PDF 里嵌中文字体或无字形字体，PDFKit 也许已经够用）；暂不引入（移动、删除字和图做不了，涂黑只能整页转图）。

## Phase 1–2 结论（2026-11-26 ASK）

- **预编译静态库**采用 [libviprs 的 PDFium chromium/8085](https://github.com/libviprs/libviprs-dep/releases/tag/pdfium-8085)，验证了 macOS arm64。绑定用 `pdfium-render` 的 `pdfium_7881`、静态链接和线程安全配置。开 `pdfium` 的构建将 `PDFIUM_STATIC_LIB_PATH` 指向归档里的 `lib` 目录。依赖和随 App 分发所需通知见 `THIRD_PARTY.md`。
- **可搜索扫描件**用 Vision 识别，lopdf 在选中页的原有对象上加不可见文字。PDFKit 存盘选项在已测样本上没有生成可提取的持久文字层。Vision + PDFium 原型能写入搜索层，但 PDFKit 会按版面重新排列复杂多栏页。引擎在页上保存 Vision 排好的文本，`read` 用 `Content::SearchableText` 交回。App 使用此能力无需开 `pdfium`。
- 保留乱码页的原有内容。引擎读回排序好的纯 OCR 文本，阅读器复制整页时可能带出原乱码。加密文件按现有编辑权限处理，由调用方用权限密码打开并明确去密码后再调用。
- 涂黑试验覆盖跨框文字、图片和框内批注。只盖黑块时 PDF 仍可取回原始内容。删除整块文字、移除批注、替换遮盖区域的图像并完整保存后，测试文件能取出的文字和批注计数各从 13 字、1 个降到 0，交叉引用表里的图像流只剩替换后的像素。跨框文字要保留框外字时，按 Phase 4 的整页后备方案处理。涂黑验证也须检查和清理页上的 OCR 文本流。
