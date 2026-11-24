# 0001 分工、版本与 1.0 的范围

2026-11-24 站主定（ASK）。

- **分工**：照 brief，Claude Code 出设计稿和分 Phase 的计划；之后每个 Phase 由 Codex 照 `AGENTS.md`（= `CLAUDE.md`）实施，Claude 审核、补测试、commit、打 tag。Codex 不 commit、不打 tag、不 push。
- **版本**：从 0.1.0 起，照站主的「App & Git 开发准则」：小修 `x.y.z → x.y.(z+1)` 不分 Phase、做完自动发版；新功能、大改 `x.y → x.(y+1)` 分 Phase、只在最后一个 Phase 发版；大版本由站主定。
- **1.0.0 是完成体**：引擎完成，且 Peck、Pith、Pomelo 三个 App 都换成调引擎、删掉各自重复的 PDF 实现、通过验收。各 App 的接入改动在它们自己的仓库里做，照它们自己的版本规矩升版。

后果：engine-pdf 的 0.4–0.6 三个版本各对应一个 App 的接入，引擎那边为接入补的接口跟着这一版发；1.0.0 冻结公共接口。
