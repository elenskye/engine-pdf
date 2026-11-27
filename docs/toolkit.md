# 工具包接线说明

适用 engine-pdf v0.7.0。每个工具的参数、结果、错误、费用、确认和例子在 `tools/*.json`，总览见 [design §9](design.md#9-agent-工具包和-pdf_findv070)。页码一律是原文件的物理页码（从 1 起），页标签另给；坐标单位 pt、左下原点、含 CropBox 偏移。

## Rust

```rust
use engine_pdf::{mathpix, tools};

let kit = tools::toolkit_with_mathpix(mathpix::Options::default())
    .with_overrides(&app_overrides)?;      // 按工具名改说明、默认值、上限、确认，或关掉
let specs = kit.manifest();                // 交给模型：engine_core::overrides::to_anthropic / to_openai
let output = kit.call("pdf_read", &args, &ctx)?;
```

不注入 Mathpix 选项时直接用 `tools::manifest()`、`tools::call()`。阻塞调用放在 App 的工作线程；`Document` 在同一线程开、用、关。工具层的类型（`Ctx`、`ToolError`、`Quote`、`Usage`……）都在 `engine_pdf::tools` 下。

`Ctx` 每次调用由 App 交：

| 字段 | App 交什么 |
| --- | --- |
| `files` | 文件标识 → 已授权的字节（`Files::read`），权限检查在 App |
| `keys` | 密码、Mathpix 密钥（`Zeroizing<String>`），键名见下 |
| `cancel` | 这次调用的 `Cancel`，长任务逐页检查 |
| `approve` | 要确认时 `call` 会问它。要先弹卡片的界面用 `prepare` → 显示报价 → 用户同意后 `run` 同一个 `Prepared` |
| `prices` | 改单价时填，Mathpix 是服务 `Mathpix`、单位 `page` |
| `progress` | 本机长任务按页报，Mathpix 按服务状态报 |

结果：`Output.text` 是给模型的 Markdown，`data` 是结构化结果，`attachments` 是页图、新 PDF、识别件（存到哪由 App 定），`next` 原样作为下一次参数。失败看 `ToolError` 的 `kind`、`message`、`hint`、`retry`，花了钱的失败带 `usage`。

## 密码和密钥

| 键名 | 用途 | 命令行环境变量 |
| --- | --- | --- |
| `pdf_password:<文件标识>` | 打开密码或权限密码 | `ENGINE_KEY_PDF_PASSWORD`（主文件）、`ENGINE_KEY_PDF_SOURCE_<n>_PASSWORD`（`pdf_edit` 第 n 个附加来源） |
| `pdf_new_owner_password:<文件标识>`、`pdf_new_user_password:<文件标识>` | `pdf_edit` 设新密码 | `ENGINE_KEY_PDF_NEW_OWNER_PASSWORD`、`ENGINE_KEY_PDF_NEW_USER_PASSWORD` |
| `mathpix_app_id`、`mathpix_app_key` | Mathpix | `ENGINE_KEY_MATHPIX_APP_ID`、`ENGINE_KEY_MATHPIX_APP_KEY` |

后缀是这次调用参数里原样的文件标识。参数里只写密码操作（`password_action`）和权限，不写密码。

## 费用和确认

| 工具 | 费用 | 默认确认 |
| --- | --- | --- |
| `pdf_info`、`pdf_read`、`pdf_render`、`pdf_ocr`、`pdf_find`、`pdf_outline`、`pdf_annotations`、`pdf_fields`、`pdf_objects` | 本机 | 不问 |
| `pdf_edit`、`pdf_searchable`、`pdf_compose_images`、`pdf_assemble`、`pdf_split` | 本机，交回新文件 | 不问 |
| `pdf_redact` | 本机，交回新文件 | 每次问 |
| `pdf_mathpix_quote` | 免费，只在本机报价 | 不问 |
| `pdf_mathpix` | 每个选中物理页 $0.005 | 每次问 |

- `pdf_objects`、`pdf_redact` 要 `pdfium` feature；没开时照样列出，调用报 `pdfium_unavailable`。
- `pdf_mathpix` 只上传确认过的那几页（升序去重）、不上传加密 PDF；报价和执行之间 App 要保持同一份文件内容。

## 命令行

```sh
cargo run --features samples --example make_samples          # 先生成样本
cargo run --bin engine-pdf -- tools
cargo run --bin engine-pdf -- describe pdf_find
cargo run --bin engine-pdf -- call pdf_read '{"file":"samples/out/mixed.pdf","pages":[1]}' --out output/read
cargo run --bin engine-pdf -- call pdf_find '{"file":"samples/out/mixed.pdf","query":"quiet valley"}'
cargo run --bin engine-pdf -- call pdf_mathpix '{"file":"samples/out/mixed.pdf","pages":[8]}'   # 只报价，退出码 3
```

要确认的工具不加 `--yes` 只交回报价（退出码 3）。`--out` 指定附件目录（默认新建临时目录），`--json-only` 标准输出只交 `data`。带 PDFium 的构建照 `scripts/check.sh` 设 `PDFIUM_STATIC_LIB_PATH`。

## 三个 App 接哪些

| App | 给模型的工具 | `pdfium` | 另外 |
| --- | --- | --- | --- |
| Peck 1.4.0 | `pdf_info`、`pdf_read`、`pdf_render`、`pdf_ocr`、`pdf_find`、`pdf_outline`、`pdf_annotations`、`pdf_fields`、`pdf_mathpix_quote`、`pdf_mathpix` | 不开 | `max_images` 按模型窗口填（每 5 万 token 一张）；说明要英文就覆盖 |
| Pomelo 1.2.0 | 柚子：`pdf_info`、`pdf_read`、`pdf_render`、`pdf_find`、`pdf_outline`、`pdf_mathpix_quote`、`pdf_mathpix` | 开 | 建索引、Pub2MD 用 Rust 接口；`pomelo_file` 不再管 PDF |
| Pith（v0.9） | 阅读器查找走 `pdf_find`；排版识别用 `pdf_mathpix` | 开 | 按物理页跳、按 `rects` 高亮，空框也能跳页 |

- 改文件的工具（`pdf_edit`、`pdf_redact`、拆拼、照片打包、可搜索副本）三家都还没给模型；要给时先定新文件存哪、要不要审批。
- 计划和验收在 `ROADMAP.md`，调用点怎么换在 design §4。

## PDFium、缓存和速度

- **开 `pdfium` 多了什么**：补 PDFKit 取丢的连字（部分期刊的 fi、fl 取成 `f` 或空格）、收紧偏高的中文行框、读 PDFKit 读不出的早期 OCR 层，外加对象编辑和涂黑（`decisions/0022`）。PDFium 一个进程只能绑定一次，App 自己别再绑，否则引擎当没有 PDFium。
- **缓存**：查找按文件内容记住每页文字、字框和识别结果（约 64 MiB，进程内），同一份文件再查很快；84 页杂志第一次约 3.6 秒，第二次 0.5 秒。
- **扫描件**：没有可搜索层的扫描页第一次查找要现做识别（每页约 0.5 秒；进程里第一次用识别模型要等 16–33 秒）。扫描书常查的，先用 `pdf_searchable` 存可搜索副本；只想查原文传 `ocr:false`。

## Web 阅读器查找

- 给 `EditorViewer` 传 `search(query, signal)`，交回 `pdf_find` 的 `Output.data`（`Found<Location>`）。`signal` 取消时停掉本机查找（接到 `Cancel`）。没传 `search` 时 App 把查找控件设为不可用。
- `await reader.find(query)` 找并跳到第一处；`find(query, "next" | "previous")` 在已返回的结果里循环；`find("")` 清空。换查询、重开文档时清掉旧高亮、忽略过期结果；失败经 Promise 交给 App。
- `onFind(current, total, { pending, result })`：当前第几处（从 1 起）、总数、是否在查、本次结果。结果截断时按 `result.hits.length` 导航、按 `total` 显示总数。没有框的命中也跳到那一页。
- 样式由 App 定：`.engine-pdf-find-match` 画高亮，`[data-current="true"]` 是当前这处。覆盖层随缩放、旋转对位，不挡鼠标和选字。
