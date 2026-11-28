# 0024 转格式、Word 的页数和接着跑旧任务

2026-11-28 站主定（Pomelo 1.2.0 Phase 5 的 ASK）。

## 定的

- **`mathpix::convert` 当不收费**：Mathpix 的价目表只列 v3/text、v3/strokes、v3/pdf、files，没有 `/v3/converter`，Pomelo 以前也这么用。所以转格式不报价、不问、不记账。Pomelo 1.2.0 Phase 6 真跑一次转换后，站主在 Mathpix 后台看有没有扣钱；扣了再改成按规矩问（`decisions/0023`）。
- **Word 先按估计记账，识别完补差额**：`submit_file` 上传时按 App 给的估计页数记一笔（标 `estimated`），`wait` 拿到服务报的页数后，多出来的补记一笔（只补一次，存在 `Submitted` 里；估多了不退）。App 报价、定要不要问用估计的页数（Pomelo 读 Word 文件里存的页数，`docProps/app.xml`；读不到就是算不出、要问）。
- **接着跑引擎之前的上传**：`Submitted::uploaded(pdf_id, 页数, opts)` 用 App 自己存的 `pdf_id` 接着等、下载（Pomelo 1.2 以前的半截任务，上传的是整份）。那次上传不在账本里，也不补记。
- **模拟服务**：`FakeTransport::echo` 也模拟转格式（能打开的 Word、一页 PDF、装着 MMD 的 zip）；没有字、有图的页（照片、扫描）交回一句模拟的话，真正的空白页仍是空的。

## 没选的

- 转格式当「算不出」、每次都问：站主嫌烦，价目表也没列。
- Word 一律当「算不出」：每份 Word 都要停下等人点。
