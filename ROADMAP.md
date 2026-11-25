# ROADMAP

现在：**v0.3.0 已发版**（本机 OCR、Mathpix）。下一步：v0.4.0 接 Peck，Phase 计划等站主定了决策点再写。

分工见 `docs/decisions/0001`：Codex 实施每个 Phase，Claude 审核、commit、打 tag。1.0.0 = 引擎完成 + 三个 App 都接上并通过验收。

## 版本表

| 版本 | 内容 | 在哪个仓库 | 状态 |
| --- | --- | --- | --- |
| v0.1.0 | 初始化、设计稿、Phase 计划 | engine-pdf | 完成 |
| v0.2.0 | 本机读取：打开、取字、页标签、页图、拆页、分类、取消、样本 | engine-pdf | 完成（tag `v0.2.0`） |
| v0.3.0 | 识别：Vision 本机 OCR、Mathpix | engine-pdf | 完成（tag `v0.3.0`） |
| v0.4.0 | 接 Peck（brief 的首轮验收在这一版） | Peck 为主，引擎补接口 | 定计划中 |
| v0.5.0 | 接 Pith | Pith 为主 | 大纲 |
| v0.6.0 | 接 Pomelo | Pomelo 为主 | 大纲 |
| v1.0.0 | 完成体：冻结接口，三个 App 钉 v1.0.0，全量回归 | 全部 | 大纲 |

App 那边每次接入是它自己的一个版本，程度到时问站主。引擎为接入补的改动跟着当期版本发（例：接 Peck 时引擎的 commit 写 `v0.4.0: …`，验收后打 `v0.4.0`，Peck 钉它）。

## v0.2.0 本机读取（完成）

5 个 Phase：骨架 → 样本生成器 → 打开与取字 → 页图与分类 → 长文档取消与收尾。Codex 实施，Claude 审核时改了：短文字页的字不再丢（`NoText.partial`、只有几个字的页算 `Text`）、页图改画 CropBox、文件头前的杂字节容忍到 1024 字节、去掉还没用到的 OCR / Mathpix 错误种类、测试按主题重排。

## v0.3.0 识别（完成）

5 个 Phase：Vision 单页识别 → 阅读顺序接进 `read` → Mathpix 客户端 → 页码映射 → 收尾。Codex 实施（含一次单页 Mathpix 真跑），Claude 审核时改了：OCR 阅读顺序改成递归切块，四栏杂志页不再串栏；仿扫描件工具从 Swift 脚本改成 `samples::scan` + `examples/make_scan`；Mathpix 响应加 1 GB 上限；`mathpix` 的流程拆到 `mathpix/job.rs`；又真跑了一次三页（空白、中文扫描、公式表格）验证映射。实测结论见 design §6。

## 之后的版本（大纲，开工前细化成 Phase 表、站主看过再做）

- **v0.4.0 接 Peck**（大纲，计划细化中）：① 在 Peck 里给旧 JXA 录基准 ② `read_pdf`、`web.rs` 换引擎，基准逐字一致 ③ 扫描页本机 OCR、空白页、失败页的说法 ④ `ocr_pdf` 换引擎、加上传前批准框、删 `gateway/mathpix.rs` ⑤ brief 首轮验收四个场景、发版、Peck 钉 `v0.4.0`。
- **v0.5.0 接 Pith**：论文后台取字（`research/content.rs`）→ Mathpix（`concept/mathpix.rs`）→ 页数与本机取字 → 页标签和 PDF.js 对照 → 删 `pdf-extract`、发版。
- **v0.6.0 接 Pomelo**：`youzi/pdf.rs` 换引擎 → 知识库导入（扫描件 OCR，行为变化先问站主）→ office、importer → Pub2MD 原页图和 Mathpix 分步续跑 → 发版。
- **v1.0.0 完成体**：公共接口审一遍、冻结；三个 App 钉 `v1.0.0` 回归；文档定稿；确认三边没有残留的重复 PDF 实现。

## 用户验收清单

要站主亲自看、亲自做的，不挡开发：

- [ ] 推送 `main` 和 tag `v0.3.0`（命令见 `HANDOFF.md`）。
- [ ] v0.4.0：在 Peck 正式版里：① 读 `mixed` 指定页，回答引用的页码对 ② 关 Wi-Fi 读扫描页能出字 ③ `ocr_pdf` 第 8 页弹出批准框（第 8 页、$0.005），点拒绝后没有上传 ④ 打开坏文件、加密文件的提示看得懂。
- [ ] v0.5.0：Pith 里一条来自扫描论文的引用，点回去落在阅读器的同一页。
- [ ] v0.6.0：Pomelo 的 Pub2MD 原页图和以前看起来一样；扫描 PDF 能进知识库。
