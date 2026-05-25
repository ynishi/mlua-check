use std::collections::{HashMap, HashSet};

/// Known global symbols and their fields, populated from Lua VM
/// introspection or manual registration.
#[derive(Debug, Clone, Default)]
pub struct SymbolTable {
    /// Top-level global names (e.g. `print`, `alc`, `table`).
    globals: HashSet<String>,
    /// First-level fields of global tables (e.g. `alc` → {`llm`, `state`}).
    global_fields: HashMap<String, HashSet<String>>,
}

impl SymbolTable {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a top-level global name.
    pub fn add_global(&mut self, name: &str) {
        self.globals.insert(name.to_string());
    }

    /// Register a field of a global table.
    pub fn add_global_field(&mut self, table: &str, field: &str) {
        self.global_fields
            .entry(table.to_string())
            .or_default()
            .insert(field.to_string());
    }

    /// Check whether a top-level global name is known.
    pub fn has_global(&self, name: &str) -> bool {
        self.globals.contains(name)
    }

    /// Check whether a field of a global table is known.
    pub fn has_global_field(&self, table: &str, field: &str) -> bool {
        self.global_fields
            .get(table)
            .is_some_and(|fields| fields.contains(field))
    }

    /// Get all known fields for a global table (for "did you mean?"
    /// suggestions).
    pub fn global_fields_for(&self, table: &str) -> Option<&HashSet<String>> {
        self.global_fields.get(table)
    }

    /// Iterate over all top-level global names.
    ///
    /// # Returns
    ///
    /// An iterator yielding each registered global name as a `&str`.
    pub fn globals_iter(&self) -> impl Iterator<Item = &str> {
        self.globals.iter().map(String::as_str)
    }

    /// Iterate over all global table names that have registered fields.
    ///
    /// # Returns
    ///
    /// An iterator yielding each table name (key in `global_fields`) as a
    /// `&str`.
    pub fn global_tables_iter(&self) -> impl Iterator<Item = &str> {
        self.global_fields.keys().map(String::as_str)
    }

    /// Iterate over the known fields of a global table.
    ///
    /// # Arguments
    ///
    /// - `table`: the name of the global table.
    ///
    /// # Returns
    ///
    /// `Some(iterator)` when the table has registered fields, `None`
    /// otherwise.
    pub fn global_fields_iter_for<'a>(
        &'a self,
        table: &str,
    ) -> Option<impl Iterator<Item = &'a str>> {
        self.global_fields
            .get(table)
            .map(|set| set.iter().map(String::as_str))
    }

    /// Pre-populate with Lua 5.4 standard library globals.
    pub fn with_lua54_stdlib(mut self) -> Self {
        let stdlib_globals = [
            "assert",
            "collectgarbage",
            "dofile",
            "error",
            "getmetatable",
            "ipairs",
            "load",
            "loadfile",
            "next",
            "pairs",
            "pcall",
            "print",
            "rawequal",
            "rawget",
            "rawlen",
            "rawset",
            "require",
            "select",
            "setmetatable",
            "tonumber",
            "tostring",
            "type",
            "warn",
            "xpcall",
            // standard library tables
            "coroutine",
            "debug",
            "io",
            "math",
            "os",
            "package",
            "string",
            "table",
            "utf8",
            // globals
            "_G",
            "_VERSION",
        ];
        for name in &stdlib_globals {
            self.globals.insert((*name).to_string());
        }
        self
    }
}
