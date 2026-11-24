# ROADMAP

现在：**v0.2.0 已发版**（本机读取）。下一步：v0.3.0（识别），**Phase 计划已写，等站主看过再开工**。

分工见 `docs/decisions/0001`：Codex 实施每个 Phase，Claude 审核、commit、打 tag。1.0.0 = 引擎完成 + 三个 App 都接上并通过验收。

## 版本表

| 版本 | 内容 | 在哪个仓库 | 状态 |
| --- | --- | --- | --- |
| v0.1.0 | 初始化、设计稿、Phase 计划 | engine-pdf | 完成 |
| v0.2.0 | 本机读取：打开、取字、页标签、页图、拆页、分类、取消、样本 | engine-pdf | 完成（tag `v0.2.0`） |
| v0.3.0 | 识别：Vision 本机 OCR、Mathpix | engine-pdf | 计划已写，等批 |
| v0.4.0 | 接 Peck（brief 的首轮验收在这一版） | Peck 为主，引擎补接口 | 大纲 |
| v0.5.0 | 接 Pith | Pith 为主 | 大纲 |
| v0.6.0 | 接 Pomelo | Pomelo 为主 | 大纲 |
| v1.0.0 | 完成体：冻结接口，三个 App 钉 v1.0.0，全量回归 | 全部 | 大纲 |

App 那边每次接入是它自己的一个版本，程度到时问站主。引擎为接入补的改动跟着当期版本发（例：接 Peck 时引擎的 commit 写 `v0.4.0: …`，验收后打 `v0.4.0`，Peck 钉它）。

## v0.2.0 本机读取（完成）

5 个 Phase：骨架 → 样本生成器 → 打开与取字 → 页图与分类 → 长文档取消与收尾。Codex 实施，Claude 审核时改了：短文字页的字不再丢（`NoText.partial`、只有几个字的页算 `Text`）、页图改画 CropBox、文件头前的杂字节容忍到 1024 字节、去掉还没用到的 OCR / Mathpix 错误种类、测试按主题重排。

## v0.3.0 识别（Phase 计划）

站主定的（2026-11-24 ASK）：Mathpix 真跑沿用 Pith 那份密钥，整版上限 $1；本机 OCR 用仿扫描件（真实论文页面画成图，原文字层当标准答案）加站主放进 `local/` 的真实扫描件验。Vision 接口的选择见 `decisions/0005`。

| Phase | 做什么 | 做完的样子 |
| --- | --- | --- |
| 1 Vision 单页识别 | `src/sys/ocr.rs`：`objc2-vision` 调 `VNRecognizeTextRequest`（accurate、语言纠正，默认 `zh-Hans` + `en-US`，`OcrOptions` 可改）；页面画成 2400px 灰度图交给它；每行交回文字、置信度、框（换成 CropBox 里的 PDF 坐标）。`Document::ocr(page, &OcrOptions)`、`Error::Ocr { page }`、`ocr::warm_up()`（系统第一次加载模型约 17 s，App 可以启动时在后台预热） | `mixed` 第 2、5 页和 `scanned_two_column` 识别出源文字：字错率英文 ≤ 2%、中文 ≤ 5% |
| 2 阅读顺序，接进 `read` | 按行框排序：找竖直栏缝，有就左栏读完再右栏；同栏相邻行用换行接、栏和块之间空一行。`Ocr::Auto(OcrOptions)`、`Content::Ocr(OcrText)`、`NoTextReason::NothingRecognized`；乱码页也走 OCR；`Summary` 加 `ocr` 计数。仿扫描件生成脚本放 `examples/`（产物只进 `local/`） | `mixed` 逐页 = `[Text, Ocr, Blank, Text, Ocr, NoText, Text, Ocr, Blank, Text]`；真实知网双栏页的仿扫描件栏序正确、字错率达标；站主的真实扫描件人看 |
| 3 Mathpix 客户端 | `mathpix` 模块照 design §3：`Keys`（zeroize）、`Quote`（只在本地）、`Options`、`Transport` trait + `ureq` 实现 + feature `fake` 的 `FakeTransport`；`submit` / `wait` / `fetch` / `delete` / `run`、`submit_file`；`MathpixError` 种类、`Failure { error, usage }`、`Usage` | `FakeTransport` 下：不调 `run` 零请求；上传件只有选中的那几页、像素和原页一致；成功、失败、取消、超时都发 `DELETE`；每种失败都带 `Usage` |
| 4 页码映射 | `.mmd` 按 `\pagebreak` 切页、`lines.json` 改成原页码、对不上时降级 `Approximate`；`.mmd.zip` 的 202 等待重试；`FakeTransport::echo()` 给三个 App 的模拟模式 | 录制的回应样例（多页、合并了空页、没有 `lines.json`）都映射对 |
| 5 收尾 | `probe` 加 `--ocr`、`--mathpix <页码>`（真跑，密钥走环境变量）；`#[ignore]` 的真跑测试；断网跑一遍 OCR；真跑 Mathpix（≤ $1）；Vision 耗时写进 design §2.5；`CHANGELOG.md`；Claude 审核后打 tag `v0.3.0` | 真跑 `mixed` 第 8 页：结果标第 8 页、Mathpix 那边删掉了、花费记进 HANDOFF；断网时 OCR 照常 |

## 之后的版本（大纲，开工前细化成 Phase 表、站主看过再做）

- **v0.4.0 接 Peck**：① 在 Peck 里给旧 JXA 录基准 ② `read_pdf`、`web.rs` 换引擎，基准逐字一致 ③ 扫描页本机 OCR、空白页、失败页的说法 ④ `ocr_pdf` 换引擎、加上传前批准框、删 `gateway/mathpix.rs` ⑤ brief 首轮验收四个场景、发版、Peck 钉 `v0.4.0`。
- **v0.5.0 接 Pith**：论文后台取字（`research/content.rs`）→ Mathpix（`concept/mathpix.rs`）→ 页数与本机取字 → 页标签和 PDF.js 对照 → 删 `pdf-extract`、发版。
- **v0.6.0 接 Pomelo**：`youzi/pdf.rs` 换引擎 → 知识库导入（扫描件 OCR，行为变化先问站主）→ office、importer → Pub2MD 原页图和 Mathpix 分步续跑 → 发版。
- **v1.0.0 完成体**：公共接口审一遍、冻结；三个 App 钉 `v1.0.0` 回归；文档定稿；确认三边没有残留的重复 PDF 实现。

## 用户验收清单

要站主亲自看、亲自做的，不挡开发：

- [ ] 推送 `main` 和 tag `v0.2.0`（命令见 `HANDOFF.md`）。
- [ ] 看 v0.3.0 的 Phase 计划；把几页真实扫描件放进 `engine-pdf/local/`。
- [ ] v0.3.0：抽查真 Mathpix 识别第 8 页的 LaTeX，和 Mathpix 控制台里没有留下文件。
- [ ] v0.4.0：在 Peck 正式版里：① 读 `mixed` 指定页，回答引用的页码对 ② 关 Wi-Fi 读扫描页能出字 ③ `ocr_pdf` 第 8 页弹出批准框（第 8 页、$0.005），点拒绝后没有上传 ④ 打开坏文件、加密文件的提示看得懂。
- [ ] v0.5.0：Pith 里一条来自扫描论文的引用，点回去落在阅读器的同一页。
- [ ] v0.6.0：Pomelo 的 Pub2MD 原页图和以前看起来一样；扫描 PDF 能进知识库。
