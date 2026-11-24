# engine-pdf 设计稿（v0.1.0）

> 依据 `docs/brief.md`，2026-11-24 核对过三个 App 的现有代码。选型见 `docs/decisions/0001`–`0004`。本文交给 Codex 实施，按 `ROADMAP.md` 的 Phase 推进。

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
├── Document          打开、页数、页标签、单页文字层、单页页图、拆页
├── read / read_each  逐页读取 + 分类（文字层 / OCR / 空白 / 没取到字 / 失败）
├── pages::parse      "3"、"1-5"、"1,3-4"、"4-" → 页码
├── Cancel            取消令牌（可克隆，跨线程）
├── Error             错误种类 + 一句中文
├── mathpix           Quote（本地报价）→ run（上传识别）；分步 submit / wait / fetch / delete
└── samples           （feature "samples"）测试样本生成器，App 的测试也能用
```

### 1.2 打开与错误

```rust
let doc = Document::open(path)?;        // 或 Document::from_bytes(&bytes)?（Pith、Pomelo 手里是字节）
doc.page_count();                        // u32
doc.label(8);                            // Option<String>：第 8 页印的页码（/PageLabels），如 "vi"、"3"
```

`Error` 是枚举，App 按种类给提示；`Display` 是一句简短中文（不带路径、密钥、服务原文）：

| 种类 | 什么时候 | `Display` |
| --- | --- | --- |
| `NotPdf` | 开头不是 `%PDF-` | 这不是 PDF 文件。 |
| `Damaged` | PDFKit 打不开 | 这份 PDF 已损坏，打不开。 |
| `Locked` | `isLocked`（要密码） | 这份 PDF 加了密码，打不开。 |
| `PageOutOfRange { page, total }` | 页码超出 | 这份 PDF 只有 {total} 页，没有第 {page} 页。 |
| `BadPageSpec(String)` | `pages::parse` 看不懂 | 页码看不懂：…（写 3、1-5、1,3-4） |
| `Render { page }` / `Ocr { page }` | 单页画不出、识别出错 | 第 {page} 页画不出来。/ 第 {page} 页识别出错。 |
| `Cancelled` | 取消 | 已停止。 |
| `Unsupported` | 非 macOS | 只有 Mac 上能读 PDF。 |
| `Io(io::Error)` | 读文件 | 读不了这个文件。 |
| `Mathpix(MathpixError)` | 见 §3.6 | 见 §3.6 |

密码 PDF 这一轮只报 `Locked`，不做解锁（以后要做加 `Document::unlock(password)`）。

### 1.3 逐页读取

```rust
pub struct ReadOptions {
    pub ocr: Ocr,                   // Off | Auto(OcrOptions)：没有文字层或文字层是乱码的页，用 Vision 在本机识别
    pub render: Option<Render>,     // 要不要页图：哪些页（All / NoText）、最长边、JPEG/PNG、最多几张
    pub text_min_chars: usize,      // 少于这么多个非空白字符算没有文字层（默认 20，沿用 Peck、Pith）
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
    Text(String),                 // 文字层：PDF 里本来就有的字（PDFKit 原样，不整理）
    Ocr(OcrText),                 // 本机 Vision 识别的字 + why（NoTextLayer / Garbled）+ 平均置信度 + 每行文字和框
    Blank,                        // 空白页：没有文字层，页面几乎全白（不跑 OCR）
    NoText(NoTextReason),         // 有画面但没取到字：OcrOff / NothingRecognized（照片、图表）/ Garbled（乱码且没开 OCR）
    Failed(Error),                // 这一页读失败；其他页照常
}
```

App 怎么分辨四种情况：`Text` = 原文、`Ocr` = 识别结果（可能有错字）、`Blank` = 空白、`Failed` = 读取失败；整份打不开在 `Document::open` 就返回 `Err`。`OcrText.lines` 的框是 PDF 坐标（左下原点、单位 pt），和 PDF.js 的 `convertToPdfPoint` 同一套，Pith 以后要在阅读器里高亮 OCR 结果可以直接用。

### 1.4 页码

- `pages::parse(spec, total, max)`：Peck 现有的 `parse_pages` 整段移过来（带测试），三个 App 共用同一种写法。
- `Page.index` 永远是原文件物理页码；`Page.label` 来自 PDFKit 的 `PDFPage.label`，和 PDF.js 的 `getPageLabels()` 读的是同一个 `/PageLabels`。**引用定位存 `index`，显示用 `label`**（Pith 阅读器按 `pageIndex = index - 1` 跳转）。验收里有一份带罗马数字前言的样本专门核对（§5）。
- Mathpix 的结果也映射回 `index`（§3.4）。

### 1.5 取消与进度

`Cancel` 是 `Arc<AtomicBool>` 的薄包装：App 在任何线程 `cancel.cancel()`，引擎**每页开始前**、每次 Vision 请求后、Mathpix 每次轮询之间（50ms 粒度）检查，返回 `Error::Cancelled`。一页内部不打断（PDFKit 取字几毫秒，Vision 一页不到 1 秒）。进度靠 `read_each` 的回调（每交一页就是进度），Mathpix 另有 `progress: &dyn Fn(Progress)`。Peck 的 `watch::Receiver<bool>` 在 Peck 那边桥一下：起一个小任务等 `wait_for` 后调 `cancel.cancel()`。

### 1.6 Peck 调用示例

```rust
use engine_pdf::{pages, read, Cancel, Content, Document, Ocr, ReadOptions, Render};

