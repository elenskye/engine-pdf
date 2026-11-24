# ROADMAP

现在：**v0.1.0 已完成**（初始化 + 设计稿）。下一步：v0.2.0 Phase 1，**等站主看过 `docs/design.md` 和下面的 v0.2.0 计划再开工**。

分工见 `docs/decisions/0001`：Codex 实施每个 Phase，Claude 审核、commit、打 tag。1.0.0 = 引擎完成 + 三个 App 都接上并通过验收。

## 版本表

| 版本 | 内容 | 在哪个仓库 | 状态 |
| --- | --- | --- | --- |
| v0.1.0 | 初始化、设计稿、Phase 计划 | engine-pdf | 完成 |
| v0.2.0 | 本机读取：打开、取字、页标签、页图、拆页、分类、取消、样本 | engine-pdf | 计划已写，等批 |
| v0.3.0 | 识别：Vision 本机 OCR、Mathpix | engine-pdf | 大纲 |
| v0.4.0 | 接 Peck（brief 的首轮验收在这一版） | Peck 为主，引擎补接口 | 大纲 |
| v0.5.0 | 接 Pith | Pith 为主 | 大纲 |
| v0.6.0 | 接 Pomelo | Pomelo 为主 | 大纲 |
| v1.0.0 | 完成体：冻结接口，三个 App 钉 v1.0.0，全量回归 | 全部 | 大纲 |

App 那边每次接入是它自己的一个版本，程度到时问站主。引擎为接入补的改动跟着当期版本发（例：接 Peck 时引擎的 commit 写 `v0.4.0: …`，验收后打 `v0.4.0`，Peck 钉它）。

## v0.2.0 本机读取（Phase 计划）

| Phase | 做什么 | 做完的样子 |
| --- | --- | --- |
| 1 骨架 | `Error`（§1.2 全部种类、中文 `Display`）、`Cancel`、`pages::parse`（Peck 的搬来带测试）、`src/sys/` 模块和非 macOS 桩、依赖登记 `THIRD_PARTY.md`、`scripts/check.sh`（test + clippy） | `cargo test`、`cargo clippy` 通过；非 macOS 也能编译 |
| 2 样本 | feature `samples`：CoreText + CGPDFContext 生成 §5.1 的全部样本（扫描页 = 文字画成图再贴进 PDF；`labels` 用手写 PDF 语法带 `/PageLabels`；`locked` 用 PDFKit 写密码）；`examples/make_samples` | 每份样本能生成，页数、哪页有字和源文字对得上 |
| 3 打开与取字 | `Document::open` / `from_bytes`（`NotPdf`、`Damaged`、`Locked`）、`page_count`、`label`、单页文字层、乱码判定（Pith 的） | 中英文、两栏样本源文字逐字都在；两栏左栏全在右栏之前（不过的话记进 HANDOFF，v0.3.0 再处理）；标签对 |
| 4 页图与分类 | `Document::render`（JPEG / PNG / 灰度，带旋转，Pomelo 的）、`image_counts`（Pomelo 的）、拆页（交回字节）、空白判定、`read` / `read_each`（`Ocr::Off` 下的 `Text`、`Blank`、`NoText`、`Failed`） | `mixed` 在不开 OCR 时的逐页类型对；拆出的 PDF 页数、文字对 |
| 5 收尾 | 长文档取消（每页检查）、`examples/probe`、实测耗时写进 design §2.5、阈值校准、`CHANGELOG.md`、打 tag `v0.2.0` | 300 页生成样本能读完、能中途停；`cargo test` 全过 |

## 之后的版本（大纲，开工前细化成 Phase 表、站主看过再做）

- **v0.3.0 识别**：① Vision 单页识别（语言、精度、行框、置信度）② 接进 `read`：`Ocr::Auto`、乱码页、两栏阅读顺序 ③ Mathpix：`Quote`、`Keys`、`Transport`（ureq + Fake）、`submit / wait / fetch / delete`、`run` ④ 页码映射（`\pagebreak`、`lines.json`）和 design §3.6 的每种收尾 ⑤ 收尾：离线验收、真 Mathpix 验收（≤ $1）、发版。
- **v0.4.0 接 Peck**：① 在 Peck 里给旧 JXA 录基准 ② `read_pdf`、`web.rs` 换引擎，基准逐字一致 ③ 扫描页本机 OCR、空白页、失败页的说法 ④ `ocr_pdf` 换引擎、加上传前批准框、删 `gateway/mathpix.rs` ⑤ brief 首轮验收四个场景、发版、Peck 钉 `v0.4.0`。
- **v0.5.0 接 Pith**：论文后台取字（`research/content.rs`）→ Mathpix（`concept/mathpix.rs`）→ 页数与本机取字 → 页标签和 PDF.js 对照 → 删 `pdf-extract`、发版。
- **v0.6.0 接 Pomelo**：`youzi/pdf.rs` 换引擎 → 知识库导入（扫描件 OCR，行为变化先问站主）→ office、importer → Pub2MD 原页图和 Mathpix 分步续跑 → 发版。
- **v1.0.0 完成体**：公共接口审一遍、冻结；三个 App 钉 `v1.0.0` 回归；文档定稿；确认三边没有残留的重复 PDF 实现。

## 用户验收清单

要站主亲自看、亲自做的，不挡开发：

- [ ] 读 `docs/design.md`（重点 §1 接口、§3 Mathpix 交界），批 v0.2.0 的 Phase 计划。
- [ ] 在 GitHub 建 `elenskye/engine-pdf`（私有可以）并 push（命令见 `HANDOFF.md`）。
- [ ] v0.3.0：抽查真 Mathpix 识别第 8 页的 LaTeX，和 Mathpix 控制台里没有留下文件。
- [ ] v0.4.0：在 Peck 正式版里：① 读 `mixed` 指定页，回答引用的页码对 ② 关 Wi-Fi 读扫描页能出字 ③ `ocr_pdf` 第 8 页弹出批准框（第 8 页、$0.005），点拒绝后没有上传 ④ 打开坏文件、加密文件的提示看得懂。
- [ ] v0.5.0：Pith 里一条来自扫描论文的引用，点回去落在阅读器的同一页。
- [ ] v0.6.0：Pomelo 的 Pub2MD 原页图和以前看起来一样；扫描 PDF 能进知识库。
