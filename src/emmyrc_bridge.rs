//! Converts [`crate::config::LintConfig`] into an `Arc<Emmyrc>` that
//! `EmmyLuaAnalysis::update_config` accepts.
//!
//! Responsibility: **conversion only**.  No diagnostic judgement logic lives
//! here (Crux #3: thin-wrapper principle).  The 1-to-1 field mapping of
//! `LintConfig.rule_severity` → `EmmyrcDiagnostic.severity` is the sole
//! concern of this module.

use std::sync::Arc;

use emmylua_code_analysis::{DiagnosticCode, DiagnosticSeveritySetting, Emmyrc};

use crate::config::{LintConfig, LintPolicy};
use crate::types::{RuleId, Severity};

/// Build an `Arc<Emmyrc>` from a [`LintConfig`].
///
/// The returned `Emmyrc` is constructed with `Emmyrc::default()` as the base
/// and the following fields overridden:
///
/// - `diagnostics.enable`: `false` when `config.policy` is `LintPolicy::Off`,
///   `true` otherwise.
/// - `diagnostics.severity`: populated from `config.rule_severity` via a
///   1-to-1 `RuleId` → `DiagnosticCode` mapping.
///
/// # Arguments
///
/// - `config`: the caller-supplied lint configuration.
///
/// # Returns
///
/// An `Arc`-wrapped `Emmyrc` ready to pass to
/// `EmmyLuaAnalysis::update_config`.
pub fn build_emmyrc(config: &LintConfig) -> Arc<Emmyrc> {
    let mut emmyrc = Emmyrc::default();

    // Map LintPolicy::Off → disable all diagnostics.
    emmyrc.diagnostics.enable = config.policy != LintPolicy::Off;

    // 1-to-1: LintConfig.rule_severity → EmmyrcDiagnostic.severity.
    for (rule_id, &severity) in &config.rule_severity {
        if let Some(code) = rule_id_to_diagnostic_code(rule_id) {
            emmyrc
                .diagnostics
                .severity
                .insert(code, severity_to_setting(severity));
        }
        // RuleId::UndefinedVariable and RuleId::Other(_) have no direct
        // DiagnosticCode equivalent; they are silently skipped to keep the
        // conversion thin (Crux #3).
    }

    Arc::new(emmyrc)
}

/// Convert a [`RuleId`] to the corresponding `DiagnosticCode`.
///
/// # Arguments
///
/// - `rule`: the rule identifier to convert.
///
/// # Returns
///
/// `Some(DiagnosticCode)` for the three well-known mappings, `None` for
/// variants without a direct emmylua counterpart.
fn rule_id_to_diagnostic_code(rule: &RuleId) -> Option<DiagnosticCode> {
    match rule {
        RuleId::UndefinedGlobal => Some(DiagnosticCode::UndefinedGlobal),
        RuleId::UndefinedField => Some(DiagnosticCode::UndefinedField),
        // emmylua's "unused" code covers unused locals
        RuleId::UnusedVariable => Some(DiagnosticCode::Unused),
        // No exact emmylua equivalent for UndefinedVariable or Other
        RuleId::UndefinedVariable | RuleId::Other(_) => None,
    }
}

