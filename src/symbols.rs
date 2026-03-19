use std::collections::{HashMap, HashSet};

/// Known global symbols and their fields, populated from Lua VM
/// introspection or manual registration.
#[derive(Debug, Clone, Default)]
pub struct SymbolTable {
    /// Top-level global names (e.g. `print`, `alc`, `table`).
    globals: HashSet<String>,
    /// First-level fields of global tables (e.g. `alc` → {`llm`, `state`}).
    global_fields: HashMap<String, HashSet<String>>,
    /// LuaCats `---@class` definitions: class name → known field names.
    class_fields: HashMap<String, HashSet<String>>,
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

    /// Register a `---@class` definition (creates an empty field set if new).
    pub fn add_class(&mut self, class_name: &str) {
        self.class_fields.entry(class_name.to_string()).or_default();
    }

    /// Register a field on a `---@class`.
    pub fn add_class_field(&mut self, class_name: &str, field: &str) {
        self.class_fields
            .entry(class_name.to_string())
            .or_default()
            .insert(field.to_string());
    }

    /// Check whether a class is known.
    pub fn has_class(&self, class_name: &str) -> bool {
        self.class_fields.contains_key(class_name)
    }

    /// Check whether a field exists on a class.
    pub fn has_class_field(&self, class_name: &str, field: &str) -> bool {
        self.class_fields
            .get(class_name)
            .is_some_and(|fields| fields.contains(field))
    }

    /// Get all known fields for a class (for "did you mean?" suggestions).
    pub fn class_fields_for(&self, class_name: &str) -> Option<&HashSet<String>> {
        self.class_fields.get(class_name)
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
