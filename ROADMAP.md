# ROADMAP

最新 tag 为 **v0.6.0**。**v0.7.0 工具包 + `pdf_find`** 已审核、提交；engine-core v0.2.1 推上 GitHub 后改钉、打 tag（`HANDOFF.md`）。之后 v0.8 接 Pomelo、v0.9 接 Pith；Peck（还没接引擎）照工具包接。

分工见 `CLAUDE.md`「二」：Codex 实施每个 Phase，本机 Claude 审核、commit、打 tag。1.0.0 = 引擎完成 + 三个 App 都接上并通过验收；engine-core 1.0 同时冻结。

## 版本表

| 版本 | 内容 | 在哪个仓库 | 状态 |
| --- | --- | --- | --- |
| v0.1.0 | 初始化、设计稿、Phase 计划 | engine-pdf | 完成 |
| v0.2.0 | 本机读取：打开、取字、页标签、页图、拆页、分类、取消、样本 | engine-pdf | 完成（tag `v0.2.0`） |
| v0.3.0 | 识别：Vision 本机 OCR、Mathpix | engine-pdf | 完成（tag `v0.3.0`） |
| v0.4.0 | 接 Peck（brief 的首轮验收在这一版） | Peck 为主，引擎补接口 | 引擎改动已发（tag `v0.4.0`）；Peck 还没接，可以开工 |
| v0.5.0 | 页面与结构 + 预览级编辑：目录书签、页标签核对、页面管理、照片打包、拼 PDF、标注、表单、密码、PDF.js 编辑套件、试用页 | engine-pdf | 完成（tag `v0.5.2`） |
| v0.6.0 | PDFium 级：可搜索扫描件、移动与删除文字图片、涂黑 | engine-pdf | 完成（tag `v0.6.0`） |
| v0.7.0 | Agent 工具包（清单、调用入口、命令行、接线说明）+ `pdf_find`，钉 engine-core；人用的查找改走 find | engine-pdf | 完成，tag 等 engine-core v0.2.1 推上去 |
| v0.8.0 | 接 Pomelo（Pomelo 1.1.0）：换掉 Pomelo 的 PDF 代码、扫描件进知识库、检索带页图、Pub2MD、见字如面拼 PDF | Pomelo 为主，引擎补接口 | 计划已写，等批（原 v0.7） |
| v0.9.0 | 接 Pith：取字、Mathpix、页标签、编辑套件进阅读器 | Pith 为主 | 大纲（原 v0.8） |
| v1.0.0 | 完成体：冻结接口（Rust 和 `web/`），三个 App 钉 v1.0.0，全量回归 | 全部 | 大纲 |

每一版都是「新增功能」：分 Phase，只在最后一个 Phase 收尾时打 tag。App 那边每次接入是它自己的一个版本，程度到时问站主（Pomelo 已定 1.1.0）；引擎为接入补的改动跟着当期版本发。

## 已完成的版本

每版的接口变化见 `CHANGELOG.md`，选型见 `docs/decisions/`。都是 Codex 实施、本机 Claude 用真实文件审核后修正：

- **v0.2.0 本机读取**：审核时修了短文字页丢字、页图改画 CropBox、文件头前杂字节。
- **v0.3.0 识别**：OCR 阅读顺序改成递归切块（四栏杂志不串栏）；Mathpix 响应加上限、流程拆到 `mathpix/job.rs`；真跑两次验页码映射（design §6）。
- **v0.5.0 页面与结构 + 预览级编辑**（tag `v0.5.2`）：写入从 PDFKit 整份重写改成 lopdf 在原对象上写（`decisions/0017`），修好知网论文、带结构树的论文、链接、页标签、表单字典；`sys/objects.rs` 修交叉引用表和缺 `/Length` 的流。
- **v0.6.0 PDFium 级**：PDFium 改为只读，移动、删除、涂黑由 lopdf 只改那几条指令、交文件前核对（`decisions/0019`）；涂黑支持带结构树和知网论文；可搜索副本换页序、合并后识别文字跟着页走。
- **v0.7.0 工具包 + pdf_find**（`decisions/0020`、`0021`，design §9）：5 个 Phase（清单草案和字框实验 → 接 engine-core → 调用入口和命令行 → `pdf_find` → 套件查找和收尾）。审核时用 39 份真实 PDF 独立核对字框（PDFium 取框内文字），改了：扫描页带几个字的页码、水印时照样识别（和 `read` 同一门槛）；engine-core 软连字符后跟空格被拼词（发 engine-core v0.2.1）；`pdf_edit` 的改动单清单写全每种字段；`pdf_read` 越界页直接报错；`pdf_find` 的 Markdown 中文不断行、按词时说明是哪几页；`pdf_objects` 带上文字内容；工具层类型挪到 `engine_pdf::tools` 下；`tools/find.rs` 单列。

