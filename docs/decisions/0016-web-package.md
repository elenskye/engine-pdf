# 0016 Web 套件从引擎 tag 构建

2026-11-26，站主确认（ASK）。App 从与 Rust 相同的引擎 tag 构建 `web/`，用 `pnpm pack` 生成包含包清单、ES 模块及类型声明的离线包。App 通过本地路径安装，沿用自己的 PDF.js 6.5 依赖并提供运行资源。

pnpm 12.6.0 实测可从 Git tag 定位 `web/`，构建需要在 App 的 `pnpm-workspace.yaml` 将该依赖加入 `allowBuilds`。离线包在独立的 App 目录直接安装，严格类型检查可读取公开接口。
