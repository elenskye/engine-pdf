# 0004 Mathpix：引擎发请求，App 管确认、密钥、记账

2026-11-24（设计稿 §3）。

- **两步**：`mathpix::quote` 只在本地数页、算钱，绝不联网；App 显示「服务、原文件页码、预计费用」，站主确认后才调 `mathpix::run`。取消 = 不调 `run`，所以一定没有请求。
- **引擎负责**：拆出指定页（内存里）、上传、轮询、下载（mmd，按需 `lines.json`、`mmd.zip`）、原页码映射、不管成败都 `DELETE`。另公开分步的 `submit / wait / fetch / delete`（`Submitted` 可序列化），给 Pomelo 续跑。
- **App 负责**：从钥匙串取密钥、每次交进来；确认框；预算上限；把 `Usage` 记进自己的账本。成功、失败都带 `Usage`；上传后失败按送去的页数记（`estimated = true`）。
- **HTTP**：同步 `ureq` 3，走 `Transport` trait；feature `fake` 的 `FakeTransport` 给引擎测试和三个 App 的模拟模式用。
- 固定带 `metadata.improve_mathpix: false`；上传文件名固定，不带本机路径。
- `/v3/converter` 不进引擎（只有 Pomelo 用）。
