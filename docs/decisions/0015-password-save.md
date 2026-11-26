# 0015 加密文件保存与权限位

2026-11-26。站主确认显式换密码或去密码，以及修改加密来源需要先用权限密码打开（ASK）。

- `Document::open_with_password` 及 `from_bytes_with_password` 使用 `Zeroizing<String>`。能打开仅设权限密码的文件时也验证输入是否获得所有者权限。`Save` 由 Rust 调用方提供，`remove_password` 与 `lock` 二选一。普通空改动返回原字节。
- 去密码时让 PDFKit 从已获所有者权限的文档导出，并核对页数、表单和逐页文字层。实测仅用 `lopdf` 0.42 读取旧安全处理器的所有者密码会生成错误的明文内容。
- 加密使用 128 位 RC4。PDFKit 可能在允许修改时一并写入批注权限。验证时读加密字典的权限位，PDFKit 输出与请求不一致时用 `lopdf` 按同一算法写入精确值。输出用 PDFKit 验证打开密码和权限密码。打开后用 `Info.permissions` 显示普通读者的实际权限位，所有者按当前权限显示。
