# ROADMAP

最新 tag 为 **v0.7.0**（工具包 + `pdf_find`）。接下来三个 App 照工具包接：Peck 1.4.0（v0.4.0 那一步，Peck 1.3.0 做完后开工）、Pomelo 1.2.0（v0.8.0）、Pith（v0.9.0）。

分工见 `CLAUDE.md`「二」：Codex 实施每个 Phase，本机 Claude 审核、commit、打 tag。1.0.0 = 引擎完成 + 三个 App 都接上并通过验收；engine-core 1.0 同时冻结。

## 版本表

| 版本 | 内容 | 在哪个仓库 | 状态 |
| --- | --- | --- | --- |
| v0.1.0 | 初始化、设计稿、Phase 计划 | engine-pdf | 完成 |
| v0.2.0 | 本机读取：打开、取字、页标签、页图、拆页、分类、取消、样本 | engine-pdf | 完成（tag `v0.2.0`） |
| v0.3.0 | 识别：Vision 本机 OCR、Mathpix | engine-pdf | 完成（tag `v0.3.0`） |
| v0.4.0 | 接 Peck（Peck 1.4.0，照工具包接；brief 的首轮验收在这一版） | Peck 为主 | 引擎改动已发（tag `v0.4.0`）；Peck 1.3.0 做完后开工，钉 `v0.7.0` |
| v0.5.0 | 页面与结构 + 预览级编辑：目录书签、页标签核对、页面管理、照片打包、拼 PDF、标注、表单、密码、PDF.js 编辑套件、试用页 | engine-pdf | 完成（tag `v0.5.2`） |
| v0.6.0 | PDFium 级：可搜索扫描件、移动与删除文字图片、涂黑 | engine-pdf | 完成（tag `v0.6.0`） |
| v0.7.0 | Agent 工具包（清单、调用入口、命令行、接线说明）+ `pdf_find`，钉 engine-core；人用的查找改走 find；开 PDFium 时补连字 | engine-pdf | 完成（tag `v0.7.0`） |
| v0.8.0 | 接 Pomelo（Pomelo 1.2.0）：换掉 Pomelo 的 PDF 代码、扫描件进知识库、检索带页图、柚子用 `pdf_*` 工具、Pub2MD、见字如面拼 PDF | Pomelo 为主，引擎补 `mathpix::convert` 等 | 进行中：Pomelo Phase 1–2 做完（2026-11-28），引擎没改；Phase 5 补引擎 |
| v0.9.0 | 接 Pith：取字、Mathpix、页标签、编辑套件进阅读器 | Pith 为主 | 大纲（原 v0.8） |
| v1.0.0 | 完成体：冻结接口（Rust 和 `web/`），三个 App 钉 v1.0.0，全量回归 | 全部 | 大纲 |

每一版都是「新增功能」：分 Phase，只在最后一个 Phase 收尾时打 tag。App 那边每次接入是它自己的一个版本，程度到时问站主（Pomelo 已定 1.1.0）；引擎为接入补的改动跟着当期版本发。

## 已完成的版本

每版的接口变化见 `CHANGELOG.md`，选型见 `docs/decisions/`。都是 Codex 实施、本机 Claude 用真实文件审核后修正：

- **v0.2.0 本机读取**：审核时修了短文字页丢字、页图改画 CropBox、文件头前杂字节。
- **v0.3.0 识别**：OCR 阅读顺序改成递归切块（四栏杂志不串栏）；Mathpix 响应加上限、流程拆到 `mathpix/job.rs`；真跑两次验页码映射（design §6）。
- **v0.5.0 页面与结构 + 预览级编辑**（tag `v0.5.2`）：写入从 PDFKit 整份重写改成 lopdf 在原对象上写（`decisions/0017`），修好知网论文、带结构树的论文、链接、页标签、表单字典；`sys/objects.rs` 修交叉引用表和缺 `/Length` 的流。
- **v0.6.0 PDFium 级**：PDFium 改为只读，移动、删除、涂黑由 lopdf 只改那几条指令、交文件前核对（`decisions/0019`）；涂黑支持带结构树和知网论文；可搜索副本换页序、合并后识别文字跟着页走。
- **v0.7.0 工具包 + pdf_find**（`decisions/0020`、`0021`，design §9）：5 个 Phase（清单草案和字框实验 → 接 engine-core → 调用入口和命令行 → `pdf_find` → 套件查找和收尾）。审核时用 39 份真实 PDF 独立核对字框（PDFium 取框内文字），改了：扫描件盖了页码、水印的页照样识别；engine-core 软连字符后跟空格被拼词（发 engine-core v0.2.1）；开 PDFium 时补 PDFKit 取丢的连字、收紧偏高的中文行框，查找加内存缓存（`decisions/0022`）；`pdf_edit` 的改动单清单写全每种字段；`pdf_read` 加 `max_images`、越界页直接报错；`pdf_find` 的 Markdown 中文不断行、按词时说明是哪几页；`pdf_objects` 带上文字内容；工具层类型挪到 `engine_pdf::tools` 下。

