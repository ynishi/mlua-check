//! mlua VM integration — automatic symbol table construction from live VM state.
//!
//! Walks `lua.globals()` to populate a [`SymbolTable`] with the actual globals
//! and their first-level fields, so that the linter knows exactly what is
//! available at runtime without manual registration.

use mlua::prelude::*;

use crate::config::LintConfig;
use crate::engine::LintEngine;
use crate::symbols::SymbolTable;
use crate::types::LintResult;

/// Build a [`SymbolTable`] by introspecting the live Lua VM globals.
///
/// Walks `lua.globals()` and registers:
/// - Every top-level key as a global name
/// - For globals that are tables, every first-level string key as a field
///
/// The Lua 5.4 stdlib names are *not* added separately — they are already
/// present in `lua.globals()` for a standard `Lua::new()` VM.
pub fn collect_symbols(lua: &Lua) -> LuaResult<SymbolTable> {
    let mut symbols = SymbolTable::new();
    let globals = lua.globals();

    for pair in globals.pairs::<String, LuaValue>() {
        let (key, value) = pair?;
        symbols.add_global(&key);

        // If the value is a table, register first-level fields
        if let LuaValue::Table(tbl) = value {
            for field_pair in tbl.pairs::<String, LuaValue>() {
                let (field_key, _) = field_pair?;
                symbols.add_global_field(&key, &field_key);
            }
        }
    }

    Ok(symbols)
}

/// Register the linter on an existing Lua VM.
///
/// Introspects `lua.globals()` to build the symbol table automatically.
/// Returns a configured [`LintEngine`] ready to lint code against this VM's
/// environment.
///
/// # Example
///
/// ```rust
/// use mlua::prelude::*;
/// use mlua_check::register;
///
/// let lua = Lua::new();
/// // Register custom globals
/// lua.globals().set("my_api", lua.create_table().unwrap()).unwrap();
///
/// let engine = register(&lua).unwrap();
/// let result = engine.lint("my_api.something()", "@test.lua");
/// // my_api is known, but "something" field was not registered on the table
/// ```
pub fn register(lua: &Lua) -> LuaResult<LintEngine> {
    register_with_config(lua, LintConfig::default())
}

/// Register with a custom [`LintConfig`].
pub fn register_with_config(lua: &Lua, config: LintConfig) -> LuaResult<LintEngine> {
    let symbols = collect_symbols(lua)?;
    let mut engine = LintEngine::with_config(config);
    *engine.symbols_mut() = symbols;
    Ok(engine)
}

/// Prepend entries to Lua's `package.path` so that `require` can find
/// modules in project-specific directories.
///
/// For each directory in `search_paths`, two patterns are added:
/// `<dir>/?.lua` and `<dir>/?/init.lua`.
fn prepend_search_paths(lua: &Lua, search_paths: &[&str]) -> Result<(), String> {
    if search_paths.is_empty() {
        return Ok(());
    }
    let package: LuaTable = lua
        .globals()
        .get("package")
        .map_err(|e| format!("Failed to get package table: {e}"))?;
    let current: String = package
        .get("path")
        .map_err(|e| format!("Failed to get package.path: {e}"))?;

    let mut prefix = String::new();
    for dir in search_paths {
        let dir = dir.trim_end_matches('/');
        prefix.push_str(dir);
        prefix.push_str("/?.lua;");
        prefix.push_str(dir);
        prefix.push_str("/?/init.lua;");
    }
    prefix.push_str(&current);

    package
        .set("path", prefix)
        .map_err(|e| format!("Failed to set package.path: {e}"))?;
    Ok(())
}

/// One-shot lint: creates a fresh Lua VM, collects stdlib symbols, and lints.
///
/// `search_paths` is prepended to `package.path` so that the VM can
/// resolve project-specific modules when building the symbol table.
/// Pass `&[]` when no extra paths are needed.
///
/// ```rust
/// let result = mlua_check::run_lint("print('hello')", "@test.lua", &[]).unwrap();
/// assert_eq!(result.diagnostics.len(), 0);
/// ```
pub fn run_lint(code: &str, chunk_name: &str, search_paths: &[&str]) -> Result<LintResult, String> {
    let lua = Lua::new();
    prepend_search_paths(&lua, search_paths)?;
    let engine = register(&lua).map_err(|e| format!("Failed to collect VM symbols: {e}"))?;
    Ok(engine.lint(code, chunk_name))
}

