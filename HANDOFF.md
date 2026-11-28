# HANDOFF

更新于 2026-11-28。**v0.7.0 已发**（tag `v0.7.0`，等站主 push）：Agent 工具包 + `pdf_find`，钉 engine-core `v0.2.1`。下一步是三个 App 照工具包接，计划在 `ROADMAP.md`。

## 这一版

Codex 实施 Phase 1–5，本机 Claude 审核两轮后改了（详见 `ROADMAP.md`「已完成的版本」、`decisions/0021`、`0022`）：

- **查找**：扫描件盖了页码、水印的页照样识别；所选页大多是文字页时只识别完全没字的页（文字版论文不用等识别模型冷启动）；按文件内容的内存缓存（`src/sys/cache.rs`，约 64 MiB）；跨行处两份取字的换行写法不同也能对位。
- **开 `pdfium` 时补连字、收紧中文框**（站主 2026-11-27 定）：`src/ligatures.rs` 按 PDFium 同一页的词表把 PDFKit 取丢连字的词换回来，`read`、`find` 都用；每份文件看第一张有连字的页决定要不要补。中文命中框高超过字宽 1.6 倍时按 PDFium 的整行上下沿收紧（`src/sys/layer.rs`）。PDFium 被别的代码先绑定时不崩。
- **工具**：`pdf_read` 加 `max_images`（给 Peck 按窗口定张数）；`pdf_edit` 改动单的清单写全；越界页报 `bad_args`；`pdf_find`、`pdf_objects` 的 Markdown 理顺。
- **engine-core v0.2.1**：软连字符后跟空格不再拼词（已推）。
- **接入计划**：Peck 1.4.0、Pomelo 1.2.0 按站主 2026-11-27/28 的回答重写（`ROADMAP.md`、design §4、`docs/toolkit.md`「三个 App 接哪些」）。

## 验证

- `./scripts/check.sh`：通过（默认、`samples,fake`、`samples,fake,pdfium` 的测试和 Clippy，Web 类型检查和测试），日志 `local/v070-review/check-final.log`。Linux `cargo check --all-features` 通过。
- **补连字**：39 份真实 PDF 全文比对开、关 `pdfium` 读出的文字，只有 Arpino、Phoenix 两篇有替换，共 546 处，逐条看过全对（`local/v070-review/repairs.log`）；Phoenix 全文 `fi` 从 3 处回到 388 处。
- **字框**：110 个随机短语 106 处框内文字正确，其余 4 处是抽样脚本按 PDFium 的顺序拼了表格和参考文献（`find-batch-fixes.log`）；李蓉蓉那篇的框从 24.8 pt 收到 13 pt，叠图核对贴字。
- **速度**：84 页杂志全书查找第一次约 3.6 秒、同一进程第二次 0.5 秒；5 页扫描件第一次 2.2 秒、第二次几乎瞬间（不含识别模型冷启动）。
- **命令行和试用页**：17 个工具在真实论文、杂志、扫描件上各跑过；试用页查找（繁简、扫描页、按词、上一个/下一个）在内置浏览器里实测。
- 没做：Mathpix 真跑（本版没改上传）；App 接入。

## 已知问题

- 不开 `pdfium` 时，连字丢失和偏高的中文框照旧（Peck 不开）。
- 扫描页上盖的字超过 20 个（JSTOR 那种「This content downloaded from…」）时，`read`、`find` 都当文字页、不识别；要识别得看页面上是不是整页大图，没做。
- 进程里第一次用识别模型要等 16–33 秒（系统的，引擎管不了）；查找的缓存只在进程里。

## 等站主

- push engine-pdf：`cd ~/Documents/GitHub/personal-engine-studio/engine-pdf && git push origin main --tags`

## 下一步

- **Peck 1.4.0**：Peck 1.3.0 做完后在 Peck 仓库开工，钉 `v0.7.0`（`ROADMAP.md`「v0.4.0」）。
- **Pomelo 1.2.0**：Phase 1（换底层）、2（扫描件进知识库）在 Pomelo 仓库做完（2026-11-28，钉 `v0.7.0`、开 `pdfium`，引擎没改），对照和验收写在 Pomelo 的 `docs/roadmap.md`「1.2.0」。到 Phase 5 时引擎补 `mathpix::convert` 和旧任务续跑，发 v0.8.0。
- **Pith（v0.9.0）**：大纲在 `ROADMAP.md`，开工前细化。

## Mathpix 真跑

发版验收用 Pith 同一份凭据，写入会话临时目录的 `600` 权限文件，通过 `MATHPIX_APP_ID` / `MATHPIX_APP_KEY` 传入。只对样本真跑，每版上限 $1。

```bash
cargo test --features samples --test mathpix_live live_mathpix_mixed_page_8 -- --ignored --exact --nocapture
```