## v0.4.0 接 Peck（计划）

> 2026-11-27：建议等 v0.7 工具包发了再接，下面第 3、4 步改成照工具包接（`pdf_read`、`pdf_mathpix_*`），程度和额度不变。

站主定的（2026-11-25 ASK）：

- **Peck 这边算小幅优化**：一轮做完、不分 Phase，做完自动更新 Peck 正式版。版本号是 Peck 手上那一版（2026-11-26 是 1.2.1）之后的下一个补丁号，开工时看 Peck 的 `package.json` 定。
- **每页都给页图**（2026-11-25 站主改定，`decisions/0007`）：能看图的模型每页都给文字（或 OCR 文字）+ 页图，空白页、读失败的页除外，看不看由模型决定；去掉 Peck 原来「一次最多 6 张」的上限（一次最多 20 页，一张图约 1,500 tokens，满 20 页约 3 万）。不能看图的模型只给文字。
- **Mathpix 上传前每次弹确认框**（第几页、预计费用），不管权限档位；可选「一直允许」，只在这个任务里有效，不存进项目设置。
- **真跑额度**：DeepSeek 10 次尝试 + Mathpix $0.05，记进 Peck 的「现行授权」账本；Mathpix 密钥沿用 Pith 那份。

**开工前**：看 Peck 工作区有没有别的会话没提交的改动（有就别动，先问）；钉的引擎 tag 已 push。在 Peck 仓库照 Peck 的 `CLAUDE.md` 做（版本四处一起改、提交说明、边界），引擎要补的在本仓库做。

| 步骤 | 在哪 | 做什么 | 做完的样子 |
| --- | --- | --- | --- |
| 1 引用和对照 | Peck | `Cargo.toml` 加 `engine-pdf`（git + 当时最新的 tag；dev-dependency 开 `samples`、`fake`）。换之前先写对照测试：旧 JXA 和引擎读 `samples` 的样本，逐字一致、出图的页一致 | 对照测试过，证明换引擎不改文字（引擎这边已在 535 页真实论文上比过） |
| 2 引擎补接口 | engine-pdf | 已先做：`ReadOptions` 默认每页附 1568px JPEG（空白、失败的页除外），去掉 `Render::only_no_text()`。接入中发现的其他小问题一起修 | 引擎测试过；打 tag `v0.4.0` |
| 3 换 `read_pdf` | Peck | `harness/tools/pdf.rs`：JXA、`page_count`、`read_pages`、`has_text`、`split` 换成 `Document` + `read(Ocr::auto())`；`parse_pages` 换成 `engine_pdf::pages::parse`；`format_pages` 留在 Peck，加「本机识别的文字（可能有错字）」「空白页」「这一页没读出来」的说法，`NoText` 的 `partial` 也给模型；能看图的模型每页附图、去掉 `MAX_IMAGES = 6`，不能看图的 `render: None`。`tools/web.rs` 下载的 PDF 同样。取消：Peck 的 `watch` 桥到 `Cancel`。第一次识别要加载模型（16–33 s），第一次用到时再加载、不在启动时预热 | 扫描页断网出字；文字页和以前逐字一样；页码照旧标「第 N 页」 |
| 4 换 `ocr_pdf` | Peck | `harness/loop.rs::ocr_job`：`mathpix::quote` → `approve_as`（确认框：第几页、预计费用、只上传这几页、读完删掉）→ `mathpix::run`；拒绝就 `Denied`、不发请求；「一直允许」只记在这个任务。结果按「--- 第 N 页 ---」分页给模型（原页码）。`Usage` 不管成败都记成 `Fee::Ocr`；远端没删掉要说。删掉 `gateway/mathpix.rs`；模拟模式用 `FakeTransport::echo()` | 测试：拒绝时零请求；上传件只有选中的页；结果标原页码；失败也记费 |
| 5 验收、收尾 | 两边 | brief 首轮验收的四个场景（下表）；Peck 钉当时最新的引擎 tag；更新 Peck 正式版；两边的文档和 HANDOFF | 四个场景都过；真跑花费记账 |

