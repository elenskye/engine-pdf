# 第三方依赖

引入依赖时在这里登记一行（`CLAUDE.md`「工程约定」）。

| 名称 | 版本 | 许可证 | 用途 | 链接 |
| --- | --- | --- | --- | --- |
| `lopdf` | `^0.42.0` | MIT | 写出改动（在原文件对象上改、拼页）、读写表单、烧平外观层、读写 PDF 加密权限 | [crate](https://crates.io/crates/lopdf) |
| `pdfium-render` | `^0.9.6` | MIT OR Apache-2.0 | `pdfium` feature 的 PDFium Rust 绑定 | [crate](https://crates.io/crates/pdfium-render) |
| PDFium | chromium/8085 | BSD-3-Clause | 在 macOS 上静态链接的可选编辑后端，关闭 V8 和 XFA | [预编译包](https://github.com/libviprs/libviprs-dep/releases/tag/pdfium-8085) · [许可证](https://pdfium.googlesource.com/pdfium/+/refs/heads/chromium/8085/LICENSE) |
| `objc2` | `^0.6.5` | MIT | Objective-C 对象生命周期和 autorelease pool | [crate](https://crates.io/crates/objc2) |
| `objc2-foundation` | `^0.3.2` | MIT | NSData、NSString、NSDictionary | [crate](https://crates.io/crates/objc2-foundation) |
| `objc2-pdf-kit` | `^0.3.2` | Zlib OR Apache-2.0 OR MIT | 本机打开、取字、页标签、页面绘制和拆页 | [crate](https://crates.io/crates/objc2-pdf-kit) |
| `objc2-app-kit` | `^0.3.2` | Zlib OR Apache-2.0 OR MIT | JPEG 和 PNG 编码 | [crate](https://crates.io/crates/objc2-app-kit) |
| `objc2-core-foundation` | `^0.3.2` | Zlib OR Apache-2.0 OR MIT | 页面坐标及 `samples` 的内存缓冲和富文本 | [crate](https://crates.io/crates/objc2-core-foundation) |
| `objc2-core-graphics` | `^0.3.2` | Zlib OR Apache-2.0 OR MIT | 位图绘制、图片资源计数和样本 PDF 生成 | [crate](https://crates.io/crates/objc2-core-graphics) |
| `objc2-core-text` | `^0.3.2` | Zlib OR Apache-2.0 OR MIT | `samples` 的系统字体和中英文排字 | [crate](https://crates.io/crates/objc2-core-text) |
| `objc2-vision` | `^0.3.2` | Zlib OR Apache-2.0 OR MIT | 本机文字识别 | [crate](https://crates.io/crates/objc2-vision) |
| `serde` | `^1.0.229` | MIT OR Apache-2.0 | 序列化 Mathpix 任务状态和 PDF 改动单 | [crate](https://crates.io/crates/serde) |
| `serde_json` | `^1.0.154` | MIT OR Apache-2.0 | 解析和生成 Mathpix 和改动单 JSON | [crate](https://crates.io/crates/serde_json) |
| `zeroize` | `^1.9.1` | Apache-2.0 OR MIT | 清零 Mathpix 密钥与 PDF 密码的存储 | [crate](https://crates.io/crates/zeroize) |
| `zip` | `^4.6.1` | MIT | `fake` 的模拟 `.mmd.zip` 下载 | [crate](https://crates.io/crates/zip) |
| `ureq` | `^3.5.1` | MIT OR Apache-2.0 | 通过系统 TLS 同步请求 Mathpix | [crate](https://crates.io/crates/ureq) |
| `log` | `^0.4.35` | MIT OR Apache-2.0 | 编译时关闭 TRACE，避免 HTTP 依赖输出原始报文 | [crate](https://crates.io/crates/log) |
| `pdfjs-dist` | `~6.5.438` | Apache-2.0 | Web 阅读器和编辑层，App 提供 peer 依赖 | [PDF.js](https://github.com/mozilla/pdf.js) |
| `typescript` | `^5.9.3` | Apache-2.0 | Web 套件的类型检查和构建 | [TypeScript](https://github.com/microsoft/TypeScript) |

PDFium 8085 静态库的上游通知以该版本的源码和 `DEPS` 为准。检查归档内容时确认包含 FreeType（FTL 或 GPL-2.0-or-later，采用 FTL）、Abseil（Apache-2.0）、ICU（Unicode 与 ICU 通知）、HarfBuzz（MIT）、AGG 2.3（MIT）、Little CMS（MIT）、OpenJPEG（BSD-2-Clause）、zlib（Zlib）和 libjpeg-turbo（IJG、BSD-3-Clause、Zlib）。App 分发时附上 PDFium 与实际链接组件的版权和许可证全文。预编译包自带 PDFium 的 `LICENSE`，其余通知从上游取。 [PDFium 8085 DEPS](https://pdfium.googlesource.com/pdfium/+/refs/heads/chromium/8085/DEPS) · [随源码保存的依赖说明](https://pdfium.googlesource.com/pdfium/+/refs/heads/chromium/8085/third_party)
