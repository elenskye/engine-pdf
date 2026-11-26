# ROADMAP

现在：**v0.5.2 已发布**（tag `v0.5.2`；v0.5.0 没单独打 tag，内容在 v0.5.1 起的各版里）。下一步是 Peck 接入或 v0.6.0。Peck（现 1.2.1，还没接引擎）随时可以按下面的计划接入，钉当时最新的 tag。后续顺序见 `decisions/0008`，v0.6 扩充 PDFium 能力，v0.7 接 Pomelo，v0.8 接 Pith。

分工见 `docs/decisions/0001`：Codex 实施每个 Phase，Claude 审核、commit、打 tag。1.0.0 = 引擎完成（含 v0.5、v0.6 的能力）+ 三个 App 都接上并通过验收。

## 版本表

| 版本 | 内容 | 在哪个仓库 | 状态 |
| --- | --- | --- | --- |
| v0.1.0 | 初始化、设计稿、Phase 计划 | engine-pdf | 完成 |
| v0.2.0 | 本机读取：打开、取字、页标签、页图、拆页、分类、取消、样本 | engine-pdf | 完成（tag `v0.2.0`） |
| v0.3.0 | 识别：Vision 本机 OCR、Mathpix | engine-pdf | 完成（tag `v0.3.0`） |
| v0.4.0 | 接 Peck（brief 的首轮验收在这一版） | Peck 为主，引擎补接口 | 引擎改动已发（tag `v0.4.0`）；Peck 还没接，可以开工 |
| v0.5.0 | 页面与结构 + 预览级编辑：目录书签、页标签核对、页面管理、照片打包、拼 PDF、标注、表单、密码、PDF.js 编辑套件、试用页 | engine-pdf | 完成（tag `v0.5.2`） |
| v0.6.0 | PDFium 级：可搜索扫描件、移动与删除文字图片、涂黑 | engine-pdf | 计划已写，等批 |
| v0.7.0 | 接 Pomelo（Pomelo 1.1.0）：换掉 Pomelo 的 PDF 代码、扫描件进知识库、检索带页图、Pub2MD、见字如面拼 PDF | Pomelo 为主，引擎补接口 | 计划已写，等批 |
| v0.8.0 | 接 Pith：取字、Mathpix、页标签、编辑套件进阅读器 | Pith 为主 | 大纲 |
| v1.0.0 | 完成体：冻结接口（Rust 和 `web/`），三个 App 钉 v1.0.0，全量回归 | 全部 | 大纲 |

每一版都是「新增功能」：分 Phase，只在最后一个 Phase 收尾时打 tag（`CLAUDE.md`「版本」）。App 那边每次接入是它自己的一个版本，程度到时问站主（Pomelo 已定 1.1.0）。引擎为接入补的改动跟着当期版本发。v0.4.0 已先发，v0.5 可以开工；Peck 接入不再挡引擎的版本。

## v0.2.0 本机读取（完成）

5 个 Phase：骨架 → 样本生成器 → 打开与取字 → 页图与分类 → 长文档取消与收尾。Codex 实施，Claude 审核时改了：短文字页的字不再丢（`NoText.partial`、只有几个字的页算 `Text`）、页图改画 CropBox、文件头前的杂字节容忍到 1024 字节、去掉还没用到的 OCR / Mathpix 错误种类、测试按主题重排。

## v0.3.0 识别（完成）

5 个 Phase：Vision 单页识别 → 阅读顺序接进 `read` → Mathpix 客户端 → 页码映射 → 收尾。Codex 实施（含一次单页 Mathpix 真跑），Claude 审核时改了：OCR 阅读顺序改成递归切块，四栏杂志页不再串栏；仿扫描件工具从 Swift 脚本改成 `samples::scan` + `examples/make_scan`；Mathpix 响应加 1 GB 上限；`mathpix` 的流程拆到 `mathpix/job.rs`；又真跑了一次三页（空白、中文扫描、公式表格）验证映射。实测结论见 design §6。

## v0.4.0 接 Peck（计划）

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

## v0.5.0 页面与结构 + 预览级编辑

目标：macOS「预览」的水准，另加目录书签、照片打包、拼 PDF。设计见 design §7，选型见 `decisions/0009`。

站主定的（2026-11-25 ASK）：

