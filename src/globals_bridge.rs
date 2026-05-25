//! Generates a `---@meta` LuaCats stub string from a
//! [`crate::symbols::SymbolTable`].
//!
//! Responsibility: **string generation only**.  No type inference or
//! diagnostic logic lives here (Crux #3: thin-wrapper principle).  The stub
//! is injected into `EmmyLuaAnalysis` via `update_file_by_path` so that
//! custom globals registered on the mlua VM are visible to the analyzer
//! (Crux #2 / Plan B-option: `---@meta` virtual stub injection).
//!
//! # Output format
//!
//! ```text
//! ---@meta
//! ---@type any
//! global_name = nil
//! ---@class TableName_Class
//! ---@field field_key any
//! TableName = nil
//! ```
//!
//! The virtual path for injection into `EmmyLuaAnalysis` is
//! `/virtual/__mlua_check_globals.lua` (recommended, fixed per `LintEngine`).

use crate::symbols::SymbolTable;

/// Build a `---@meta` LuaCats stub that declares all globals in `symbols`
/// so `EmmyLuaAnalysis` treats them as known.
///
/// # Arguments
///
/// - `symbols`: the [`SymbolTable`] populated from mlua VM introspection or
///   manual registration.
///
/// # Returns
///
/// A `String` containing valid LuaCats `---@meta` annotations that can be
/// passed to `EmmyLuaAnalysis::update_file_by_path` as an in-memory virtual
/// file at path `/virtual/__mlua_check_globals.lua`.
///
/// Returns a string starting with `---@meta\n` even when `symbols` is empty,
/// so the virtual file is always valid Lua.
pub fn build_meta_stub(symbols: &SymbolTable) -> String {
    let mut out = String::from("---@meta\n");

    // First emit table globals (those that have registered fields), so that
    // the ---@class annotation appears before the global assignment.
    let table_names: Vec<&str> = symbols.global_tables_iter().collect();
    let mut sorted_tables = table_names;
    sorted_tables.sort_unstable();

    for table_name in sorted_tables {
        let class_name = format!("{table_name}_Class");
        out.push_str(&format!("---@class {class_name}\n"));

        // Collect fields for deterministic output order.
        let mut fields: Vec<&str> = symbols
            .global_fields_iter_for(table_name)
            .map(|it| it.collect())
            .unwrap_or_default();
        fields.sort_unstable();

        for field in fields {
            out.push_str(&format!("---@field {field} any\n"));
        }

        // Declare the global itself as the class type.
        out.push_str(&format!("---@type {class_name}\n"));
        out.push_str(&format!("{table_name} = nil\n"));
    }

    // Then emit plain scalar globals (those without registered fields).
    let all_global_names: Vec<&str> = symbols.globals_iter().collect();
    let mut sorted_globals = all_global_names;
    sorted_globals.sort_unstable();

    for name in sorted_globals {
        // Skip globals already emitted as class-typed tables above.
        if symbols.global_fields_iter_for(name).is_some() {
            continue;
        }
        out.push_str(&format!("---@type any\n{name} = nil\n"));
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::symbols::SymbolTable;

    // --- T1: happy path (required by Acceptance Criteria) ---

    #[test]
    fn builds_meta_header() {
        // SymbolTable with one global → stub must start with "---@meta".
        let mut symbols = SymbolTable::new();
        symbols.add_global("myGlobal");
        let stub = build_meta_stub(&symbols);
        assert!(
            stub.starts_with("---@meta\n"),
            "stub should start with ---@meta, got: {:?}",
            &stub[..stub.len().min(30)]
        );
    }

    #[test]
    fn plain_global_appears_in_stub() {
        let mut symbols = SymbolTable::new();
        symbols.add_global("alc");
        let stub = build_meta_stub(&symbols);
        assert!(
            stub.contains("alc = nil"),
            "stub missing global declaration"
        );
        assert!(
            stub.contains("---@type any"),
            "stub missing type annotation"
        );
    }

    #[test]
    fn table_global_with_fields_uses_class_annotation() {
        let mut symbols = SymbolTable::new();
        symbols.add_global("alc");
        symbols.add_global_field("alc", "llm");
        symbols.add_global_field("alc", "state");
        let stub = build_meta_stub(&symbols);

        // Class declaration
        assert!(stub.contains("---@class alc_Class\n"), "missing ---@class");
        // Fields (order may vary, but both must be present)
        assert!(stub.contains("---@field llm any\n"), "missing llm field");
        assert!(
            stub.contains("---@field state any\n"),
            "missing state field"
        );
        // Global typed as the class
        assert!(stub.contains("---@type alc_Class\n"), "missing class type");
        assert!(stub.contains("alc = nil\n"), "missing global assignment");
    }

    // --- T2: boundary / edge cases ---

    #[test]
    fn empty_symbol_table_produces_meta_only() {
        let symbols = SymbolTable::new();
        let stub = build_meta_stub(&symbols);
        assert_eq!(stub, "---@meta\n");
    }

    #[test]
    fn table_global_not_duplicated_as_plain_global() {
        // A global that also has fields must NOT appear twice (once as class,
        // once as ---@type any).
        let mut symbols = SymbolTable::new();
        symbols.add_global("tbl");
        symbols.add_global_field("tbl", "key");
        let stub = build_meta_stub(&symbols);

        // Count occurrences of "tbl = nil"
        let count = stub.matches("tbl = nil").count();
        assert_eq!(count, 1, "global 'tbl' should appear exactly once");
        // Should NOT have a bare ---@type any for tbl
        assert!(
            !stub.contains("---@type any\ntbl = nil"),
            "tbl should not appear as plain ---@type any"
        );
    }

    #[test]
    fn multiple_globals_all_present() {
        let mut symbols = SymbolTable::new();
        symbols.add_global("foo");
        symbols.add_global("bar");
        let stub = build_meta_stub(&symbols);
        assert!(stub.contains("foo = nil"));
        assert!(stub.contains("bar = nil"));
    }

    // --- T3: error path ---

    #[test]
    fn global_with_no_fields_does_not_emit_class() {
        let mut symbols = SymbolTable::new();
        symbols.add_global("simple");
        let stub = build_meta_stub(&symbols);
        // No class annotation when there are no fields
        assert!(
            !stub.contains("---@class"),
            "should not emit ---@class for plain global"
        );
    }

    #[test]
    fn field_only_table_without_add_global_still_emits_class() {
        // add_global_field without add_global — global_tables_iter still
        // finds the table because global_fields is populated.
        let mut symbols = SymbolTable::new();
        symbols.add_global_field("pkg", "load");
        let stub = build_meta_stub(&symbols);
        assert!(stub.contains("---@class pkg_Class\n"));
        assert!(stub.contains("---@field load any\n"));
        assert!(stub.contains("pkg = nil\n"));
    }
}