/// Convert a [`Severity`] to a `DiagnosticSeveritySetting`.
///
/// # Arguments
///
/// - `severity`: the severity level to convert.
///
/// # Returns
///
/// The matching `DiagnosticSeveritySetting` variant.
fn severity_to_setting(severity: Severity) -> DiagnosticSeveritySetting {
    match severity {
        Severity::Error => DiagnosticSeveritySetting::Error,
        Severity::Warning => DiagnosticSeveritySetting::Warning,
        Severity::Info => DiagnosticSeveritySetting::Information,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{LintConfig, LintPolicy};
    use crate::types::{RuleId, Severity};

    // --- T1: happy path (required by Acceptance Criteria) ---

    #[test]
    fn default_config_builds_emmyrc() {
        // build_emmyrc(&LintConfig::default()) must not panic and must return
        // an Arc<Emmyrc>.
        let arc = build_emmyrc(&LintConfig::default());
        // Default policy is Warn → diagnostics enabled.
        assert!(arc.diagnostics.enable);
        // Default rule_severity is empty → no severity overrides.
        assert!(arc.diagnostics.severity.is_empty());
    }

    #[test]
    fn strict_policy_keeps_diagnostics_enabled() {
        let config = LintConfig::default().with_policy(LintPolicy::Strict);
        let arc = build_emmyrc(&config);
        assert!(arc.diagnostics.enable);
    }

    #[test]
    fn off_policy_disables_diagnostics() {
        let config = LintConfig::default().with_policy(LintPolicy::Off);
        let arc = build_emmyrc(&config);
        assert!(!arc.diagnostics.enable);
    }

    #[test]
    fn rule_severity_undefined_global_maps_to_emmyrc() {
        use std::collections::HashMap;
        let mut severity_map = HashMap::new();
        severity_map.insert(RuleId::UndefinedGlobal, Severity::Error);
        let config = LintConfig {
            policy: LintPolicy::Warn,
            rule_severity: severity_map,
        };
        let arc = build_emmyrc(&config);
        let setting = arc.diagnostics.severity.get(&DiagnosticCode::UndefinedGlobal);
        assert!(
            matches!(setting, Some(DiagnosticSeveritySetting::Error)),
            "expected Error, got {:?}",
            setting
        );
    }

    #[test]
    fn rule_severity_unused_variable_maps_to_unused_code() {
        use std::collections::HashMap;
        let mut severity_map = HashMap::new();
        severity_map.insert(RuleId::UnusedVariable, Severity::Info);
        let config = LintConfig {
            policy: LintPolicy::Warn,
            rule_severity: severity_map,
        };
        let arc = build_emmyrc(&config);
        let setting = arc.diagnostics.severity.get(&DiagnosticCode::Unused);
        assert!(
            matches!(setting, Some(DiagnosticSeveritySetting::Information)),
            "expected Information, got {:?}",
            setting
        );
    }

    // --- T2: boundary / edge cases ---

    #[test]
    fn other_rule_id_is_silently_skipped() {
        // RuleId::Other and RuleId::UndefinedVariable have no DiagnosticCode
        // equivalent; severity map should remain empty.
        use std::collections::HashMap;
        let mut severity_map = HashMap::new();
        severity_map.insert(RuleId::Other("future-code".to_string()), Severity::Warning);
        severity_map.insert(RuleId::UndefinedVariable, Severity::Error);
        let config = LintConfig {
            policy: LintPolicy::Warn,
            rule_severity: severity_map,
        };
        let arc = build_emmyrc(&config);
        assert!(
            arc.diagnostics.severity.is_empty(),
            "expected empty severity map, got {:?}",
            arc.diagnostics.severity
        );
    }

    #[test]
    fn all_severity_variants_map_correctly() {
        assert!(matches!(
            severity_to_setting(Severity::Error),
            DiagnosticSeveritySetting::Error
        ));
        assert!(matches!(
            severity_to_setting(Severity::Warning),
            DiagnosticSeveritySetting::Warning
        ));
        assert!(matches!(
            severity_to_setting(Severity::Info),
            DiagnosticSeveritySetting::Information
        ));
    }

    // --- T3: error path ---

    #[test]
    fn undefined_field_severity_override_present_in_output() {
        use std::collections::HashMap;
        let mut severity_map = HashMap::new();
        severity_map.insert(RuleId::UndefinedField, Severity::Warning);
        let config = LintConfig {
            policy: LintPolicy::Strict,
            rule_severity: severity_map,
        };
        let arc = build_emmyrc(&config);
        // Policy is Strict → enabled
        assert!(arc.diagnostics.enable);
        let setting = arc.diagnostics.severity.get(&DiagnosticCode::UndefinedField);
        assert!(
            matches!(setting, Some(DiagnosticSeveritySetting::Warning)),
            "expected Warning, got {:?}",
            setting
        );
    }
}
