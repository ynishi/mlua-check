# mlua-check

Lua checker on [mlua](https://github.com/mlua-rs/mlua) — undefined variable / global / field detection with LuaCats support.

Designed to run **before** Lua execution, providing a safety net for AI-driven and programmatic Lua code generation.

Backed by [emmylua_code_analysis](https://github.com/EmmyLuaLs/emmylua-analyzer-rust) for accurate Lua 5.4 diagnostics.

## Features

- **Undefined variable** — reference to a variable not defined in any enclosing scope
- **Undefined global** — reference to a global name not in the known symbol table
- **Undefined field** — access to a field not declared in a `---@class` definition
- **Unused variable** — a local variable that is declared but never referenced
- **VM introspection** — automatically builds symbol table from live `mlua` globals

## Quick start

```rust
// third argument: optional search paths prepended to Lua's package.path
let result = mlua_check::run_lint("print('hello')", "@main.lua", &[]).unwrap();
assert_eq!(result.diagnostics.len(), 0);
```

## With an existing VM

```rust
use mlua::prelude::*;
use mlua_check::register;

let lua = Lua::new();

let alc = lua.create_table().unwrap();
alc.set("llm", lua.create_function(|_, ()| Ok(())).unwrap()).unwrap();
lua.globals().set("alc", alc).unwrap();

// register() introspects the VM and builds a symbol table automatically
let engine = register(&lua).unwrap();
let result = engine.lint("alc.llm('hello')", "@main.lua");
assert_eq!(result.diagnostics.len(), 0);

let result = engine.lint("alc.unknown('hello')", "@main.lua");
assert!(result.warning_count > 0);
```

## Configuration

```rust
use mlua_check::{LintConfig, LintPolicy};

let config = LintConfig::default().with_policy(LintPolicy::Strict);
```

| Policy | Behavior |
|--------|----------|
| `Strict` | Lint errors block execution |
| `Warn` | Issues reported, execution proceeds (default) |
| `Off` | Linting disabled |

## Diagnostics

`RuleId` covers the four common categories (`UndefinedVariable`, `UndefinedGlobal`, `UndefinedField`, `UnusedVariable`) plus `Other(String)` for any additional codes reported by the analyser. `RuleId` is `#[non_exhaustive]`, so `match` expressions require a wildcard arm.

The following issues from the previous walker-based implementation are resolved:

- `for`-`in` loop variables no longer produce false-positive undefined-global diagnostics.
- The Lua 5.4 standard global `arg` is now recognised without manual declaration.
- Custom globals registered on the `mlua` VM via `register(&lua)` are injected into the analyser as a `---@meta` virtual stub, eliminating false positives for host-defined names.

## License

Licensed under either of

- [MIT license](LICENSE-MIT)
- [Apache License, Version 2.0](LICENSE-APACHE)

at your option.
