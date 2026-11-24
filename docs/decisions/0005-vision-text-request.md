# 0005 本机 OCR 用 VNRecognizeTextRequest，不用 Swift 的 RecognizeDocumentsRequest

2026-11-24（v0.3.0 计划前实测，Claude 定）。

macOS 26 起 Vision 有只能从 Swift 调的 `RecognizeDocumentsRequest`（给段落、表格、列表）。和 objc2 能直接调的 `VNRecognizeTextRequest` 对比（M3 Pro、1568px 页图）：

| | 生成的双栏中文扫描 | 生成的表格页 | 真实知网双栏中文页 |
| --- | --- | --- | --- |
| `VNRecognizeTextRequest` | 顺序对 | 公式认错、表格拆成散字 | 两栏的行交错，但**一行一条、带框**，引擎能按框重排；字基本对 |
| `RecognizeDocumentsRequest` | 顺序对、按段 | 认出表格，但缺格 | **左右栏的行被并进同一段**，拆不开；夹带日文假名（「増大に」） |

两者热启动都是 0.15–0.8 s/页；系统第一次加载识别模型约 17 s。

结论：用 `VNRecognizeTextRequest`（objc2，不加 Swift 构建步骤），栏序由引擎按行框排（design §2.3）。公式、表格本来就交给 Mathpix。以后苹果的文档接口改进了可以再比一次。
