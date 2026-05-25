//! VM-aware linting — injecting custom globals from a live `mlua::Lua` VM.
//!
//! Shows the two-step granular API: first create a Lua VM and register
//! application-specific globals on it, then call `mlua_check::register(&lua)`
//! so the linter introspects the VM automatically.
//!
//! The example verifies:
//!   - `alc.llm(...)` is recognised as a known call (no diagnostic)
//!   - `alc.unknown(...)` triggers a warning (field not registered on `alc`)
//!   - a completely unknown global produces an error
//!
//! Run with: `cargo run --example with_vm`

use mlua::prelude::*;

fn main() {
    // --- Step 1: create a VM and populate the `alc` global table ---
    let lua = Lua::new();

    let alc = lua.create_table().expect("create alc table");
    alc.set(
        "llm",
        lua.create_function(|_, _: String| Ok(String::new()))
            .expect("create alc.llm"),
    )
    .expect("set alc.llm");
    alc.set(
        "state",
        lua.create_function(|_, ()| Ok(()))
            .expect("create alc.state"),
    )
    .expect("set alc.state");
    lua.globals().set("alc", alc).expect("set alc global");

    // --- Step 2: build a LintEngine that knows about the live VM globals ---
    let engine = mlua_check::register(&lua).expect("register failed");

    // known call — should produce zero diagnostics
    let result = engine.lint("alc.llm('hello world')", "@with_vm.lua");
    println!(
        "alc.llm    — errors: {}, warnings: {}",
        result.error_count, result.warning_count
    );
    assert_eq!(
        result.diagnostics.len(),
        0,
        "alc.llm is registered; expected no diagnostics, got: {:#?}",
        result.diagnostics
    );

    // unknown field on a known global — should produce a warning
    let result = engine.lint("alc.unknown('oops')", "@with_vm.lua");
    println!(
        "alc.unknown— errors: {}, warnings: {}",
        result.error_count, result.warning_count
    );
    for d in &result.diagnostics {
        println!("  [{}] line {}: {}", d.rule, d.line, d.message);
    }
    assert!(
        result.warning_count > 0,
        "alc.unknown is not registered; expected a warning, got: {:#?}",
        result.diagnostics
    );

    // completely unknown global — should produce an error
    let result = engine.lint("no_such_global()", "@with_vm.lua");
    println!(
        "no_such    — errors: {}, warnings: {}",
        result.error_count, result.warning_count
    );
    assert!(
        result.error_count > 0,
        "no_such_global is unknown; expected an error"
    );

    println!("with_vm example completed successfully.");
}