// read_pdf（在 spawn_blocking 里）
let doc = Document::open(&file)?;                                   // Locked / Damaged → 原样告诉模型和站主
let wanted = pages::parse(str_arg(input, "pages"), doc.page_count(), MAX_PAGES)?;
let opts = ReadOptions {
    ocr: Ocr::auto(),                                               // 扫描页在本机识别，断网也行
    render: vision.then(|| Render::jpeg(1568).only_no_text().at_most(6)),
    ..Default::default()
};
for p in read(&doc, &wanted, &opts, &cancel)? {
    let body = match &p.content {
        Content::Text(t) => t.trim().to_string(),
        Content::Ocr(o) => format!("（这一页没有文字层，下面是本机 OCR 识别的，可能有错字）\n{}", o.text),
        Content::Blank => "（空白页）".into(),
        Content::NoText(_) => attach_or_hint(&p.image),            // 照旧：附图，或提示用 ocr_pdf
        Content::Failed(e) => format!("（这一页没读出来：{e}）"),
    };
    out.push_str(&format!("\n--- 第 {} 页 ---\n{body}\n", p.index));
}

// ocr_pdf：先报价（不联网）→ Peck 的批准框 → 才上传
let quote = mathpix::quote(&doc, &wanted)?;                          // { service: "Mathpix", pages: [8], usd: 0.005 }
let ok = approve_as(ToolKind::Other, call_id, format!("送 Mathpix 识别第 {} 页？", quote.pages_text()),
                    format!("预计 ${:.3}（每页 $0.005），只上传这几页，读完删掉 Mathpix 那边的", quote.usd)).await;
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
| 纯 Rust（Pith 的 `pdf-extract`、pdfium） | 不要：中文乱码、扫描件无解；pdfium 要随包带一个大动态库 |
| **objc2 进程内直调 PDFKit、Vision** | **用**：Pomelo 正式版已经这样跑；引擎静态链进各 App 的二进制，「从自己的安装包调用」天然成立 |

依赖用 objc2 官方的框架绑定（`objc2-pdf-kit`、`objc2-vision`、`objc2-app-kit`、`objc2-core-graphics`、`objc2-foundation`，都是 0.3.x），绑定缺的方法再退回 Pomelo 那样的 `msg_send!`。所有 `unsafe` 收在 `src/sys/`，对外只露安全的 Rust 类型。

### 2.2 PDFKit：直接搬的代码

| 能力 | 来源 | 怎么改 |
| --- | --- | --- |
| 打开（`initWithData` / `initWithURL`、`isLocked`） | Pomelo `pdf.rs::open` | 错误换成 `Error::{Damaged, Locked}`；先看 `%PDF-` 头（Peck `resolve_pdf`）报 `NotPdf` |
| 逐页取字（`pageAtIndex` → `string`，每页一个 autorelease pool） | Pomelo `pdf.rs::pages` | 改成按页取，不一次取全份 |
| 页标签 | 新写：`PDFPage.label` | — |
| 画页（`boundsForBox` + 旋转 + `thumbnailOfSize:forBox:` → JPEG） | Pomelo `pdf.rs::jpeg_page`、`jpeg_of` | 加 PNG 和灰度小图（空白判定、OCR 用的不编码成 JPEG，直接拿 `CGImage`） |
| 拆页（新建 `PDFDocument`，`insertPage(page.copy)`） | Peck JXA 的 `split` | 用 objc2 重写，交回 `dataRepresentation` 的字节，不落临时文件 |
| 哪些页有图（CGPDF 数 XObject） | Pomelo `pdf.rs::image_pages` | 原样搬，作 `Document::image_counts()`（Pomelo 的 `office.rs` 在用） |
| 乱码判定（替换字符、私用区、控制字符 > 20%） | Pith `content.rs::garbled` | 原样搬 |
| 页码解析 | Peck `pdf.rs::parse_pages` + 测试 | 原样搬到 `pages::parse` |
| 测试用的最小 PDF（手写 PDF 语法） | Pomelo `pdf.rs::tests::sample_pdf` | 搬进 `samples`，加 `/PageLabels` |