- 排在 v0.4.0 之后、接 Pomelo 之前（`decisions/0008`）。
- **编辑套件由引擎出、不带样式**：PDF.js 上的编辑逻辑在 `web/`，按钮和样子由各 App 用自己的设计系统画。
- **用引擎自带的试用页验收**；Pith 到 v0.8 接引擎时再接套件，Pomelo、Peck 先用无界面的接口。

编辑统一走 `edit::Plan` 和 `edit::apply`，交回新字节。坐标使用 PDF 坐标，填表只写字段数据。写入方案见 `decisions/0009`、`0013`–`0017`。

| Phase | 做什么 | 做完的样子 |
| --- | --- | --- |
| 1 实验与改动单 | ① **PDFKit 写盘实测**：样本和 `~/ResearchLab-test` 的真实论文，原样存一遍、每种标注各加一个存一遍，看文件大小、文字层是否逐字不变、链接和目录在不在、页图像素差、在预览和 PDF.js 里看是否一样；查加密算法、表单控件写值后别的阅读器认不认、macOS 27 高亮多出评论气泡的问题。② **页标签对照**：`scripts/labels.mjs` 用 PDF.js 6.5 读 `labels` 样本和真实论文，和引擎逐页比。③ `edit::Plan`、`Save`、`Mark` 等类型（serde）和 `tests/fixtures/plan/`。④ 新样本：`outline`（多层书签）、`form`（文本、勾选、单选、下拉）、`annotated`（每种标注各一个）、`links` | 实测结论进 design §6，不行的地方改 `decisions/0009`；页标签的差异有结论；改动单的类型 Claude 审过 |
| 2 页面与结构 | `info`、`outline` 读写；`Plan.pages`（合并、插入、删除、旋转、重排、抽页）；`compose::images`（照搬 Pomelo `images_to_pdf`，连同测试）、`compose::assemble`（照搬 Pomelo `print.rs::assemble`）；页标签修正（没有 `/PageLabels` 交 `None`） | 拼出的文件用引擎读回：页序、旋转、链接目标（页和高度、网址）、目录层级、标题都对；Pomelo 原来的测试搬过来照样过 |
| 3 标注 | 读已有标注；加、删：高亮、下划线、删除线、便签、文本框、绘图、形状、图片（签名）、链接 | 每种写进去再读回一致；页图上标注的位置有颜色；预览和 PDF.js 里看得到、删得掉 |
| 4 表单与密码 | 读字段、填值、烧平；`open_with_password`（`WrongPassword`）；存时设打开密码、权限密码和权限；去掉密码 | `form` 样本填完读回值对；加密后不给密码打不开、给了能开、权限对；测试查密码不进日志和错误信息 |
| 5 编辑套件 | `web/`：viewer、tools（含自做的下划线、删除线、形状）、capture、organizer、forms、outline；TS 严格模式，测试吃同一套 fixtures；`check.sh` 加套件的类型检查和测试；定 App 怎么引用（pnpm git 子目录，或复制 `web/dist`） | 每个工具产出的改动单和 fixtures 一致；Rust 吃同一份 fixtures 写出的文件读回对 |
| 6 试用页与收尾 | `examples/studio`；README、design、CHANGELOG（页标签行为变化写清楚）；用户验收清单 | 站主在试用页里把每样工具用一遍，存出的文件在预览里看对；`./scripts/check.sh` 过；打 tag `v0.5.0` |

要留意：保存会生成新文件（数字签名会失效）；PDF.js 编辑器的序列化格式随版本变，套件钉 6.5 这一线；`web/` 给引擎仓库加了 Node 工具链，依赖照样登记 `THIRD_PARTY.md`。

Codex 实施 6 个 Phase。Claude 审核时用 38 份真实论文实测，改了：写入从「PDFKit 整份重写再补回」改成 lopdf 在原文件的对象上写（`decisions/0017`），修好知网论文无法编辑、带结构树的论文加不上标注、原有链接被删、页标签丢失、空表单字典挡住调页；新增 `sys/objects.rs`（交叉引用表修复、补回没有 `/Length` 的流，后者修好「预览」里中文文本框乱码）；文本框不再自动换行。实测结论见 design §6。

## v0.6.0 PDFium 级（计划）

目标：可搜索扫描件、移动与删除文字图片、基础涂黑。设计见 design §8，选型见 `decisions/0010`。

站主定的（2026-11-25 ASK）：先做实验，PDFium 只补 PDFKit 做不到的（移动删除、涂黑）；可搜索扫描件比过两种做法再定。

