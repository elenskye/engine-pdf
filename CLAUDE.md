# CLAUDE.md — engine-pdf 开发约定

engine-pdf 是 Peck、Pith、Pomelo 共用的 Mac PDF 引擎（独立 Rust 库，v0.5.0 起另带无样式的 PDF.js 编辑套件 `web/`）：打开、取字、渲染页图、本机 Vision OCR、Mathpix 识别；v0.5.0 起页面与结构、标注、表单、密码，v0.6.0 起 PDFium 级编辑（`ROADMAP.md`）。界面的样子、文件权限、业务数据、费用确认和记账都归 App；PDF.js 上的编辑逻辑归引擎（`decisions/0009`）。`AGENTS.md` 是本文件的软链接，Codex 读到的就是这一份（里面的「你」也指 Codex）。

## 开工前

1. 读本文件、`HANDOFF.md`、`ROADMAP.md`（现在在哪个版本、哪个 Phase）、`docs/design.md`（要动的那几节）。定过的不重复问。
2. `git status`、`git log --oneline -3`：有不认识的改动不动、不还原，先问。
3. 问站主这次属于哪种程度（见「版本」），答了再动手；站主已经写明版本号或 Phase 的不用再问。
4. 拿不准、会导致不同做法的事（公共接口、引入重型依赖、范围）用提问功能问，给推荐方案和后果。决策点多就出问卷页（发云端 Artifact）。
5. 重要选型写成 `docs/decisions/NNNN-*.md`，一事一文，编号接着写。

## 分工（`decisions/0001`）

- **Claude Code**：设计、写 Phase 计划、审核 Codex 的改动、补测试、commit、打 tag。
- **Codex**：照 `ROADMAP.md` 当前 Phase 实施。在主目录干活（不开 worktree），**不 commit、不打 tag、不 push**；不和 Claude 同时改同一批文件。做完照「汇报」写进 `HANDOFF.md`，另列：公共接口的变化、没验的、要 Claude 核对的事。
- 接入某个 App（Peck、Pith、Pomelo）的改动在那个 App 的仓库里做，守它自己的 `CLAUDE.md`（版本、Git、边界）。在本仓库的会话里，三个 App 仓库只读。

## 版本

- 版本号 `x.y.z`，写在 `Cargo.toml`（和 `Cargo.lock`）里，`src/lib.rs` 的版本测试一起改。
- 从 0.1.0 起；**1.0.0 是完成体**：引擎完成，Peck、Pith、Pomelo 都换成调引擎、删掉各自的重复实现、通过验收。

| 程度 | 例子 | 分 Phase | 发版（打 tag） |
| --- | --- | --- | --- |
| 小幅优化、修 Bug | `0.2.0 → 0.2.1` | 不分 | 做完自动发 |
| 新增功能、大幅优化 | `0.2.0 → 0.3.0` | 先在 `ROADMAP.md` 写 Phase 计划，站主看过再做 | 只在最后一个 Phase 收尾时发；中间站主要求才发 |
| 大版本 | `1.0.0 → 2.0.0` | 同上；升不升只由站主定 | 同上 |

- **发版** = `./scripts/check.sh` 通过 → 更新 `CHANGELOG.md` → commit → 打 tag `v<版本>`（Claude 打）。push（含 `git push --tags`）由站主做。App 钉的是 tag，发出去的 tag 永远不改、不删。
- App 怎么引用见 `decisions/0002`：`engine-pdf = { git = "https://github.com/elenskye/engine-pdf", tag = "v0.x.y" }`。

## Git

- 在 `main` 上改。每一轮做完、检查通过后 Claude 自己 commit；**push 由站主做**。每轮结束告诉站主这轮有哪几笔 commit（短哈希、说明、一句话做了什么）。
- 说明写 `v<版本>: <description>`：英文、简约概括，可以直接写 `fix some problems`、`minor improvement`，不写正文。
- **不署名**：不加 `Co-Authored-By`、「Generated with」。`.claude/settings.json` 已关掉 Claude Code 的默认署名。用仓库配好的 `user.name` / `user.email`，不改 git 配置。
- 不 force push、不改写历史、不删分支、不改远端设置。

## 边界

