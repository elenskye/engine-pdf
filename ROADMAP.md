# ROADMAP

现在：**v0.3.0 已发版**（本机 OCR、Mathpix）。下一步：v0.4.0 接 Peck，**计划已写，等 Peck 1.1.10 提交、站主看过再开工**。

分工见 `docs/decisions/0001`：Codex 实施每个 Phase，Claude 审核、commit、打 tag。1.0.0 = 引擎完成 + 三个 App 都接上并通过验收。

## 版本表

| 版本 | 内容 | 在哪个仓库 | 状态 |
| --- | --- | --- | --- |
| v0.1.0 | 初始化、设计稿、Phase 计划 | engine-pdf | 完成 |
| v0.2.0 | 本机读取：打开、取字、页标签、页图、拆页、分类、取消、样本 | engine-pdf | 完成（tag `v0.2.0`） |
| v0.3.0 | 识别：Vision 本机 OCR、Mathpix | engine-pdf | 完成（tag `v0.3.0`） |
| v0.4.0 | 接 Peck（brief 的首轮验收在这一版） | Peck 为主，引擎补接口 | 计划已写，等批 |
| v0.5.0 | 接 Pith | Pith 为主 | 大纲 |
| v0.6.0 | 接 Pomelo | Pomelo 为主 | 大纲 |
| v1.0.0 | 完成体：冻结接口，三个 App 钉 v1.0.0，全量回归 | 全部 | 大纲 |

App 那边每次接入是它自己的一个版本，程度到时问站主。引擎为接入补的改动跟着当期版本发（例：接 Peck 时引擎的 commit 写 `v0.4.0: …`，验收后打 `v0.4.0`，Peck 钉它）。

## v0.2.0 本机读取（完成）

5 个 Phase：骨架 → 样本生成器 → 打开与取字 → 页图与分类 → 长文档取消与收尾。Codex 实施，Claude 审核时改了：短文字页的字不再丢（`NoText.partial`、只有几个字的页算 `Text`）、页图改画 CropBox、文件头前的杂字节容忍到 1024 字节、去掉还没用到的 OCR / Mathpix 错误种类、测试按主题重排。

## v0.3.0 识别（完成）

5 个 Phase：Vision 单页识别 → 阅读顺序接进 `read` → Mathpix 客户端 → 页码映射 → 收尾。Codex 实施（含一次单页 Mathpix 真跑），Claude 审核时改了：OCR 阅读顺序改成递归切块，四栏杂志页不再串栏；仿扫描件工具从 Swift 脚本改成 `samples::scan` + `examples/make_scan`；Mathpix 响应加 1 GB 上限；`mathpix` 的流程拆到 `mathpix/job.rs`；又真跑了一次三页（空白、中文扫描、公式表格）验证映射。实测结论见 design §6。

## v0.4.0 接 Peck（计划）

站主定的（2026-11-25 ASK）：

- **Peck 这边算小幅优化**：一轮做完、不分 Phase，做完自动更新 Peck 正式版。版本号是 Peck 1.1.10（另一个会话正在做）之后的下一个补丁号，开工时看 Peck 的 `package.json` 定。
- **扫描页给模型 OCR 文字 + 页图**：能看图的模型两样都给，不能看图的只给 OCR 文字。页图沿用 Peck 现有的「一次最多 6 张」。
- **Mathpix 上传前每次弹确认框**（第几页、预计费用），不管权限档位；可选「一直允许」，只在这个任务里有效，不存进项目设置。
- **真跑额度**：DeepSeek 10 次尝试 + Mathpix $0.05，记进 Peck 的「现行授权」账本；Mathpix 密钥沿用 Pith 那份。

**开工前**：Peck 1.1.10 已提交（现在 Peck 工作区有别的会话没提交的改动，别动）；engine-pdf `v0.3.0` 已 push。在 Peck 仓库照 Peck 的 `CLAUDE.md` 做（版本四处一起改、提交说明、边界），引擎要补的在本仓库做。