## v0.4.0 接 Peck（Peck 1.4.0，计划）

站主定的：

- **版本**（2026-11-27 ASK）：Peck 1.3.0（评测基线）做完后的 **1.4.0**，算新增功能、分 Phase，最后一期装正式版；改前后各跑一轮 Peck 的评测。改动在 Peck 仓库照 Peck 的 `CLAUDE.md` 做，开工时 Peck 的会话把下表抄进 Peck 的 `docs/roadmap.md`。
- **工具**（2026-11-27）：引擎的只读工具 `pdf_info`、`pdf_read`、`pdf_render`、`pdf_ocr`、`pdf_find`、`pdf_outline`、`pdf_annotations`、`pdf_fields`，加 Mathpix 的 `pdf_mathpix_quote`、`pdf_mathpix`；替掉 Peck 的 `read_pdf`、`ocr_pdf`。改文件的先不给。工具名不改；Peck 的工具说明是英文，要英文就按工具名覆盖说明。不开 `pdfium`。
- **页图**（2026-11-27）：保留 Peck 1.1.12 的按窗口定张数（每 5 万 token 一张），填进 `pdf_read` 的 `max_images`；超出的页引擎会写明怎么拿到图。
- **Mathpix 每次弹确认**（2026-11-25）：第几页、预计费用；「一直允许」只在这个任务里有效。真跑额度：DeepSeek 10 次尝试 + Mathpix $0.05，记进 Peck 的账本；密钥沿用 Pith 那份。

| Phase | 做什么 | 做完的样子 |
| --- | --- | --- |
| 1 基线 | 换之前跑一轮 Peck 评测（默认配置）；用引擎 `samples` 的样本记下旧 `read_pdf` 每页的文字、哪些页出图。评测题 `peck-pdf-image-budget` 考的是旧代码，照 Peck 的规矩改题或退役 | 基线分数和对照数据在案 |
| 2 换读取 | `Cargo.toml` 钉 engine-pdf `v0.7.0`（dev 开 `samples`、`fake`）；用 `tools::toolkit_with_mathpix` 注册上面 8 个只读工具，`Ctx` 接线：`Files` 只放行项目里的文件和这次的附件（守 Peck 原来的边界）、`Progress` 进任务事件、附件里的页图转成给看图模型的图、长文字附件落进 spill 文件；`image_budget` 填 `max_images`；`web_fetch` 下载到的 PDF 改用 `read`；删 JXA、`read_pdf` 和它的测试 | 样本的每页文字和旧的一致；扫描页断网能读出字；页码照旧是物理页 |
| 3 换 Mathpix | `pdf_mathpix_quote`、`pdf_mathpix` 接 Peck 的审批卡片（`Approve`）和 TOOL KEYS（`Keys`）；拒绝时零请求；用量不管成败记成 `Fee::Ocr`；删 `gateway/mathpix.rs`，模拟模式注入 `FakeTransport::echo()` | 测试：拒绝零请求；只上传选中的页；结果标原页码；失败也记费 |
| 4 验收、收尾 | 下面四个场景；改后再跑一轮评测、和基线比；真跑；装正式版；两边的文档和 HANDOFF | 四个场景都过；评测不退步；花费记账 |

**首轮验收**（brief）：

| 场景 | 自动 | 在 Peck 里看 |
| --- | --- | --- |
| 读指定页，回答回指原页码；中文、双栏查缺字和顺序 | 对照测试 | DeepSeek 读 `mixed` 和一篇中文双栏论文，问一个问题，回答引用的页码对 |
| 文字、扫描、空白混排；断网能识别 | `mixed` 逐页说法对 | 关 Wi-Fi 读 `mixed`，扫描页出字、空白页标空白 |
| 只选第 8 页识别：确认框、拒绝零请求、只传一页、结果标第 8 页 | `FakeTransport` | 确认框显示第 8 页和 $0.005；点拒绝后没有请求；点允许后结果标第 8 页 |
| 坏文件、加密文件提示明确；从安装包调用 | 引擎测试 | 正式版里读坏文件、加密文件看提示；读 PDF 时没有 `osascript` 子进程 |

## v0.8.0 接 Pomelo（Pomelo 1.2.0，计划）

依据 `docs/brief-pomelo.md`（目标、调用点、要注意的事）。在 Pomelo 仓库照 Pomelo 的 `CLAUDE.md` 做；开工时 Pomelo 的会话把下表抄进 Pomelo 的 `docs/roadmap.md`。调用点和换法见 design §4.3。

