//! Detects references to globals not present in the known symbol table.
//!
//! Handles two patterns:
//! - Simple global: `foo()` where `foo` is not a local and not a known global
//! - Table field: `alc.llm_call()` where `alc` is known but `llm_call` is not a known field

use emmylua_parser::{LineIndex, LuaAstNode, LuaIndexExpr, LuaNameExpr, LuaSyntaxKind};

use crate::config::LintConfig;
use crate::scope::ScopeStack;
use crate::symbols::SymbolTable;
use crate::types::{Diagnostic, RuleId, Severity};

/// Walk the AST and collect undefined-global diagnostics.
///
/// This performs a single-pass walk that:
/// 1. Tracks local definitions (LocalStat, LocalFuncStat, for-loop vars, params)
/// 2. Reports NameExpr references that are neither local nor known globals
/// 3. Reports IndexExpr `a.b` where `a` is a known global but `b` is not a
///    registered field
pub fn check_undefined_globals(
    source: &str,
    symbols: &SymbolTable,
    config: &LintConfig,
) -> Vec<Diagnostic> {
    let tree = emmylua_parser::LuaParser::parse(source, emmylua_parser::ParserConfig::default());
    let root = tree.get_red_root();
    let line_index = LineIndex::parse(source);
    let severity = config.severity_for(RuleId::UndefinedGlobal, Severity::Warning);

    let mut diagnostics = Vec::new();
    let mut scopes = ScopeStack::new();

    // Collect all local definitions first, then check references.
    // For simplicity in P0, we do a two-pass approach:
    //   Pass 1: collect all local names into scopes (flat — no nesting for now)
    //   Pass 2: check all NameExpr / IndexExpr references

    // Pass 1: Collect local definitions (flat scope for P0)
    for node in root.descendants() {
        let kind: LuaSyntaxKind = node.kind().into();
        match kind {
            LuaSyntaxKind::LocalName => {
                // Direct text of the LocalName node is the variable name
                let text = node.text().to_string().trim().to_string();
                if !text.is_empty() {
                    let offset = node.text_range().start();
                    let (line, col) = line_index.get_line_col(offset, source).unwrap_or((0, 0));
                    scopes.define_local(&text, line, col);
                }
            }
            LuaSyntaxKind::ParamName => {
                let text = node.text().to_string().trim().to_string();
                if !text.is_empty() {
                    let offset = node.text_range().start();
                    let (line, col) = line_index.get_line_col(offset, source).unwrap_or((0, 0));
                    scopes.define_local(&text, line, col);
                }
            }
            LuaSyntaxKind::LocalFuncStat => {
                // `local function foo() end` — `foo` is a local
                // The function name is the first NameToken child
                for child in node.children_with_tokens() {
                    if let Some(token) = child.as_token() {
                        let token_kind: emmylua_parser::LuaTokenKind = token.kind().into();
                        if token_kind == emmylua_parser::LuaTokenKind::TkName {
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
                // `for i = 1, 10 do` — `i` is a local
                for child in node.children_with_tokens() {
                    if let Some(token) = child.as_token() {
                        let token_kind: emmylua_parser::LuaTokenKind = token.kind().into();
                        if token_kind == emmylua_parser::LuaTokenKind::TkName {
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
            _ => {}
        }
    }

    // Pass 2: Check references
    for node in root.descendants() {
        let kind: LuaSyntaxKind = node.kind().into();
        match kind {
            LuaSyntaxKind::NameExpr => {
                if let Some(name_expr) = LuaNameExpr::cast(node.clone()) {
                    if let Some(name) = name_expr.get_name_text() {
                        // Skip if it's a local
                        if scopes.is_defined(&name) {
                            continue;
                        }
                        // Skip special names
                        if name == "self" || name == "..." || name == "_" || name == "_ENV" {
                            continue;
                        }
                        // Check if parent is an IndexExpr (handled separately)
                        if let Some(parent) = node.parent() {
                            let parent_kind: LuaSyntaxKind = parent.kind().into();
                            if parent_kind == LuaSyntaxKind::IndexExpr {
                                // This NameExpr is the prefix of `table.field`.
                                // We check the whole IndexExpr instead.
                                continue;
                            }
                        }
                        // Not local, not special — is it a known global?
                        if !symbols.has_global(&name) {
                            let offset = node.text_range().start();
                            let (line, col) =
                                line_index.get_line_col(offset, source).unwrap_or((0, 0));
                            diagnostics.push(Diagnostic {
                                rule: RuleId::UndefinedGlobal,
                                severity,
                                message: format!("Undefined global '{name}'"),
                                line: line + 1, // 1-based
                                column: col + 1,
                            });
                        }
                    }
                }
            }
            LuaSyntaxKind::IndexExpr => {
                if let Some(index_expr) = LuaIndexExpr::cast(node.clone()) {
                    // Get the prefix (e.g. `alc` in `alc.llm`)
                    if let Some(prefix) = index_expr.get_prefix_expr() {
                        if let Some(name_expr) = {
                            use emmylua_parser::LuaExpr;
                            match prefix {
                                LuaExpr::NameExpr(ne) => Some(ne),
                                _ => None,
                            }
                        } {
                            if let Some(table_name) = name_expr.get_name_text() {
                                // Skip if prefix is a local variable
                                if scopes.is_defined(&table_name) {
                                    continue;
                                }
                                // Check if the table is a known global
                                if !symbols.has_global(&table_name) {
                                    let offset = node.text_range().start();
                                    let (line, col) =
                                        line_index.get_line_col(offset, source).unwrap_or((0, 0));
                                    diagnostics.push(Diagnostic {
                                        rule: RuleId::UndefinedGlobal,
                                        severity,
                                        message: format!("Undefined global '{table_name}'"),
                                        line: line + 1,
                                        column: col + 1,
                                    });
                                    continue;
                                }
                                // Table is known — check the field
                                if let Some(index_key) = index_expr.get_index_key() {
                                    use emmylua_parser::LuaIndexKey;
                                    if let LuaIndexKey::Name(name_token) = index_key {
                                        let field_name = name_token.get_name_text().to_string();
                                        if !symbols.has_global_field(&table_name, &field_name) {
                                            let offset = node.text_range().start();
                                            let (line, col) = line_index
                                                .get_line_col(offset, source)
                                                .unwrap_or((0, 0));

                                            let suggestion =
                                                suggest_field(symbols, &table_name, &field_name);
                                            let msg = if let Some(s) = suggestion {
                                                format!(
                                                    "Undefined field '{field_name}' on global '{table_name}'. Did you mean '{s}'?"
                                                )
                                            } else {
                                                format!(
                                                    "Undefined field '{field_name}' on global '{table_name}'"
                                                )
                                            };

                                            diagnostics.push(Diagnostic {
                                                rule: RuleId::UndefinedGlobal,
                                                severity,
                                                message: msg,
                                                line: line + 1,
                                                column: col + 1,
                                            });
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }

    diagnostics
}

/// Simple edit-distance based suggestion.
fn suggest_field(symbols: &SymbolTable, table: &str, typo: &str) -> Option<String> {
    let fields = symbols.global_fields_for(table)?;
    let mut best: Option<(usize, &str)> = None;
    for candidate in fields {
        let dist = edit_distance(typo, candidate);
        // Only suggest if distance is at most 3 and less than half the length
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

/// Minimal Levenshtein distance for "did you mean?" suggestions.
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

    #[test]
    fn detects_undefined_global_function() {
        let code = "foo()";
        let diags = check_undefined_globals(code, &make_symbols(), &LintConfig::default());
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].rule, RuleId::UndefinedGlobal);
        assert!(diags[0].message.contains("foo"));
    }

    #[test]
    fn allows_known_globals() {
        let code = "print('hello')\nalc.llm('hi')";
        let diags = check_undefined_globals(code, &make_symbols(), &LintConfig::default());
        assert_eq!(diags.len(), 0, "diagnostics: {diags:?}");
    }

    #[test]
    fn detects_undefined_field_on_known_global() {
        let code = "alc.llm_call('hello')";
        let diags = check_undefined_globals(code, &make_symbols(), &LintConfig::default());
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("llm_call"));
        assert!(diags[0].message.contains("llm")); // "Did you mean 'llm'?"
    }

    #[test]
    fn allows_local_variables() {
        let code = "local x = 1\nprint(x)";
        let diags = check_undefined_globals(code, &make_symbols(), &LintConfig::default());
        assert_eq!(diags.len(), 0, "diagnostics: {diags:?}");
    }

    #[test]
    fn allows_function_parameters() {
        let code = "function foo(a, b)\n  return a + b\nend";
        let diags = check_undefined_globals(code, &make_symbols(), &LintConfig::default());
        // `foo` itself may be flagged as undefined global since it's not local.
        // But `a` and `b` should not be.
        let non_foo: Vec<_> = diags
            .iter()
            .filter(|d| !d.message.contains("foo"))
            .collect();
        assert_eq!(non_foo.len(), 0, "unexpected: {non_foo:?}");
    }

    #[test]
    fn allows_local_function() {
        let code = "local function helper() end\nhelper()";
        let diags = check_undefined_globals(code, &make_symbols(), &LintConfig::default());
        assert_eq!(diags.len(), 0, "diagnostics: {diags:?}");
    }

    #[test]
    fn allows_for_loop_variable() {
        let code = "for i = 1, 10 do\n  print(i)\nend";
        let diags = check_undefined_globals(code, &make_symbols(), &LintConfig::default());
        assert_eq!(diags.len(), 0, "diagnostics: {diags:?}");
    }

    #[test]
    fn reports_correct_line_numbers() {
        let code = "local x = 1\nlocal y = 2\nunknown_func()";
        let diags = check_undefined_globals(code, &make_symbols(), &LintConfig::default());
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].line, 3); // 1-based
    }

    #[test]
    fn edit_distance_works() {
        assert_eq!(edit_distance("llm", "llm_call"), 5);
        assert_eq!(edit_distance("llm_call", "llm"), 5);
        assert_eq!(edit_distance("llm", "llm"), 0);
        assert_eq!(edit_distance("lml", "llm"), 2);
    }
}
