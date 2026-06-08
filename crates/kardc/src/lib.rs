//! kardashev — Gen 2.
//!
//! A self-contained toolchain for a small systems language built around Zig's
//! philosophy: no hidden control flow, no hidden allocations, compile-time
//! evaluation instead of macros, explicit `defer` cleanup, and first-class
//! tests. The compiler is plain Rust with zero external dependencies.
//!
//! Pipeline: `source → lex → parse → sema → emit C → cc → native binary`.

pub mod ast;
pub mod backend;
pub mod build_system;
pub mod cli;
pub mod const_eval;
pub mod diag;
pub mod emit_c;
pub mod fmt;
pub mod lexer;
pub mod parser;
pub mod scaffold;
pub mod sema;
pub mod span;
pub mod token;
pub mod types;

use diag::Diagnostic;
use emit_c::EmitMode;

/// The toolchain version. Single source of truth; keep in sync with
/// `Cargo.toml` and `CHANGELOG.md`.
pub const VERSION: &str = "0.121.0";

/// Front-to-middle pipeline: lex, parse and type-check `src`, then lower the
/// validated module to C source text for `mode`.
///
/// Returns the C source on success, or every diagnostic gathered along the way.
pub fn compile_to_c(src: &str, mode: EmitMode) -> Result<String, Vec<Diagnostic>> {
    let tokens = lexer::lex(src)?;
    let module = parser::parse(&tokens)?;
    let structs = sema::check(&module)?;
    if mode == EmitMode::Program && !has_main(&module) {
        return Err(vec![Diagnostic::error(
            span::Span::DUMMY,
            "E0150",
            "program has no `fn main`",
        )]);
    }
    Ok(emit_c::emit(&module, &structs, mode))
}

/// True if the module declares a top-level `fn main`.
fn has_main(module: &ast::Module) -> bool {
    module
        .items
        .iter()
        .any(|it| matches!(it, ast::Item::Func(f) if f.name == "main"))
}

/// Parse and re-emit `src` in canonical form (used by `kard fmt`).
pub fn format(src: &str) -> Result<String, Vec<Diagnostic>> {
    fmt::format_source(src)
}