站主定的：

- **版本**（2026-11-27 ASK）：Pomelo **1.2.0**，分 Phase，只在最后一期装正式版（原定的 1.1.0 已被别的工作用掉）。
- **开 `pdfium`**（2026-11-27）：知识库里丢连字的期刊能搜对、中文框更准；构建脚本加 `PDFIUM_STATIC_LIB_PATH`，打包、签名、`THIRD_PARTY` 通知跟着改（+7.3 MiB）。
- **柚子直接用引擎的 `pdf_*` 工具**（2026-11-27）：给只读的 `pdf_info`、`pdf_read`、`pdf_render`、`pdf_find`、`pdf_outline`；`pdf_ocr` 不给（入库时扫描件已经识别过）；改文件的先不给；`pdf_mathpix_quote`、`pdf_mathpix` 给，识别每次确认（第几页、预计费用），费用记进 Pomelo 现有的账（2026-11-28 ASK）。工具名不改，改说明、默认值、上限、确认方式用按工具名的覆盖；模型读引擎交回的 Markdown，Pomelo 要别的样子在自己这边转。以后接别的引擎照这个办法，慢慢替掉 Pomelo 的原生工具。
- **扫描件默认在本机识别，库里已有的扫描件升级后也自动补**（2026-11-25）：后台一份一份做。
- **检索命中 PDF（含扫描件）的段落带那一页的页图**（2026-11-25），给能看图的云端模型；「发给云端」关掉的来源、本地模型、原文件找不到或改过的只给文字；`view=look` 照旧。
- 页码一律用原文件的物理页码，页标签另给；存库、引用、回跳都用物理页码。

| Phase | 做什么 | 做完的样子 |
| --- | --- | --- |
| 1 换底层（行为不变） | 先用旧实现跑对照、记下结果：知识库分段逐字、原页图宽高（±1px，CropBox 和 MediaBox 不同的文件除外）、见字如面导出的页数、链接、目录、标题。钉 engine-pdf（开 `pdfium`，构建和打包跟着改），换 design §4.3 表里的调用；`sha256_hex` 留在 Pomelo | 对照全过（补过连字的页按预期不同）；Pomelo 里除 `print.rs` 的 WebKit 排版外不再直接调 PDFKit |
| 2 扫描件进知识库 | 入库时 PDF 的文字由引擎取（`read(Ocr::auto(), render: None)`：读取、扫描件识别、补连字）；后台队列一次一份、一页一页，来源那一行显示「识别中 第 12/300 页」、能停、重开 App 接着做；升级后已有的扫描件排进队列；第一次加载识别模型显示「准备识别」；识别的段落标「本机识别」。分段和存库照 Pomelo 现有的做，动到数据结构先写进计划 | 扫描 PDF 能搜到、引用带页码；断网能建；文字 PDF 建索引的速度不变 |
| 3 检索带页图 | 命中 PDF 段落时现画那一页（1568px JPEG）随段落给能看图的云端模型，一次最多 6 段（约多 9 千 tokens）；不给图的三种情况照上面。看图上限这些规则都在接线层管 | 测试：三种情况都不发图；发图时页码对 |
| 4 柚子的 PDF 工具 | 用工具包注册上面的 `pdf_*`；`Files::read` 只放行这次提问带的文件和知识库里的文件；密码、Mathpix 密钥经 `Keys`。`pomelo_file` 去掉 PDF（`outline`、`read`、`find`、`images`、`look` 遇到 PDF 都不处理），说明里写清两套各管哪些格式、「PDF 用 `pdf_*`」。`cite.rs` 加一种 PDF 位置：物理页码，另附页标签和框；回答按页引用，点回原文按物理页码跳。长材料预选（提问前先附最相关几段）可以改用引擎的 find、read 取片段，挑哪几段由 Pomelo 定；跨文件、跨知识库的检索仍归 Pomelo | 模型不会把 PDF 交给 `pomelo_file`；引用点得回原页；看图规则、权限边界和以前一样 |
| 5 Pub2MD | 原页图改 `read_each` 逐页画、逐页存、能取消；Mathpix 换成 `submit`（`Submitted` 存进任务文件）、`wait`、`fetch`、`delete`，Word 走 `submit_file`，转 Word、LaTeX 走 `mathpix::convert`，报价和确认照引擎的规矩；模拟模式换 `FakeTransport::echo()`；删掉 `pub2md/mathpix.rs`；旧任务文件升级后照样续跑 | 模拟服务跑完照片、PDF、Word 三种；中途退出再继续不重复上传；旧版留下的半截任务能续跑 |
| 6 收尾 | 钉 `v0.8.0`；下面的验收；升级测试后装正式版；两边的文档和 HANDOFF；真跑 Mathpix（含一次转换）在每版 $1 内 | 验收都过；打 tag `v0.8.0` |