| 步骤 | 在哪 | 做什么 | 做完的样子 |
| --- | --- | --- | --- |
| 1 引用和对照 | Peck | `Cargo.toml` 加 `engine-pdf`（git + tag `v0.3.0`；dev-dependency 开 `samples`、`fake`）。换之前先写对照测试：旧 JXA 和引擎读 `samples` 的样本，逐字一致、出图的页一致 | 对照测试过，证明换引擎不改文字（引擎这边已在 535 页真实论文上比过） |
| 2 引擎补接口 | engine-pdf | `Render::only_no_text()` 现在只给 `NoText` 页出图；改成「没有文字层的页」都出图（`Ocr` 和 `NoText`），名字改成 `only_without_text()`。接入中发现的其他小问题一起修 | 引擎测试过；打 tag `v0.4.0` |
| 3 换 `read_pdf` | Peck | `harness/tools/pdf.rs`：JXA、`page_count`、`read_pages`、`has_text`、`split` 换成 `Document` + `read(Ocr::auto())`；`parse_pages` 换成 `engine_pdf::pages::parse`；`format_pages` 留在 Peck，加「本机识别的文字（可能有错字）」「空白页」「这一页没读出来」的说法，`NoText` 的 `partial` 也给模型。`tools/web.rs` 下载的 PDF 同样。取消：Peck 的 `watch` 桥到 `Cancel`。第一次识别要加载模型（16–33 s），第一次用到时再加载、不在启动时预热 | 扫描页断网出字；文字页和以前逐字一样；页码照旧标「第 N 页」 |
| 4 换 `ocr_pdf` | Peck | `harness/loop.rs::ocr_job`：`mathpix::quote` → `approve_as`（确认框：第几页、预计费用、只上传这几页、读完删掉）→ `mathpix::run`；拒绝就 `Denied`、不发请求；「一直允许」只记在这个任务。结果按「--- 第 N 页 ---」分页给模型（原页码）。`Usage` 不管成败都记成 `Fee::Ocr`；远端没删掉要说。删掉 `gateway/mathpix.rs`；模拟模式用 `FakeTransport::echo()` | 测试：拒绝时零请求；上传件只有选中的页；结果标原页码；失败也记费 |
| 5 验收、收尾 | 两边 | brief 首轮验收的四个场景（下表）；Peck 钉 `v0.4.0`；更新 Peck 正式版；两边的文档和 HANDOFF | 四个场景都过；真跑花费记账 |

**首轮验收**（brief）：

| 场景 | 自动 | 在 Peck 里看 |
| --- | --- | --- |
| 读指定页，回答回指原页码；中文、双栏查缺字和顺序 | 对照测试；`format_pages` 每段带「第 N 页」 | DeepSeek 读 `mixed` 和一篇中文双栏论文，问一个问题，回答引用的页码对 |
| 文字、扫描、空白混排；断网能识别 | `mixed` 逐页说法对 | 关 Wi-Fi 读 `mixed`，扫描页出字、空白页标空白 |
| 只选第 8 页识别：确认框、取消零请求、只传一页、结果标第 8 页 | Peck 测试用 `FakeTransport` | 确认框显示第 8 页和 $0.005；点拒绝后没有请求；点允许后结果标第 8 页 |
| 坏文件、加密文件提示明确；从安装包调用 | 引擎测试 | 正式版里读坏文件、加密文件看提示；读 PDF 时没有 `osascript` 子进程 |

## 之后的版本（大纲，开工前细化成 Phase 表、站主看过再做）

- **v0.5.0 接 Pith**：论文后台取字（`research/content.rs`）→ Mathpix（`concept/mathpix.rs`）→ 页数与本机取字 → 页标签和 PDF.js 对照 → 删 `pdf-extract`、发版。
- **v0.6.0 接 Pomelo**：`youzi/pdf.rs` 换引擎 → 知识库导入（扫描件 OCR，行为变化先问站主）→ office、importer → Pub2MD 原页图和 Mathpix 分步续跑 → 发版。
- **v1.0.0 完成体**：公共接口审一遍、冻结；三个 App 钉 `v1.0.0` 回归；文档定稿；确认三边没有残留的重复 PDF 实现。

## 用户验收清单

要站主亲自看、亲自做的，不挡开发：

- [ ] 推送 `main` 和 tag `v0.3.0`（命令见 `HANDOFF.md`）。
- [ ] v0.4.0：在 Peck 正式版里：① 读 `mixed` 指定页，回答引用的页码对 ② 关 Wi-Fi 读扫描页能出字 ③ `ocr_pdf` 第 8 页弹出批准框（第 8 页、$0.005），点拒绝后没有上传 ④ 打开坏文件、加密文件的提示看得懂。
- [ ] v0.5.0：Pith 里一条来自扫描论文的引用，点回去落在阅读器的同一页。
- [ ] v0.6.0：Pomelo 的 Pub2MD 原页图和以前看起来一样；扫描 PDF 能进知识库。
