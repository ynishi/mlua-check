//! Basic linting — detecting undefined globals with `run_lint`.
//!
//! Demonstrates the one-shot API: pass a Lua source string, a chunk name,
//! and an optional search-path slice.  The linter flags any name that is
//! not part of the standard Lua 5.4 library.
//!
//! Run with: `cargo run --example basic_lint`

fn main() {
    // --- clean code: stdlib-only access, no diagnostics expected ---
    let clean = r#"
        local t = {}
        table.insert(t, "hello")
        print(string.upper(t[1]))
    "#;

    let result = mlua_check::run_lint(clean, "@clean.lua", &[]).expect("lint failed");
    println!(
        "clean.lua  — errors: {}, warnings: {}",
        result.error_count, result.warning_count
    );
    assert_eq!(result.error_count, 0, "clean code should produce no errors");

    // --- code with an undefined global: should trigger an error ---
    let bad = r#"
        local x = unknown_global()
        print(x)
    "#;

    let result = mlua_check::run_lint(bad, "@bad.lua", &[]).expect("lint failed");
    println!(
        "bad.lua    — errors: {}, warnings: {}",
        result.error_count, result.warning_count
    );
    for d in &result.diagnostics {
        println!("  [{}] line {}: {}", d.rule, d.line, d.message);
    }
    assert!(
        result.error_count > 0,
        "expected at least one error for unknown_global"
    );

    println!("basic_lint example completed successfully.");
}
