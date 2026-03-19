use std::collections::HashMap;

/// Tracks variable definitions and references across nested scopes.
///
/// Each scope level maintains its own set of local definitions.
/// Variable lookups walk outward from the innermost scope.
#[derive(Debug)]
pub struct ScopeStack {
    /// Stack of scopes, innermost last.
    scopes: Vec<Scope>,
}

#[derive(Debug)]
struct Scope {
    /// Local variable names defined in this scope → (line, column, referenced).
    locals: HashMap<String, LocalDef>,
}

#[derive(Debug)]
pub struct LocalDef {
    pub line: usize,
    pub column: usize,
    pub referenced: bool,
    /// Optional LuaCats class type (from `---@param` or `---@type`).
    pub class_type: Option<String>,
}

impl Default for ScopeStack {
    fn default() -> Self {
        Self::new()
    }
}

impl ScopeStack {
    pub fn new() -> Self {
        Self {
            scopes: vec![Scope {
                locals: HashMap::new(),
            }],
        }
    }

    /// Enter a new scope (function body, block, etc.).
    pub fn push_scope(&mut self) {
        self.scopes.push(Scope {
            locals: HashMap::new(),
        });
    }

    /// Leave the current scope. Returns unreferenced locals for
    /// unused-variable detection.
    pub fn pop_scope(&mut self) -> Vec<(String, LocalDef)> {
        let scope = self.scopes.pop().unwrap_or(Scope {
            locals: HashMap::new(),
        });
        scope
            .locals
            .into_iter()
            .filter(|(_, def)| !def.referenced)
            .collect()
    }

    /// Define a local variable in the current scope.
    pub fn define_local(&mut self, name: &str, line: usize, column: usize) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.locals.insert(
                name.to_string(),
                LocalDef {
                    line,
                    column,
                    referenced: false,
                    class_type: None,
                },
            );
        }
    }

    /// Define a local variable with an associated LuaCats class type.
    pub fn define_local_typed(
        &mut self,
        name: &str,
        line: usize,
        column: usize,
        class_type: String,
    ) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.locals.insert(
                name.to_string(),
                LocalDef {
                    line,
                    column,
                    referenced: false,
                    class_type: Some(class_type),
                },
            );
        }
    }

    /// Look up the class type of a local variable (if any).
    pub fn class_type_of(&self, name: &str) -> Option<&str> {
        for scope in self.scopes.iter().rev() {
            if let Some(def) = scope.locals.get(name) {
                return def.class_type.as_deref();
            }
        }
        None
    }

    /// Check whether a name is defined in any enclosing scope.
    /// If found, marks it as referenced.
    pub fn resolve_and_mark(&mut self, name: &str) -> bool {
        for scope in self.scopes.iter_mut().rev() {
            if let Some(def) = scope.locals.get_mut(name) {
                def.referenced = true;
                return true;
            }
        }
        false
    }

    /// Check whether a name is defined without marking it.
    pub fn is_defined(&self, name: &str) -> bool {
        self.scopes
            .iter()
            .rev()
            .any(|scope| scope.locals.contains_key(name))
    }
}
