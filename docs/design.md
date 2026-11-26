# engine-pdf 设计稿

> 依据 `docs/brief.md`、`docs/brief-2.md`、`docs/brief-pomelo.md`，选型见 `docs/decisions/`。§1–3 已实现（v0.2.0 本机读取，v0.3.0 本机 OCR 和 Mathpix）；§4 是各 App 的接入计划；§6 是实测结论；§7 是 v0.5.0 页面与结构、预览级编辑（已实现），§8（v0.6.0 PDFium 级）是计划。

## 0. 现状

三个 App 各写了一套，能力和做法都不一样：

| | 打开、取字 | 页图 | 本机 OCR | Mathpix |
| --- | --- | --- | --- | --- |
| **Peck** `harness/tools/pdf.rs`、`gateway/mathpix.rs` | 每次起 `osascript -l JavaScript` 跑一段 JXA 调 PDFKit，JSON 走 stdout | 没文字层的页画 JPEG（1568px，最多 6 张） | 无（扫描页只附图给模型看） | 选页 → JXA 拆页 → async reqwest 上传 → 取 `.mmd` → DELETE；**没有上传前确认**，按页记费 |
| **Pith** `research/content.rs`、`concept/mathpix.rs` | 纯 Rust 的 `pdf-extract`（不分页或按页），扫描件直接报「取文字失败」，中文常乱码 | 无（阅读器是 PDF.js，在前端） | 无 | 整份上传 → sync ureq → 取 `lines.json` + `.mmd` → DELETE；上传前查月上限、确认框，按 Mathpix 的页数记账 |
| **Pomelo** `youzi/pdf.rs`、`youzi/files.rs`、`pub2md/` | `objc2` 进程内直调 PDFKit（逐页 autorelease） | 每页 JPEG（原页图）、单页 JPEG、哪些页有图（CGPDF） | 无（扫描件在知识库里只列文件名，靠云端模型看图） | 整份上传（Word 也传）→ async reqwest → `.mmd` + `.mmd.zip`；`pdf_id` 存进任务好续跑；另有 `/v3/converter` |

结论：PDFKit 调用以 **Pomelo 的 objc2 写法**为底（已在正式版里跑），页码解析取 **Peck**，Mathpix 的「上传后失败也记账、照样删」取 **Pith**，「`pdf_id` 先存好再等」取 **Pomelo**。本机 Vision OCR 三边都没有，是新写的。

---

## 1. 库怎么用

### 1.1 总览

全部是**同步**接口：引擎不带运行时、不开线程；Peck、Pomelo 这类异步 App 用 `spawn_blocking` 包。`Document` 包着 PDFKit 的 `PDFDocument`，不跨线程（`!Send`），在哪个线程打开就在哪个线程用完。

```text
engine_pdf
├── Document          打开、页数、页标签、单页文字层、单页页图、拆页、单页 OCR
├── read / read_each  逐页读取 + 分类（文字层 / OCR / 空白 / 没取到字 / 失败）
├── pages::parse      "3"、"1-5"、"1,3-4"、"4-" → 页码
├── Cancel            取消令牌（可克隆，跨线程）
├── Error             错误种类 + 一句中文
├── ocr               Vision 预热和识别类型
├── mathpix           （v0.3.0）Quote（本地报价）→ run（上传识别）；分步 submit / wait / fetch / delete
├── edit              （v0.5.0）改动单 Plan → apply：页面管理、标注、填表、目录、文件信息、密码（§7）
├── compose           （v0.5.0）照片打包成 PDF、把几份 PDF 拼成带链接和目录的一份（§7.4）
├── objects / redact  （v0.6.0，feature "pdfium"）移动、删除页上的文字和图片，涂黑（§8）
└── samples           （feature "samples"）测试样本生成器和手写最小 PDF 的 raw_pdf，App 的测试也能用

web/                  （v0.5.0）无样式的 PDF.js 编辑套件（TS），界面的样子由 App 画（§7.5）
```

### 1.2 打开与错误

```rust
let doc = Document::open(path)?;        // 或 Document::from_bytes(&bytes)?（Pith、Pomelo 手里是字节）
doc.page_count();                        // u32
doc.label(8);                            // Option<String>：第 8 页印的页码（/PageLabels），如 "vi"；没定义时 PDFKit 给 "8"
doc.text(8)?;                            // String：PDFKit 原始文字层，扫描页为空字符串
doc.render(8, ImageFormat::Png, 1568)?;   // Image：白底，画 CropBox（阅读器显示的范围），带旋转，最长边 1568 像素
doc.ocr(8, &OcrOptions::default())?;      // OcrText：主动使用 Vision 识别，why 为 Requested
doc.image_counts(&cancel)?;              // Vec<(u32, usize)>：有图片资源的物理页和数量
doc.split(&[8, 2, 8], &cancel)?;          // Vec<u8>：按指定顺序复制页面，允许重复
```

`Error` 是枚举，App 按种类给提示；`Display` 是一句简短中文（不带路径、密钥、服务原文）：

| 种类 | 什么时候 | `Display` |
| --- | --- | --- |
| `NotPdf` | 前 1024 字节里没有 `%PDF-`（和 Acrobat、PDFKit 一样容忍前面的杂字节） | 这不是 PDF 文件。 |
| `Damaged` | PDFKit 打不开 | 这份 PDF 已损坏，打不开。 |
| `Locked` | `isLocked`（要密码） | 这份 PDF 加了密码，打不开。 |
| `PageOutOfRange { page, total }` | 页码超出 | 这份 PDF 只有 {total} 页，没有第 {page} 页。 |
| `BadPageSpec(String)` | `pages::parse` 不接受的选页 | 引擎写的固定提示（格式、范围反向、超上限），**不回显输入原文**（防止粘进来的路径、密钥进提示） |
| `Render { page }` | 单页画不出 | 第 {page} 页画不出来。 |
| `Ocr { page }` | Vision 单页识别失败 | 第 {page} 页识别失败。 |
| `Cancelled` | 取消 | 已停止。 |
| `Unsupported` | 非 macOS | 只有 Mac 上能读 PDF。 |
| `Io(io::Error)` | 读文件 | 读不了这个文件。 |

`Mathpix(MathpixError)` 的种类见 §3.6。要打开密码的 PDF 只报 `Locked`，不做解锁；只设了权限密码（禁止复制等）的照常读。截断一半的文件报 `Damaged`；只坏了尾部目录的，PDFKit 会自己修好照常读。

### 1.3 逐页读取

```rust
pub struct ReadOptions {
    pub ocr: Ocr,                   // Off | Auto(OcrOptions)：文字层不够或是乱码的页，用 Vision 在本机识别
    pub render: Option<Render>,     // 页图：默认每页都附 1568px JPEG（空白、失败的页除外）；不给模型看的场景设 None
    pub text_min_chars: usize,      // 少于这么多个非空白字符，文字层不足以代表整页（默认 20，沿用 Peck、Pith）
    pub blank_max_dark_fraction: f64,  // 64px 灰度小图里深色像素（< 240）少于这个比例算几乎全白（默认 0.2%）
    pub garbled_min_fraction: f64,  // 可疑字符（替换符、私用区、控制字符）多于这个比例算乱码（默认 20%）
}

pub fn read(doc: &Document, pages: &[u32], opts: &ReadOptions, cancel: &Cancel) -> Result<Vec<Page>, Error>;
pub fn read_each(doc: &Document, pages: &[u32], opts: &ReadOptions, cancel: &Cancel,
                 on_page: impl FnMut(Page) -> Flow) -> Result<Summary, Error>;   // 长文档用：一页交一页，不攒在内存里
```

