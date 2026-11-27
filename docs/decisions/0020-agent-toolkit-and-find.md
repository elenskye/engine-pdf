# 0020 v0.7 插一版：Agent 工具包 + pdf_find，接 App 往后挪

2026-11-27 站主定（引擎总仓库 `docs/decisions/0003`、`0004`、`0007`，开发准则 `GUIDE.md`）。改了 `0008` 的版本顺序。

- **v0.7.0 = 工具包 + `pdf_find`**：把现有每项能力定义成给模型用的工具（清单、调用入口、命令行 `engine-pdf tools / describe / call`、`docs/toolkit.md`），再加统一的 `pdf_find`。原 v0.7 接 Pomelo 改为 v0.8，原 v0.8 接 Pith 改为 v0.9。
- **engine-core 不由本仓库建**：它已在 `../engine-core` 单独建（总仓库 `0007`）。本仓库钉它的 tag（`v0.2.0`：工具包骨架 + find）；`Cancel`、错误和费用约定、清单格式、find 的归一化和命中类型都从它来，engine-pdf 再导出一遍，对外接口不变。engine-core 发 v0.2.0 之前，v0.7 只做不依赖它的部分（工具清单草案、字框取法实验）。
- **`pdf_find` 的位置**：物理页码 + 页标签 + 命中处的框（PDF 坐标）。文字层页用原文的字和字框；扫描页用可搜索副本里存的识别结果，或现做 OCR（本机，免费、只花时间），来源标 OCR。归一化不写在本仓库。`web/` 给人用的查找改走同一个 find。
- **开发理念写进 `CLAUDE.md`**：照抄 `GUIDE.md`「一」到「五」（由总仓库 `scripts/compose-claude.py` 生成，别在本仓库改）。
- **Peck 建议等 v0.7 后照工具包接**：省得先接一遍 Rust 接口、再改接工具包（总仓库 `ROADMAP.md`）。
