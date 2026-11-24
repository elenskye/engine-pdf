# 0003 PDFKit、Vision 经 objc2 在进程内调用

2026-11-24（设计稿 §2.1）。

- 用 objc2 的框架绑定（`objc2-pdf-kit`、`objc2-vision`、`objc2-app-kit`、`objc2-core-graphics`、`objc2-foundation`，0.3.x）；绑定缺的退回 `msg_send!`。所有 `unsafe` 只在 `src/sys/`。
- 以 Pomelo `youzi/pdf.rs` 为原型（已在正式版里跑）。
- 接口同步；`Document` 不跨线程；每页一个 autorelease pool。

没选的：Peck 的 JXA / `osascript`（每次起进程、难取消、难调 Vision）；Swift 小工具（每个 App 多打包签名一个二进制）；纯 Rust（`pdf-extract` 中文乱码、没有 OCR；pdfium 要带大动态库）。
