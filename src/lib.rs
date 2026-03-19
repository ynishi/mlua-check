//! Lua checker on mlua — undefined variable/global/field detection with LuaCats support.
//!
//! Detects undefined variables, undefined globals, and undefined fields
//! on known global tables.  Designed to run **before** Lua execution,
//! providing a safety net for AI-driven and programmatic Lua code generation.
//!
//! # Quick start (one-shot)
//!
//! ```rust
//! let result = mlua_check::run_lint("print('hello')", "@main.lua").unwrap();
//! assert_eq!(result.diagnostics.len(), 0);
//! ```
//!
//! # Granular control (existing VM)
//!
//! ```rust
//! use mlua::prelude::*;
//! use mlua_check::register;
//!
//! let lua = Lua::new();
//! // Register custom globals on the VM
//! let alc = lua.create_table().unwrap();
//! alc.set("llm", lua.create_function(|_, ()| Ok(())).unwrap()).unwrap();
//! lua.globals().set("alc", alc).unwrap();
//!
//! // register() introspects the VM and builds a symbol table automatically
//! let engine = register(&lua).unwrap();
//! let result = engine.lint("alc.llm('hello')", "@main.lua");
//! assert_eq!(result.diagnostics.len(), 0);
//!
//! let result = engine.lint("alc.unknown('hello')", "@main.lua");
//! assert!(result.warning_count > 0);
//! ```

pub mod config;
pub mod engine;
mod rules;
pub mod scope;
pub mod symbols;
pub mod types;
pub mod vm;
mod walker;

pub use config::{LintConfig, LintPolicy};
pub use engine::LintEngine;
pub use types::{Diagnostic, LintResult, RuleId, Severity};
pub use vm::{collect_symbols, lint, register, register_with_config, run_lint};
