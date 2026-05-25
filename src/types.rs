use serde::ser::Serializer;
use serde::Serialize;
use std::fmt;

/// Unique identifier for each lint rule.
///
/// The `Other` variant acts as a catch-all for diagnostic codes produced by
/// `emmylua_code_analysis` that do not map to the four well-known variants.
///
/// # Serialization
///
/// All variants serialize as plain strings (e.g. `"undefined_global"`).
/// `Other(s)` serializes as the raw `s` string without any envelope.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum RuleId {
    /// Reference to a variable not defined in any enclosing scope.
    UndefinedVariable,
    /// Reference to a global name not in the known symbol table.
    UndefinedGlobal,
    /// Access to a field not declared in a `---@class` definition.
    UndefinedField,
    /// A local variable that is declared but never referenced.
    UnusedVariable,
    /// Catch-all for emmylua diagnostic codes not matched above.
    Other(String),
}

impl fmt::Display for RuleId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UndefinedVariable => write!(f, "undefined-variable"),
            Self::UndefinedGlobal => write!(f, "undefined-global"),
            Self::UndefinedField => write!(f, "undefined-field"),
            Self::UnusedVariable => write!(f, "unused-variable"),
            Self::Other(s) => write!(f, "{s}"),
        }
    }
}

impl Serialize for RuleId {
    /// Serializes the rule ID as a plain string.
    ///
    /// Known variants use their canonical kebab-case names; `Other` emits
    /// the inner string unchanged. This preserves the existing wire format
    /// while accommodating new emmylua diagnostic codes.
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

/// Severity level for a diagnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
    Info,
}

/// A single lint diagnostic.
///
/// # Fields
///
/// - `rule`: which lint rule triggered
/// - `severity`: the effective severity level
/// - `message`: human-readable explanation
/// - `line`: 1-based line number in the source
/// - `column`: 1-based column number in the source
#[derive(Debug, Clone, Serialize)]
pub struct Diagnostic {
    pub rule: RuleId,
    pub severity: Severity,
    pub message: String,
    pub line: usize,
    pub column: usize,
}

/// Aggregated lint result returned by `run_lint` and `LintEngine::lint`.
///
/// `error_count` and `warning_count` are pre-computed sums over `diagnostics`.
#[derive(Debug, Clone, Serialize)]
pub struct LintResult {
    pub diagnostics: Vec<Diagnostic>,
    pub error_count: usize,
    pub warning_count: usize,
}

impl LintResult {
    /// Construct a `LintResult` from a list of diagnostics.
    ///
    /// # Arguments
    ///
    /// - `diagnostics`: the full list produced by a lint run
    ///
    /// # Returns
    ///
    /// A `LintResult` with `error_count` and `warning_count` summed from
    /// the severity of each diagnostic.
    pub fn new(diagnostics: Vec<Diagnostic>) -> Self {
        let error_count = diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .count();
        let warning_count = diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Warning)
            .count();
        Self {
            diagnostics,
            error_count,
            warning_count,
        }
    }

    /// Returns `true` if any diagnostic has error severity.
    pub fn has_errors(&self) -> bool {
        self.error_count > 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- T1: happy path ---

    #[test]
    fn rule_id_display_known_variants() {
        assert_eq!(RuleId::UndefinedVariable.to_string(), "undefined-variable");
        assert_eq!(RuleId::UndefinedGlobal.to_string(), "undefined-global");
        assert_eq!(RuleId::UndefinedField.to_string(), "undefined-field");
        assert_eq!(RuleId::UnusedVariable.to_string(), "unused-variable");
    }

    #[test]
    fn rule_id_display_other() {
        let r = RuleId::Other("some-code".to_string());
        assert_eq!(r.to_string(), "some-code");
    }

    #[test]
    fn rule_id_serialize_known_variants() {
        // Existing wire format must be preserved: plain string, no envelope.
        let s = serde_json::to_string(&RuleId::UndefinedGlobal).unwrap();
        assert_eq!(s, "\"undefined-global\"");
        let s = serde_json::to_string(&RuleId::UnusedVariable).unwrap();
        assert_eq!(s, "\"unused-variable\"");
    }

    // --- T2: edge / boundary cases ---

    #[test]
    fn rule_id_other_empty_string() {
        let r = RuleId::Other(String::new());
        assert_eq!(r.to_string(), "");
        let s = serde_json::to_string(&r).unwrap();
        assert_eq!(s, "\"\"");
    }

    #[test]
    fn rule_id_other_serialize_preserves_raw_string() {
        // Other must emit the inner string, not a tagged object.
        let code = "undeclared-function".to_string();
        let r = RuleId::Other(code.clone());
        let s = serde_json::to_string(&r).unwrap();
        assert_eq!(s, format!("\"{}\"", code));
    }

    #[test]
    fn lint_result_counts_from_diagnostics() {
        let diags = vec![
            Diagnostic {
                rule: RuleId::UndefinedGlobal,
                severity: Severity::Error,
                message: "err".to_string(),
                line: 1,
                column: 1,
            },
            Diagnostic {
                rule: RuleId::UnusedVariable,
                severity: Severity::Warning,
                message: "warn".to_string(),
                line: 2,
                column: 1,
            },
        ];
        let result = LintResult::new(diags);
        assert_eq!(result.error_count, 1);
        assert_eq!(result.warning_count, 1);
        assert!(result.has_errors());
    }

    // --- T3: error path ---

    #[test]
    fn lint_result_empty_has_no_errors() {
        let result = LintResult::new(vec![]);
        assert_eq!(result.error_count, 0);
        assert_eq!(result.warning_count, 0);
        assert!(!result.has_errors());
    }

    #[test]
    fn rule_id_clone_works_for_other() {
        // Other(String) must be Clone since Copy is removed.
        let r = RuleId::Other("x".to_string());
        let r2 = r.clone();
        assert_eq!(r, r2);
    }

    #[test]
    fn rule_id_eq_other_distinguishes_by_content() {
        let a = RuleId::Other("a".to_string());
        let b = RuleId::Other("b".to_string());
        let a2 = RuleId::Other("a".to_string());
        assert_ne!(a, b);
        assert_eq!(a, a2);
    }
}