不搬的：Pomelo 的 `images_to_pdf`（只有 Pub2MD 用，留在 Pomelo）、`print.rs`（打印是 Pomelo 自己的流程）、`sha256_hex`（不是 PDF 处理）。

### 2.3 Vision：本机 OCR（新写）

- `VNRecognizeTextRequest`：`recognitionLevel = accurate`、`usesLanguageCorrection = true`、`recognitionLanguages` 默认 `["zh-Hans", "zh-Hant", "en-US"]`（`OcrOptions` 可改）。`VNImageRequestHandler(cgImage:)` 的 `performRequests` 是同步的，正好配同步接口。完全在本机跑，不联网。
- 输入：页面用 CoreGraphics 画成灰度 `CGImage`，最长边默认 2400px（Pomelo 给模型看图用的也是 2400），白底。
- 输出：每个 `VNRecognizedTextObservation` 取 `topCandidates(1)`，得到文字、置信度、框；框从 Vision 的归一化坐标换成 PDF 坐标。
- **阅读顺序**：Vision 不保证两栏的顺序。引擎按行框排：先找有没有竖直的栏缝（页面中部一条没有行跨过的竖带，两边都有足够多行），有就左栏从上到下、再右栏；没有就整页从上到下、同一行从左到右。双栏扫描样本专门验（§5）。
- 以后可评估：macOS 26 的 Vision 新增了能认文档结构（段落、表格、阅读顺序）的接口，但只有 Swift 能调；要用得加一层 Swift 静态库，1.0 不做。

### 2.4 一页怎么分类

```text
取文字层 ─ 有效字符 ≥ text_min_chars 且不是乱码 ──────────────→ Text
   │否
   ├─ 画 64px 灰度小图，深色像素 < 0.2% ─────────────────────→ Blank
   │否
   ├─ ocr = Off ──────────────────→ NoText(OcrOff 或 Garbled)
   │
   └─ Vision 识别 ─ 认出字 ──────→ Ocr(why = NoTextLayer 或 Garbled)
                  └ 没认出 ──────→ NoText(NothingRecognized)
任何一步出错 ──────────────────────→ Failed(Error)，接着读下一页
```

空白阈值（0.2%）、乱码比例（20%）、`text_min_chars`（20）都放进 `ReadOptions`，默认值用样本校准。

### 2.5 长文档

- 一页一个 autorelease pool，页图在回调里交出去，`read_each` 的内存只跟一页有关；`read` 只给页数少的场景（Peck 一次最多 20 页）。
- 粗估（M3 Pro，要用样本实测后写进本节）：取字每页几毫秒；画 1568px JPEG 每页几十毫秒；Vision 精确模式每页零点几秒到 1 秒，300 页扫描书约 3–5 分钟，所以 App 要显示进度、能取消。
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

```rust
pub struct Quote {
    pub service: &'static str,      // "Mathpix"
    pub pages: Vec<u32>,            // 原文件页码，升序去重；就是要上传的那几页
    pub price_per_page: f64,        // 默认 0.005，App 可在 Options 里覆盖（Pith 有自己的价目设置）
    pub usd: f64,                   // pages.len() × price_per_page
}
```

`run` 只上传 `quote.pages`，不会多传。确认框里显示什么、怎么措辞、预算怎么查，全是 App 的事。

### 3.2 密钥、记账

