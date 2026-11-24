# engine-pdf

Peck、Pith、Pomelo 共用的 Mac PDF 引擎（Rust 库）。App 把 PDF 和要的页码交进来，拿回每页的文字、页图或识别结果；PDF 处理只维护这一份，界面、权限、业务数据、费用确认和记账各 App 自己管。

> 现在是 **v0.1.0**：只有设计稿，接口从 v0.2.0 起实现。进度见 `ROADMAP.md`，交接见 `HANDOFF.md`。

## 能做什么（按设计）

- **打开**：路径或字节；损坏、加密、不是 PDF 都有明确的错。
- **取字**：逐页取文字层，每页标明是原文、本机 OCR、空白、没取到字还是读失败；页码一律是原文件的物理页码，另给页标签。
- **渲染**：任意页画成 JPEG / PNG。
- **本机 OCR**：没有文字层的扫描页用 macOS Vision 在本机识别，断网也能用。
- **Mathpix**：公式、表格这类难页——先在本地报价（页码、预计费用），App 确认后只拆出、上传这几页，结果映射回原页码，不管成败都删掉 Mathpix 那边的副本。

只支持 macOS（PDFKit、Vision 经 objc2 在进程里调）；其他平台能编译，调用返回 `Unsupported`。

## App 怎么引用

钉住一个 tag（`docs/decisions/0002`）：

```toml
[dependencies]
engine-pdf = { git = "https://github.com/elenskye/engine-pdf", tag = "v0.4.0" }
```

仓库是私有的，App 根目录的 `.cargo/config.toml` 加：

```toml
[net]
git-fetch-with-cli = true
```

升级改 tag，回退改回上一个 tag。每版接口变化和升级要改什么见 `CHANGELOG.md`。

## 开发

```bash
cargo test
```

```bash
cargo clippy --all-targets -- -D warnings
```

规则见 `CLAUDE.md`（Codex 读 `AGENTS.md`，是同一份）。设计见 `docs/design.md`，选型见 `docs/decisions/`。
