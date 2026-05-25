//! Converts [`crate::config::LintConfig`] into an `Arc<Emmyrc>` that
//! `EmmyLuaAnalysis::update_config` accepts.
//!
//! Responsibility: **conversion only**.  No diagnostic judgement logic lives
//! here (Crux #3: thin-wrapper principle).  The 1-to-1 field mapping of
//! `LintConfig.rule_severity` → `EmmyrcDiagnostic.severity` is deferred to
//! the full implementation in ST4.

use std::sync::Arc;

use emmylua_code_analysis::Emmyrc;

/// Build an `Arc<Emmyrc>` from a [`crate::config::LintConfig`].
///
/// # Arguments
///
/// - `config`: the caller-supplied lint configuration.
///
/// # Returns
///
/// An `Arc`-wrapped `Emmyrc` ready to pass to
/// `EmmyLuaAnalysis::update_config`.
///
/// # Panics
///
/// This function is a skeleton stub.  It will panic with
/// `"not yet implemented"` until the full implementation is added in ST4.
#[allow(dead_code, clippy::todo)]
pub fn build_emmyrc(config: &crate::config::LintConfig) -> Arc<Emmyrc> {
    let _ = config;
    todo!()
}

#[cfg(test)]
mod tests {
    // Smoke test: the module compiles and the stub is wired up correctly.
    // Full unit tests are added in ST4 when the function body is filled in.
    #[test]
    fn smoke() {
        // No assertion needed — this verifies the module resolves at compile
        // time and the test harness can discover it.
    }
}
