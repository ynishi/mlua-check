//! Top-level lint engine that coordinates emmylua analysis and result
//! aggregation.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use emmylua_code_analysis::EmmyLuaAnalysis;
use tokio_util::sync::CancellationToken;

use crate::config::LintConfig;
use crate::diagnostic_bridge;
use crate::emmyrc_bridge;
use crate::globals_bridge;
use crate::symbols::SymbolTable;
use crate::types::LintResult;

/// Main entry point for linting Lua source code.
///
/// Internally delegates to `emmylua_code_analysis` for all diagnostic
/// judgement.  Custom globals registered via [`SymbolTable`] are injected
/// as a `---@meta` virtual stub file so the analyzer treats them as known
/// (Plan B-option, Crux #2: single-path custom globals bridge).
///
/// # Example
///
/// ```
/// use mlua_check::{LintEngine, LintConfig, LintPolicy};
///
/// let mut engine = LintEngine::new();
/// engine.symbols_mut().add_global("alc");
/// engine.symbols_mut().add_global_field("alc", "llm");
///
/// let result = engine.lint("alc.llm('hello')", "@main.lua");
/// assert_eq!(result.diagnostics.len(), 0);
/// ```
#[derive(Debug, Clone)]
pub struct LintEngine {
    /// Custom globals registered from mlua VM introspection.  Retained as a
    /// thin intermediate representation for building the `---@meta` stub.
    symbols: SymbolTable,
    config: LintConfig,
    /// emmylua analysis instance.  `Arc<Mutex<_>>` because `update_file_by_path`
    /// requires `&mut self`, so interior mutability is needed for `Arc`-shared
    /// access.  `Clone` is cheap via `Arc::clone`.
    analysis: Arc<Mutex<EmmyLuaAnalysis>>,
}

impl LintEngine {
    /// Create a new engine with Lua 5.4 stdlib pre-populated and default
    /// config (policy: Warn).
    pub fn new() -> Self {
        Self::with_config(LintConfig::default())
    }

    /// Create with a specific config.
    pub fn with_config(config: LintConfig) -> Self {
        let mut analysis = EmmyLuaAnalysis::new();
        let emmyrc = emmyrc_bridge::build_emmyrc(&config);
        analysis.update_config(emmyrc);
        analysis.init_std_lib(None);
        // Register /virtual as the main workspace so that files injected via
        // update_file_by_path at /virtual/... are treated as workspace members
        // and receive full diagnostic coverage (including UndefinedGlobal).
        analysis.add_main_workspace(PathBuf::from("/virtual"));
        Self {
            symbols: SymbolTable::new(),
            config,
            analysis: Arc::new(Mutex::new(analysis)),
        }
    }

    /// Mutable access to the symbol table for registration.
    pub fn symbols_mut(&mut self) -> &mut SymbolTable {
        &mut self.symbols
    }

    /// Immutable access to the symbol table.
    pub fn symbols(&self) -> &SymbolTable {
        &self.symbols
    }

    /// Mutable access to config.
    pub fn config_mut(&mut self) -> &mut LintConfig {
        &mut self.config
    }

    /// Run all enabled lint rules on the given source code.
    ///
    /// `chunk_name` is used as the virtual file name inside emmylua (e.g.
    /// `"@main.lua"` → `/virtual/main.lua`).  Leading `@` is stripped.
    pub fn lint(&self, source: &str, chunk_name: &str) -> LintResult {
        // (1) Build ---@meta stub from registered custom globals.
        let stub_text = globals_bridge::build_meta_stub(&self.symbols);

        // Virtual paths: stub and user file.  Using fixed stub path avoids
        // repeated FileId churn across multiple lint() calls on the same engine.
        let stub_path = PathBuf::from("/virtual/__mlua_check_globals.lua");
        let bare_name = chunk_name.trim_start_matches('@');
        let user_path = PathBuf::from(format!("/virtual/{bare_name}"));

        let user_file_id = {
            let mut guard = self
                .analysis
                .lock()
                .expect("emmylua analysis mutex poisoned");

            // (2a) Inject custom globals stub.
            guard.update_file_by_path(&stub_path, Some(stub_text));

            // (2b) Inject the user source.
            let file_id = guard.update_file_by_path(&user_path, Some(source.to_string()));

            match file_id {
                Some(id) => id,
                None => return LintResult::new(vec![]),
            }
        };

        // (3) Run diagnostics.  diagnose_file takes &self so we can release
        //     the write lock first; re-acquire as read via Arc.
        let lsp_diags = {
            let guard = self
                .analysis
                .lock()
                .expect("emmylua analysis mutex poisoned");
            guard
                .diagnose_file(user_file_id, CancellationToken::new())
                .unwrap_or_default()
        };

        // (4) Map lsp_types::Diagnostic → crate::Diagnostic.
        let mut diagnostics: Vec<_> = lsp_diags
            .into_iter()
            .map(|d| diagnostic_bridge::map_diagnostic(d, source))
            .collect();

        // (5) Sort by line then column for stable output.
        diagnostics.sort_by(|a, b| a.line.cmp(&b.line).then(a.column.cmp(&b.column)));

        LintResult::new(diagnostics)
    }
}

impl Default for LintEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn engine_detects_undefined_global() {
        let engine = LintEngine::new();
        let result = engine.lint("unknown_func()", "@test.lua");
        assert!(
            result.error_count > 0,
            "expected error for unknown_func (UndefinedGlobal is ERROR severity)"
        );
        assert!(
            result.diagnostics[0].message.contains("unknown_func"),
            "message should mention unknown_func, got: {}",
            result.diagnostics[0].message
        );
    }

    #[test]
    fn engine_with_custom_globals() {
        let mut engine = LintEngine::new();
        engine.symbols_mut().add_global("alc");
        engine.symbols_mut().add_global_field("alc", "llm");

        let result = engine.lint("alc.llm('hello')", "@test.lua");
        assert_eq!(
            result.diagnostics.len(),
            0,
            "alc.llm should be known, got: {:?}",
            result.diagnostics
        );

        let result = engine.lint("alc.llm_call('hello')", "@test.lua");
        assert_eq!(
            result.diagnostics.len(),
            1,
            "alc.llm_call should be unknown"
        );
        assert!(
            result.diagnostics[0].message.contains("llm_call"),
            "message should mention llm_call"
        );
    }

    #[test]
    fn engine_empty_code_no_errors() {
        let engine = LintEngine::new();
        let result = engine.lint("", "@test.lua");
        assert_eq!(result.diagnostics.len(), 0);
    }

    #[test]
    fn engine_detects_unused_variable() {
        let engine = LintEngine::new();
        let result = engine.lint("local unused = 42\nprint('hi')", "@test.lua");
        let unused: Vec<_> = result
            .diagnostics
            .iter()
            .filter(|d| d.rule == crate::types::RuleId::UnusedVariable)
            .collect();
        assert_eq!(
            unused.len(),
            1,
            "expected 1 UnusedVariable diagnostic, got: {:?}",
            result.diagnostics
        );
        assert!(unused[0].message.contains("unused") || unused[0].message.contains("local"));
    }

    #[test]
    fn engine_scoped_local_out_of_scope() {
        let engine = LintEngine::new();
        let result = engine.lint("do\n  local x = 1\nend\nprint(x)", "@test.lua");
        let globals: Vec<_> = result
            .diagnostics
            .iter()
            .filter(|d| d.rule == crate::types::RuleId::UndefinedGlobal)
            .collect();
        // x is out of scope after do...end
        assert_eq!(globals.len(), 1, "diagnostics: {globals:?}");
    }
}
