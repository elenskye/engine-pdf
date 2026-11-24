# HANDOFF

更新：2026-11-24 · 版本 v0.1.0（初始化 + 设计稿）· Claude Code

## 现在在哪

v0.1.0 做完：仓库初始化好了，设计稿 `docs/design.md` 可以交给 Codex。**v0.2.0 等站主看过设计稿和 `ROADMAP.md` 的 Phase 计划再开工。**

站主这轮定的（ASK，记在 `docs/decisions/0001`、`0002`）：照 brief 由 Codex 实施、Claude 审核；1.0.0 = 引擎 + 三个 App 都接上；App 用 GitHub 仓库 + tag 引用。

## 本轮做了什么

- `git init`（`main`）；`.gitignore`；`.claude/settings.json`（关掉 Claude Code 的默认署名）。
- `CLAUDE.md`（规则：开工前、分工、版本、Git、边界、工程约定、文档）；`AGENTS.md` → `CLAUDE.md` 软链接。
- `docs/design.md`：回答 brief 的五个问题（接口、原生复用、Mathpix 交界、三个 App 的迁移、验收），带 Peck 调用示例，收尾写了第一步和要用样本验的事。
- `docs/decisions/0001–0004`：分工与版本、git tag 引用、objc2 进程内调用、Mathpix 交界。
- `ROADMAP.md`：v0.1.0 → v1.0.0 的版本表、v0.2.0 的 5 个 Phase、之后版本的大纲、用户验收清单。
- `README.md`、`CHANGELOG.md`、`THIRD_PARTY.md`。
- `Cargo.toml` + `src/lib.rs`：只有 `VERSION` 和一个版本测试的空壳（v0.2.0 起才写接口）。
- `brief.md` 移到 `docs/brief.md`，内容没改。

核对过的现有实现（只读）：Peck `harness/tools/pdf.rs`、`gateway/mathpix.rs`、`harness/loop.rs::ocr_job`；Pith `research/content.rs`、`concept/mathpix.rs`、`concept/commands.rs`、`crates/concept-engine/src/mathpix.rs`、`src/reader/controller.ts`；Pomelo `youzi/pdf.rs`、`youzi/files.rs`、`youzi/office.rs`、`pub2md/mathpix.rs`、`pub2md/job.rs`。结论写在 design §0。

## 测试

- `cargo test`：通过（1 个版本测试）。
- `cargo clippy --all-targets -- -D warnings`：通过。

## 未完成 / 已知问题

- 设计稿里的耗时（§2.5）、三个阈值（空白、乱码、字数）、OCR 字错率门槛都是估计，v0.2.0 / v0.3.0 用样本实测后改。
- Vision 对两栏扫描的阅读顺序靠引擎自己排（§2.3），效果要样本验。
- 真实 Mathpix 回应里 `\pagebreak` 和页数对不对得上（§3.4），v0.3.0 真跑一次才知道。

## 等你

1. **看设计稿、批 v0.2.0 的计划**：`docs/design.md`（重点 §1 接口、§3 Mathpix 交界）和 `ROADMAP.md`「v0.2.0」。有要改的直接说。
2. **建 GitHub 远端并 push**：在 GitHub 建私有仓库 `elenskye/engine-pdf`（不要勾初始化 README），然后在本仓库跑：

   ```bash
   git remote add origin https://github.com/elenskye/engine-pdf.git
   ```

   ```bash
   git push -u origin main
   ```

3. **（v0.3.0 前，不急）真跑 Mathpix**：沿用 Pith 真跑时给过的那个密钥文件夹里的 mathpix 密钥；每版上限默认 $1，用的是样本第 8 页（每次 $0.005）。

## 下一轮从哪开始

- 站主批了计划后：让 Codex 照 `AGENTS.md` 做 **v0.2.0 Phase 1（骨架）**，接着 Phase 2（样本生成器）。可以这样交代 Codex：「读 AGENTS.md、HANDOFF.md、ROADMAP.md 和 docs/design.md §1、§2、§5.1，做 v0.2.0 Phase 1，做完照 AGENTS.md『汇报』写 HANDOFF」。
- Codex 做完后回到 Claude：审核、跑 `cargo test` 和 `cargo clippy`、commit `v0.2.0: …`。
