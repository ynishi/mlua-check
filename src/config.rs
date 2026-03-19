use std::collections::HashMap;

use crate::types::{RuleId, Severity};

/// Controls whether lint errors block execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LintPolicy {
    /// Lint errors block execution.
    Strict,
    /// Lint issues are reported but execution proceeds.
    Warn,
    /// Linting is disabled entirely.
    Off,
}

impl LintPolicy {
    /// Parse from a string (e.g. MCP parameter).  Returns `Warn` for
    /// unrecognised values.
    pub fn from_str_lossy(s: &str) -> Self {
        match s.to_ascii_lowercase().as_str() {
            "strict" => Self::Strict,
            "off" | "none" | "disable" => Self::Off,
            _ => Self::Warn,
        }
    }
}

/// Per-rule severity overrides and global policy.
#[derive(Debug, Clone)]
pub struct LintConfig {
    pub policy: LintPolicy,
    /// Override the default severity for individual rules.
    pub rule_severity: HashMap<RuleId, Severity>,
}

impl Default for LintConfig {
    fn default() -> Self {
        Self {
            policy: LintPolicy::Warn,
            rule_severity: HashMap::new(),
        }
    }
}

impl LintConfig {
    pub fn with_policy(mut self, policy: LintPolicy) -> Self {
        self.policy = policy;
        self
    }

    /// Resolve the effective severity for a rule.  Falls back to the
    /// provided default when no override is configured.
    pub fn severity_for(&self, rule: RuleId, default: Severity) -> Severity {
        self.rule_severity.get(&rule).copied().unwrap_or(default)
    }
}