每页交回：

```rust
pub struct Page {
    pub index: u32,               // 原文件的物理页码，从 1 起。引用、映射、存库一律用它
    pub label: Option<String>,    // 页标签（印在页上的页码），显示用
    pub content: Content,
    pub image: Option<Image>,     // 要了才有：{ format, width, height, bytes }
}

pub enum Content {
    Text(String),                 // 文字层能代表这一页（PDFKit 原样，不整理）；也包括除了几个字别无他物的页（只有页码、一句话）
    Ocr(OcrText),                 // 本机 Vision 识别的字 + why（NoTextLayer / Garbled）+ 平均置信度 + 每行文字和框
    Blank,                        // 空白页：没有文字层，页面几乎全白（不跑 OCR）
    NoText { reason, partial },   // 有画面但没取到能代表它的字：OcrOff / Garbled / NothingRecognized；
                                  // partial 是文字层里那几个字（标题、图注），不丢；乱码时为空
    Failed(Error),                // 这一页读失败；其他页照常
}
```

- `read`、`read_each` 按传入数组的顺序处理，重复页照常处理；越界页交 `Failed(PageOutOfRange)` 后接着读（`pages::parse` 出来的本来就排序去重）。
- `read_each` 回调返回 `Flow::Stop` 就停，交回 `Summary`（已读页数、各类页数、附图数，其中 `ocr` 统计识别出的页）。
- **默认每页都附页图**（`decisions/0007`）：空白页、读失败的页除外，有文字层的页也附——页上的图表、公式、表格文字层表达不了，看不看由模型决定。做索引、存库这类不给模型看的场景把 `render` 设成 `None`。`Render::jpeg / png / gray(max_side)` 换格式和尺寸，`.at_most(n)` 限张数（默认不限）。`Gray` 是 8 位像素，左上起逐行、每行 `width` 字节，0 黑 255 白，空白判定和 OCR 直接用。

整份打不开时 `Document::open` 返回 `Err`。`OcrText.lines` 的框在原 PDF 坐标系中（左下原点、单位 pt、包含 CropBox 偏移，并换回页面原旋转前的方向）。它与 PDF.js 的 `convertToPdfPoint` 使用同一坐标系，App 可以用来标示识别结果。

### 1.4 页码

- `pages::parse(spec, total, max)`：Peck 的语法和测试原样搬来（`3`、`1-5`、`1,3-4`、`4-`，中文逗号也行，排序去重，右端超出截到最后一页）。
- `Document::split` 也按传入顺序、允许重复，但任一页越界整个返回错误（要的是一份完整的 PDF）。
- `Page.index` 永远是原文件物理页码；文档有 `/PageLabels` 时，`Page.label` 来自 PDFKit 的 `PDFPage.label`，否则返回 `None`。**引用定位存 `index`，显示有标签时用 `label`**（Pith 阅读器按 `pageIndex = index - 1` 跳转）。两套读法的逐页对照见 §6。
- Mathpix 的结果也映射回 `index`（§3.4）。

### 1.5 取消与进度

`Cancel` 用 `Arc<AtomicBool>` 跨线程共享。App 在任何线程调用 `cancel.cancel()`，本机读页、拆页和图片计数在每页开始前检查，发现取消时返回 `Error::Cancelled`。PDFKit 和 Vision 正在处理的单页会完成。`read_each` 在单页 OCR 前后也检查取消，发现取消就停止交付。接入 Peck 时可以等 `watch::Receiver<bool>::wait_for` 后调用 `cancel.cancel()`。

### 1.6 Peck 调用示例

```rust
use engine_pdf::{pages, read, Cancel, Content, Document, Ocr, ReadOptions, Render};

// read_pdf（在 spawn_blocking 里）
let doc = Document::open(&file)?;                                   // Locked / Damaged → 原样告诉模型和站主
let wanted = pages::parse(str_arg(input, "pages"), doc.page_count(), MAX_PAGES)?;
let opts = ReadOptions {
    ocr: Ocr::auto(),                                               // 扫描页在本机识别，断网也行
    render: vision.then(|| Render::jpeg(1568)),                    // 能看图的模型每页都给图；不能看的不画
    ..Default::default()
};
for p in read(&doc, &wanted, &opts, &cancel)? {
    let body = match &p.content {
        Content::Text(t) => t.trim().to_string(),
        Content::Ocr(o) => format!("（这一页没有文字层，下面是本机 OCR 识别的，可能有错字）\n{}", o.text),
        Content::Blank => "（空白页）".into(),
        Content::NoText { partial, .. } => no_text_hint(partial),     // 提示可用 ocr_pdf；页图照常附在后面
        Content::Failed(e) => format!("（这一页没读出来：{e}）"),
    };
    out.push_str(&format!("\n--- 第 {} 页 ---\n{body}\n", p.index));
}

// ocr_pdf：先报价（不联网）→ Peck 的批准框 → 才上传
let quote = mathpix::quote(&doc, &wanted)?;                          // { service: "Mathpix", pages: [8], usd: 0.005 }
let ok = approve_as(ToolKind::Other, call_id, format!("送 Mathpix 识别第 {} 页？", quote.pages_text()),
                    format!("预计 ${:.3}（每页 $0.005），只上传这几页，读完删掉 Mathpix 那边的", quote.usd())).await;
if !ok.allowed() { return ReadOut::of(ToolStatus::Denied, "站主没同意上传。".into()); }   // 没发任何请求
let done = spawn_blocking(move || {
    let doc = Document::open(&file)?;
    mathpix::run(&doc, &quote, &keys, &mathpix::Options::default(), &cancel, &|_| {})
}).await?;
match done {
    Ok(r) => { /* r.pages[i].index 是原文件页码；r.usage.usd 记成 Fee::Ocr */ }
    Err(f) => { /* f.error 给模型看；f.usage 照样记（上传后失败也收费）；f.usage.remote_deleted == Some(false) 要告诉站主 */ }
}
```

---

## 2. 原生实现怎么复用

### 2.1 为什么在进程里用 objc2（`decisions/0003`）

| 做法 | 结论 |
| --- | --- |
| Peck 的 JXA（`osascript`） | 不要：每次起一个进程（几百毫秒），数据走 JSON 和临时文件，调 Vision 很别扭，取消只能杀进程 |
| Swift 小工具 + 进程间通信 | 不要：每个 App 要多打包、签名一个二进制，三份构建脚本 |
| 纯 Rust（Pith 的 `pdf-extract`、pdfium） | 不要：中文乱码、扫描件无解；pdfium 要随包带一个大动态库（v0.6.0 起 PDFium 只补 PDFKit 做不到的编辑，§8、`decisions/0010`） |
| **objc2 进程内直调 PDFKit、Vision** | **用**：Pomelo 正式版已经这样跑；引擎静态链进各 App 的二进制，「从自己的安装包调用」天然成立 |

