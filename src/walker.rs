//! AST walker with scope tracking.
//!
//! Performs a depth-first walk of the Lua AST, maintaining a scope stack
//! that mirrors Lua's lexical scoping rules.  Collects diagnostics for
//! all scope-dependent rules in a single pass:
//!
//! - **UndefinedGlobal**: NameExpr/IndexExpr referencing unknown globals
//! - **UnusedVariable**: local declarations that are never referenced

use std::collections::HashMap;

use emmylua_parser::{
    LineIndex, LuaAstNode, LuaDocFieldKey, LuaDocTagClass, LuaDocTagField, LuaDocTagParam,
    LuaDocTagType, LuaDocType, LuaIndexExpr, LuaNameExpr, LuaSyntaxKind,
};
use rowan::WalkEvent;

use crate::config::LintConfig;
use crate::scope::ScopeStack;
use crate::symbols::SymbolTable;
use crate::types::{Diagnostic, RuleId, Severity};

/// Run the scope-aware walker and return all diagnostics.
pub fn walk(source: &str, symbols: &SymbolTable, config: &LintConfig) -> Vec<Diagnostic> {
    let tree = emmylua_parser::LuaParser::parse(source, emmylua_parser::ParserConfig::default());
    let root = tree.get_red_root();
    let line_index = LineIndex::parse(source);

    let undefined_sev = config.severity_for(RuleId::UndefinedGlobal, Severity::Warning);
    let undefined_field_sev = config.severity_for(RuleId::UndefinedField, Severity::Warning);
    let unused_sev = config.severity_for(RuleId::UnusedVariable, Severity::Warning);

    // ── Pass 1: Collect all @class/@field definitions ──
    let class_defs = collect_class_definitions(&root);

    // ── Pass 2: Scope-aware analysis ──
    let mut diagnostics = Vec::new();
    let mut scopes = ScopeStack::new();
    let mut scope_depth: usize = 0;

    // Pending annotations for param/type → variable type binding
    let mut pending_param_types: HashMap<String, String> = HashMap::new();
    let mut pending_type: Option<String> = None;

    for event in root.preorder() {
        match event {
            WalkEvent::Enter(node) => {
                let kind: LuaSyntaxKind = node.kind().into();

                // ── Collect @param/@type annotations ──
                match kind {
                    LuaSyntaxKind::DocTagParam => {
                        if let Some(tag) = LuaDocTagParam::cast(node.clone()) {
                            if let Some(name_token) = tag.get_name_token() {
                                if let Some(type_name) =
                                    tag.get_type().and_then(|t| extract_name_type(&t))
                                {
                                    let param_name = name_token.get_name_text().to_string();
                                    pending_param_types.insert(param_name, type_name);
                                }
                            }
                        }
                    }
                    LuaSyntaxKind::DocTagType => {
                        if let Some(tag) = LuaDocTagType::cast(node.clone()) {
                            if let Some(first_type) = tag.get_type_list().next() {
                                if let Some(type_name) = extract_name_type(&first_type) {
                                    pending_type = Some(type_name);
                                }
                            }
                        }
                    }
                    _ => {}
                }

                // Scope-introducing nodes: push a new scope
                if is_scope_boundary(kind) {
                    scopes.push_scope();
                    scope_depth += 1;
                }

                match kind {
                    // ── Local definitions ──
                    LuaSyntaxKind::LocalName => {
                        let text = node.text().to_string().trim().to_string();
                        if !text.is_empty() {
                            let offset = node.text_range().start();
                            let (line, col) =
                                line_index.get_line_col(offset, source).unwrap_or((0, 0));
                            // Check for pending @type annotation
                            if let Some(type_name) = pending_type.take() {
                                scopes.define_local_typed(&text, line, col, type_name);
                            } else {
                                scopes.define_local(&text, line, col);
                            }
                        }
                    }
                    LuaSyntaxKind::ParamName => {
                        let text = node.text().to_string().trim().to_string();
                        if !text.is_empty() {
                            let offset = node.text_range().start();
                            let (line, col) =
                                line_index.get_line_col(offset, source).unwrap_or((0, 0));
                            // Check for pending @param type
                            if let Some(type_name) = pending_param_types.remove(&text) {
                                scopes.define_local_typed(&text, line, col, type_name);
                            } else {
                                scopes.define_local(&text, line, col);
                            }
                        }
                    }
                    LuaSyntaxKind::LocalFuncStat => {
                        // `local function foo()` — foo is a local in the
                        // *enclosing* scope (before the function body scope).
                        for child in node.children_with_tokens() {
                            if let Some(token) = child.as_token() {
                                let tk: emmylua_parser::LuaTokenKind = token.kind().into();
                                if tk == emmylua_parser::LuaTokenKind::TkName {
                                    let name = token.text().to_string();
                                    let offset = token.text_range().start();
                                    let (line, col) =
                                        line_index.get_line_col(offset, source).unwrap_or((0, 0));
                                    scopes.define_local(&name, line, col);
                                    break;
                                }
                            }
                        }
                    }
                    LuaSyntaxKind::ForStat => {
                        // `for i = 1, 10 do` — i is local to the for body
                        for child in node.children_with_tokens() {
                            if let Some(token) = child.as_token() {
                                let tk: emmylua_parser::LuaTokenKind = token.kind().into();
                                if tk == emmylua_parser::LuaTokenKind::TkName {
                                    let name = token.text().to_string();
                                    let offset = token.text_range().start();
                                    let (line, col) =
                                        line_index.get_line_col(offset, source).unwrap_or((0, 0));
                                    scopes.define_local(&name, line, col);
                                    break;
                                }
                            }
                        }
                    }

                    // ── Variable references ──
                    LuaSyntaxKind::NameExpr => {
                        if let Some(name_expr) = LuaNameExpr::cast(node.clone()) {
                            if let Some(name) = name_expr.get_name_text() {
                                // Try to resolve as local (marks as referenced)
                                if scopes.resolve_and_mark(&name) {
                                    // Local — fine
                                    continue;
                                }
                                // Skip special names
                                if is_special_name(&name) {
                                    continue;
                                }
                                // Skip if parent is IndexExpr (handled below)
                                if let Some(parent) = node.parent() {
                                    let pk: LuaSyntaxKind = parent.kind().into();
                                    if pk == LuaSyntaxKind::IndexExpr {
                                        continue;
                                    }
                                }
                                // Not local — check globals
                                if !symbols.has_global(&name) {
                                    let offset = node.text_range().start();
                                    let (line, col) =
                                        line_index.get_line_col(offset, source).unwrap_or((0, 0));
                                    diagnostics.push(Diagnostic {
                                        rule: RuleId::UndefinedGlobal,
                                        severity: undefined_sev,
                                        message: format!("Undefined global '{name}'"),
                                        line: line + 1,
                                        column: col + 1,
                                    });
                                }
                            }
                        }
                    }
                    LuaSyntaxKind::IndexExpr => {
                        let ctx = IndexCheckCtx {
                            symbols,
                            class_defs: &class_defs,
                            line_index: &line_index,
                            source,
                            global_severity: undefined_sev,
                            field_severity: undefined_field_sev,
                        };
                        check_index_expr(&node, &mut scopes, &ctx, &mut diagnostics);
                    }
                    _ => {}
                }
            }
            WalkEvent::Leave(node) => {
                let kind: LuaSyntaxKind = node.kind().into();
                if is_scope_boundary(kind) && scope_depth > 0 {
                    scope_depth -= 1;
                    let unreferenced = scopes.pop_scope();
                    for (name, def) in unreferenced {
                        // Skip `_` prefixed variables (Lua convention for
                        // intentionally unused)
                        if name.starts_with('_') {
                            continue;
                        }
                        diagnostics.push(Diagnostic {
                            rule: RuleId::UnusedVariable,
                            severity: unused_sev,
                            message: format!("Unused variable '{name}'"),
                            line: def.line + 1,
                            column: def.column + 1,
                        });
                    }
                }
            }
        }
    }

    // Pop the root scope
    let unreferenced = scopes.pop_scope();
    for (name, def) in unreferenced {
        if name.starts_with('_') {
            continue;
        }
        diagnostics.push(Diagnostic {
            rule: RuleId::UnusedVariable,
            severity: unused_sev,
            message: format!("Unused variable '{name}'"),
            line: def.line + 1,
            column: def.column + 1,
        });
    }

    diagnostics
}