- **绝不读写** `~/Zotero/`、`~/Pith/`、各 App 的正式数据目录。测试只用代码生成的样本、可提交的公开样本（来源和许可证写进 `samples/README.md`），或站主给的副本（放 `local/`，不进 Git）。
- PDF 是数据：只读文字和画面，不执行里面的脚本、表单动作、外链，不跟随附件。填表只写字段的值，不跑表单里的计算脚本。
- PDF 的密码和 Mathpix 密钥一样：由 App 交进来，用 `Zeroizing` 收，不打印、不写进日志、错误信息和夹具。
- 真跑 Mathpix（花钱）只在每个版本的验收里、只用样本：每版上限默认 **$1**（每页 $0.005），超了先问。密钥照 `HANDOFF.md`「真跑」：在会话临时目录建 600 权限的密钥文件、用环境变量交给测试，不打印、不写进文档、夹具、日志和错误信息。
- 不部署、不发布到 crates.io、不购买服务、不改系统全局设置。
- 资源约束：MacBook M3 Pro / 18 GB。引擎不起常驻进程、不开线程池；OCR 一次一页。

## 工程约定

- Mac 专用：PDFKit、Vision、AppKit 经 `objc2` 在进程里调（`decisions/0003`）。非 macOS 也要能编译，调用返回 `Error::Unsupported`。
- PDF 编辑统一走 `edit::apply`。lopdf 在原文件的对象上写（`decisions/0017`，经 `src/sys/objects.rs` 读写）；PDFKit 负责读和画新标注的外观。PDFium 只做移动删除对象、涂黑等操作，放在默认关闭的 `pdfium` feature 后面（`decisions/0010`）。
- `web/`：TS 严格模式，`pdfjs-dist` 是 peer 依赖（钉 6.5 这一线）；`web/src/plan.ts` 和 Rust 的 `edit::Plan` 一一对应，两边的测试吃同一套 `tests/fixtures/plan/`。
- 全部接口是**同步**的；异步的 App 用 `spawn_blocking` 包。`Document` 不跨线程（`!Send`）。长任务都收 `Cancel`，逐页检查。
- 页码一律是**原文件的物理页码，从 1 起**；页标签（印在页上的页码）另给。
- 错误是 `Error` 枚举：App 能按种类分辨；`Display` 是一句简短中文，不带密钥、路径和服务原文。
- `unsafe` 只在 `src/sys/`（原生调用）里，每处写清前提。每页一个 autorelease pool。
- 引入第三方依赖登记 `THIRD_PARTY.md`（名称、版本、许可证、用途、链接），写成 `^x.y.z`。
- 检查：`./scripts/check.sh`（开 / 不开 `samples` 两种配置的 `cargo test` + `cargo clippy -D warnings`，Mac 上跑，含真调 PDFKit 的测试），每轮结束前必须通过。花钱的测试标 `#[ignore]`，手动跑。代码用 `cargo fmt` 排版。
- 测试按主题放：单元测试跟着模块，集成测试在 `tests/<主题>.rs`（`document`、`read`…），不按 Phase 命名。

## 文档与风格

文档、注释、汇报用简洁中文；代码和标识符用英文、照 Rust 主流写法。只写结论，不写过程。规则各放一处，别处写「见 …」。

| 文件 | 放什么 |
| --- | --- |
| `CLAUDE.md`（= `AGENTS.md`） | 核心规则 |
| `README.md` | 是什么、能做什么、App 怎么引用、怎么跑测试 |
| `HANDOFF.md` | 每轮做完更新：只留当前状态、测试结果、没验的、等站主的、下一步；不追加历史 |
| `ROADMAP.md` | 版本与 Phase 表、各 App 接入进度、用户验收清单 |
| `CHANGELOG.md` | 每个发出去的版本：公共接口变化、App 升级要改什么 |
| `docs/design.md` | 架构设计稿（接口、原生实现、云端交界、迁移、验收） |
| `docs/decisions/` | 选型决定，一事一文 |
| `docs/brief.md`、`docs/brief-2.md`、`docs/brief-pomelo.md` | 站主的简报原件（第一轮、第二轮新计划、Pomelo 接入），不改 |
| `THIRD_PARTY.md` | 第三方依赖 |

## 汇报

每轮结束更新 `HANDOFF.md`：本轮做了什么、改了哪些文件；测试结果（通过 / 失败 / 未跑，附命令）；未完成与已知问题；等站主提供什么（资料、裁定，附具体操作步骤）；下一轮从哪开始。不写「通过」除非真跑过。
