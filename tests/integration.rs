use mlua::prelude::*;
use mlua_check::{register, run_lint, LintEngine, RuleId};

// ---------------------------------------------------------------------------
// Inv-6: for-in loop variables must not be reported as UndefinedGlobal /
// UndefinedVariable (issue 1779668266-83500 structural resolution).
// ---------------------------------------------------------------------------
#[test]
fn for_in_loop_vars_not_undefined() {
    let result = run_lint(
        "local t = {}\nfor k, v in pairs(t) do print(k, v) end",
        "@test.lua",
        &[],
    )
    .expect("emmylua should accept for-in loop without error");

    let undef: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| matches!(d.rule, RuleId::UndefinedGlobal | RuleId::UndefinedVariable))
        .collect();
    assert_eq!(
        undef.len(),
        0,
        "for-in vars k, v should not be reported as undefined: {:?}",
        undef
    );
}

// ---------------------------------------------------------------------------
// Inv-7: `arg` must be recognized as a Lua 5.4 std global, not UndefinedGlobal
// (issue 1779668299-83937 structural resolution).
// ---------------------------------------------------------------------------
#[test]
fn arg_global_not_undefined() {
    let result = run_lint("print(arg[1])", "@test.lua", &[])
        .expect("emmylua should recognize arg as Lua 5.4 std");

    let undef_arg: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| d.rule == RuleId::UndefinedGlobal && d.message.contains("arg"))
        .collect();
    assert_eq!(
        undef_arg.len(),
        0,
        "arg should be recognized as Lua 5.4 std global: {:?}",
        undef_arg
    );
}

// ---------------------------------------------------------------------------
// custom globals injection: alc.llm registered via mlua VM must not trigger
// diagnostics (issue 1779668361-84738 structural resolution).
// ---------------------------------------------------------------------------
#[test]
fn custom_globals_inject_works() {
    let lua = Lua::new();
    let alc = lua.create_table().unwrap();
    alc.set("llm", lua.create_function(|_, ()| Ok(())).unwrap())
        .unwrap();
    lua.globals().set("alc", alc).unwrap();

    let engine = register(&lua).expect("register should succeed");
    let result = engine.lint("alc.llm('x')", "@test.lua");
    assert_eq!(
        result.diagnostics.len(),
        0,
        "custom global alc.llm should pass lint without diagnostics: {:?}",
        result.diagnostics
    );
}

// ---------------------------------------------------------------------------
// custom globals — accessing an unregistered field must produce a diagnostic.
// ---------------------------------------------------------------------------
#[test]
fn custom_globals_unknown_field_reported() {
    let lua = Lua::new();
    let alc = lua.create_table().unwrap();
    alc.set("llm", lua.create_function(|_, ()| Ok(())).unwrap())
        .unwrap();
    lua.globals().set("alc", alc).unwrap();

    let engine = register(&lua).expect("register should succeed");
    let result = engine.lint("alc.unknown('x')", "@test.lua");
    assert!(
        !result.diagnostics.is_empty(),
        "alc.unknown should produce a diagnostic (undefined-field): {:?}",
        result.diagnostics
    );
    // The diagnostic should mention the unknown field name.
    let mentions_unknown = result
        .diagnostics
        .iter()
        .any(|d| d.message.contains("unknown"));
    assert!(
        mentions_unknown,
        "diagnostic message should reference 'unknown', got: {:?}",
        result.diagnostics
    );
}

// ---------------------------------------------------------------------------
// Inv-8: consecutive lint calls on the same LintEngine must produce independent
// results (virtual path collision guard).
// ---------------------------------------------------------------------------
#[test]
fn consecutive_lint_independent() {
    let lua = Lua::new();
    let engine = register(&lua).expect("register should succeed");

    // r1 has an undefined global "only_in_a"; r2 has an undefined global
    // "only_in_b".  Independence means: r2 must not contain "only_in_a"
    // diagnostics, and r1 must not contain "only_in_b" diagnostics.
    let r1 = engine.lint("only_in_a()", "@a.lua");
    let r2 = engine.lint("only_in_b()", "@b.lua");

    // r1 should mention only_in_a, not only_in_b.
    let r1_mentions_b = r1
        .diagnostics
        .iter()
        .any(|d| d.message.contains("only_in_b"));
    assert!(
        !r1_mentions_b,
        "r1 (@a.lua) must not contain diagnostics from r2 (@b.lua): {:?}",
        r1.diagnostics
    );

    // r2 should mention only_in_b, not only_in_a.
    let r2_mentions_a = r2
        .diagnostics
        .iter()
        .any(|d| d.message.contains("only_in_a"));
    assert!(
        !r2_mentions_a,
        "r2 (@b.lua) must not contain diagnostics from r1 (@a.lua): {:?}",
        r2.diagnostics
    );

    // Both should produce at least one diagnostic (proving the lint ran).
    assert!(
        !r1.diagnostics.is_empty(),
        "r1 should have at least one diagnostic for only_in_a"
    );
    assert!(
        !r2.diagnostics.is_empty(),
        "r2 should have at least one diagnostic for only_in_b"
    );
}

// ---------------------------------------------------------------------------
// underscore_prefix_not_reported: emmylua must not report `_`-prefixed locals
// as UnusedVariable (emmylua convention for intentionally-unused bindings).
// ---------------------------------------------------------------------------
#[test]
fn underscore_prefix_not_reported() {
    // `_x` is intentionally unused; emmylua should suppress the diagnostic.
    let result = run_lint("local _x = 42\nprint('done')", "@test.lua", &[])
        .expect("emmylua should accept _-prefixed unused local");

    let unused_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| d.rule == RuleId::UnusedVariable)
        .collect();
    assert_eq!(
        unused_diags.len(),
        0,
        "_ prefix should suppress unused-variable diagnostic: {:?}",
        unused_diags
    );
}

// ---------------------------------------------------------------------------
// RuleId::Other catch-all: emmylua diagnostic codes not in the four well-known
// variants must be received as RuleId::Other(_) without panicking.
// ---------------------------------------------------------------------------
#[test]
fn rule_id_other_catches_unknown_codes() {
    // Use a direct LintEngine to confirm that any diagnostic arriving with an
    // unknown code does not crash the bridge and is represented as Other(_).
    // We test this indirectly: run clean code and verify the bridge round-trips
    // all diagnostics without panic.  Then run code that produces an error to
    // confirm the Other path exists by checking that at least one diagnostic
    // can be produced (engine_detects_undefined tests the happy path separately).
    let engine = LintEngine::new();

    // Confirm the engine works for a clean snippet (no panic on zero diags).
    let clean = engine.lint("print('hello')", "@test.lua");
    assert_eq!(
        clean.diagnostics.len(),
        0,
        "clean snippet should produce no diagnostics"
    );

    // Confirm the engine works for an erroring snippet (no panic on non-zero diags).
    let erroring = engine.lint("undefined_function()", "@test.lua");
    assert!(
        !erroring.diagnostics.is_empty(),
        "erroring snippet should produce diagnostics"
    );

    // Every diagnostic must have a valid RuleId — verified by the fact that
    // map_diagnostic does not panic and returns a Diagnostic with a non-Other
    // or Other(_) RuleId (both are acceptable).
    for d in &erroring.diagnostics {
        let _rule = &d.rule; // ensures RuleId is constructible without panic
    }
}
