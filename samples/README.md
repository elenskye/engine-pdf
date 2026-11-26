# PDF 样本

`cargo run --features samples --example make_samples` 将生成样本写到 `samples/out/`。样本都由本仓库代码生成，产物不提交。

| 样本 | 内容 |
| --- | --- |
| `outline.pdf` | 三页，多层目录，含页内跳转和网址书签 |
| `form.pdf` | 文本、勾选、单选（两个选项）、下拉 |
| `annotated.pdf` | 高亮、下划线、删除线、便签、文本框、绘图、矩形、椭圆、直线、箭头、图片印章、链接 |
| `links.pdf` | 两页，首面含页内跳转和网址链接 |

文字、扫描、页标签和异常输入样本见 `docs/design.md` §5.1。

HEIC 打包用 libheif 的 `example.heic` 实测（blob `829384037820e545467a4af49aa6414c2b0f2885`），样本只用于本机核对。[样本](https://github.com/strukturag/libheif/blob/master/examples/example.heic) · [MIT 许可证](https://github.com/strukturag/libheif/blob/master/examples/COPYING)
