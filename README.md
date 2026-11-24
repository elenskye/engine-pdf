# engine-pdf

Peck、Pith、Pomelo 共用的 Mac PDF 引擎（Rust 库）。App 把 PDF 和要的页码交进来，拿回每页的文字、页图或识别结果；PDF 处理只维护这一份，界面、权限、业务数据、费用确认和记账各 App 自己管。

> 现在是 **v0.2.0**：本机读取完成；本机 OCR 和 Mathpix 在 v0.3.0。进度见 `ROADMAP.md`，交接见 `HANDOFF.md`。

## 能做什么

- **打开**：路径或字节；损坏、要密码、不是 PDF 都有明确的错。
- **取字**：逐页取文字层，每页标明是原文、空白、没取到字（扫描页、图）还是读失败；只有几个字的页，字也交回去。页码是原文件的物理页码，另给页标签。
- **页图**：任意页画成 JPEG、PNG 或灰度像素，范围和阅读器看到的一样（CropBox、带旋转）。
- **拆页**：把指定页复制成一份新 PDF（给 Mathpix 只传这几页用）；统计哪些页有图片。
- **本机 OCR、Mathpix**：v0.3.0。

只支持 macOS（PDFKit 经 objc2 在进程里调）；其他平台能编译，打开文档返回 `Error::Unsupported`。

## App 怎么引用

钉住一个 tag（`docs/decisions/0002`）：

```toml
[dependencies]
engine-pdf = { git = "https://github.com/elenskye/engine-pdf", tag = "v0.2.0" }
```

仓库是私有的，App 根目录的 `.cargo/config.toml` 加：

```toml
[net]
git-fetch-with-cli = true
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
cargo run --release --example probe -- samples/out/mixed.pdf --pages 1-10 --text --render samples/out/probe
```

`probe` 逐页打印页码、标签、分类、字数和耗时；`--text` 打印交回的文字，`--render` 写出 1568px 的 JPEG，`--pages` 省略时读全份。人工验收用它看实际输出。

规则见 `CLAUDE.md`（Codex 读 `AGENTS.md`，是同一份）。设计见 `docs/design.md`，选型见 `docs/decisions/`。
