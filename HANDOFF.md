# HANDOFF

更新于 2026-11-26。**v0.5.1 已发布**（tag `v0.5.1`，含 v0.5.0 全部内容；v0.5.0 没单独打 tag）。

## 本轮（v0.5.1：搬家 + 代验收）

- **仓库搬进 `~/Documents/GitHub/personal-engine-studio/engine-pdf`**：仍是独立 git 仓库，远端不变（App 钉的 GitHub tag 不受影响）。
  - 总仓库加 `.gitignore`，不收 `engine-*/`；README、CLAUDE.md 的路径改了，总仓库另有一笔 `plan:` 提交。
  - `cargo clean -p engine-pdf` 清掉烤进旧路径的构建缓存（6.6 GB），新位置 `check.sh` 全过。
  - Pith、Pomelo、Peck、Atelier 里没有引用旧路径。`docs/brief*.md` 是站主原件，按规矩不改。
- **ROADMAP「用户验收清单」v0.5.0 三条，Claude 用 full control 代验**：试用页在应用内浏览器里操作，「预览」用电脑控制看。产物在 `local/acceptance/`（不进 Git）。
  - 标注：知网论文加全套标注，删一个已有标注（椭圆）。「预览」里位置、外观、中文文本框都对，「高亮和注释」侧栏列出便签、高亮、下划线、删除线的文字，链接跳到第 2 页。
  - 页面和目录：17 页论文删页、转页、中间插一份 11 页 PDF、加目录，存出 27 页。「预览」里页序、旋转、目录跳转都对，引文 [1] 跳到参考文献（新第 24 页），引擎核对 27 页文字全一致。
  - 表单和密码：四类控件填值、设打开密码。「预览」要密码；去密码的副本在「预览」里四个值都对，还是活表单；烧平副本外观一样、不再是表单。
- **验收中修的**：
  - 新便签没有弹窗，「预览」里点了不展开。现在新加的便签带一个收起的 `/Popup`（`src/sys/write.rs`），测试加了断言。
  - 试用页新目录项指向页底（`y: 0`），改成页顶（`examples/studio/studio.mjs`）。
- **更新过时信息**：
  - Peck 现 1.2.1、还没接引擎：ROADMAP v0.4.0 一节改成「随时可接、钉当时最新的 tag」。
  - v0.6 Phase 1 的 PDFium 只用现成预编译库、不从源码编（站主定）：ROADMAP、design §8.2、`decisions/0010`。
- 版本 0.5.1：`Cargo.toml`、`Cargo.lock`、`src/lib.rs`、`web/package.json`。CHANGELOG 加 v0.5.1。

## 验证

- `./scripts/check.sh` 通过（新位置，两种配置共 100 项，付费 1 项忽略）。
- 38 份真实论文重跑：1,778 项通过。没过的 6 项和上一轮一样，是有/无结构树的文件合并（已知行为）。
- 「预览」逐项看过，见上。

## 等站主

- **一步**：在「预览」里打开 `local/acceptance/3-form-locked.pdf`，输入样本测试密码（`src/samples.rs` 的 `LOCKED_PASSWORD`），看四个控件是不是「验收用名字」、勾上、第二个单选、Beta。Claude 按规矩不在别的 App 的密码框里输密码。
- push：引擎 `main` 和 tag `v0.5.1`（`git push && git push --tags`），总仓库 `personal-engine-studio` 的 `main`（还没有远端的话先不用）。

## 已知问题

- 用压缩对象流的文件，保存后最多变成原来的 1.6 倍大。
- 有/无结构树的文件合并时，整份不带结构树。
- `Mark::Image` 的字节在 JSON 里是数字数组，签名图大时请求体偏大（等接 App 时一起定）。
- 试用页的「插入 PDF」「图片」是系统文件框。应用内浏览器打不开它，这轮验收用脚本把文件交给同一个输入框。

## 下一轮

照 ROADMAP：Peck 接入（Peck 1.2.1，钉 `v0.5.1`），或 v0.6.0 PDFium 级（Phase 1 先找预编译库，计划等批）。

## Mathpix 真跑

发版验收用 Pith 同一份凭据，写入会话临时目录的 `600` 权限文件，通过 `MATHPIX_APP_ID` / `MATHPIX_APP_KEY` 传入。只对样本真跑，每版上限 $1。v0.5.x 没动 Mathpix，没有真跑。

```bash
cargo test --features samples --test mathpix_live live_mathpix_mixed_page_8 -- --ignored --exact --nocapture
```