/// Collect all `---@class` definitions and their `---@field` declarations
/// from the AST in a single pre-pass.
fn collect_class_definitions(
    root: &emmylua_parser::LuaSyntaxNode,
) -> HashMap<String, std::collections::HashSet<String>> {
    let mut class_defs: HashMap<String, std::collections::HashSet<String>> = HashMap::new();
    let mut pending_class: Option<String> = None;

    for event in root.preorder() {
        if let WalkEvent::Enter(node) = event {
            let kind: LuaSyntaxKind = node.kind().into();
            match kind {
                LuaSyntaxKind::DocTagClass => {
                    if let Some(tag) = LuaDocTagClass::cast(node) {
                        if let Some(name_token) = tag.get_name_token() {
                            let class_name = name_token.get_name_text().to_string();
                            class_defs.entry(class_name.clone()).or_default();
                            pending_class = Some(class_name);
                        }
                    }
                }
                LuaSyntaxKind::DocTagField => {
                    if let Some(class_name) = &pending_class {
                        if let Some(tag) = LuaDocTagField::cast(node) {
                            if let Some(LuaDocFieldKey::Name(name_token)) = tag.get_field_key() {
                                let field_name = name_token.get_name_text().to_string();
                                class_defs
                                    .entry(class_name.clone())
                                    .or_default()
                                    .insert(field_name);
                            }
                        }
                    }
                }
                // Any statement-level node ends the @class/@field sequence.
                // Doc-related and comment kinds do NOT break the sequence.
                _ => {
                    if is_statement_kind(kind) {
                        pending_class = None;
                    }
                }
            }
        }
    }

    class_defs
}

