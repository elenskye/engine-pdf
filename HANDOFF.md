# HANDOFF

更新于 2026-11-26。**v0.5.2 已完成、验收完毕**（本地 tag `v0.5.2`，待 push）。v0.5.0 没单独打 tag，内容在 v0.5.1 起的各版里。

## 本轮（v0.5.2：加密改由 lopdf 写）

- **问题**：站主在 Edge 里打开验收用的加密表单，输入密码后只剩文字标签，四个控件全不见。
- **排查**：
  - 只设权限密码、不要打开密码的副本：PDFKit 加密的，Edge 不显示控件；lopdf 同算法（128 位 RC4）加密的，Edge 正常。
  - 设了打开密码：不管 PDFKit 还是 lopdf 加密，Edge 都不显示控件。
  - 同样的文件在 Chrome（站主验）、「预览」的引擎、PDF.js、PDFium（本机 Codex 带的 pypdfium2）里都正常。
  - Edge 的新阅读器用的是 Adobe 引擎（截图右下角有 Acrobat 标），网上有 Acrobat 自己生成的加密表单在 Edge 里同样不显示的报告。站主定：忽略这个 Edge 的问题。
  - 试过 AES：lopdf 的 AES-128 三家都能读；AES-256 在 PDFKit 里画成乱码，不能用。没有换算法，仍是 128 位 RC4。
- **改法**：`src/sys/password.rs` 的 `encrypt` 只用 lopdf 加密，PDFKit 只负责打开核对：打开密码、权限密码、权限位。去掉了「PDFKit 多给批注权限时再用 lopdf 重写」的绕路。记进 `decisions/0015`、design §6、CHANGELOG。
- 重新生成 `local/acceptance/3-form-locked.pdf`（旧的改名 `3-form-locked-v051.pdf` 留作对照）。
- 版本 0.5.2：`Cargo.toml`、`Cargo.lock`、`src/lib.rs`、`web/package.json`。

## 验证

- `./scripts/check.sh` 通过。
- 38 份真实论文重跑：1,778 项通过，没过的 6 项是已知的有/无结构树合并。其中每份都设打开密码、限权限，并验证不给密码打不开、错密码报错、权限位精确、文字不变、读者不能改、去密码。
- ROADMAP「用户验收清单」v0.5.0 三条都完成：标注、页面和目录由 Claude 代验；表单和密码的最后一步由站主在 Chrome 里输密码确认。

## 等站主

- push：`git push && git push --tags`（`main` 和 `v0.5.2`；`v0.5.1` 已 push）。

## 已知问题

- 设了打开密码的表单，Edge 新版阅读器不显示控件（Edge 的限制，不处理）。
- 用压缩对象流的文件，保存后最多变成原来的 1.6 倍大。
- 有/无结构树的文件合并时，整份不带结构树。
- `Mark::Image` 的字节在 JSON 里是数字数组，签名图大时请求体偏大（等接 App 时一起定）。
- 试用页的「插入 PDF」「图片」是系统文件框，应用内浏览器打不开它，验收时用脚本交文件。

## 下一轮

照 ROADMAP：Peck 接入（Peck 1.2.1，钉 `v0.5.2`），或 v0.6.0 PDFium 级（Phase 1 先找预编译库，计划等批）。

## Mathpix 真跑

发版验收用 Pith 同一份凭据，写入会话临时目录的 `600` 权限文件，通过 `MATHPIX_APP_ID` / `MATHPIX_APP_KEY` 传入。只对样本真跑，每版上限 $1。v0.5.x 没动 Mathpix，没有真跑。

```bash
cargo test --features samples --test mathpix_live live_mathpix_mixed_page_8 -- --ignored --exact --nocapture
```
