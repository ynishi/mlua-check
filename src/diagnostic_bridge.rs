//! Maps `emmylua_code_analysis` diagnostics to [`crate::types::Diagnostic`].
//!
//! Responsibility: **conversion only**.  No diagnostic judgement or severity
//! decision logic lives here (Crux #3: thin-wrapper principle).  Line/column
//! 1-based conversion and diagnostic code → [`crate::types::RuleId`] mapping
//! are the sole concerns of this module.
//!
//! # Input type
//!
//! `EmmyLuaAnalysis::diagnose_file` returns `Option<Vec<lsp_types::Diagnostic>>`.
//! Each `lsp_types::Diagnostic` carries the diagnostic code as
//! `code: Some(NumberOrString::String(code_name))` where `code_name` is the
//! kebab-case serialization of `emmylua_code_analysis::DiagnosticCode`
//! (e.g. `"undefined-global"`, `"unused"`, `"undefined-field"`).

use lsp_types::{Diagnostic as LspDiagnostic, DiagnosticSeverity, NumberOrString};

use crate::types::{Diagnostic, RuleId, Severity};

/// Map a single `lsp_types::Diagnostic` produced by
/// `EmmyLuaAnalysis::diagnose_file` to the public [`Diagnostic`] type.
///
/// # Arguments
///
/// - `d`: an `lsp_types::Diagnostic` from `EmmyLuaAnalysis::diagnose_file`.
/// - `source`: the Lua source text.  Reserved for future UTF-8 aware
///   line/column extraction; currently unused because LSP range fields
///   already encode line and character positions.
///
/// # Returns
///
/// A [`Diagnostic`] with 1-based `line` and `column` coordinates.
///
/// # Notes
///
/// `d.range.start.line` and `.character` are 0-based (LSP convention).
/// This function adds 1 to each to produce the 1-based coordinates expected
/// by the public API.
///
/// Unknown diagnostic codes (not among the four well-known `RuleId` variants)
/// are mapped to `RuleId::Other(code_string)`.
#[allow(unused_variables)]
pub fn map_diagnostic(d: LspDiagnostic, source: &str) -> Diagnostic {
    let rule = extract_rule_id(&d.code);
    let severity = extract_severity(d.severity);

    // LSP positions are 0-based; public API uses 1-based.
    let line = d.range.start.line as usize + 1;
    let column = d.range.start.character as usize + 1;

    Diagnostic {
        rule,
        severity,
        message: d.message,
        line,
        column,
    }
}

/// Extract a [`RuleId`] from an LSP `NumberOrString` code field.
///
/// # Arguments
///
/// - `code`: the `code` field from `lsp_types::Diagnostic`.
///
/// # Returns
///
/// A [`RuleId`] matched by the code string, or `RuleId::Other` for unknown
/// codes.
fn extract_rule_id(code: &Option<NumberOrString>) -> RuleId {
    let code_str = match code {
        Some(NumberOrString::String(s)) => s.as_str(),
        Some(NumberOrString::Number(n)) => return RuleId::Other(n.to_string()),
        None => return RuleId::Other(String::new()),
    };

    match code_str {
        "undefined-global" => RuleId::UndefinedGlobal,
        "undefined-field" => RuleId::UndefinedField,
        // emmylua uses "unused" for all unused-local diagnostics
        "unused" => RuleId::UnusedVariable,
        // emmylua does not emit "undefined-variable" as a distinct code; map
        // it defensively for forward compatibility
        "undefined-variable" => RuleId::UndefinedVariable,
        other => RuleId::Other(other.to_string()),
    }
}