- **密钥**：App 从自己的钥匙串取出来，`Keys { app_id, app_key }`（`Zeroizing`）每次调用交进来。引擎不存、不写日志，错误里不带。
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
2. **上传**：`POST /v3/pdf`，multipart（`options_json` + `file`），文件名固定 `pages.pdf`（不带本机路径）。选项固定带 `include_page_breaks: true`（映射页码要用）和 `metadata.improve_mathpix: false`（不让 Mathpix 拿去训练）；`Options` 可加 `lines`（要 `lines.json`）、`mmd_zip`（要带裁图的压缩包）、公式分隔符（Peck 用 `$`）和其他原样透传的字段。
3. **等**：每 2 秒 `GET /v3/pdf/{id}`，读 `status`、`percent_done`、`num_pages`；最长等 `Options.max_wait`（默认 10 分钟）。注意 Mathpix 的错误常常是 HTTP 200 带 `error_info`，每次都看 body（Peck 的教训）。
4. **下载**：`.mmd` 必取；要了才取 `.lines.json`、`.mmd.zip`（202 = 还没打包好，过一会儿再取，Pomelo 的做法）。
5. **删除**：不管成败，只要拿到过 `pdf_id` 就 `DELETE /v3/pdf/{id}`，结果写进 `usage.remote_deleted`。

`/v3/converter`（Pomelo 把 mmd 转 docx 等）不进引擎，留在 Pomelo。

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
let sub: Submitted = mathpix::submit(&doc, &quote, &keys, &opts, &cancel)?;   // 可序列化：{ pdf_id, page_map, pages_sent, options }
// App 立刻把 sub 存进自己的任务文件
mathpix::wait(&sub, &keys, &opts, &cancel, &progress)?;
let r = mathpix::fetch(&sub, &keys, &opts)?;
mathpix::delete(&sub, &keys);                                                    // -> bool
```

Pomelo 上传 Word 文件走 `mathpix::submit_file(bytes, mime, ...)`：不拆页、没有页码映射（`page_map` 为空，结果标 `Approximate`）。

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

- 引擎自带同步 HTTP：`ureq` 3（native-tls，Pith 在用），走 `Transport` trait。App 有特殊网络需求（比如要用系统代理）可以自己实现 `Transport` 交进来。
- feature `fake` 给 `FakeTransport`：按脚本回应、记下每个请求。引擎自己的测试用它核对「取消时零请求」「上传件只有一页」「失败也 DELETE」；三个 App 的模拟模式（Peck `Mathpix::Mock`、Pith `mock_mode`、Pomelo 的 mock）也换成它的 `FakeTransport::echo()`：把上传件每页的 PDFKit 文字当成识别结果，不联网、不记费。
- 只允许 `https://api.mathpix.com`；`Options.base` 只为测试放开 `http://127.0.0.1`（Pith 有 `mathpix_base` 设置）。

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

### 4.3 Pomelo

| 入口 | 现在 | 换成 |
| --- | --- | --- |
| `youzi/pdf.rs` 的 `open`、`pages`、`render_pages`、`page_jpeg`、`jpeg_page`、`jpeg_of`、`image_pages` | Pomelo 自己的 objc2 | 引擎（这些就是引擎的原型） |
| `youzi/files.rs::prepare`、`pictures_only`（知识库导入） | 没文字层的 PDF 只列文件名 | `read(Ocr::auto())`：扫描件在本机就能进知识库（行为变化，届时问站主） |
| `youzi/office.rs`（`embedded_images`、`images_text`、`look_at`）、`assistants/importer.rs` | `pdf::pages`、`image_pages`、`page_jpeg` | `Document::image_counts`、`read`、`Document::render` |
| `pub2md/job.rs` 的原页图（`render_pages`） | 整份画完存文件夹 | `read_each` 逐页画、逐页存（能取消） |
| `pub2md/mathpix.rs` 的 submit / status / 下载 | Pomelo 自己的客户端 | `mathpix::submit`（`Submitted` 存进任务）/ `wait` / `fetch(mmd_zip)` / `delete`；Word 走 `submit_file` |
| 留在 Pomelo | — | `sha256_hex`、`images_to_pdf`、`print.rs`、`/v3/converter`、`mmd.rs` 的切块 |

核对：文字 PDF 的知识库分段应逐字一致（同是 PDFKit）；原页图比宽高、比像素平均差（阈值用样本定）；Pub2MD 用 `FakeTransport` 跑一遍完整任务，`pdf_id` 存进任务、中途退出再继续不重复上传。

---

## 5. 怎么验收

### 5.1 样本

不提交二进制，提交**生成器**：feature `samples` 用 CoreText + CGPDFContext 现场生成（系统字体只用来画，PDF 不进 Git，也就没有字体再分发的问题），引擎和三个 App 的测试都调它；`cargo run --example make_samples` 把它们写进 `samples/out/` 给人看。源文字写死在代码里，所以「缺没缺字」「顺序对不对」能自动比。

