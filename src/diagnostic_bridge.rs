//! Maps `emmylua_code_analysis` diagnostics to [`crate::types::Diagnostic`].
//!
//! Responsibility: **conversion only**.  No diagnostic judgement or severity
//! decision logic lives here (Crux #3: thin-wrapper principle).  Line/column
//! 1-based conversion and `DiagnosticCode` → [`crate::types::RuleId`] mapping
//! are implemented in ST4.
//!
//! # Naming alias
//!
//! `emmylua_code_analysis::LuaDiagnostic` is imported as `EmmyDiagnostic`
//! throughout this module to avoid confusion with
//! `crate::types::Diagnostic`.

use emmylua_code_analysis::LuaDiagnostic as EmmyDiagnostic;

/// Map a single `emmylua_code_analysis` diagnostic to the public
/// [`crate::types::Diagnostic`] type.
///
/// # Arguments
///
/// - `d`: an `EmmyDiagnostic` produced by
///   `EmmyLuaAnalysis::diagnose_file`.
/// - `source`: the Lua source text (used for line/column extraction when
///   the LSP range needs to be resolved against the original buffer).
///
/// # Returns
///
/// A [`crate::types::Diagnostic`] with line/column in 1-based coordinates.
///
/// # Panics
///
/// This function is a skeleton stub.  It will panic with
/// `"not yet implemented"` until the full implementation is added in ST4.
#[allow(dead_code, clippy::todo)]
pub fn map_diagnostic(d: EmmyDiagnostic, source: &str) -> crate::types::Diagnostic {
    let _ = (d, source);
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