/// Check if a syntax kind represents a Lua statement (non-doc, non-comment).
fn is_statement_kind(kind: LuaSyntaxKind) -> bool {
    matches!(
        kind,
        LuaSyntaxKind::LocalStat
            | LuaSyntaxKind::AssignStat
            | LuaSyntaxKind::FuncStat
            | LuaSyntaxKind::LocalFuncStat
            | LuaSyntaxKind::CallExprStat
            | LuaSyntaxKind::DoStat
            | LuaSyntaxKind::WhileStat
            | LuaSyntaxKind::RepeatStat
            | LuaSyntaxKind::IfStat
            | LuaSyntaxKind::ForStat
            | LuaSyntaxKind::ForRangeStat
            | LuaSyntaxKind::ReturnStat
            | LuaSyntaxKind::BreakStat
            | LuaSyntaxKind::GotoStat
            | LuaSyntaxKind::LabelStat
    )
}

/// Nodes that introduce a new lexical scope in Lua.
fn is_scope_boundary(kind: LuaSyntaxKind) -> bool {
    matches!(
        kind,
        LuaSyntaxKind::ClosureExpr
            | LuaSyntaxKind::DoStat
            | LuaSyntaxKind::WhileStat
            | LuaSyntaxKind::RepeatStat
            | LuaSyntaxKind::ForStat
            | LuaSyntaxKind::ForRangeStat
            | LuaSyntaxKind::IfStat
    )
}

fn is_special_name(name: &str) -> bool {
    matches!(name, "self" | "..." | "_" | "_ENV" | "_G")
}

/// Shared context for index expression checking, avoids long parameter lists.
struct IndexCheckCtx<'a> {
    symbols: &'a SymbolTable,
    class_defs: &'a HashMap<String, std::collections::HashSet<String>>,
    line_index: &'a LineIndex,
    source: &'a str,
    global_severity: Severity,
    field_severity: Severity,
}