**引擎这边要补的**（engine-pdf v0.8.0）：`mathpix::convert`（`fake` 里也能模拟）；从 Pomelo 旧任务接着跑的 `Submitted`（旧任务文件存的是自己的 `pdf_id`、上传的是整份）。Pomelo 先钉 `v0.7.0`，收尾改钉 `v0.8.0`。

**验收**（Pomelo 没有评测，用真实提问各过几遍）：带 PDF 附件提问，回答引用的页码对；查知识库里的扫描件，能找到、能引用；中文、繁简、跨行的句子能找到；云端模型能看页图、本地模型不给图；Mathpix 先报价、确认了才上传；见字如面导出的 PDF 链接、目录、标题都在。

**过渡期**：每接一个引擎，就从 `pomelo_file` 里拿掉那一种格式，直到它只剩 Pomelo 独有的东西或整个删掉；每拿掉一种都要确认引用、权限、看图规则跟着搬过去了。以后接 Word 补「段落和格子」的引用位置，Excel 补「单元格」。

## 之后的版本（大纲，开工前细化成 Phase 表、站主看过再做）

- **v0.9.0 接 Pith**（原 v0.8，照工具包接）：论文后台取字（`research/content.rs`）→ Mathpix（`concept/mathpix.rs`）→ 页数与本机取字 → 页标签在阅读器里端到端核对（v0.5 已对过样本）→ 编辑套件进阅读器（写进原文件还是另存、和库里 Zotero 格式的标注怎么并存，开工前问站主）→ 删 `pdf-extract`、发版。Pith 的「PDF 目录和缩略图」候选可以直接用 `outline` 和套件的 `organizer`。
- **v1.0.0 完成体**：公共接口（Rust 和 `web/`）审一遍、冻结；三个 App 钉 `v1.0.0` 回归；文档定稿；确认三边没有残留的重复 PDF 实现。

## 用户验收清单

要站主亲自看、亲自做的，不挡开发：

- [ ] v0.4.0（Peck 1.4.0）：在 Peck 正式版里：① 读 `mixed` 指定页，回答引用的页码对 ② 关 Wi-Fi 读扫描页能出字 ③ `pdf_mathpix` 第 8 页弹出确认（第 8 页、$0.005），点拒绝后没有上传 ④ 打开坏文件、加密文件的提示看得懂。
- [x] v0.5.0 标注（2026-11-26 Claude 代验，见 HANDOFF；修了便签弹窗）。在试用页打开一份真实论文（含一份知网论文），添加高亮、下划线、删除线、便签、文本框（写中文）、绘图、形状、签名、图片和链接。删掉一个已有标注，保存后用「预览」核对位置和外观。
- [x] v0.5.0 页面和目录（2026-11-26 Claude 代验；修了试用页新目录项指向页底）。在试用页删一页、转一页、把另一份 PDF 插到中间，添加一项目录。用「预览」核对页序、旋转、目录及内部链接。
- [x] v0.5.0 表单和密码（2026-11-26：Claude 验填值、「预览」要密码、去密码和烧平；站主在 Chrome 输密码后四个值都对。Edge 新版阅读器不显示加密表单的控件，是 Edge 的限制，站主定不处理）。
- [x] v0.6.0（2026-11-27 Claude 代验）：试用页里拖动、删除、涂黑一篇真实论文，把中文扫描件转成可搜索副本；「预览」里搜得到扫描页的字、高亮落在原字上；全选复制涂黑副本，剪贴板里没有框里的字。
- [ ] v0.7.0：在终端跑 `engine-pdf tools` 和几个 `call`（读一页、找一句中文、找扫描页上的字），看说明和结果读起来顺不顺；Mathpix 不加 `--yes` 只报价。在试用页查找条里搜，高亮落在对的字上（2026-11-27 Claude 已代跑一遍，见 `HANDOFF.md`）。
- [ ] v0.8.0（Pomelo 1.2.0）：Pomelo 正式版里 ① 扫描 PDF 进知识库后能搜到、引用带页码，点回原文落在那一页 ② 柚子引用 PDF 段落时云端模型能说出那一页图表的内容，本地模型不给图 ③ 带 PDF 附件提问，柚子用 `pdf_*` 工具、引用的页码对 ④ Pub2MD 原页图和以前看起来一样（知网文件边缘少一截除外）⑤ 见字如面导出的 PDF 链接、目录、标题都在。
- [ ] v0.9.0：Pith 里一条来自扫描论文的引用，点回去落在阅读器的同一页。