| Phase | 做什么 | 做完的样子 |
| --- | --- | --- |
| 1 实验 | ① PDFium 怎么进来：只用现成的预编译库（先找 macOS arm64 的静态库，没有就用 dylib 随 App 带），量 App 二进制多大、链接多久，查许可证；不从源码编（磁盘和时间都多，和 Peck 的 Tauri 构建同时跑 18 GB 内存吃紧），预编译库都不行时先问站主。② 可搜索扫描件两种做法（引擎 OCR + PDFium 不可见文字层 / PDFKit `SaveTextFromOCR`）在 `mixed`、`scanned_two_column`、站主的真实扫描件上比：再读回的字和顺序、预览和 PDF.js 里选中和搜索的位置、文件大小。③ 涂黑在边界页上试：字对象跨框、图片跨框、标注在框里 | `decisions/0010` 补上结论：静态还是带 dylib、可搜索走哪条路；feature `pdfium` 搭好，`check.sh` 加一种配置 |
| 2 可搜索扫描件 | `searchable`；标记识别出来的页，引擎再读能分辨 | 扫描页在预览里能搜到、能选中复制；别的页不变；断网可做 |
| 3 移动与删除 | `doc.objects`；`Plan.objects`（移动、删除）；套件加「选对象」工具 | 移动后取字、页图对；删掉的取不到；别的页逐字不变 |
| 4 涂黑 | `Plan.redact`；字只有一部分在框里时整页退回「画图 → 涂黑 → OCR 补回框外的字」；套件加涂黑工具（先标、再确认执行） | 边界页全过硬条件（框里取不到字、页图全黑、图片里没有原像素），过不了的报错不交文件 |
| 5 收尾 | 试用页加这三样；文档、CHANGELOG | `check.sh` 开、不开 `pdfium` 都过；打 tag `v0.6.0` |

## v0.7.0 接 Pomelo（计划）

依据 `docs/brief-pomelo.md`（目标、去向表、要换的约 23 处调用、要注意的事都在那里）。Pomelo 的改动在 Pomelo 仓库照 Pomelo 的 `CLAUDE.md` 做（版本三处一起改、提交、正式版、数据规则）；开工时 Pomelo 的会话把下面的 Phase 抄进 Pomelo 的 `docs/roadmap.md`。

站主定的（2026-11-25 ASK）：

- **Pomelo 1.1.0，分 Phase**，只在最后一轮装正式版。排在 v0.7（brief 原写 v0.6，`decisions/0008`）。
- **扫描件默认在本机识别，库里已有的扫描件升级后也自动补**：后台一份一份做。
- **检索命中 PDF（含扫描件）的段落就带那一页的页图**，给能看图的云端模型；「发给云端」关掉的来源、本地模型、原文件找不到或改过的只给文字；`view=look` 照旧。

brief 留给计划定的（Claude 定）：Mathpix 转 Word、LaTeX 搬进引擎（`mathpix::convert`，`decisions/0011`）；页图不缓存、不存盘（现画一页约 40 ms）；照片打包、拼 PDF 的接口是 design §7.4（v0.5 已做好）；各处怎么核对写在下表。

引擎这边要补的：`mathpix::convert`（`fake` 里也能模拟）；从旧任务接着跑的 `Submitted`（Pomelo 旧任务文件里存的是自己的 `pdf_id`，上传的是整份）。Pomelo 先钉当时最新的 tag，收尾改钉 `v0.7.0`。

