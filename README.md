# engine-pdf

Peck、Pith、Pomelo 共用的 Mac PDF 引擎（Rust 库）。App 把 PDF 和要的页码交进来，拿回每页的文字、页图或识别结果；PDF 处理只维护这一份，界面、权限、业务数据、费用确认和记账各 App 自己管。

> 现在是 **v0.3.0**：本机读取、本机 OCR、Mathpix 都能用；下一步接 Peck（v0.4），之后是页面与结构、预览级编辑（v0.5），PDFium 级编辑（v0.6），再接 Pomelo（v0.7）、Pith（v0.8）。进度见 `ROADMAP.md`，交接见 `HANDOFF.md`。

## 能做什么

- **打开**：路径或字节；损坏、要密码、不是 PDF 都有明确的错。
- **取字**：逐页取文字层，每页标明是原文、空白、没取到字（扫描页、图）还是读失败；只有几个字的页，字也交回去。页码是原文件的物理页码，另给页标签。
- **页图**：逐页读取时默认每页都附一张 1568px 的 JPEG（空白页、读失败的页除外），看不看由模型决定；任意页也能单独画成 JPEG、PNG 或灰度像素，范围和阅读器看到的一样（CropBox、带旋转）。
- **拆页**：把指定页复制成一份新 PDF（给 Mathpix 只传这几页用）；统计哪些页有图片。
- **本机 OCR**：扫描页在本机用 macOS Vision 识别，断网也行；简体、繁体、英文，两栏、多栏按阅读顺序拼好，另给每行的置信度和位置。`read` 里开 `Ocr::auto()` 就会自动识别没有文字层的页。
- **Mathpix**：公式、表格这类难页——先在本地报价（第几页、预计多少钱），App 确认后只拆出、上传这几页，结果映射回原页码，不管成败都删掉 Mathpix 那边的副本；可以分步续跑。`fake` feature 给测试和模拟模式用。

只支持 macOS（PDFKit 经 objc2 在进程里调）；其他平台能编译，打开文档返回 `Error::Unsupported`。

## App 怎么引用

钉住一个 tag（`docs/decisions/0002`）：

```toml
[dependencies]
engine-pdf = { git = "https://github.com/elenskye/engine-pdf", tag = "v0.3.0" }
```

升级改 tag，回退改回上一个 tag。每版接口变化见 `CHANGELOG.md`。

## 开发

```bash
./scripts/check.sh
```

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