| 样本 | 内容 | 验什么 |
| --- | --- | --- |
| `text_en` / `text_zh` | 3 页英文；3 页简体中文（中英数字标点混排） | 取字、缺字 |
| `two_column_en` / `two_column_zh` | 两栏，每段开头有编号 | 文字层的阅读顺序 |
| `mixed` | 10 页：1 文字、2 英文扫描（文字画成图）、3 空白、4 文字、5 中文扫描、6 无字的图、7 文字、**8 公式和表格（画成图）**、9 空白、10 文字 | 页类型、断网 OCR、Mathpix 只传第 8 页 |
| `scanned_two_column` | 两栏中文扫描 | OCR 的阅读顺序 |
| `labels` | 前 3 页标 i–iii，之后 1 起 | 页标签、引用定位 |
| `locked` / `damaged` / `not_pdf` | 用户密码；截断的 `text_en`；一段纯文本 | 明确的错误 |

真实世界的文件（站主自己的中文扫描书、双栏论文）只放 `local/` 做人工抽查；要一份可提交的公开双栏论文的话，届时问站主（下载要同意，来源和许可证记进 `samples/README.md`）。

### 5.2 场景

| brief 的场景 | 自动验证（`cargo test`，Mac 上） | 要看实际输出 |
| --- | --- | --- |
| Peck 读指定页，取到文字或页图，回答回指原页码；中文、双栏查缺字和顺序 | 引擎：样本源文字逐字都在、两栏左栏全部在右栏之前、`index`/`label` 对；Peck：基准测试新旧一致，`format_pages` 每段带「第 N 页」 | Peck 正式版里问一个样本的问题，回答引用的页码对；站主的真实中文 PDF 抽查 |
| 文字、扫描、空白混排；断网能识别；分得清原文 / OCR / 空白 | `mixed` 逐页类型 = `[Text, Ocr, Blank, Text, Ocr, NoText, Text, Ocr, Blank, Text]`；OCR 字错率：英文 ≤ 2%、中文 ≤ 5%（门槛用样本校准）；这条路径不构造 `Transport` | 关掉 Wi-Fi 在 Peck 里读 `mixed`；双栏扫描的顺序看一眼 |
| 只选第 8 页识别公式表格：确认框显示第 8 页和费用；取消零请求；上传件只有这一页；结果标第 8 页 | `FakeTransport`：`quote.pages == [8]`、`usd == 0.005`；不调 `run` 时请求数 = 0；调了只有 1 个上传、上传件 1 页且画出来和原第 8 页像素一致；结果 `index == 8`；成功、失败、取消、超时都发了 `DELETE` | 真跑一次 Mathpix（`#[ignore]` 测试，$0.005）：LaTeX 质量、真实回应的映射、Mathpix 那边删掉了；Peck 里看批准框、点拒绝后日志里没有请求 |
| 损坏、要密码时明确提示；App 从自己的安装包调用引擎 | `damaged` → `Damaged`，`locked` → `Locked`，`not_pdf` → `NotPdf` | 三个 App 里各看一次提示；正式版里读 PDF 时没有 `osascript` 子进程（Peck），`otool -L` 看到 App 二进制链着 PDFKit、Vision，`cargo tree -i engine-pdf` 看到钉的 tag |

另外每个版本发版前跑一次 `cargo run --example probe -- <文件> --pages 1-10 --ocr --render samples/out/`，把每页类型、字数、耗时打出来，耗时记进 §2.5。

---

## 6. 第一步

**第一步做 v0.2.0 Phase 1–2：引擎骨架（`Error`、`Cancel`、`pages::parse`、`src/sys/` 的 PDFKit 打开和取字）加样本生成器。** 原因：渲染、OCR、Mathpix 拆页，以及 Peck 的基准对照，都站在「打开 + 按页取字 + 一套已知内容的样本」上；而 Peck 第一个接的就是这一块，新旧都是 PDFKit，能逐字对照，桥写错了马上看得出来。代码大半是从 Pomelo `pdf.rs`、Peck `parse_pages` 搬的，风险最低。

还要用样本实测才能定的：PDFKit 对两栏和中文的顺序、缺字；Vision 的中文准确率和两栏顺序（§2.3 的排序办法）；空白、乱码、字数三个阈值；真实 Mathpix 回应里 `\pagebreak` 和页数是否总对得上（§3.4）；PDFKit 页标签和 PDF.js 是否一致；各步耗时（§2.5）。
