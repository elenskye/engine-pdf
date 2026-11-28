//! 花钱的规矩和共用账本：v0.8.0 起用 engine-core 的 `spend`（总仓库 `decisions/0011`，本仓库 `decisions/0023`），
//! 所有引擎、所有 App 一样。这里只再导出，App 照旧写 `engine_pdf::mathpix::spend::…`。
//!
//! Mathpix 的账在 `<底>/engine-pdf/mathpix-ledger.jsonl`（底默认 `~/Library/Application Support`，所有 App 共用；
//! `ENGINE_LEDGER_DIR` 或 `Ledger::At(底)` 改）。用 [`super::Options::book`] 拿这一份。

pub use engine_core::spend::*;
