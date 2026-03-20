# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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
