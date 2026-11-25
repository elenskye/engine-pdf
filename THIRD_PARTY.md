# 第三方依赖

引入依赖时在这里登记一行（`CLAUDE.md`「工程约定」）。

| 名称 | 版本 | 许可证 | 用途 | 链接 |
| --- | --- | --- | --- | --- |
| `objc2` | `^0.6.5` | MIT | Objective-C 对象生命周期和 autorelease pool | [crate](https://crates.io/crates/objc2) |
| `objc2-foundation` | `^0.3.2` | MIT | NSData、NSString、NSDictionary | [crate](https://crates.io/crates/objc2-foundation) |
| `objc2-pdf-kit` | `^0.3.2` | Zlib OR Apache-2.0 OR MIT | 本机打开、取字、页标签、页面绘制和拆页 | [crate](https://crates.io/crates/objc2-pdf-kit) |
| `objc2-app-kit` | `^0.3.2` | Zlib OR Apache-2.0 OR MIT | JPEG 和 PNG 编码 | [crate](https://crates.io/crates/objc2-app-kit) |
| `objc2-core-foundation` | `^0.3.2` | Zlib OR Apache-2.0 OR MIT | 页面坐标及 `samples` 的内存缓冲和富文本 | [crate](https://crates.io/crates/objc2-core-foundation) |
| `objc2-core-graphics` | `^0.3.2` | Zlib OR Apache-2.0 OR MIT | 位图绘制、图片资源计数和样本 PDF 生成 | [crate](https://crates.io/crates/objc2-core-graphics) |
| `objc2-core-text` | `^0.3.2` | Zlib OR Apache-2.0 OR MIT | `samples` 的系统字体和中英文排字 | [crate](https://crates.io/crates/objc2-core-text) |
| `objc2-vision` | `^0.3.2` | Zlib OR Apache-2.0 OR MIT | 本机文字识别 | [crate](https://crates.io/crates/objc2-vision) |
| `serde` | `^1.0.229` | MIT OR Apache-2.0 | 序列化 Mathpix 任务状态 | [crate](https://crates.io/crates/serde) |
| `serde_json` | `^1.0.154` | MIT OR Apache-2.0 | 解析和生成 Mathpix JSON | [crate](https://crates.io/crates/serde_json) |
| `zeroize` | `^1.9.1` | Apache-2.0 OR MIT | 清零 Mathpix 密钥的存储 | [crate](https://crates.io/crates/zeroize) |
| `zip` | `^4.6.1` | MIT | `fake` 的模拟 `.mmd.zip` 下载 | [crate](https://crates.io/crates/zip) |
| `ureq` | `^3.5.1` | MIT OR Apache-2.0 | 通过系统 TLS 同步请求 Mathpix | [crate](https://crates.io/crates/ureq) |
| `log` | `^0.4.35` | MIT OR Apache-2.0 | 编译时关闭 TRACE，避免 HTTP 依赖输出原始报文 | [crate](https://crates.io/crates/log) |
