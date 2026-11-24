//! engine-pdf：Peck、Pith、Pomelo 共用的 Mac PDF 引擎。
//!
//! 设计见 `docs/design.md`；现在是 v0.1.0（只有设计稿），接口从 v0.2.0 起按 `ROADMAP.md` 的 Phase 实现。

/// 引擎版本（App 记日志、核对钉住的 tag 用）。
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    #[test]
    fn version_matches_manifest() {
        assert_eq!(super::VERSION, "0.1.0");
    }
}
