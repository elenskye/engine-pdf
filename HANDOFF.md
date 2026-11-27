# HANDOFF

更新于 2026-11-27。**v0.6.0 已发**（tag `v0.6.0`，待站主 push）。

下一步：引擎总仓库 2026-11-27 定了新顺序（`personal-engine-studio/docs/decisions/0003`、`0004`、`0007`）——v0.7 改为智能体工具包 + 统一的 `find`，钉 engine-core 的 tag；接 Pomelo 顺延到 v0.8、接 Pith 到 v0.9。本仓库 `ROADMAP.md` 的版本表还没改，等站主转来的提示词再更新。Peck 接入（v0.4.0 计划）不受影响。

## 本轮（Claude 审核 v0.6.0）

Codex 实施了 Phase 1–5。审核时在 38 份真实论文和站主的扫描件上实测，发现 PDFium 整页重写会改坏页面别处，改了写法（`decisions/0019`）：

- `src/sys/content.rs`（新）：数出每个页面对象由哪条指令画出、和 PDFium 清单对上，只改那几条指令（移动、删除、换图、盖黑框）。`src/sys/check.rs`（新）：改动区外的画面和字必须不变。`src/sys/tags.rs`（新）：涂黑页的结构树去掉替代文字。
- `src/sys/pdfium.rs` 改成只读；`src/sys/redact.rs` 重写流程：先在原内容上涂，做不到或核对不过再整页位图；只沾框边的字、穿过框的线条不再触发位图；补回的搜索层按字切；位图黑白页存灰度；去掉页上的 `PieceInfo`。
- `src/sys/write.rs`：换页序、合并时带上可搜索副本的识别文字。
- 试用页去掉「对象编辑会话」（`examples/studio/main.rs`、`studio.mjs`）：对象编号只由文件内容决定。
- 新增测试：`content`、`tags`、`redact` 的单元测试；`tests/object_edits.rs` 字距不被改、删接续字要字宽；`tests/redaction.rs` 带结构树的页原地涂黑；`tests/searchable.rs` 换页序后读回识别文字。
- 文档：`decisions/0019`（新）、`0018`、`0010`，design §6–§8，README，ROADMAP（已完成版本压成一段），CHANGELOG，`CLAUDE.md` 的 PDFium 一条。

## 验证

- `./scripts/check.sh` 通过：默认、`samples,fake`、`samples,fake,pdfium` 三种配置的测试和 Clippy，Web 类型检查和测试。付费 Mathpix 测试照旧忽略。
- 真实论文检查程序在 `local/v060/review/v060_real.rs`（不进 Git；用时复制到 `examples/`，`--features samples,pdfium`）。结果见 design §6「对象编辑和涂黑」。
- 试用页（开 `pdfium`）实操：Shore 2019 第 2 页拖一行、删一行、在句中涂黑；中文扫描件生成可搜索副本。输出在 `local/studio/`。
- 「预览」原生窗口（Claude 临时接管屏幕）：可搜索副本搜「供應鏈效率」命中并高亮在原字上；涂黑副本全选复制，剪贴板里没有框里的字。

## 已知限制

- 字形压在框边、框碰到 Form 或斜放、带透明的图片时整页位图：画面变成图，框外的字靠 OCR 补回，紧贴框边的个别词可能补不回来。
- 删除同一行中间的一段字要字宽表：标准 14 字体（没有 `/Widths`）、非 `Identity-H` 的 CID 字体、竖排会报 `ObjectNotEditable`；移动不受影响。
- 页上有内联图片时这一页的对象编辑报 `ObjectEditFailed`（lopdf 解析不全），涂黑走整页位图。
- Intel Mac 和 App 安装包没验。开 `pdfium` 的 App 分发时附 `THIRD_PARTY.md` 所列通知。

## Mathpix 真跑

发版验收用 Pith 同一份凭据，写入会话临时目录的 `600` 权限文件，通过 `MATHPIX_APP_ID` / `MATHPIX_APP_KEY` 传入。只对样本真跑，每版上限 $1。

```bash
cargo test --features samples --test mathpix_live live_mathpix_mixed_page_8 -- --ignored --exact --nocapture
```
