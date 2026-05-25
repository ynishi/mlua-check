//! Generates a `---@meta` LuaCats stub string from a
//! [`crate::symbols::SymbolTable`].
//!
//! Responsibility: **string generation only**.  No type inference or
//! diagnostic logic lives here (Crux #3: thin-wrapper principle).  The stub
//! is injected into `EmmyLuaAnalysis` via `update_file_by_path` so that
//! custom globals registered on the mlua VM are visible to the analyzer
//! (Crux #2 / Plan B-option: `---@meta` virtual stub injection).

/// Build a `---@meta` LuaCats stub that declares all globals in `symbols`
/// so `EmmyLuaAnalysis` treats them as known.
///
/// # Arguments
///
/// - `symbols`: the [`crate::symbols::SymbolTable`] populated from mlua VM
///   introspection or manual registration.
///
/// # Returns
///
/// A `String` containing valid LuaCats `---@meta` annotations that can be
/// passed to `EmmyLuaAnalysis::update_file_by_path` as an in-memory virtual
/// file.
///
/// # Panics
///
/// This function is a skeleton stub.  It will panic with
/// `"not yet implemented"` until the full implementation is added in ST4.
#[allow(dead_code, clippy::todo)]
pub fn build_meta_stub(symbols: &crate::symbols::SymbolTable) -> String {
    let _ = symbols;
    todo!()
}

#[cfg(test)]
mod tests {
    // Smoke test: the module compiles and the stub is wired up correctly.
    // Full unit tests are added in ST4 when the function body is filled in.
    #[test]
    fn smoke() {
        // No assertion needed — this verifies the module resolves at compile
        // time and the test harness can discover it.
    }
}