/// Lint code against an existing VM's environment.
///
/// Convenience wrapper that calls [`register`] and then [`LintEngine::lint`].
///
/// ```rust
/// use mlua::prelude::*;
///
/// let lua = Lua::new();
/// lua.globals().set("custom_fn",
///     lua.create_function(|_, ()| Ok(42)).unwrap()
/// ).unwrap();
///
/// let result = mlua_check::lint(&lua, "custom_fn()", "@test.lua").unwrap();
/// assert_eq!(result.diagnostics.len(), 0);
/// ```
pub fn lint(lua: &Lua, code: &str, chunk_name: &str) -> LuaResult<LintResult> {
    let engine = register(lua)?;
    Ok(engine.lint(code, chunk_name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collect_symbols_includes_stdlib() {
        let lua = Lua::new();
        let symbols = collect_symbols(&lua).unwrap();
        // Standard Lua globals should be present
        assert!(symbols.has_global("print"));
        assert!(symbols.has_global("table"));
        assert!(symbols.has_global("string"));
        assert!(symbols.has_global("math"));
    }

    #[test]
    fn collect_symbols_includes_table_fields() {
        let lua = Lua::new();
        let symbols = collect_symbols(&lua).unwrap();
        // table.insert, string.format, etc.
        assert!(symbols.has_global_field("table", "insert"));
        assert!(symbols.has_global_field("string", "format"));
        assert!(symbols.has_global_field("math", "floor"));
    }

    #[test]
    fn collect_symbols_includes_custom_globals() {
        let lua = Lua::new();
        let tbl = lua.create_table().unwrap();
        tbl.set("llm", lua.create_function(|_, ()| Ok(())).unwrap())
            .unwrap();
        tbl.set("state", lua.create_function(|_, ()| Ok(())).unwrap())
            .unwrap();
        lua.globals().set("alc", tbl).unwrap();

        let symbols = collect_symbols(&lua).unwrap();
        assert!(symbols.has_global("alc"));
        assert!(symbols.has_global_field("alc", "llm"));
        assert!(symbols.has_global_field("alc", "state"));
    }

    #[test]
    fn register_creates_working_engine() {
        let lua = Lua::new();
        let engine = register(&lua).unwrap();

        // print is known
        let result = engine.lint("print('hello')", "@test.lua");
        assert_eq!(result.diagnostics.len(), 0);

        // unknown_func is not
        let result = engine.lint("unknown_func()", "@test.lua");
        assert!(result.warning_count > 0);
    }

    #[test]
    fn run_lint_detects_undefined() {
        let result = run_lint("unknown_func()", "@test.lua", &[]).unwrap();
        assert!(result.warning_count > 0);
        assert!(result.diagnostics[0].message.contains("unknown_func"));
    }

    #[test]
    fn run_lint_allows_stdlib() {
        let result = run_lint("print(table.insert)", "@test.lua", &[]).unwrap();
        assert_eq!(result.diagnostics.len(), 0);
    }

    #[test]
    fn lint_with_custom_globals() {
        let lua = Lua::new();
        let tbl = lua.create_table().unwrap();
        tbl.set("llm", lua.create_function(|_, ()| Ok(())).unwrap())
            .unwrap();
        lua.globals().set("alc", tbl).unwrap();

        // alc.llm is known
        let result = lint(&lua, "alc.llm('hello')", "@test.lua").unwrap();
        assert_eq!(result.diagnostics.len(), 0);

        // alc.unknown is not
        let result = lint(&lua, "alc.unknown('hello')", "@test.lua").unwrap();
        assert!(!result.diagnostics.is_empty());
    }
}
