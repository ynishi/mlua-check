use serde::Serialize;
use std::fmt;

/// Unique identifier for each lint rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RuleId {
    /// Reference to a variable not defined in any enclosing scope.
    UndefinedVariable,
    /// Reference to a global name not in the known symbol table.
    UndefinedGlobal,
    /// Access to a field not declared in a `---@class` definition.
    UndefinedField,
    /// A local variable that is declared but never referenced.
    UnusedVariable,
}

impl fmt::Display for RuleId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UndefinedVariable => write!(f, "undefined-variable"),
            Self::UndefinedGlobal => write!(f, "undefined-global"),
            Self::UndefinedField => write!(f, "undefined-field"),
            Self::UnusedVariable => write!(f, "unused-variable"),
        }
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
#[derive(Debug, Clone, Serialize)]
pub struct Diagnostic {
    pub rule: RuleId,
    pub severity: Severity,
    pub message: String,
    pub line: usize,
    pub column: usize,
}

/// Aggregated lint result.
#[derive(Debug, Clone, Serialize)]
pub struct LintResult {
    pub diagnostics: Vec<Diagnostic>,
    pub error_count: usize,
    pub warning_count: usize,
}

impl LintResult {
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
