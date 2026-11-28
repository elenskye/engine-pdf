# engine-pdf

Peck、Pith、Pomelo 共用的 Mac PDF 引擎。Rust 库负责读取、查找、识别和保存编辑结果，`web/` 提供无样式的 PDF.js 查找和编辑套件。界面、文件权限、业务数据、费用确认和记账由各 App 管理。

> 最新 tag 为 **v0.7.0**。本地放在引擎总仓库 `personal-engine-studio/` 下，是独立的 Git 仓库。进度见 [ROADMAP.md](ROADMAP.md)，交接见 [HANDOFF.md](HANDOFF.md)。

## 能做什么

- **打开**：路径或字节；损坏、要密码、不是 PDF 都有明确的错。
- **取字**：逐页取文字层，每页标明是原文、空白、没取到字（扫描页、图）还是读失败；只有几个字的页，字也交回去。页码是原文件的物理页码，另给页标签。开了 `pdfium` 时，PDFKit 在部分期刊取丢的 fi、fl 连字会补回来（`decisions/0022`）。
- **查找**：`find::find`（工具 `pdf_find`）和阅读器的查找条用同一套匹配：简繁、全角半角、连字、行尾断词都能找到，整句没有时按同页的词找。交回原文、物理页码、页标签和每行的坐标框；扫描页用可搜索层或本机 Vision 现做识别；同一份文件再查走内存缓存。
- **页图**：逐页读取时默认每页都附一张 1568px 的 JPEG（空白页、读失败的页除外），看不看由模型决定；任意页也能单独画成 JPEG、PNG 或灰度像素，范围和阅读器看到的一样（CropBox、带旋转）。
- **拆页**：把指定页复制成一份新 PDF（给 Mathpix 只传这几页用）；统计哪些页有图片。
- **本机 OCR**：扫描页在本机用 macOS Vision 识别，断网也行；简体、繁体、英文，两栏、多栏按阅读顺序拼好，另给每行的置信度和位置。`read` 里开 `Ocr::auto()` 就会自动识别没有文字层的页。
- **可搜索扫描件**：`searchable` 在选中的扫描页写入和原字对齐的不可见文字层，「预览」、PDF.js 里能搜、能选中复制。引擎再读时交回 `Content::SearchableText`（识别时排好的阅读顺序）。
- **移动、删除文字和图片，涂黑**（feature `pdfium`）：`Document::objects` 列出页面对象，`edit::apply` 移动或删除文字和图片，或永久涂黑选定区域。只改那几条指令，别处原样保留，交文件前核对（`docs/decisions/0019`）。涂黑先在原内容上删，删不干净就整页画成图再抹黑、补回框外的搜索层（`0018`）。
- **Mathpix**：公式、表格这类难页——先在本地报价（第几页、预计多少钱），App 确认后只拆出、上传这几页，结果映射回原页码，不管成败都删掉 Mathpix 那边的副本；可以分步续跑。`fake` feature 给测试和模拟模式用。
- **编辑**：一张 `edit::Plan` 表达页面重排、插入、旋转、抽页、目录、链接、文档信息、标注（高亮、下划线、删除线、便签、文本框、绘图、形状、图片、链接）和表单值。`edit::apply` 返回新 PDF 字节，保存选项还支持烧平和设置或去掉密码。在原文件的对象上改：没动到的页面内容、页标签、表单、链接原样保留。
- **新建**：`compose::images` 把照片打包成 PDF，`compose::assemble` 拼接 PDF 并附上目录、链接或文档信息。

只支持 macOS（PDFKit 经 objc2 在进程里调）；其他平台能编译，打开文档返回 `Error::Unsupported`。

## App 怎么引用

钉住已发布的 tag（`docs/decisions/0002`）。v0.7.0 对应以下配置。

```toml
[dependencies]
engine-pdf = { git = "https://github.com/elenskye/engine-pdf", tag = "v0.7.0" }
```

升级改 tag，回退改回上一个 tag。每版接口变化见 `CHANGELOG.md`。

接入 Web 编辑器的 App 从同一个引擎 tag 构建离线包。`pnpm --dir web install --frozen-lockfile` 安装构建依赖，`pnpm --dir web build` 生成模块和声明，`pnpm --dir web pack` 生成可用 `pnpm add <包路径>` 安装的包。App 提供 `pdfjs-dist` 6.5（已验 6.5.438）、`pdf_viewer.css` 及 worker 等本地资源。挂载和改动单接口见 `docs/design.md` §7.5。

## 工具包和命令行