| Phase（Pomelo 1.1.0） | 做什么 | 做完的样子 |
| --- | --- | --- |
| 1 换底层（行为不变） | 先用旧实现跑对照、记下结果：知识库分段逐字、原页图宽高（±1px，CropBox 和 MediaBox 不同的文件除外）、见字如面导出的页数、链接目标、目录、标题。再换：`youzi/pdf.rs`（`pages`、`render_pages`、`page_jpeg`、`image_pages`、`images_to_pdf`）、`print.rs` 的 `assemble` 和 `png_of_pdf`、`office.rs`、`assistants/importer.rs`、`assistants/seeker/files.rs`、`pub2md/source.rs`；`sha256_hex` 留在 Pomelo | 对照全过；Pomelo 里除 `print.rs` 的 WebKit 排版外不再直接调 PDFKit |
| 2 扫描件进知识库 | `files::prepare` 走 `read(Ocr::auto(), render: None)`；后台队列一次一份、一页一页，来源那一行显示「识别中 第 12/300 页」、能停、重开 App 后接着做；升级后把已有的扫描件排进队列；第一次加载识别模型（16–33 s）显示「准备识别」；识别的段落标「本机识别」，给模型时说可能有错字。分段怎么存照 Pomelo 现有的知识库，动到数据结构先写进计划给站主看 | 扫描 PDF 能搜到、引用带页码；断网能建；文字 PDF 建索引的速度不变 |
| 3 检索带页图 | 命中 PDF 段落时现画那一页（1568px JPEG）随段落给能看图的云端模型，一次最多 6 段（约多 9 千 tokens）；不给图的三种情况照上面 | 测试：三种情况都不发图；发图时页码对 |
| 4 Pub2MD | 原页图改 `read_each` 逐页画、逐页存、能取消；Mathpix 换成 `submit`（`Submitted` 存进任务文件）、`wait`、`fetch`、`delete`，Word 走 `submit_file`，转 Word、LaTeX 走 `mathpix::convert`；模拟模式换 `FakeTransport::echo()`；删掉 `pub2md/mathpix.rs`；旧任务文件升级后照样续跑 | 模拟服务跑完照片、PDF、Word 三种；中途退出再继续不重复上传（请求记录里没有第二次上传）；旧版留下的半截任务夹具能续跑 |
| 5 收尾 | 钉 `v0.7.0`；brief 的验收场景；升级测试后装正式版；两边的文档和 HANDOFF；真跑 Mathpix（含一次转换）在每版 $1 内 | 验收场景都过；打 tag `v0.7.0` |

## 之后的版本（大纲，开工前细化成 Phase 表、站主看过再做）

- **v0.8.0 接 Pith**：论文后台取字（`research/content.rs`）→ Mathpix（`concept/mathpix.rs`）→ 页数与本机取字 → 页标签在阅读器里端到端核对（v0.5 已对过样本）→ 编辑套件进阅读器（写进原文件还是另存、和库里 Zotero 格式的标注怎么并存，开工前问站主）→ 删 `pdf-extract`、发版。Pith 的「PDF 目录和缩略图」候选可以直接用 `outline` 和套件的 `organizer`。
- **v1.0.0 完成体**：公共接口（Rust 和 `web/`）审一遍、冻结；三个 App 钉 `v1.0.0` 回归；文档定稿；确认三边没有残留的重复 PDF 实现。

## 用户验收清单

要站主亲自看、亲自做的，不挡开发：

- [ ] v0.4.0：在 Peck 正式版里：① 读 `mixed` 指定页，回答引用的页码对 ② 关 Wi-Fi 读扫描页能出字 ③ `ocr_pdf` 第 8 页弹出批准框（第 8 页、$0.005），点拒绝后没有上传 ④ 打开坏文件、加密文件的提示看得懂。
- [x] v0.5.0 标注（2026-11-26 Claude 代验，见 HANDOFF；修了便签弹窗）。在试用页打开一份真实论文（含一份知网论文），添加高亮、下划线、删除线、便签、文本框（写中文）、绘图、形状、签名、图片和链接。删掉一个已有标注，保存后用「预览」核对位置和外观。
- [x] v0.5.0 页面和目录（2026-11-26 Claude 代验；修了试用页新目录项指向页底）。在试用页删一页、转一页、把另一份 PDF 插到中间，添加一项目录。用「预览」核对页序、旋转、目录及内部链接。
- [x] v0.5.0 表单和密码（2026-11-26：Claude 验填值、「预览」要密码、去密码和烧平；站主在 Chrome 输密码后四个值都对。Edge 新版阅读器不显示加密表单的控件，是 Edge 的限制，站主定不处理）。
- [ ] v0.6.0：试用页里 ① 把一份扫描件存成可搜索的，在「预览」里搜到字 ② 删掉页上的一张图、挪动一段字 ③ 涂黑一行字，存出来后在「预览」里复制不出那行字。
- [ ] v0.7.0：Pomelo 正式版里 ① 扫描 PDF 进知识库后能搜到、引用带页码 ② 柚子引用 PDF 段落时云端模型能说出那一页图表的内容 ③ Pub2MD 原页图和以前看起来一样（知网文件边缘少一截除外）④ 见字如面导出的 PDF 链接、目录、标题都在。
- [ ] v0.8.0：Pith 里一条来自扫描论文的引用，点回去落在阅读器的同一页。