**首轮验收**（brief）：

| 场景 | 自动 | 在 Peck 里看 |
| --- | --- | --- |
| 读指定页，回答回指原页码；中文、双栏查缺字和顺序 | 对照测试；`format_pages` 每段带「第 N 页」 | DeepSeek 读 `mixed` 和一篇中文双栏论文，问一个问题，回答引用的页码对 |
| 文字、扫描、空白混排；断网能识别 | `mixed` 逐页说法对 | 关 Wi-Fi 读 `mixed`，扫描页出字、空白页标空白 |
| 只选第 8 页识别：确认框、取消零请求、只传一页、结果标第 8 页 | Peck 测试用 `FakeTransport` | 确认框显示第 8 页和 $0.005；点拒绝后没有请求；点允许后结果标第 8 页 |
| 坏文件、加密文件提示明确；从安装包调用 | 引擎测试 | 正式版里读坏文件、加密文件看提示；读 PDF 时没有 `osascript` 子进程 |

## v0.8.0 接 Pomelo（计划，原 v0.7）

依据 `docs/brief-pomelo.md`（目标、去向表、要换的约 23 处调用、要注意的事都在那里）。Pomelo 的改动在 Pomelo 仓库照 Pomelo 的 `CLAUDE.md` 做（版本三处一起改、提交、正式版、数据规则）；开工时 Pomelo 的会话把下面的 Phase 抄进 Pomelo 的 `docs/roadmap.md`。

站主定的（2026-11-25 ASK）：

- **Pomelo 1.1.0，分 Phase**，只在最后一轮装正式版。排在 v0.8（brief 原写 v0.6，`decisions/0008`、`0020`）。Pomelo 照工具包接。
- **扫描件默认在本机识别，库里已有的扫描件升级后也自动补**：后台一份一份做。
- **检索命中 PDF（含扫描件）的段落就带那一页的页图**，给能看图的云端模型；「发给云端」关掉的来源、本地模型、原文件找不到或改过的只给文字；`view=look` 照旧。

brief 留给计划定的（Claude 定）：Mathpix 转 Word、LaTeX 搬进引擎（`mathpix::convert`，`decisions/0011`）；页图不缓存、不存盘（现画一页约 40 ms）；照片打包、拼 PDF 的接口是 design §7.4（v0.5 已做好）；各处怎么核对写在下表。

引擎这边要补的：`mathpix::convert`（`fake` 里也能模拟）；从旧任务接着跑的 `Submitted`（Pomelo 旧任务文件里存的是自己的 `pdf_id`，上传的是整份）。Pomelo 先钉当时最新的 tag，收尾改钉 `v0.8.0`。

| Phase（Pomelo 1.1.0） | 做什么 | 做完的样子 |
| --- | --- | --- |
| 1 换底层（行为不变） | 先用旧实现跑对照、记下结果：知识库分段逐字、原页图宽高（±1px，CropBox 和 MediaBox 不同的文件除外）、见字如面导出的页数、链接目标、目录、标题。再换：`youzi/pdf.rs`（`pages`、`render_pages`、`page_jpeg`、`image_pages`、`images_to_pdf`）、`print.rs` 的 `assemble` 和 `png_of_pdf`、`office.rs`、`assistants/importer.rs`、`assistants/seeker/files.rs`、`pub2md/source.rs`；`sha256_hex` 留在 Pomelo | 对照全过；Pomelo 里除 `print.rs` 的 WebKit 排版外不再直接调 PDFKit |
| 2 扫描件进知识库 | `files::prepare` 走 `read(Ocr::auto(), render: None)`；后台队列一次一份、一页一页，来源那一行显示「识别中 第 12/300 页」、能停、重开 App 后接着做；升级后把已有的扫描件排进队列；第一次加载识别模型（16–33 s）显示「准备识别」；识别的段落标「本机识别」，给模型时说可能有错字。分段怎么存照 Pomelo 现有的知识库，动到数据结构先写进计划给站主看 | 扫描 PDF 能搜到、引用带页码；断网能建；文字 PDF 建索引的速度不变 |
| 3 检索带页图 | 命中 PDF 段落时现画那一页（1568px JPEG）随段落给能看图的云端模型，一次最多 6 段（约多 9 千 tokens）；不给图的三种情况照上面 | 测试：三种情况都不发图；发图时页码对 |
| 4 Pub2MD | 原页图改 `read_each` 逐页画、逐页存、能取消；Mathpix 换成 `submit`（`Submitted` 存进任务文件）、`wait`、`fetch`、`delete`，Word 走 `submit_file`，转 Word、LaTeX 走 `mathpix::convert`；模拟模式换 `FakeTransport::echo()`；删掉 `pub2md/mathpix.rs`；旧任务文件升级后照样续跑 | 模拟服务跑完照片、PDF、Word 三种；中途退出再继续不重复上传（请求记录里没有第二次上传）；旧版留下的半截任务夹具能续跑 |
| 5 收尾 | 钉 `v0.8.0`；brief 的验收场景；升级测试后装正式版；两边的文档和 HANDOFF；真跑 Mathpix（含一次转换）在每版 $1 内 | 验收场景都过；打 tag `v0.8.0` |

