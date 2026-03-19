//! Top-level lint engine that coordinates parsing, rule execution, and result
//! aggregation.

use crate::config::LintConfig;
use crate::symbols::SymbolTable;
use crate::types::LintResult;
use crate::walker;

/// Main entry point for linting Lua source code.
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
/// let result = engine.lint("alc.llm_call('hello')", "@main.lua");
/// assert!(result.has_errors() || result.warning_count > 0);
/// ```
#[derive(Debug, Clone)]
pub struct LintEngine {
    symbols: SymbolTable,
    config: LintConfig,
}

impl LintEngine {
    /// Create a new engine with Lua 5.4 stdlib pre-populated and default
    /// config (policy: Warn).
    pub fn new() -> Self {
        Self {
            symbols: SymbolTable::new().with_lua54_stdlib(),
            config: LintConfig::default(),
        }
    }

    /// Create with a specific config.
    pub fn with_config(config: LintConfig) -> Self {
        Self {
            symbols: SymbolTable::new().with_lua54_stdlib(),
            config,
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
    /// `_chunk_name` is reserved for future use in multi-file analysis.
    pub fn lint(&self, source: &str, _chunk_name: &str) -> LintResult {
        // Single-pass scope-aware walk: collects UndefinedGlobal and
        // UnusedVariable diagnostics with proper lexical scoping.
        let mut all_diagnostics = walker::walk(source, &self.symbols, &self.config);

        // Sort by line, then column for stable output
        all_diagnostics.sort_by(|a, b| a.line.cmp(&b.line).then(a.column.cmp(&b.column)));

        LintResult::new(all_diagnostics)
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
        assert_eq!(result.warning_count, 1);
        assert!(result.diagnostics[0].message.contains("unknown_func"));
    }

    #[test]
    fn engine_with_custom_globals() {
        let mut engine = LintEngine::new();
        engine.symbols_mut().add_global("alc");
        engine.symbols_mut().add_global_field("alc", "llm");

        let result = engine.lint("alc.llm('hello')", "@test.lua");
        assert_eq!(result.diagnostics.len(), 0);

        let result = engine.lint("alc.llm_call('hello')", "@test.lua");
        assert_eq!(result.diagnostics.len(), 1);
        assert!(result.diagnostics[0].message.contains("llm_call"));
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
        assert_eq!(unused.len(), 1);
        assert!(unused[0].message.contains("unused"));
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