上面每项能力都是一个给模型用的工具（`tools/*.json`，共 17 个）。`engine_pdf::tools::manifest()` 给清单，`tools::call` 按名字调用；App 怎么交文件、密码、确认和价格见 [工具包接线说明](docs/toolkit.md)。

```bash
cargo run --bin engine-pdf -- tools
cargo run --bin engine-pdf -- describe pdf_find
cargo run --bin engine-pdf -- call pdf_find '{"file":"samples/out/mixed.pdf","pages":"2","query":"quiet valley"}'
```

先用下文命令生成样本。Mathpix 在 CLI 中默认只报价，带 `--yes` 才上传选中的页。

## 开发

安装 Node.js 22.13 或更新版本及 pnpm。首次跑 macOS 全套检查时，在项目内准备 PDFium 预编译静态库。以下命令适用于 Apple Silicon。

```bash
pnpm --dir web install --frozen-lockfile
mkdir -p local/pdfium
curl -fL https://github.com/libviprs/libviprs-dep/releases/download/pdfium-8085/pdfium-mac-arm64.tgz |
  tar -xz -C local/pdfium
./scripts/check.sh
```

在其他目录准备了静态库时，将 `PDFIUM_STATIC_LIB_PATH` 指向 `libpdfium.a` 所在的绝对路径。App 开启 `pdfium` 时同样使用该环境变量，具体见 `docs/decisions/0010`；App 用 `cp -c` 把这里的 `local/pdfium/` 克隆（APFS，不多占空间）到自己不进 Git 的文件夹，在 `.cargo/config.toml` 的 `[env]` 里指过去（站主 2026-11-28 定，Pomelo 的 `scripts/pdfium.sh` 是例子）。

## 本地试用页

```bash
pnpm --dir web build
cargo run --features samples --example make_samples
cargo run --release --example studio -- samples/out/mixed.pdf
```

用浏览器打开终端打印的 `127.0.0.1` 地址（要固定端口就设 `ENGINE_PDF_STUDIO_PORT`）。也可省略 PDF 路径，再在页面中选择文件。保存后自动打开 `local/runs/v<版本>/studio/` 下的新副本（这一版的中间产物，发版后清进废纸篓，要留的另存）。

- 输入关键词后按 Enter 或点「查找」，上一个和下一个在本次结果间跳转。用 `mixed.pdf` 查「記錄清晨」和 `quiet valley`，可分别看到中文原文和本机 OCR 高亮。
- 「生成可搜索副本」用本机 Vision 识别指定物理页。用 `mixed.pdf` 试第 2 页，保存后可查找 `quiet valley`。有未保存编辑时，先保存并在重开的副本上选页。加密 PDF 需先用权限密码保存一份去密码的副本。
- 「选对象」点选、框选、拖动或删除文字和图片；「涂黑」画草稿框，保存前确认。这两项要开 `pdfium`：

```bash
PDFIUM_STATIC_LIB_PATH="$PWD/local/pdfium/pdfium-mac-arm64/lib" \
  cargo run --release --features pdfium --example studio -- samples/out/mixed.pdf
```

用 `form.pdf` 试文本框、勾选、单选和下拉，再在保存选项中设置密码。完整验收清单见 `ROADMAP.md`。

## 样本和读取验证

```bash
cargo run --features samples --example make_samples
```

生成的样本在 `samples/out/`。App 的测试可以开 `samples` feature：`Sample::generate()` 拿字节，`Sample::source_text(page)` 拿那一页的已知文字，`samples::raw_pdf` 手写边界页。样本清单见 `docs/design.md` §5.1。

```bash
cargo run --release --example probe -- samples/out/mixed.pdf --pages 1-10 --ocr --text --render samples/out/probe
```

`probe` 逐页打印页码、标签、分类、字数和耗时。`--ocr` 为缺少可用文字层的页面启用本机识别，`--text` 打印文字，`--render` 写出 1568px 的 JPEG。`--pages` 省略时读全份。

用 `probe` 真跑 Mathpix 时设置 `MATHPIX_APP_ID` 和 `MATHPIX_APP_KEY`。下面的命令只上传生成样本的第 8 页，按一页计费。

```bash
cargo run --release --example probe -- samples/out/mixed.pdf --mathpix 8 --text
```

`cargo run --features samples --example make_scan -- <PDF> <页码>` 把真实论文的一页做成仿扫描件（产物在 `local/scans/`，不进 Git），算 OCR 字错率用。付费集成测试的手动运行方式见 `HANDOFF.md`「真跑」。

规则见 `CLAUDE.md`（Codex 读 `AGENTS.md`，是同一份）。设计见 `docs/design.md`，选型见 `docs/decisions/`。