/// Map an LSP `DiagnosticSeverity` to the public [`Severity`] type.
///
/// # Arguments
///
/// - `sev`: the optional severity from `lsp_types::Diagnostic`.
///
/// # Returns
///
/// `Severity::Warning` when `sev` is `None` (conservative default).
fn extract_severity(sev: Option<DiagnosticSeverity>) -> Severity {
    match sev {
        Some(DiagnosticSeverity::ERROR) => Severity::Error,
        Some(DiagnosticSeverity::INFORMATION) | Some(DiagnosticSeverity::HINT) => Severity::Info,
        // WARNING or unknown (None) → Warning
        _ => Severity::Warning,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lsp_types::{NumberOrString, Position, Range};

    // --- helpers ---

    fn make_lsp_diag(
        code: Option<NumberOrString>,
        severity: Option<DiagnosticSeverity>,
        message: &str,
        line: u32,
        col: u32,
    ) -> LspDiagnostic {
        LspDiagnostic {
            range: Range {
                start: Position {
                    line,
                    character: col,
                },
                end: Position {
                    line,
                    character: col + 1,
                },
            },
            severity,
            code,
            message: message.to_string(),
            ..Default::default()
        }
    }

    // --- T1: happy path (required by Acceptance Criteria) ---

    #[test]
    fn maps_undefined_global_code() {
        let d = make_lsp_diag(
            Some(NumberOrString::String("undefined-global".into())),
            Some(DiagnosticSeverity::WARNING),
            "undefined global `foo`",
            2,
            4,
        );
        let result = map_diagnostic(d, "");
        assert_eq!(result.rule, RuleId::UndefinedGlobal);
        assert_eq!(result.severity, Severity::Warning);
        // 0-based line 2 → 1-based 3
        assert_eq!(result.line, 3);
        assert_eq!(result.column, 5);
        assert_eq!(result.message, "undefined global `foo`");
    }

    #[test]
    fn maps_undefined_field_code() {
        let d = make_lsp_diag(
            Some(NumberOrString::String("undefined-field".into())),
            Some(DiagnosticSeverity::WARNING),
            "undefined field",
            0,
            0,
        );
        let result = map_diagnostic(d, "");
        assert_eq!(result.rule, RuleId::UndefinedField);
        assert_eq!(result.line, 1);
        assert_eq!(result.column, 1);
    }

    #[test]
    fn maps_unused_code_to_unused_variable() {
        let d = make_lsp_diag(
            Some(NumberOrString::String("unused".into())),
            Some(DiagnosticSeverity::HINT),
            "unused local",
            0,
            0,
        );
        let result = map_diagnostic(d, "");
        assert_eq!(result.rule, RuleId::UnusedVariable);
        // HINT maps to Info
        assert_eq!(result.severity, Severity::Info);
    }

    #[test]
    fn maps_error_severity() {
        let d = make_lsp_diag(
            Some(NumberOrString::String("syntax-error".into())),
            Some(DiagnosticSeverity::ERROR),
            "syntax error",
            0,
            0,
        );
        let result = map_diagnostic(d, "");
        assert_eq!(result.severity, Severity::Error);
        // unknown code → Other
        assert!(matches!(result.rule, RuleId::Other(_)));
    }

    // --- T2: boundary / edge cases ---

    #[test]
    fn maps_none_code_to_other_empty() {
        let d = make_lsp_diag(None, Some(DiagnosticSeverity::WARNING), "msg", 0, 0);
        let result = map_diagnostic(d, "");
        assert_eq!(result.rule, RuleId::Other(String::new()));
    }

    #[test]
    fn maps_numeric_code_to_other_with_number_string() {
        let d = make_lsp_diag(
            Some(NumberOrString::Number(42)),
            Some(DiagnosticSeverity::ERROR),
            "numeric code",
            0,
            0,
        );
        let result = map_diagnostic(d, "");
        assert_eq!(result.rule, RuleId::Other("42".to_string()));
    }

    #[test]
    fn maps_none_severity_to_warning() {
        let d = make_lsp_diag(
            Some(NumberOrString::String("undefined-global".into())),
            None,
            "msg",
            0,
            0,
        );
        let result = map_diagnostic(d, "");
        assert_eq!(result.severity, Severity::Warning);
    }

    #[test]
    fn one_based_conversion_at_first_line() {
        // LSP line=0, character=0 → public line=1, column=1
        let d = make_lsp_diag(None, None, "x", 0, 0);
        let result = map_diagnostic(d, "");
        assert_eq!(result.line, 1);
        assert_eq!(result.column, 1);
    }

    // --- T3: error path ---

    #[test]
    fn unknown_code_string_produces_other() {
        let d = make_lsp_diag(
            Some(NumberOrString::String("some-future-code".into())),
            Some(DiagnosticSeverity::WARNING),
            "future diagnostic",
            10,
            5,
        );
        let result = map_diagnostic(d, "source text irrelevant");
        assert_eq!(result.rule, RuleId::Other("some-future-code".to_string()));
    }

    #[test]
    fn maps_information_severity_to_info() {
        let d = make_lsp_diag(
            None,
            Some(DiagnosticSeverity::INFORMATION),
            "info msg",
            0,
            0,
        );
        let result = map_diagnostic(d, "");
        assert_eq!(result.severity, Severity::Info);
    }
}
