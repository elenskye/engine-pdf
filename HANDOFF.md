# HANDOFF

更新于 2026-11-27。**v0.7.0 已审核、提交，还没打 tag**：它钉的 engine-core v0.2.1（本轮修的）还没推上 GitHub，开发期用 `[patch]` 指本机路径。最新已发 tag 仍是 `v0.6.0`。

## 本轮：本机 Claude 审核 Codex 的 v0.7.0

Codex 实施了 Phase 1–5（工具清单、接 engine-core、调用入口和命令行、`pdf_find`、Web 查找）。架构照 GUIDE 搭得对：清单在 `tools/*.json`，`src/tools/` 只做参数转换和 Markdown，业务逻辑留在原模块；`find` 的归一化全在 engine-core。审核改了：

- **扫描页带几个字也要识别**（`src/sys/find.rs`）：原来文字层有 1 个字就不做 OCR，扫描软件盖的页码、水印让整页找不到。改成和 `read` 同一门槛，分支理顺（design §9.2）；新测试 `recognizes_scans_that_carry_only_a_short_text_stamp`。
- **engine-core v0.2.1**：软连字符后跟空格被当成行尾断词，`all­ observations`、`between-­ and` 拼成一个词，整句、按词都找不到（Duke 出版社期刊）。在 engine-core 修了、打了 tag。
- **工具清单**：`pdf_edit` 的 `plan` 原来只写了类型，模型拼不出标注、填表、目录——补全每种字段，加一个高亮例子；两处 `id` 的说明错抄成「物理页码」；`pdf_find` 说明加上扫描件慢、先做可搜索副本。
- **工具输出**：`pdf_read` 越界页直接报 `bad_args`（原来交回一页 failed、退出码 0）；`pdf_find` 的 Markdown 中文之间不再插空格、按词时说清是哪几页；`pdf_objects` 带上文字内容（模型才知道动哪个编号）；Mathpix 执行时单价和报价同一处取（原来从金额反推）。
- **结构**：`find` 单列 `src/tools/find.rs`；`ToolError`、`Quote`、`Usage` 从根上挪到 `engine_pdf::tools`（根上的 `Quote` 和 `mathpix::Quote` 同名不同义）；`page()` 改名 `uint()`。
- **试用页**：可设 `ENGINE_PDF_STUDIO_PORT` 固定端口。
- **文档**：design §9、`docs/toolkit.md` 重写；ROADMAP 已完成的版本缩成一段；`decisions/0021` 补审核记录；CHANGELOG、README、THIRD_PARTY、CLAUDE.md 跟着改。

Codex 留给审核的几件：Web 查找的异步和取消（在试用页里实测换查询、导航都对）；旧 OCR 框回退（读过，保留）；Mathpix 上传无任务 ID 记 $0（照 design §3.6 保留，每次重试仍要确认）；工具错误的重试提示（读过，合理）。

## 验证

- `./scripts/check.sh`：通过（三种配置的测试和 Clippy、Web 类型检查和测试），日志 `local/v070-review/check-after.log`。engine-core `./scripts/check.sh` 和 Linux `cargo check` 通过。
- **命令行实跑**（release + pdfium）：17 个工具在真实论文、杂志、扫描件上各跑过：读、页图、OCR、目录、标注、表单、对象、找（中英文、跨行、按词、扫描页现做识别、可搜索副本里的识别层）、改（按 find 的框加高亮、移动对象、设密码）、涂黑（不加 `--yes` 只报确认，加了之后框里的字 PDFium 也取不到）、拆页、拼接、照片打包、Mathpix 报价（不加 `--yes` 退出码 3）、错误种类（坏文件、加密、密码错、越界、不是 PDF）。
- **字框独立核对**：39 份真实 PDF 随机抽 110 个短语，PDFium 读框内文字比对，106 处通过；其余 4 处是抽样按 PDFium 的取字顺序拼了表格、参考文献（PDFKit 原文里本来就没有这句），没有真正漏找。脚本和日志在 `local/v070-review/`（README）。
- **试用页**（内置浏览器）：繁体查简体原文、扫描页「quiet valley」标「本机识别」且高亮落在字上、按词查找、上一个/下一个循环都对。
- **没做**：Mathpix 真跑（本版没改上传逻辑）；App 接入。

## 已知问题

- **PDFKit 丢 fi 连字**：部分期刊取字时 `fi` 变成 `f` 或空格（Phoenix 2024：PDFium 403 处、PDFKit 3 处；Arpino 2022：134→5），这些词 `read`、`find` 都对不上。v0.2 起就有，PDFium 取得对。
- **个别中文字体的行框偏高**：李蓉蓉那篇行框 24.8 pt、行距 15 pt，高亮会罩到下一行（「预览」里选中也这样）。PDFKit 内收不紧，要用 PDFium 字形框。
- **扫描件查找慢**：没有可搜索层的扫描页每次查找都现做 Vision（热启动约 0.5 秒一页，进程里第一次加载模型 16–50 秒），引擎不缓存；84 页文字杂志全书查一次约 3.4 秒（主要是 PDFKit 取字）。接 Pith 阅读器时考虑在 `Document` 里缓存每页文字和识别结果。

## 等站主

1. **先推 engine-core，再告诉我**：

   ```bash
   cd ~/Documents/GitHub/personal-engine-studio/engine-core && git push origin main --tags
   ```

   推上后下一轮：`Cargo.toml` 的 engine-core 改钉 `tag = "v0.2.1"`、删掉 `[patch]` 段，`cargo update -p engine-core`，`./scripts/check.sh`，提交，打 `v0.7.0`。之后再推 engine-pdf（`git push origin main --tags`）。
2. **要不要改取字来源**（上面第一条已知问题）：开了 `pdfium` 时 `read`、`find` 改用 PDFium 取字，或检测到连字丢失时才换。会改变三个 App 已有的取字结果，所以等你定；建议放进 v0.8 前做一个小版本。

## 下一步

engine-core 推上后照「等站主」第 1 条收尾、打 `v0.7.0`。然后 v0.8 接 Pomelo（计划在 ROADMAP，等批），Peck 照工具包接（ROADMAP v0.4）。

## Mathpix 真跑

发版验收用 Pith 同一份凭据，写入会话临时目录的 `600` 权限文件，通过 `MATHPIX_APP_ID` / `MATHPIX_APP_KEY` 传入。只对样本真跑，每版上限 $1。

```bash
cargo test --features samples --test mathpix_live live_mathpix_mixed_page_8 -- --ignored --exact --nocapture
```
