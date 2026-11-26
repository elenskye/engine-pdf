# HANDOFF

更新于 2026-11-26。**v0.5.2 已发布**（tag `v0.5.2`）。v0.5.0 没单独打 tag，内容在 v0.5.1 起的各版里。

## 本轮（v0.5.2：加密改由 lopdf 写）

- **问题**：站主在 Edge 里打开验收用的加密表单，输入密码后只剩文字标签，四个控件全不见。
- **排查**：
  - 同一份表单不加密，Edge 显示正常；只设权限密码（不要打开密码）的加密副本，Edge 一样不显示。所以是加密输出的问题，和密码无关。
  - 同一文件用 lopdf 按同样算法（128 位 RC4）加密，Edge 显示正常。
  - PDFKit 加密的那份，「预览」的引擎和 PDF.js 都能正常读出四个值。看来是 PDFKit 写出的加密文件和 PDFium 系阅读器（Edge、Chrome）不对付，结构上没看出差别。
- **改法**：`src/sys/password.rs` 的 `encrypt` 只用 lopdf 加密，PDFKit 只负责打开核对：打开密码、权限密码、权限位。顺带去掉了「PDFKit 多给批注权限时再用 lopdf 重写」的绕路。记进 `decisions/0015`、design §6、CHANGELOG。
- 重新生成 `local/acceptance/3-form-locked.pdf`（旧的改名 `3-form-locked-v051.pdf` 留作对照）。
- 版本 0.5.2：`Cargo.toml`、`Cargo.lock`、`src/lib.rs`、`web/package.json`。

## 验证

- `./scripts/check.sh` 通过。
- 38 份真实论文重跑：1,778 项通过，没过的 6 项是已知的有/无结构树合并。其中每份都设打开密码、限权限，并验证不给密码打不开、错密码报错、权限位精确、文字不变、读者不能改、去密码。
- 新的加密表单：
  - Edge 里（只设权限密码的副本）四个控件都在。
  - PDFKit 用打开密码画出的页图、PDF.js、引擎都读出「验收用名字」、勾上、第二个单选、Beta。
  - 不给密码打不开。

## 等站主

- **一步**：用「预览」或 Edge 打开新的 `local/acceptance/3-form-locked.pdf`，输入样本测试密码（`src/samples.rs` 的 `LOCKED_PASSWORD`），看四个控件的值。Claude 按规矩不在别的 App 的密码框里输密码。
- push：`git push && git push --tags`（`main`、`v0.5.1`、`v0.5.2`）。

## 已知问题

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
