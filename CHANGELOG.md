# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.3.0] - 2026-05-25

### Added

- `examples/` directory with three runnable demos (`basic_lint`, `with_vm`, `luacats`); run any with `cargo run --example <name>`
- `RuleId::Other(String)` variant — diagnostics from `emmylua_code_analysis` that do not map to the four named variants are captured here rather than silently dropped
- `#[non_exhaustive]` on `RuleId` — callers must handle future variants via a wildcard arm (breaking for exhaustive `match` without wildcard)
- `custom globals` injection API — `register(&lua)` / `register_with_config(&lua, config)` now build a `---@meta` virtual stub from the live VM symbol table and feed it into the analyser, resolving undefined-global false positives for host-defined globals
- Integration tests in `tests/integration.rs` covering: undefined global detection, `for`-`in` loop variable scoping, Lua 5.4 `arg` global recognition, custom globals injection, and `_`-prefix unused-variable suppression

### Changed

- **BREAKING**: `RuleId` is now `#[non_exhaustive]`; exhaustive `match` expressions require a `_ => { ... }` wildcard arm
- Internal lint engine replaced: self-written walker / scope / rules / symbols modules removed; all diagnostic generation delegated to `emmylua_code_analysis 0.23`
- `emmylua_parser` bumped from `0.24` to `0.26` (required by `emmylua_code_analysis 0.23`)

### Fixed

- `for`-`in` loop variables were incorrectly reported as undefined globals (false positive); `emmylua_code_analysis` correctly recognises them as declarations
- Lua 5.4 standard-library global `arg` was missing from the known-globals set; `init_std_lib()` now covers it
- `_chunk_name` parameter of `LintEngine::lint` was silently discarded; it is now used as the virtual file path passed to the analyser

### Dependencies

- Added: `emmylua_code_analysis = "0.23"` — core analysis backend
- Added: `tokio = { version = "1", features = ["sync"] }` — required transitively
- Added: `tokio-util = "0.7"` — provides `CancellationToken` for `diagnose_file`
- Added: `lsp_types` alias (`emmy_lsp_types = "0.1.0"`) — LSP diagnostic type re-export
- Bumped: `emmylua_parser` `0.24` → `0.26`
- Removed (direct): `rowan = "0.16"` — was used only by the deleted walker; now a transitive dependency via `emmylua_code_analysis`

### Known issues

- `LintEngine::lint` acquires the internal `Mutex<EmmyLuaAnalysis>` twice per call (once to inject the meta stub, once to inject user source) introducing a TOCTOU window on the shared analysis state (`engine.rs:101-129`). This is a known limitation and will be addressed in a follow-up fix.

## [0.2.0] - 2026-03-21

### Changed

- **BREAKING**: `run_lint()` now takes a third argument `search_paths: &[&str]` to prepend directories to Lua's `package.path`. Pass `&[]` for previous behavior.

### Added

- `search_paths` support — allows the VM to resolve project-specific modules when building the symbol table for static analysis

## [0.1.0] - 2026-03-20

### Added

- Undefined variable detection (scope-aware)
- Undefined global detection with symbol table
- Undefined field detection with `---@class` LuaCats support
- Unused variable detection
- VM introspection via `register()` — auto-builds symbol table from live `mlua` globals
- One-shot `run_lint()` API
- Configurable lint policy (Strict / Warn / Off)
- Per-rule severity overrides