/// Check an IndexExpr (e.g. `alc.llm_call` or `ctx.field`) for undefined
/// globals/fields and class field access.
fn check_index_expr(
    node: &emmylua_parser::LuaSyntaxNode,
    scopes: &mut ScopeStack,
    ctx: &IndexCheckCtx<'_>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some(index_expr) = LuaIndexExpr::cast(node.clone()) else {
        return;
    };
    let Some(prefix) = index_expr.get_prefix_expr() else {
        return;
    };

    // Only handle simple `name.field` patterns (not `a.b.c`)
    let name_expr = match prefix {
        emmylua_parser::LuaExpr::NameExpr(ne) => ne,
        _ => return,
    };
    let Some(table_name) = name_expr.get_name_text() else {
        return;
    };

    // If prefix is a local, mark as referenced and check class type
    if scopes.resolve_and_mark(&table_name) {
        // Check if this local has a class type annotation
        if let Some(class_name) = scopes.class_type_of(&table_name).map(|s| s.to_string()) {
            check_class_field_access(
                node,
                &index_expr,
                &table_name,
                &class_name,
                ctx,
                diagnostics,
            );
        }
        return;
    }

    // Check if the table is a known global
    if !ctx.symbols.has_global(&table_name) {
        if is_special_name(&table_name) {
            return;
        }
        let offset = node.text_range().start();
        let (line, col) = ctx
            .line_index
            .get_line_col(offset, ctx.source)
            .unwrap_or((0, 0));
        diagnostics.push(Diagnostic {
            rule: RuleId::UndefinedGlobal,
            severity: ctx.global_severity,
            message: format!("Undefined global '{table_name}'"),
            line: line + 1,
            column: col + 1,
        });
        return;
    }

    // Table is known — check the field
    let Some(index_key) = index_expr.get_index_key() else {
        return;
    };
    if let emmylua_parser::LuaIndexKey::Name(name_token) = index_key {
        let field_name = name_token.get_name_text().to_string();
        if !ctx.symbols.has_global_field(&table_name, &field_name) {
            let offset = node.text_range().start();
            let (line, col) = ctx
                .line_index
                .get_line_col(offset, ctx.source)
                .unwrap_or((0, 0));

            let suggestion = suggest_field(ctx.symbols, &table_name, &field_name);
            let msg = if let Some(s) = suggestion {
                format!(
                    "Undefined field '{field_name}' on global '{table_name}'. Did you mean '{s}'?"
                )
            } else {
                format!("Undefined field '{field_name}' on global '{table_name}'")
            };

            diagnostics.push(Diagnostic {
                rule: RuleId::UndefinedGlobal,
                severity: ctx.global_severity,
                message: msg,
                line: line + 1,
                column: col + 1,
            });
        }
    }
}

/// Check a field access on a class-typed local variable.
fn check_class_field_access(
    node: &emmylua_parser::LuaSyntaxNode,
    index_expr: &LuaIndexExpr,
    var_name: &str,
    class_name: &str,
    ctx: &IndexCheckCtx<'_>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    // Look up class fields: first from inline @class defs, then from SymbolTable
    let inline_fields = ctx.class_defs.get(class_name);
    let symbol_fields = ctx.symbols.class_fields_for(class_name);

    // If the class is not defined anywhere, skip (no false positives)
    if inline_fields.is_none() && symbol_fields.is_none() {
        return;
    }

    let Some(index_key) = index_expr.get_index_key() else {
        return;
    };
    if let emmylua_parser::LuaIndexKey::Name(name_token) = index_key {
        let field_name = name_token.get_name_text().to_string();

        // Check both inline and symbol table class fields
        let known_inline = inline_fields.is_some_and(|f| f.contains(&field_name));
        let known_symbol = ctx.symbols.has_class_field(class_name, &field_name);

        if !known_inline && !known_symbol {
            let offset = node.text_range().start();
            let (line, col) = ctx
                .line_index
                .get_line_col(offset, ctx.source)
                .unwrap_or((0, 0));

            let suggestion =
                suggest_class_field(ctx.class_defs, ctx.symbols, class_name, &field_name);
            let msg = if let Some(s) = suggestion {
                format!(
                    "Undefined field '{field_name}' on '{var_name}' (class '{class_name}'). Did you mean '{s}'?"
                )
            } else {
                format!("Undefined field '{field_name}' on '{var_name}' (class '{class_name}')")
            };

            diagnostics.push(Diagnostic {
                rule: RuleId::UndefinedField,
                severity: ctx.field_severity,
                message: msg,
                line: line + 1,
                column: col + 1,
            });
        }
    }
}

/// Extract a simple type name from a `LuaDocType`.
/// Returns `Some("ClassName")` for `LuaDocType::Name`, `None` for complex types.
fn extract_name_type(doc_type: &LuaDocType) -> Option<String> {
    match doc_type {
        LuaDocType::Name(name_type) => name_type
            .get_name_token()
            .map(|t| t.get_name_text().to_string()),
        _ => None,
    }
}

