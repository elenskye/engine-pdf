# 0006 保留 ureq，关闭 Mathpix 构建的 TRACE 日志

2026-11-25，站主通过 ASK 裁定。

`ureq-proto` 在 TRACE 级别记录原始 HTTP 字节。回环试验确认，即使请求头的值已标记为敏感，TRACE 仍会输出鉴权头和响应正文。

保留 `ureq` 和系统 TLS。macOS 构建使用 `log` 的 `max_level_debug` 和 `release_max_level_debug`，Debug 和 Release 都把编译期上限设为 DEBUG。与引擎共用同一 `log` 依赖的 App 也采用该上限。DEBUG 及以上仍可用。