依赖用 objc2 官方的框架绑定（`objc2-pdf-kit`、`objc2-vision`、`objc2-app-kit`、`objc2-core-graphics`、`objc2-foundation`，都是 0.3.x），绑定缺的方法再退回 Pomelo 那样的 `msg_send!`。所有 `unsafe` 收在 `src/sys/`，对外只露安全的 Rust 类型。

### 2.2 PDFKit：直接搬的代码

| 能力 | 来源 | 怎么改 |
| --- | --- | --- |
| 打开（`initWithData` / `initWithURL`、`isLocked`） | Pomelo `pdf.rs::open` | 错误换成 `Error::{Damaged, Locked}`；先看 `%PDF-` 头（Peck `resolve_pdf`）报 `NotPdf` |
| 逐页取字（`pageAtIndex` → `string`，每页一个 autorelease pool） | Pomelo `pdf.rs::pages` | 改成按页取，不一次取全份 |
| 页标签 | 新写：`PDFPage.label` | — |
| 画页（`boundsForBox` + 旋转 → 白底位图） | Pomelo `pdf.rs::jpeg_page`、`jpeg_of` | 改画 CropBox（Pomelo、Peck 画的是 MediaBox，知网等文件两者不同）；PDFKit 绘到 CoreGraphics 位图，AppKit 编码 JPEG / PNG，灰度直接交回像素 |
| 拆页（新建 `PDFDocument`，`insertPage(page.copy)`） | Peck JXA 的 `split` | 用 objc2 重写，交回 `dataRepresentation` 的字节，不落临时文件 |
| 哪些页有图（CGPDF 数 XObject） | Pomelo `pdf.rs::image_pages` | 原样搬，作 `Document::image_counts()`（Pomelo 的 `office.rs` 在用） |
| 乱码判定（替换字符、私用区、控制字符 > 20%） | Pith `content.rs::garbled` | 原样搬 |
| 页码解析 | Peck `pdf.rs::parse_pages` + 测试 | 原样搬到 `pages::parse` |
| 测试用的最小 PDF（手写 PDF 语法） | Pomelo `pdf.rs::tests::sample_pdf` | 作 `samples::raw_pdf`，可加 Catalog 条目（`/PageLabels`）和页字典条目（`/Rotate`、`/CropBox`） |

v0.5.0 再搬：Pomelo 的 `images_to_pdf`、`print.rs::assemble`（§7.4）。不搬的：`print.rs` 的 WebKit 排版（Pomelo 自己的流程）、`sha256_hex`（不是 PDF 处理）。

### 2.3 Vision：本机 OCR（新写）

- `VNRecognizeTextRequest` 用准确模式和语言纠正。`OcrOptions` 默认 `["zh-Hans", "zh-Hant", "en-US"]`，调用方可指定语言。macOS 13 及以上为多语言请求开启自动检测，让 Vision 区分中文和拉丁文字。`VNImageRequestHandler(cgImage:)` 在本机同步执行。
- 输入：页面用 CoreGraphics 画成白底灰度 `CGImage`，最长边 2400px（Pomelo 给模型看图用的也是 2400）。
- 输出：每个 `VNRecognizedTextObservation` 取 `topCandidates(1)`，得到文字、置信度、框；框从 Vision 的归一化坐标换成 PDF 坐标。
- **阅读顺序**（`src/ocr/order.rs`）：Vision 不保证栏的顺序（真实知网双栏页、杂志四栏页实测都会左右行交错）。引擎递归切块：一块里先找竖直栏缝（两边各有几行正文、上下并排；跨过栏缝的只能是上面的通栏标题或下面的页脚），有就按「通栏标题、左、右、页脚」分开再各自递归；没有就在比一行字高还宽的横向空白处上下分开；都没有就是一块正文，从上到下、同一行从左到右（表格的一行格子接在一起）。两栏、四栏、中间插图的杂志页都排对了。
- 不用 macOS 26 只有 Swift 能调的 `RecognizeDocumentsRequest`：实测它会把左右栏的行并进同一段（`decisions/0005`）。
- 系统第一次加载识别模型要 16–33 s，App 可以启动时在后台调 `ocr::warm_up()`。之后每页 0.1–0.6 s。

### 2.4 一页怎么分类

```text
取文字层 ─ 正常字符 ≥ text_min_chars 且不是乱码 ─────────────→ Text
   │否
   ├─ 画 64px 灰度小图，深色像素 < 0.2% ─ 有正常的字 ───────→ Text（只有页码、一句话的页）
   │                                └ 没有 ──────────→ Blank
   ├─ ocr = Off ──────────────────→ NoText { OcrOff 或 Garbled, partial }
   │
   └─ Vision 识别 ─ 认出字 ─→ Ocr(why = NoTextLayer 或 Garbled)
                           └ 没认出 ─→ NoText { NothingRecognized, partial }
任何一步出错 ──────────────────────→ Failed(Error)，接着读下一页
```

三个阈值（20 字、0.2%、20%）在生成的边界页上校准过：0.195% 判空白、0.22% 不判；20% 可疑字符仍算文字、21% 算乱码；19 字不够、20 字够。

### 2.5 长文档与耗时

- 页图随回调逐页交出，`read_each` 不攒结果；`read` 只给页数少的场景（Peck 一次最多 20 页）。每页一个 autorelease pool。
- 本机读取实测（M3 Pro、Release）：29 篇真实论文共 535 页，取字加分类每份 30–220 ms（每页约 2–10 ms）。真实论文画 1568px JPEG 约 40 ms/页（35 页 1.4 s）。生成的 300 页样本取字加分类共约 35 ms，中途取消在下一页前停下。
- Vision（M3 Pro）：首次加载模型 16–33 s；之后生成样本每页 0.1–0.3 s、真实扫描页 0.4–0.6 s。用 `sandbox-exec` 禁网跑过，照常识别。
- 引擎不并行：一次一页。App 想同时处理几份文件自己开几个阻塞任务；资源约束下不建议同时跑两份 OCR。

---

## 3. 云端调用的交界（Mathpix，`decisions/0004`）

### 3.1 两步走：报价在本地，确认在 App

```text
App: mathpix::quote(&doc, &pages)        ← 纯本地，只数页、算钱，绝不联网
App: 显示「Mathpix · 原文件第 8 页 · 预计 $0.005」，查自己的预算上限
     ├─ 站主取消 → App 不调 run，流程结束（没有任何请求）
     └─ 站主确认 → mathpix::run(&doc, &quote, &keys, &opts, &cancel, &progress)
```

`Quote` 的 `service()`、`pages()`、`price_per_page()` 和 `usd()` 用于确认框。页码升序去重，单价默认 $0.005。`quote_with_options` 使用 App 提供的 `Options::price_per_page`。确认后继续使用同一份报价，待上传页和记账单价均取自报价。

`run` 只上传 `quote.pages`，不会多传。确认框里显示什么、怎么措辞、预算怎么查，全是 App 的事。

### 3.2 密钥、记账