/// Suggest a field name from class definitions (inline + symbol table).
fn suggest_class_field(
    class_defs: &HashMap<String, std::collections::HashSet<String>>,
    symbols: &SymbolTable,
    class_name: &str,
    typo: &str,
) -> Option<String> {
    let mut best: Option<(usize, String)> = None;

    // Check inline class defs
    if let Some(fields) = class_defs.get(class_name) {
        for candidate in fields {
            let dist = edit_distance(typo, candidate);
            if dist <= 3 && dist < typo.len() {
                match &best {
                    Some((d, _)) if dist < *d => best = Some((dist, candidate.clone())),
                    None => best = Some((dist, candidate.clone())),
                    _ => {}
                }
            }
        }
    }

    // Check symbol table class defs
    if let Some(fields) = symbols.class_fields_for(class_name) {
        for candidate in fields {
            let dist = edit_distance(typo, candidate);
            if dist <= 3 && dist < typo.len() {
                match &best {
                    Some((d, _)) if dist < *d => best = Some((dist, candidate.clone())),
                    None => best = Some((dist, candidate.clone())),
                    _ => {}
                }
            }
        }
    }

    best.map(|(_, s)| s)
}

fn suggest_field(symbols: &SymbolTable, table: &str, typo: &str) -> Option<String> {
    let fields = symbols.global_fields_for(table)?;
    let mut best: Option<(usize, &str)> = None;
    for candidate in fields {
        let dist = edit_distance(typo, candidate);
        if dist <= 3 && dist < typo.len() {
            match best {
                Some((d, _)) if dist < d => best = Some((dist, candidate)),
                None => best = Some((dist, candidate)),
                _ => {}
            }
        }
    }
    best.map(|(_, s)| s.to_string())
}