## 之后的版本（大纲，开工前细化成 Phase 表、站主看过再做）

- **v0.9.0 接 Pith**（原 v0.8，照工具包接）：论文后台取字（`research/content.rs`）→ Mathpix（`concept/mathpix.rs`）→ 页数与本机取字 → 页标签在阅读器里端到端核对（v0.5 已对过样本）→ 编辑套件进阅读器（写进原文件还是另存、和库里 Zotero 格式的标注怎么并存，开工前问站主）→ 删 `pdf-extract`、发版。Pith 的「PDF 目录和缩略图」候选可以直接用 `outline` 和套件的 `organizer`。
- **v1.0.0 完成体**：公共接口（Rust 和 `web/`）审一遍、冻结；三个 App 钉 `v1.0.0` 回归；文档定稿；确认三边没有残留的重复 PDF 实现。

## 用户验收清单

要站主亲自看、亲自做的，不挡开发：

- [ ] v0.4.0：在 Peck 正式版里：① 读 `mixed` 指定页，回答引用的页码对 ② 关 Wi-Fi 读扫描页能出字 ③ `ocr_pdf` 第 8 页弹出批准框（第 8 页、$0.005），点拒绝后没有上传 ④ 打开坏文件、加密文件的提示看得懂。
- [x] v0.5.0 标注（2026-11-26 Claude 代验，见 HANDOFF；修了便签弹窗）。在试用页打开一份真实论文（含一份知网论文），添加高亮、下划线、删除线、便签、文本框（写中文）、绘图、形状、签名、图片和链接。删掉一个已有标注，保存后用「预览」核对位置和外观。
- [x] v0.5.0 页面和目录（2026-11-26 Claude 代验；修了试用页新目录项指向页底）。在试用页删一页、转一页、把另一份 PDF 插到中间，添加一项目录。用「预览」核对页序、旋转、目录及内部链接。
- [x] v0.5.0 表单和密码（2026-11-26：Claude 验填值、「预览」要密码、去密码和烧平；站主在 Chrome 输密码后四个值都对。Edge 新版阅读器不显示加密表单的控件，是 Edge 的限制，站主定不处理）。
- [x] v0.6.0（2026-11-27 Claude 代验）：试用页里拖动、删除、涂黑一篇真实论文，把中文扫描件转成可搜索副本；「预览」里搜得到扫描页的字、高亮落在原字上；全选复制涂黑副本，剪贴板里没有框里的字。
- [ ] v0.7.0：在终端跑 `engine-pdf tools` 和几个 `call`（读一页、找一句中文、找扫描页上的字），看说明和结果读起来顺不顺；Mathpix 不加 `--yes` 只报价。在试用页查找条里搜，高亮落在对的字上（2026-11-27 Claude 已代跑一遍，见 `HANDOFF.md`）。
- [ ] v0.8.0：Pomelo 正式版里 ① 扫描 PDF 进知识库后能搜到、引用带页码 ② 柚子引用 PDF 段落时云端模型能说出那一页图表的内容 ③ Pub2MD 原页图和以前看起来一样（知网文件边缘少一截除外）④ 见字如面导出的 PDF 链接、目录、标题都在。
- [ ] v0.9.0：Pith 里一条来自扫描论文的引用，点回去落在阅读器的同一页。