- **密钥**：App 从自己的钥匙串取出来，构造 `Keys::new(app_id, app_key)`（`Zeroizing`）交进来。可序列化的续跑任务不含密钥。
- **记账**：引擎不知道预算，只如实交回 `Usage`，成功、失败都有：

```rust
pub struct Usage {
    pub pdf_id: Option<String>,        // 上传成功才有
    pub pages_billed: u32,             // Mathpix 报的 num_pages；没报或中途失败就按送去的页数
    pub usd: f64,
    pub estimated: bool,               // 不是 Mathpix 报的数就为 true
    pub remote_deleted: Option<bool>,  // 上传过才有：Mathpix 那边的原件和结果删没删掉
}
```

App 照旧记进自己的账本：Peck 记成 `Fee::Ocr`，Pith 交给 `meter.record`，Pomelo 写进任务。

### 3.3 拆页、上传、下载

1. **拆页**：`quote.pages` 是全份时直接传原件字节；否则用 PDFKit 拆出只有这几页的新 PDF（内存里，不落盘），核对页数等于 `quote.pages.len()`。
2. **上传**：`POST /v3/pdf`，multipart（`options_json` + `file`），文件名固定 `pages.pdf`（不带本机路径）。选项固定带 `include_page_breaks: true` 和 `metadata.improve_mathpix: false`。`Options` 可选行数据、裁图压缩包和公式分隔符。额外字段允许透传，选页、分页标记、数据留存和请求方式由引擎控制。
3. **等**：每 2 秒 `GET /v3/pdf/{id}`，读 `status`、`percent_done`、`num_pages`；最长等 `Options.max_wait`（默认 10 分钟）。注意 Mathpix 的错误常常是 HTTP 200 带 `error_info`，每次都看 body（Peck 的教训）。
4. **下载**：`.mmd` 必取。按提交时设置下载 `.lines.json`、`.mmd.zip`。行数据缺失时按可用结果映射。ZIP 返回 202 时按 `Retry-After` 或轮询间隔重试，等待可取消、有上限。见 [Mathpix PDF 接口文档](https://docs.mathpix.com/reference/post-v3-pdf)。
5. **删除**：不管成败，只要拿到过 `pdf_id` 就 `DELETE /v3/pdf/{id}`，结果写进 `usage.remote_deleted`。

`/v3/converter`（Pomelo 把 mmd 转 docx 等）v0.7.0 搬进引擎，作 `mathpix::convert`（`decisions/0011`）。

### 3.4 原页码映射

上传件的第 k 页 = `quote.pages[k-1]`。交回：

```rust
pub struct Recognized {
    pub pages: Vec<RecognizedPage>,   // 每页：index（原文件页码）、mmd（这一页的 Mathpix Markdown）、lines（这一页的 lines.json，page 字段已改成原页码）
    pub mmd: String,                  // 整份原文（含 \pagebreak），给要自己切块的 Pomelo
    pub mmd_zip: Option<Vec<u8>>,
    pub mapping: Mapping,             // Exact | Approximate
    pub usage: Usage,
}
```

- `mmd` 按 `\pagebreak` 切页；切出来的段数等于上传页数 → `Exact`，逐段配原页码。
- 段数对不上（Mathpix 偶尔合并空页）→ 有 `lines.json` 就按它的 `page` 字段重排出每页 mmd，仍是 `Exact`；没有就整份挂在第一页、标 `Approximate`，App 自己决定怎么说。
- `lines.json` 的 `page` 字段逐个换成原页码，Pith 现有的 `concept_engine::mathpix::from_lines` 不用改就能吃（`Recognized::lines_json()` 拼回整份）。

### 3.5 分步接口（Pomelo 续跑）

Pomelo 的规矩是「每次上传都收费，`pdf_id` 一拿到就存进任务，继续时只等和下载」。所以 `run` 拆成四步也公开：

```rust
let mut sub: Submitted = mathpix::submit(&doc, &quote, &keys, &opts, &cancel)?;
// App 立刻把 sub 存进自己的任务文件
mathpix::wait(&mut sub, &keys, &opts, &cancel, &progress)?;
// App 保存已确认页数的 sub，之后可以继续下载
let r = mathpix::fetch(&sub, &keys, &opts, &cancel)?;
mathpix::delete(&sub, &keys, &opts);                                              // -> bool
```

Pomelo 上传 Word 用 `mathpix::submit_file(bytes, mime, estimated_pages, &keys, &opts, &cancel)`。结果保留整份 MMD 和可选 ZIP，原始页码为空并标为 `Approximate`。App 可提供预计页数。页数未知且服务没有返回时，`pages_billed` 为 0、`estimated` 为 `true`。

### 3.6 取消、失败怎么收尾

| 发生在 | 引擎做什么 | 返回 | 收费 |
| --- | --- | --- | --- |
| 确认前取消 | 什么也没发生（App 没调 `run`） | — | 无 |
| `run` 开始、上传前 `cancel` 已置 | 不发请求 | `Cancelled` | 无 |
| 上传没成（断网、401/403 密钥错、402 余额、413 太大、429 限流、5xx） | 不重试（每次上传都可能收费） | `Mathpix(Network / Auth / Quota / TooLarge / RateLimited / Service)` | `usage.pdf_id = None`，$0 |
| 上传成了，之后取消 / 超时 / 转换失败 / 下载失败 | 尽量 `DELETE` | 对应错误 + `Usage` | 按送去的页数记，`estimated = true`（宁多勿少，沿用 Pith） |
| 成功 | 下载 → `DELETE` | `Recognized` | 按 `num_pages`，`estimated = false` |
| `DELETE` 没成 | 不算失败 | `remote_deleted = Some(false)` | — |

失败统一是 `Failure { error, usage }`，App 不用猜收没收费。`MathpixError` 带种类和一句固定中文；服务原文只放 `detail`（截短、去掉密钥），给日志用，不直接给站主看（Pomelo 的规矩）。

### 3.7 HTTP 与测试替身

- 引擎自带同步 HTTP，使用 `ureq` 3 和系统 TLS，走 `Transport` trait。构建时关闭 `log` 的 TRACE，避免原始 HTTP 字节进入日志。影响范围见 [决策 0006](decisions/0006-mathpix-trace-logging.md)。App 有特殊网络需求可以自己实现 `Transport`。
- App 用 `Options::transport` 注入自定义传输。`fake` feature 提供 `FakeTransport`，可按脚本回应并记录请求，或用 `FakeTransport::echo()` 回显上传 PDF 的文字层。回显模式本地生成 MMD、行数据和 ZIP，Word 使用模拟文字，模拟用量为零。
- 生产请求固定使用 `https://api.mathpix.com`。`Options::base` 另接受测试用 `http://127.0.0.1`。续跑沿用任务的地址、输出设置和账单基准。分步接口的清理由 App 负责。

---

## 4. 三个 App 怎么迁移

共通：每个 App 的接入是它自己的一个版本（程度届时问站主），在它自己的仓库里改、守它自己的规矩；`Cargo.toml` 加 `engine-pdf = { git = "https://github.com/elenskye/engine-pdf", tag = "v0.4.0" }`（`decisions/0002`）。**回退**两层：引擎有问题 → App 把 tag 改回上一版、发一个小版本；接入本身有问题 → 装回 App 的上一版正式版，或 `git revert` 接入那几笔 commit。不在 App 里留新旧两套的运行时开关（多一套就多一份要维护的重复实现）。

### 4.1 Peck（第一个接）

| 入口 | 现在 | 换成 |
| --- | --- | --- |
| `harness/tools/pdf.rs` 的 `pdfkit()`（JXA）、`page_count`、`read_pages`、`has_text`、`split` | osascript 子进程 | `Document`、`read`；`split` 删掉（引擎的 Mathpix 自己拆） |
| `parse_pages` | Peck 自己的 | `engine_pdf::pages::parse`（同一段代码搬过去的） |
| `format_pages` | 给模型的文字 | **留在 Peck**，加 `Ocr`、`Blank`、`Failed` 的说法 |
| `tools/web.rs:270`（下载到的 PDF） | `read_pages` | `read` |
| `harness/loop.rs::ocr_job` | 拆页 → `gateway::mathpix` 上传，**不问** | `quote` → `approve_as` 批准框（第几页、预计费用）→ `mathpix::run`；拒绝就 `Denied` |
| `gateway/mathpix.rs` | Peck 的 Mathpix 客户端 | 删掉；`Mathpix::Mock` 换成 `FakeTransport::echo()` |

**先改 `read_pdf` 的取字**：旧的 JXA 和引擎读的都是 PDFKit 的 `page.string`，换完文字应该**逐字一致**，差异就是桥的 bug，基准最干净。核对办法：换之前在 Peck 里加一个基准测试，用 `samples` 生成的样本跑旧 JXA，记下每页文字、哪些页出了图、图的宽高；换成引擎后同一个测试必须一致（文字逐字、图宽高 ±1px）。这一步过了，再分开加新行为（扫描页本机 OCR、空白页的说法、`ocr_pdf` 的批准框），这样每一步的变化都看得清。

### 4.2 Pith

| 入口 | 现在 | 换成 |
| --- | --- | --- |
| `research/content.rs::pdf_text`（论文后台取字） | `pdf-extract` 整份取；扫描件、乱码报「取文字失败」 | `Document::from_bytes` + `read(Ocr::auto())`；按页拼回正文，`garbled` 判定搬进引擎。扫描件改成有 OCR 文字（报告里怎么标「OCR」由 Pith 定） |
| `concept/mathpix.rs::Mathpix::convert` | Pith 自己的 ureq 客户端，整份上传 | `mathpix::quote` + `run`（整份、`lines: true`）；`meter.check` 用 `quote.usd`，`meter.record` 用 `Usage`；`concept_engine::mathpix::from_lines` 不动 |
| `concept/mathpix.rs::local_pages`、`concept/commands.rs` 里数页数 | `pdf-extract` | `Document::page_count` + `read(Ocr::Off)`；加密、损坏有明确的错 |
| `src/reader/controller.ts`（PDF.js 阅读器） | — | **不动**（brief 定的） |

页码：引擎的 `index` = PDF.js 的 `pageIndex + 1`，`label` 应等于 PDF.js 的页标签；Pith 存 `index`、显示 `label`，引用就能跳回阅读器的那一页。核对：带罗马数字前言的样本，引擎和 PDF.js 逐页比标签。旧行为核对：拿 Pith 现有的夹具（`research/fixtures.rs` 的 `survey_pdf`、`scanned_pdf`）和样本，比 `pdf-extract` 和引擎的结果——这里**预期会不一样**（换了底层），比的是「没有缺字」「两栏顺序对」，不是逐字相等。替换掉的：`pdf-extract` 依赖（`THIRD_PARTY.md` 删一行）和 Pith 的 Mathpix HTTP 代码。已经存进库的正文不自动重取。

### 4.3 Pomelo（v0.7.0，Phase 计划见 `ROADMAP.md`）

| 入口 | 现在 | 换成 |
| --- | --- | --- |
| `youzi/pdf.rs` 的 `open`、`pages`、`render_pages`、`page_jpeg`、`jpeg_page`、`jpeg_of`、`image_pages` | Pomelo 自己的 objc2 | 引擎（这些就是引擎的原型） |
| `youzi/files.rs::prepare`、`pictures_only`（知识库导入） | 没文字层的 PDF 只列文件名 | `read(Ocr::auto(), render: None)`：扫描件在本机就能进知识库；默认开，已有的扫描件升级后在后台自动补（站主定） |
| `youzi/office.rs`（`embedded_images`、`images_text`、`look_at`）、`assistants/importer.rs` | `pdf::pages`、`image_pages`、`page_jpeg` | `Document::image_counts`、`read`、`Document::render` |
| `pub2md/job.rs` 的原页图（`render_pages`） | 整份画完存文件夹 | `read_each` 逐页画、逐页存（能取消） |
| `pub2md/mathpix.rs` 的 submit / status / 下载 / converter | Pomelo 自己的 reqwest 客户端 | `mathpix::submit`（`Submitted` 存进任务）/ `wait` / `fetch(mmd_zip)` / `delete`；Word 走 `submit_file`；转 Word、LaTeX 走 `mathpix::convert`；整份删掉 |
| `pdf::images_to_pdf`（Pub2MD 的照片） | Pomelo 自己的 objc2 | `compose::images` |
| `print.rs::assemble`、`png_of_pdf`（见字如面导出） | Pomelo 自己的 objc2 | `compose::assemble`；`Document::render(Png)` |
| `knowledge.rs` 检索命中 PDF 段落 | 只给文字；`view=look` 现画 | 命中就现画那一页给能看图的云端模型（站主定） |
| 留在 Pomelo | — | `sha256_hex`、`print.rs` 的 WebKit 排版、`mmd.rs` 的切块、Pub2MD 其余 |

核对：文字 PDF 的知识库分段应逐字一致（同是 PDFKit）；原页图比宽高、比像素平均差（阈值用样本定）；Pub2MD 用 `FakeTransport` 跑一遍完整任务，`pdf_id` 存进任务、中途退出再继续不重复上传。

---

## 5. 怎么验收

### 5.1 样本

生成器通过 `samples` feature 用 CoreText + CGPDFContext 现场生成 PDF，供引擎和三个 App 的测试使用。`cargo run --features samples --example make_samples` 将样本写入 `samples/out/`。PDF 产物留在本机。源文字随生成器提供，可自动比对缺字和顺序。

| 样本 | 内容 | 验什么 |
| --- | --- | --- |
| `text_en` / `text_zh` | 3 页英文；3 页简体中文（中英数字标点混排） | 取字、缺字 |
| `two_column_en` / `two_column_zh` | 两栏，每段开头有编号 | 文字层的阅读顺序 |
| `mixed` | 10 页：1 文字、2 英文扫描（文字画成图）、3 空白、4 文字、5 中文扫描、6 无字的图、7 文字、**8 公式和表格（画成图）**、9 空白、10 文字 | 页类型、断网 OCR、Mathpix 只传第 8 页 |
| `scanned_two_column` | 两栏中文扫描 | OCR 的阅读顺序 |
| `labels` | 前 3 页标 i–iii，之后 1 起 | 页标签、引用定位 |
| `outline` | 多层目录，含页内和网址目标 | 目录层级、目标页和高度 |
| `form` | 文本、勾选、单选、下拉 | 控件值和阅读器兼容性 |
| `annotated` | 标注及其外观 | 存盘和画面比较 |
| `links` | 页内链接和网址链接 | 重排后跳到正确的页和高度，网址保持一致 |
| `locked` / `damaged` / `not_pdf` | 用户密码；截断的 `text_en`；一段纯文本 | 明确的错误 |
| 300 页 | `tests/read.rs` 里把 `mixed` 拆页重复 30 次 | 连续交付与中途取消 |
| 边界页 | `samples::raw_pdf` 手写：旋转、CropBox、只有页码或一句标题的页 | 页图范围、短文字不丢 |

真实世界的文件（站主自己的扫描件、论文）只放 `local/` 做人工抽查。`samples::scan(&doc, page)` 把任意一页画成只有图的仿扫描 PDF；`cargo run --features samples --example make_scan -- <PDF> <页码>` 用它把真实论文的一页做成仿扫描件，原文字层另存成标准答案（都在 `local/scans/`），算 OCR 字错率。

### 5.2 场景

| brief 的场景 | 自动验证（`cargo test`，Mac 上） | 要看实际输出 |
| --- | --- | --- |
| Peck 读指定页，取到文字或页图，回答回指原页码；中文、双栏查缺字和顺序 | 引擎：样本源文字逐字都在、两栏左栏全部在右栏之前、`index`/`label` 对；Peck：基准测试新旧一致，`format_pages` 每段带「第 N 页」 | Peck 正式版里问一个样本的问题，回答引用的页码对；站主的真实中文 PDF 抽查 |
| 文字、扫描、空白混排；断网能识别；分得清原文 / OCR / 空白 | `mixed` 逐页类型 = `[Text, Ocr, Blank, Text, Ocr, NoText, Text, Ocr, Blank, Text]`；OCR 字错率：英文 ≤ 2%、中文 ≤ 5%（门槛用样本校准）；这条路径不构造 `Transport` | 关掉 Wi-Fi 在 Peck 里读 `mixed`；双栏扫描的顺序看一眼 |
| 只选第 8 页识别公式表格：确认框显示第 8 页和费用；取消零请求；上传件只有这一页；结果标第 8 页 | `FakeTransport`：`quote.pages == [8]`、`usd == 0.005`；不调 `run` 时请求数 = 0；调了只有 1 个上传、上传件 1 页且画出来和原第 8 页像素一致；结果 `index == 8`；成功、失败、取消、超时都发了 `DELETE` | 真跑一次 Mathpix（`#[ignore]` 测试，$0.005）：LaTeX 质量、真实回应的映射、Mathpix 那边删掉了；Peck 里看批准框、点拒绝后日志里没有请求 |
| 损坏、要密码时明确提示；App 从自己的安装包调用引擎 | `damaged` → `Damaged`，`locked` → `Locked`，`not_pdf` → `NotPdf` | 三个 App 里各看一次提示；正式版里读 PDF 时没有 `osascript` 子进程（Peck），`otool -L` 看到 App 二进制链着 PDFKit、Vision，`cargo tree -i engine-pdf` 看到钉的 tag |

每个版本发版前用 `probe`（`--text` 看文字、`--render` 出页图）在样本和 `~/ResearchLab-test/` 的真实论文上过一遍，人看输出。

---

## 6. 实测结论

**本机读取（v0.2.0）**

- 和 Peck 现在的读法逐字一致：29 篇真实论文（中英文、单双栏）535 页，引擎的文字层和 Peck 现有 JXA 读出来的完全相同——换 Peck 时文字不会变。
- 中文知网论文、英文双栏论文的文字层顺序正常；PDFKit 原样交出连字（`ﬀ`）和全角数字，整理归 App。

**本机 OCR（v0.3.0）**

- 站主给的真实扫描图（《经济学人》两栏、四栏页，繁体中文长文）：栏序全对；繁体整段几乎无错（一页约 3 处，如「〇」认成「•」）。
- 仿扫描件按整页（含页眉、脚注、全半角差异）算字错率：英文约 3%，简体中文 9–11%。Vision 常漏「的」（一页 57 个漏 14 个左右），换分辨率、关语言纠正、关自动检测都一样，是 Vision 本身的限制；要逐字准确的页交给 Mathpix。
- 简体页不会被认成繁体，繁体页保持繁体，默认语言（简、繁、英）不用改。

**Mathpix（v0.3.0）**

- 真跑 `mixed` 第 3（空白）、5（中文扫描）、8（公式和表格）页：三页都精确映射回原页码，空白页交回空内容，公式是正确的 LaTeX，表格完整；远端已删；共 $0.015。另有一次单页（第 8 页）真跑，$0.005。

**页标签（v0.5.0 Phase 1–2）**

- `scripts/labels.mjs` 用 PDF.js 6.5.438 逐页对照 `labels` 和 28 份真实论文，合计 538 页。修正前，14 份未定义 `/PageLabels` 的文档有 236 页差异，PDFKit 返回物理页码，PDF.js 返回空；显式标签逐页一致。引擎查 Catalog 后，538 页全部一致。

**写入（v0.5.0，macOS 27.2.1）**

- PDFKit 整份重写（Phase 1–6 的做法）会丢掉 `/PageLabels`、`/AcroForm`、结构树和内容流里的 `/ActualText`，要逐项补回，补回后仍有论文过不了文字层核对。Claude 审核时改成 lopdf 在原文件的对象上写，PDFKit 只画新标注的外观（`decisions/0017`）。
- 新写法在 38 份真实论文（28 份 Zotero 副本 + 10 份 `local/pdf`，含 5 份知网论文、4 份带结构树的期刊论文、84 页杂志）上逐份跑：整份倒序、只转一页、倒序前三页加复制一页再删末页、第 1 页加全套 13 种标注再删掉、读回的标注原样重建、设打开密码和权限再去掉、和另一份论文合并。共 1,778 项检查通过。
  - 页序不变和单一来源调页序时，逐页文字层不变，页标签、目录、全部已有标注和链接（含 arXiv 原本找不到目标的具名链接）保留。
  - 编辑耗时 4–50 ms，原做法最慢 7.9 s，合并要 15 s。
  - 没过的 6 项都是「有结构树的论文和没有结构树的合并」。这时整份不带结构树，带结构树那份的页按版面取字（和「预览」一样）。两份都有结构树时合并后取字不变。
- 知网论文的交叉引用表写成 `xref 0 133`，lopdf 读不了，引擎扫描对象重建。PDFKit 写的 CFF 字体流没有 `/Length`，lopdf 会读成空流，引擎补回；否则「预览」里中文文本框画成乱码，PDF.js 正常。
- 用压缩对象流的文件，lopdf 写回时展开，大小最多约 1.6 倍（4,702 个对象的论文）。其余在 0.9–1.1 倍之间。
- 测试的加密参数在该系统上写成 Standard RC4 128 位（R3）。

**标注、表单与密码（v0.5.0 Phase 3–4）**

- 自由文本和图片图章的标准外观由 PDFKit 画出。PDF.js 6.5.438 能显示文本透明度、边框颜色及透明 PNG。删除后，标注区域恢复底图，便签弹窗一并清除。
- 实际引擎输出的普通表单与非零裁切原点、旋转 90° 的表单，经 PDFKit 和 PDF.js 6.5.438 读回，文字、勾选、单选、下拉及中文外观正确。烧平采用矢量层，文字层和导航链接读回保持。
- 加密由 lopdf 写、PDFKit 验证；PDFKit 自己加密的文件在 Edge 里不显示表单控件（v0.5.2，`decisions/0015`）。普通读者的权限以加密字典的原始位值为准。错密码、读者权限、去密码和换密码用生成样本验证。详见 `decisions/0015`。

真实 App 的确认框、断网和安装包在各 App 接入时验收。

---

## 7. 页面与结构、预览级编辑（v0.5.0）

目标是 macOS「预览」的水准：高亮、批注、绘图、文本框、图片签名；合并、插入、删除、旋转、重排；填表、设密码；另加读写目录书签、照片打包、拼 PDF。实现顺序见 `ROADMAP.md`，写入选型见 `decisions/0009`。

### 7.1 读

```rust
doc.info();                     // Info { title, author, subject, keywords, creator, producer, created, modified, encrypted, permissions }
doc.outline();                  // Vec<OutlineItem { title, target, children }>；target = Page { page, y } | Url | None
doc.annotations(8)?;            // Vec<Annotation>，包含可恢复的 MarkKind
doc.fields(&cancel)?;            // Vec<Field>，一个逻辑字段可带多个控件
Document::open_with_password(path, &password)?;   // 也可用 from_bytes_with_password
```

- **页标签**：Catalog 没定义 `/PageLabels` 时返回 `None`，与 PDF.js 6.5 的读法一致。PDFKit 的页标签属性为只读。逐页对照见 §6，接口行为变化在 v0.5.0 发版时写进 CHANGELOG。[Apple PDFPage.label](https://developer.apple.com/documentation/pdfkit/pdfpage/label)
- 目录的 `target.page` 是物理页码（从 1 起），`y` 是 PDF 坐标；指向别的文件、跑脚本的动作不读（PDF 是数据）。
- `InfoPatch` 可写标题、作者、主题、关键词和 Creator。`Info` 另读 Producer、创建与修改日期（Unix 秒）、加密状态及权限。改文件信息时写入修改日期，并去掉旧的 XMP 元数据。

### 7.2 改动单

```rust
pub struct Plan {
    pub pages: Option<Vec<Pick>>,          // 新文件的页序；None = 页不变
    pub add: Vec<Mark>,                    // 新标注，挂在 (source, page) 上：页挪到哪，标注跟到哪
    pub remove: Vec<MarkId>,               // 删掉已有的标注
    pub fill: Vec<Fill>,                   // 填表：字段名 → 值
    pub outline: Option<Vec<OutlineItem>>, // 整棵换掉；页码指新文件的页
    pub info: Option<InfoPatch>,
}
pub struct Pick { pub source: u32, pub page: u32, pub rotate: i32 }   // source 0 = 本文件，1.. = others[source-1]
pub struct Save { pub lock: Option<Lock>, pub flatten: bool, pub remove_password: bool }
pub fn apply(doc: &Document, others: &[&Document], plan: &Plan, save: &Save, cancel: &Cancel) -> Result<Vec<u8>, Error>;
```

- **页面管理都是 `pages`**：合并 = 两个来源的页接起来；插入 = 在中间放进别的来源的页；删除 = 不列；重排 = 换顺序；旋转 = `rotate`（90 的倍数，叠在原旋转上）；抽页 = 只列几页。重复列同一页也行。
- 各参与来源的原目录及页内链接随页序重映射。目标页重复时指向第一份，失去目标的页内链接删除，目录标题保留为分组。明确传入的目录替换原有目录。新增链接挂在原 `(source, page)`，内部目标与新目录都采用输出物理页码。
- `Plan` 的 JSON 使用 camelCase 和 `kind` 标签，见 `tests/fixtures/plan/`。`Save` 由 Rust 调用方提供，密码以 `Zeroizing` 持有。
- **标注种类**（PDFKit 都有对应的类型）：

  | 种类 | 存什么 |
  | --- | --- |
  | 高亮、下划线、删除线 | 每行一个四边形（quads）、颜色 |
  | 便签 | 位置、文字（弹出框） |
  | 文本框 | 框、文字、字号、颜色、边框 |
  | 绘图 | 若干笔画（点列）、粗细、颜色 |
  | 形状 | 矩形、椭圆、直线、箭头 |
  | 图片（签名、印章） | 框、PNG 或 JPEG 字节 |
  | 链接 | 框、去哪一页的哪个高度，或网址 |

  每个标注可带作者和批注文字。
- **坐标**一律是 PDF 坐标：pt、原点在页面左下（含 CropBox 偏移），和 OCR 的框、PDF.js 的 `convertToPdfPoint` 相同。从左上量的调用方（Pomelo 的 WebKit 排版）用 `Rect::from_top(page_height, x, y, w, h)` 换。
- **表单**：文本、勾选、单选、下拉。`Fill.name` 是主文件字段的全限定名称。页序和旋转不变时表单原样留在文件里，调整页面时先烧平（`/Fields` 为空的不算表单）。计算脚本保持未执行，XFA 报 `Error::UnsupportedForm`。见 `decisions/0014`。
- **密码**：`Lock` 设打开密码（可选）、权限密码及打印、复制、修改、加标注权限。修改加密来源先用权限密码打开，并显式给出新 `lock` 或 `remove_password`。空改动返回原字节。见 `decisions/0015`。
- **原文件不动**：`apply` 只交回字节。重写后数字签名会失效，App 的确认框里怎么说由 App 定。
- 错误照 §1.2 的规矩加种类（如 `WrongPassword`、找不到字段、找不到标注、页序越界），`Display` 一句中文。

### 7.3 一次写入的顺序

写法见 `decisions/0017`，代码在 `src/sys/write.rs`。

1. 没有改动时返回原字节。
2. 加密来源先用权限密码导出明文，再写表单值。调页序且要烧平时，先把控件画进来源页面。
3. lopdf 读入用到的来源，删掉 `remove` 指定的标注和它的弹窗。
4. 页序不变（可以带旋转）时在原文件的对象上改。否则按 `pages` 把页对象搬进新文件，链接、目录、页标签、结构树照 `decisions/0017` 处理。
5. 新标注由 PDFKit 在同样几何的空白页上画好后搬到输出页。链接直接写字典，内部目标用输出物理页码。
6. 写目录和文件信息，存盘。按 `Save` 烧平，最后加密。

处理在内存进行，每页检查 `Cancel`。v0.6.0 的对象编辑和涂黑由 PDFium 先改好对应页面（§8）。

### 7.4 新建：照片打包、拼 PDF

```rust
compose::images(&[ImageSource], &Cancel) -> Result<Vec<u8>, Error>;                    // ImageSource = Path | Bytes
compose::assemble(parts: &[&[u8]], how: &Assembly, &Cancel) -> Result<Vec<u8>, Error>; // Assembly { links, outline, info }
```

- `images`：照搬 Pomelo `pdf::images_to_pdf`（NSImage 读 macOS 认的所有格式，含 HEIC，按 EXIF 转正；多页 TIFF 一页一张），连同它的测试。读不了的图报错时说第几张，不带路径。
- `assemble`：照搬 Pomelo `print.rs::assemble`：把几份 PDF 依次拼成一份，加页内链接（去某页某高度）、网址链接（无边框）、多层目录、标题和生成者。内部就是 `apply`（`pages` 接起各来源，`add` 放链接，`outline`、`info`）。
- Pith 以后的 PDF 编辑器（整理页面、编辑目录）用同一套 `Plan.pages`、`Plan.outline`。

### 7.5 PDF.js 编辑套件（`web/`）

- TS 包，严格模式；`pdfjs-dist` 6.5.x 由 App 提供（peer 依赖，Pith 现在是 6.5.438）。不带样式、不依赖 React 等框架：交出状态和事件，App 用自己的设计系统画工具栏、缩略图栏、对话框。PDF.js 自己的 `pdf_viewer.css` 由 App 引入。
- 模块：

  | 模块 | 做什么 |
  | --- | --- |
  | `viewer` | `EditorViewer` 接收容器、PDF.js 本地资源路径和界面回调，提供页标签、缩放、查找、原生编辑和表单控件 |
  | `tools` | `EditorTools` 统一切换工具。文本框、绘图、图片、签名、批注和高亮接 PDF.js。下划线、删除线、便签、形状和链接由套件捕获 PDF 坐标并显示覆盖层 |
  | `capture` | 把编辑器里的内容换成 `Plan.add`、`Plan.remove` |
  | `organizer` | 缩略图（PDF.js 画）、拖动排序、删除、旋转、从别的文件插入 → `Plan.pages` |
  | `forms` | PDF.js 表单层的值 → `Plan.fill` |
  | `outline` | 目录树的增、删、改名、挪位置 → `Plan.outline` |

- **存**：App 调 `EditorViewer.capture(fields)`，合并 `EditorTools.add/remove`、`PageOrganizer.plan()` 和 `OutlineEditor.plan()`。改动单交给 Rust 的 `edit::apply`，新字节交回套件重开。自绘标注接入 PDF.js 的撤销和重做队列。文件保存由 App 负责。
- **已有标注**：App 提供页面的 Rust `Document::annotations` 结果，套件将 PDF.js 标注 ID 映射到原物理页的 `MarkId`，编辑已有标注时生成删除和新增。套件保留页内链接导航，禁用外链、附件和表单动作。
- **签名**：`insertSignature` 接收打字、笔迹或图片，捕获时生成透明 PNG。跨文档复用签名由 App 存储和传入。
- `web/src/plan.ts` 与 Rust 的 `Plan` 使用相同 JSON 字段。`tests/fixtures/plan/*.json` 由 TS 生成对照，Rust 用同一份改动单保存并读回。
- **App 怎么引用**：从与 Rust 相同的引擎 tag 构建并打包 `web/`，在 App 中安装本地包。复现实测与选型见 `decisions/0016`。
- PDF.js 编辑器的内部序列化格式会随版本变：套件钉 6.5 这一线，升 PDF.js 要过套件的测试。

### 7.6 试用页

先构建 `web/`，再运行 `cargo run --release --example studio -- [PDF]`。Rust 标准库服务绑定 `127.0.0.1` 的临时端口，接收同源的 PDF 字节、改动单和分开的密码选项。密码在浏览器中保留于当前会话，Rust 接收后用 `Zeroizing` 持有。保存结果写到 `local/studio/`，浏览器自动重开，也能下载副本。

试用页在编辑区显示主文件原始页码，在缩略图栏整理输出页序。插入页和旋转用于输出预览，保存重开后可直接编辑这些页面。

---

## 8. PDFium 级（v0.6.0，计划）

### 8.1 分工（`decisions/0010`）

| 能力 | PDFKit | PDFium | 定 |
| --- | --- | --- | --- |
| 可搜索扫描件 | 存盘时 `PDFDocumentSaveTextFromOCROption`（macOS 13 起，系统自己识别、写进文字层） | 写不可见文字（render mode invisible），用引擎 OCR 的行、框和阅读顺序 | Phase 1 比 |
| 移动、删除文字和图片 | 做不到 | 页面对象：列出、变换、删除、重新生成内容流 | PDFium |
| 涂黑 | 没有真正的涂黑（redact 只是标注） | 删对象、改图片像素、盖黑块 | PDFium |

### 8.2 接入

- Rust 绑定用 `pdfium-render`；feature `pdfium` 默认关，只有要的 App 打开。`check.sh` 加一种开 `pdfium` 的配置。
- 只用现成的预编译库，不从源码编（站主定，`decisions/0010`）。优先 macOS arm64 静态库链进 App，没有就随 App 带 `libpdfium.dylib`。Phase 1 量 App 二进制多大、链接多久、许可证（PDFium 和它带的 FreeType 等）再定。
- PDFium 的 `unsafe` 在绑定里；引擎对外只露安全类型，和 PDFKit 一样不跨线程、每页查 `Cancel`。PDFium 文档对象只在 `edit::apply` 里短暂存在，改完交回字节再给 PDFKit。

### 8.3 可搜索扫描件

```rust
pub fn searchable(doc: &Document, pages: &[u32], opts: &OcrOptions, cancel: &Cancel, progress: &dyn Fn(u32)) -> Result<Vec<u8>, Error>;
```

- 只给没有文字层（或乱码）的页加字，别的页、标注、链接、目录不动；断网可做。
- 加的字和页上的字对齐：预览、PDF.js 里选中、搜索高亮落在对的位置。
- 标记哪些页的文字是识别加上的；引擎再读时能分出这些页（`Content` 怎么交 Phase 2 定），App 照样能告诉模型「这是识别的字，可能有错字」。
- 生成的文件比原件大多少、Phase 1 的结论写进 §6。

### 8.4 移动与删除文字、图片

```rust
doc.objects(8)?;   // Vec<Object { id, kind: Text | Image | Path | Other, bounds, text }>（feature "pdfium"）
```

- 改动单加 `objects: Vec<ObjectOp>`：`Move { page, id, dx, dy }`、`Delete { page, id }`；`id` 只在同一次打开里有效。
- 套件加「选对象」工具：点选、框选、拖动预览、删除；结果进改动单。
- 改完再取字、画页图：移动的字在新位置能取到，删掉的取不到，别的页逐字不变。

### 8.5 涂黑

- 改动单加 `redact: Vec<Redaction { page, rects }>`。框里的字、图片像素、标注都去掉，再盖黑块。
- 字对象只有一部分在框里时，这一页退回「整页画成图 → 涂黑 → 本机 OCR 把框外的字加回成不可见文字」，那一页仍能搜框外的字。
- **硬条件**（`decisions/0010`）：涂完用引擎再取字，框里一个字都取不到；页图里框内全黑；图片资源里也没有原像素。达不到就报错、不交文件。
- 基础涂黑不管文件信息、书签、附件里的同一段字；App 的确认框要说清楚。套件的涂黑工具是先标、再确认执行。