fn edit_distance(a: &str, b: &str) -> usize {
    let a_bytes = a.as_bytes();
    let b_bytes = b.as_bytes();
    let m = a_bytes.len();
    let n = b_bytes.len();

    let mut prev = (0..=n).collect::<Vec<_>>();
    let mut curr = vec![0; n + 1];

    for i in 1..=m {
        curr[0] = i;
        for j in 1..=n {
            let cost = if a_bytes[i - 1] == b_bytes[j - 1] {
                0
            } else {
                1
            };
            curr[j] = (prev[j] + 1).min(curr[j - 1] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }

    prev[n]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::LintConfig;
    use crate::symbols::SymbolTable;

    fn make_symbols() -> SymbolTable {
        let mut s = SymbolTable::new().with_lua54_stdlib();
        s.add_global("alc");
        s.add_global_field("alc", "llm");
        s.add_global_field("alc", "state");
        s.add_global_field("alc", "json_encode");
        s
    }

    // ── UndefinedGlobal (carried forward from P0) ──

    #[test]
    fn detects_undefined_global_function() {
        let diags = walk("foo()", &make_symbols(), &LintConfig::default());
        let globals: Vec<_> = diags
            .iter()
            .filter(|d| d.rule == RuleId::UndefinedGlobal)
            .collect();
        assert_eq!(globals.len(), 1);
        assert!(globals[0].message.contains("foo"));
    }

    #[test]
    fn allows_known_globals() {
        let code = "print('hello')\nalc.llm('hi')";
        let diags = walk(code, &make_symbols(), &LintConfig::default());
        let globals: Vec<_> = diags
            .iter()
            .filter(|d| d.rule == RuleId::UndefinedGlobal)
            .collect();
        assert_eq!(globals.len(), 0, "diagnostics: {globals:?}");
    }

    #[test]
    fn detects_undefined_field_on_known_global() {
        let code = "alc.llm_call('hello')";
        let diags = walk(code, &make_symbols(), &LintConfig::default());
        assert!(diags.iter().any(|d| d.message.contains("llm_call")));
        assert!(diags.iter().any(|d| d.message.contains("llm")));
    }

    #[test]
    fn allows_local_variables() {
        let code = "local x = 1\nprint(x)";
        let diags = walk(code, &make_symbols(), &LintConfig::default());
        let globals: Vec<_> = diags
            .iter()
            .filter(|d| d.rule == RuleId::UndefinedGlobal)
            .collect();
        assert_eq!(globals.len(), 0, "diagnostics: {globals:?}");
    }

    #[test]
    fn allows_function_parameters() {
        let code = "function foo(a, b)\n  return a + b\nend";
        let diags = walk(code, &make_symbols(), &LintConfig::default());
        let non_foo: Vec<_> = diags
            .iter()
            .filter(|d| d.rule == RuleId::UndefinedGlobal && !d.message.contains("foo"))
            .collect();
        assert_eq!(non_foo.len(), 0, "unexpected: {non_foo:?}");
    }

    #[test]
    fn allows_local_function() {
        let code = "local function helper() end\nhelper()";
        let diags = walk(code, &make_symbols(), &LintConfig::default());
        let globals: Vec<_> = diags
            .iter()
            .filter(|d| d.rule == RuleId::UndefinedGlobal)
            .collect();
        assert_eq!(globals.len(), 0, "diagnostics: {globals:?}");
    }

    #[test]
    fn allows_for_loop_variable() {
        let code = "for i = 1, 10 do\n  print(i)\nend";
        let diags = walk(code, &make_symbols(), &LintConfig::default());
        let globals: Vec<_> = diags
            .iter()
            .filter(|d| d.rule == RuleId::UndefinedGlobal)
            .collect();
        assert_eq!(globals.len(), 0, "diagnostics: {globals:?}");
    }

    // ── Scoping (new in P1) ──

    #[test]
    fn scope_block_local_not_visible_outside() {
        let code = "do\n  local x = 1\nend\nprint(x)";
        let diags = walk(code, &make_symbols(), &LintConfig::default());
        let globals: Vec<_> = diags
            .iter()
            .filter(|d| d.rule == RuleId::UndefinedGlobal && d.message.contains("'x'"))
            .collect();
        // x is out of scope after `end`, so `print(x)` accesses a global `x`
        assert_eq!(globals.len(), 1, "diagnostics: {globals:?}");
    }

    #[test]
    fn nested_scope_shadows_outer() {
        let code = r#"
local x = "outer"
do
    local x = "inner"
    print(x)
end
print(x)
"#;
        let diags = walk(code, &make_symbols(), &LintConfig::default());
        let globals: Vec<_> = diags
            .iter()
            .filter(|d| d.rule == RuleId::UndefinedGlobal)
            .collect();
        assert_eq!(globals.len(), 0, "diagnostics: {globals:?}");
    }

    // ── UnusedVariable (new in P1) ──

    #[test]
    fn detects_unused_local() {
        let code = "local unused = 42\nprint('hello')";
        let diags = walk(code, &make_symbols(), &LintConfig::default());
        let unused: Vec<_> = diags
            .iter()
            .filter(|d| d.rule == RuleId::UnusedVariable)
            .collect();
        assert_eq!(unused.len(), 1);
        assert!(unused[0].message.contains("unused"));
    }

    #[test]
    fn used_local_not_reported() {
        let code = "local x = 1\nprint(x)";
        let diags = walk(code, &make_symbols(), &LintConfig::default());
        let unused: Vec<_> = diags
            .iter()
            .filter(|d| d.rule == RuleId::UnusedVariable)
            .collect();
        assert_eq!(unused.len(), 0, "diagnostics: {unused:?}");
    }

    #[test]
    fn underscore_prefix_not_reported() {
        let code = "local _ignored = 42\nprint('hello')";
        let diags = walk(code, &make_symbols(), &LintConfig::default());
        let unused: Vec<_> = diags
            .iter()
            .filter(|d| d.rule == RuleId::UnusedVariable)
            .collect();
        assert_eq!(unused.len(), 0, "diagnostics: {unused:?}");
    }

    #[test]
    fn unused_in_block_scope() {
        let code = "do\n  local y = 99\nend";
        let diags = walk(code, &make_symbols(), &LintConfig::default());
        let unused: Vec<_> = diags
            .iter()
            .filter(|d| d.rule == RuleId::UnusedVariable)
            .collect();
        assert_eq!(unused.len(), 1);
        assert!(unused[0].message.contains("y"));
    }

    #[test]
    fn for_range_variables_not_false_positive() {
        // `k` and `v` are used inside the loop body
        let code = "local t = {a=1, b=2}\nfor k, v in pairs(t) do\n  print(k, v)\nend";
        let diags = walk(code, &make_symbols(), &LintConfig::default());
        let unused: Vec<_> = diags
            .iter()
            .filter(|d| d.rule == RuleId::UnusedVariable)
            .collect();
        assert_eq!(unused.len(), 0, "diagnostics: {unused:?}");
    }

    // ── UndefinedField with LuaCats @class (P4) ──

    #[test]
    fn class_field_access_known_field_ok() {
        let code = r#"
---@class Context
---@field name string
---@field value number

---@param ctx Context
local function process(ctx)
    print(ctx.name)
    print(ctx.value)
end
"#;
        let diags = walk(code, &make_symbols(), &LintConfig::default());
        let fields: Vec<_> = diags
            .iter()
            .filter(|d| d.rule == RuleId::UndefinedField)
            .collect();
        assert_eq!(fields.len(), 0, "diags: {fields:?}");
    }

    #[test]
    fn class_field_access_unknown_field_detected() {
        let code = r#"
---@class Context
---@field name string
---@field value number

---@param ctx Context
local function process(ctx)
    print(ctx.unknown)
end
"#;
        let diags = walk(code, &make_symbols(), &LintConfig::default());
        let fields: Vec<_> = diags
            .iter()
            .filter(|d| d.rule == RuleId::UndefinedField)
            .collect();
        assert_eq!(fields.len(), 1, "diags: {fields:?}");
        assert!(fields[0].message.contains("unknown"));
        assert!(fields[0].message.contains("Context"));
    }

    #[test]
    fn class_field_access_with_suggestion() {
        let code = r#"
---@class Context
---@field name string
---@field value number

---@param ctx Context
local function process(ctx)
    print(ctx.nme)
end
"#;
        let diags = walk(code, &make_symbols(), &LintConfig::default());
        let fields: Vec<_> = diags
            .iter()
            .filter(|d| d.rule == RuleId::UndefinedField)
            .collect();
        assert_eq!(fields.len(), 1, "diags: {fields:?}");
        assert!(
            fields[0].message.contains("name"),
            "expected suggestion 'name', got: {}",
            fields[0].message
        );
    }

    #[test]
    fn class_type_annotation_on_local() {
        let code = r#"
---@class Config
---@field timeout number
---@field retries number

---@type Config
local cfg = get_config()
print(cfg.timeout)
print(cfg.missing)
"#;
        let diags = walk(code, &make_symbols(), &LintConfig::default());
        let fields: Vec<_> = diags
            .iter()
            .filter(|d| d.rule == RuleId::UndefinedField)
            .collect();
        assert_eq!(fields.len(), 1, "diags: {fields:?}");
        assert!(fields[0].message.contains("missing"));
        assert!(fields[0].message.contains("Config"));
    }

    #[test]
    fn class_without_fields_no_false_positive() {
        // If a class has no @field declarations, don't report field accesses
        // (the class may have dynamic fields).
        let code = r#"
---@class Dynamic

---@param obj Dynamic
local function use_obj(obj)
    print(obj.anything)
end
"#;
        let diags = walk(code, &make_symbols(), &LintConfig::default());
        let fields: Vec<_> = diags
            .iter()
            .filter(|d| d.rule == RuleId::UndefinedField)
            .collect();
        // Empty class = no declared fields → we report the access as undefined
        // since the user declared @class but no @field.
        // However, this is debatable. For now, report it.
        assert_eq!(fields.len(), 1, "diags: {fields:?}");
    }

    #[test]
    fn untyped_local_no_class_check() {
        // Locals without a class type should not trigger UndefinedField
        let code = r#"
local obj = {}
obj.anything = 1
print(obj.anything)
"#;
        let diags = walk(code, &make_symbols(), &LintConfig::default());
        let fields: Vec<_> = diags
            .iter()
            .filter(|d| d.rule == RuleId::UndefinedField)
            .collect();
        assert_eq!(fields.len(), 0, "diags: {fields:?}");
    }

    #[test]
    fn multiple_classes_independent() {
        let code = r#"
---@class Foo
---@field x number

---@class Bar
---@field y string

---@param f Foo
---@param b Bar
local function test(f, b)
    print(f.x)
    print(b.y)
    print(f.y)
end
"#;
        let diags = walk(code, &make_symbols(), &LintConfig::default());
        let fields: Vec<_> = diags
            .iter()
            .filter(|d| d.rule == RuleId::UndefinedField)
            .collect();
        // f.y should be flagged (y belongs to Bar, not Foo)
        assert_eq!(fields.len(), 1, "diags: {fields:?}");
        assert!(fields[0].message.contains("y"));
        assert!(fields[0].message.contains("Foo"));
    }
}
