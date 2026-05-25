//! LuaCats annotation — class and field definitions for stricter field checking.
//!
//! Shows how `---@class` and `---@field` annotations in Lua source code
//! enable emmylua to detect accesses to undeclared fields.  When a class is
//! annotated, any field not listed with `---@field` is flagged as an
//! `undefined-field` diagnostic.
//!
//! Run with: `cargo run --example luacats`

use mlua_check::{LintEngine, RuleId};

fn main() {
    let engine = LintEngine::new();

    // --- source with ---@class / ---@field annotations ---
    // Accessing a declared field (`name`) should be clean.
    // Accessing an undeclared field (`age`) should produce a diagnostic.
    let source_clean = r#"
        ---@class Person
        ---@field name string

        ---@type Person
        local p = {}
        print(p.name)
    "#;

    let result = engine.lint(source_clean, "@luacats_clean.lua");
    println!(
        "clean field access  — errors: {}, warnings: {}",
        result.error_count, result.warning_count
    );
    assert_eq!(
        result.error_count, 0,
        "p.name is a declared field; expected 0 errors, got: {:#?}",
        result.diagnostics
    );

    // --- accessing an undeclared field should trigger a diagnostic ---
    let source_bad = r#"
        ---@class Config
        ---@field host string
        ---@field port number

        ---@type Config
        local cfg = {}
        print(cfg.host)
        print(cfg.port)
        print(cfg.unknown_field)
    "#;

    let result = engine.lint(source_bad, "@luacats_bad.lua");
    println!(
        "unknown field access— errors: {}, warnings: {}",
        result.error_count, result.warning_count
    );
    for d in &result.diagnostics {
        println!("  [{}] line {}: {}", d.rule, d.line, d.message);
    }

    let field_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| d.rule == RuleId::UndefinedField)
        .collect();
    assert!(
        !field_diags.is_empty(),
        "cfg.unknown_field is not declared; expected an UndefinedField diagnostic, got: {:#?}",
        result.diagnostics
    );
    assert!(
        field_diags
            .iter()
            .any(|d| d.message.contains("unknown_field")),
        "diagnostic should mention 'unknown_field', got: {:#?}",
        field_diags
    );

    println!("luacats example completed successfully.");
}
