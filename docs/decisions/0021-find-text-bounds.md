# 0021 pdf_find 的字框从 PDFKit 选区和已有 OCR 取

2026-11-27 Phase 1 实验后由站主确认，供 Claude 审核。

- **文字层**：用 PDFKit 的页面字符串作原文。engine-core 给回原文 UTF-8 区间后，转换成 UTF-16 区间，用 PDFKit 选区取文本并核对这段确实对应原文，再按 `selectionsByLine()` 取行框。返回原 PDF 坐标，保留 CropBox 偏移。跨行命中分别返回各行的框。
- **备用**：`pdfium` 保持默认关闭。启用时可以从同一页的 PDFium 字符序列取字和框，避免把一个后端的索引套进另一个后端。已有原文命中但框无法核实，仍返回原文与物理页码，`rects` 为空。
- **扫描页**：优先用引擎已有的 OCR 阅读顺序及可定位的文字层。缺少可定位结果时默认用 Vision 现做 OCR，按已有识别行及字符跨度组框。关闭补识别仍可查已有 OCR 层。有引擎识别依据的命中标 `ocr`。保持逐页可取消。

## 实验

M3 Pro、macOS 27.2.1。本机探针在 `local/v070/find-probe/`。从仓库根目录运行 `local/v070/find-probe/.venv/bin/python local/v070/find-probe/compare.py`。取字对照用仓库已有的 PDFium 8085，落点图由独立的 pypdfium2 渲染。

| 输入 | 结果 |
| --- | --- |
| 合成中英文、双栏、跨行，知网论文、Attention、Shore、XGBoost | PDFKit 选区按原文的 UTF-16 区间取回同一文字，含 `eﬀective` 连字和行尾断词。知网跨行样本有两个行框。 |
| Shore 第 2 页、XGBoost 第 2 页 | 直接用 `characterBounds(at:)` 套入页面字符串的索引时，与 PDFium 的重合分别为 0/19、0/14。改用 PDFKit 行选区时分别为 19/19、14/14。 |
| 无文字层扫描页、带 OCR 层的扫描件 | 无文字层样本两边均无命中，对页图运行 Vision 后取到 `quiet valley` 和识别行框。两个早期 OCR 层样本 PDFKit 为空，PDFium 取得英文 `quiet valley` 和中文「木桥」并在扫描图上对位。已有旋转 90°、非零 CropBox 的引擎输出，两边都能定位。 |

18 个实例中，PDFKit 可读的 15 个实例均能按原文区间取回选区文字。两后端同取到的 12 个实例中，139 个非空白 PDFium 字框均与对应 PDFKit 行选区相交。目视叠框检查覆盖知网跨行、双栏中文、Shore、XGBoost 连字、英文 OCR 层和旋转裁切页。

Phase 4 用实际 find 结果独立叠图复核，见 `HANDOFF.md`。Vision 有时将同一个英文词框返回给相邻字符，find 可复用该词框。更细的框无法核实时，现做 OCR 可用识别行框。可搜索副本的文字层写入和涂黑沿用原有字框规则。

Phase 5 用真实多栏旧 OCR 副本补验。整页读序对不上时，允许用同页两份文本中均唯一的原样命中定位，并验证实际选区文字。仍无法定位时按需做 Vision，保留已存的 OCR 原文作为匹配依据。

## 审核补记（2026-11-27，本机 Claude）

- **扫描页上盖了几个字也要识别**：原来只要文字层有 1 个字就不做 OCR，扫描软件加的页码、水印让整页找不到。改成和 `read` 同一门槛（`design.md` §9.2），测试 `recognizes_scans_that_carry_only_a_short_text_stamp`。
- **独立核对**：39 份真实 PDF（Schwab 账户文件不读）随机抽 110 个短语（三成跨行、含中文），PDFium 取框内文字比对，记录在本机 `local/v070-review/`。没过的都是抽样按 PDFium 的取字顺序拼了表格、参考文献，或下面两条局限。
- **局限一：PDFKit 丢 fi 连字**。Phoenix 2024 全文 PDFium 有 403 处 `fi`，PDFKit 只剩 3 处（`signi cant`）；Arpino 2022 134 → 5（`fve`）。PDFium 取得对。这是 v0.2 起取字就有的问题，`read`、`find` 都受影响；改不改取字的来源由站主定（`HANDOFF.md`）。
- **局限二：个别中文字体的行框偏高**。李蓉蓉那篇的行框 24.8 pt，行距 15 pt，框会罩到下一行一半；「预览」里选中同一段也这样。逐字框（`characterBounds`）的下标和页面文字对不上，逐字选区框一样高，PDFKit 内收不紧；要收紧只能用 PDFium 的字形框。
- **软连字符**：Duke 出版社的期刊在 `all\u{ad} observations` 这种词间空格前也放软连字符，engine-core v0.2.0 当断词把两个词拼在一起；engine-core v0.2.1 修了。

