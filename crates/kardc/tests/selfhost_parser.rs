//! Self-host stage 2 (v0.160): differential test of `selfhost/parser.ks` — the
//! kardashev parser written in kardashev — against the Rust reference parser.
//!
//! `selfhost/astdump.ks` is compiled ONCE (full file-based pipeline + `-O0`
//! cc build) and then executed on every corpus file; its stdout — the
//! canonical line-based AST dump defined below — must be byte-identical to
//! [`rust_dump`], which renders `kardc::parser::parse`'s output in the same
//! format.
//!
//! # The dump format (the contract; `selfhost/astdump.ks` mirrors it)
//!
//! One node per line, depth-first, two spaces of indentation per tree level:
//!
//! ```text
//! <indent><KIND> <off> <len>[ <extras>]
//! ```
//!
//! `<off> <len>` is the node's span (byte offset + length — `Span.start` and
//! `Span.end - Span.start`). `<extras>` are per-kind, in a fixed order:
//!
//! - `FN off len <p> <name>` (`p` ∈ {0,1} = `is_pub`): children = `PARAM`s
//!   (each with its `TYPE` child), the return `TYPE`, the body `BLOCK`.
//! - `PARAM off len <ct> <name>` (`ct` = `is_comptime`): child = `TYPE`.
//! - `CONST off len <p> <name>`: children = the annotation `TYPE` (only when
//!   written), then the value expression.
//! - `TEST off len`: child = `BLOCK`. (The test NAME is not printed: the
//!   Rust AST stores the decoded string while the selfhost AST stores its
//!   span, and the span already pins the text byte-exactly.)
//! - `STRUCT off len <p> <name>`: children = `SFIELD`s then method `FN`s.
//! - `SFIELD off len <name>`: child = `TYPE`.
//! - `ENUM off len <p> <name>`: children = `VARIANT`s.
//! - `VARIANT off len <name>[ =<value>]` (`=<value>` only when explicit).
//! - `UNION off len <p> <name>`: children = `UVAR`s.
//! - `UVAR off len <name>`: child = the payload `TYPE`.
//! - `IMPORT off len` (the path is span-pinned, not printed).
//! - `ERRSET off len <p> <name>[ <member>]*` (members inline; no children).
//! - `TYPE off len <name>[ opt][ err][ errset=<S>][ ptr][ slice][ arr=<N>]
//!   [ arrp=<P>][ app]`: children = the ctor-arg `TYPE`s (only with ` app`;
//!   `Name()` prints ` app` with zero children). `@This()` prints as `Self`.
//! - `BLOCK off len`: children = the statements.
//! - `LET off len <c> <name>` (`c` = `is_const`): children = the annotation
//!   `TYPE` (when written), then the initializer.
//! - `ASSIGN off len <name> <op>` (`op` ∈ {none,add,sub,mul,div,rem}):
//!   child = the value. `PASSIGN off len <op>`: children = place, value.
//! - `RETURN off len`: child = the value (when present).
//! - `IF off len[ cap=<name>]`: children = cond, then-`BLOCK`, else-stmt
//!   (when present; an `else if` chain is a nested `IF`).
//! - `WHILE off len[ label=<name>][ cont]`: children = cond, the continue
//!   statement (only with ` cont`), body `BLOCK`.
//! - `FOR off len <elem>[ idx=<name>][ label=<name>]`: children = iter,
//!   body `BLOCK`.
//! - `BREAK off len[ label=<name>]`, `CONTINUE off len[ label=<name>]`.
//! - `DEFER off len` / `ERRDEFER off len`: child = the deferred statement.
//! - `SWITCH off len`: children = scrutinee, `ARM`s, then the default
//!   `BLOCK` (when present).
//! - `ARM off len[ cap=<name>][ r=<lo>..<hi>]*`: children = the value-label
//!   expressions, then the body `BLOCK`.
//! - Expressions: `INT off len <value>`, `FLOAT off len`, `STR off len`
//!   (float/string text is span-pinned, not re-rendered), `BOOL off len
//!   <0|1>`, `IDENT off len <name>`, `UNARY off len <neg|not|bnot>`,
//!   `BIN off len <op>` (op ∈ {add,sub,mul,div,rem,eq,ne,lt,le,gt,ge,and,or,
//!   band,bor,bxor,shl,shr}), `CALL off len <name>`, `COMPTIME off len`,
//!   `SLIT off len <name>` with `FINIT off len <name>` children (each with a
//!   value child), `FIELD off len <name>`, `UNREACHABLE off len`,
//!   `BUILTIN off len <name>`, `STRUCTTYPE off len` (children = `SFIELD`s
//!   then `FN`s), `MCALL off len <name>` (children = receiver then args),
//!   `NULL off len`, `ORELSE off len`, `UNWRAP off len`,
//!   `ERRLIT off len <name>`, `ENUMLIT off len <name>`, `ALIT off len`
//!   (children = elem `TYPE` then elements), `INDEX off len`,
//!   `ADDROF off len`, `DEREF off len`, `SLICE off len` (base, lo, hi),
//!   `TRY off len`, `CATCH off len[ cap=<name>]` (expr, default).
//!
//! A statement-expression prints the expression directly (no wrapper line);
//! a parenthesised expression prints its inner node (the Rust parser does
//! not extend spans over `( )`).
//!
//! # Errors
//!
//! For an input that fails to lex or parse the WHOLE dump is exactly one
//! line, `ERROR <code> <pos>`: code = the numeric part of the first
//! diagnostic (1/2 for lexical E0001/E0002; 200/201 for parse E0200/E0201)
//! and pos = its span start. The Rust parser collects several diagnostics
//! via recovery, but it pushes them in strict source order, so its FIRST
//! diagnostic always coincides with the selfhost parser's first-error stop
//! (E0201 — `pub test` — is the one non-fatal diagnostic: both sides record
//! it and keep parsing, so a later hard error never displaces it).
//!
//! Corpus: the v0.159 lexer corpus — every `.ks` under `tests/spec`,
//! `tests/std`, `tests/selfhost`, `examples`, `selfhost`, plus the bundled
//! `crates/kardc/src/std.ks`.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

use kardc::ast::{
    ArraySize, BinOp, Block, Expr, Func, Item, Module, Stmt, SwitchArm, TypeExpr, UnOp,
};
use kardc::backend::{BuildOptions, OptLevel};
use kardc::emit_c::EmitMode;
use kardc::span::Span;

/// Dump-line kinds the selfhost parser does NOT implement yet: a corpus file
/// whose reference dump contains one of these as its line kind is skipped
/// (visibly counted, bounded by [`MAX_DECLARED_SKIPS`]) instead of failing.
/// v0.160 implements the full grammar, so the list is EMPTY.
const DECLARED_UNIMPLEMENTED: &[&str] = &[];

/// Upper bound on files skipped via [`DECLARED_UNIMPLEMENTED`].
const MAX_DECLARED_SKIPS: usize = 0;

static COUNTER: AtomicUsize = AtomicUsize::new(0);

/// A process-unique temp path (the e2e/std-suite helper).
fn temp_path(tag: &str) -> PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!("kardc_selfhost_{}_{}_{}", tag, std::process::id(), n))
}

/// The repository root (this file lives in `crates/kardc/tests/`).
fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root should canonicalize")
}

// ---- the reference dumper ---------------------------------------------------

/// Renders a [`Module`] in the canonical dump format (module docs above).
struct Dumper {
    out: String,
}

/// `<name> <off> <len>` — every line's fixed prefix.
fn head(name: &str, sp: Span) -> String {
    format!("{} {} {}", name, sp.start, sp.end - sp.start)
}

/// The canonical spelling of a binary operator (the `BIN`/`ASSIGN` tables).
fn bin_name(op: BinOp) -> &'static str {
    match op {
        BinOp::Add => "add",
        BinOp::Sub => "sub",
        BinOp::Mul => "mul",
        BinOp::Div => "div",
        BinOp::Rem => "rem",
        BinOp::Eq => "eq",
        BinOp::Ne => "ne",
        BinOp::Lt => "lt",
        BinOp::Le => "le",
        BinOp::Gt => "gt",
        BinOp::Ge => "ge",
        BinOp::And => "and",
        BinOp::Or => "or",
        BinOp::BitAnd => "band",
        BinOp::BitOr => "bor",
        BinOp::BitXor => "bxor",
        BinOp::Shl => "shl",
        BinOp::Shr => "shr",
    }
}

/// The canonical spelling of a unary operator (the `UNARY` table).
fn un_name(op: UnOp) -> &'static str {
    match op {
        UnOp::Neg => "neg",
        UnOp::Not => "not",
        UnOp::BitNot => "bnot",
    }
}

/// The `<op>` slot of an `ASSIGN`/`PASSIGN` line: `none` for a plain `=`.
fn assign_name(op: &Option<BinOp>) -> &'static str {
    match op {
        None => "none",
        Some(b) => bin_name(*b),
    }
}

impl Dumper {
    fn line(&mut self, depth: usize, text: &str) {
        for _ in 0..depth {
            self.out.push_str("  ");
        }
        self.out.push_str(text);
        self.out.push('\n');
    }

    fn module(&mut self, m: &Module) {
        for it in &m.items {
            self.item(it, 0);
        }
    }

    fn item(&mut self, it: &Item, d: usize) {
        match it {
            Item::Func(f) => self.func(f, d),
            Item::Const(c) => {
                self.line(
                    d,
                    &format!("{} {} {}", head("CONST", c.span), c.is_pub as u8, c.name),
                );
                if let Some(ty) = &c.ty {
                    self.ty(ty, d + 1);
                }
                self.expr(&c.value, d + 1);
            }
            Item::Test(t) => {
                self.line(d, &head("TEST", t.span));
                self.block(&t.body, d + 1);
            }
            Item::Struct(s) => {
                self.line(
                    d,
                    &format!("{} {} {}", head("STRUCT", s.span), s.is_pub as u8, s.name),
                );
                for f in &s.fields {
                    self.line(d + 1, &format!("{} {}", head("SFIELD", f.span), f.name));
                    self.ty(&f.ty, d + 2);
                }
                for m in &s.methods {
                    self.func(m, d + 1);
                }
            }
            Item::Enum(e) => {
                self.line(
                    d,
                    &format!("{} {} {}", head("ENUM", e.span), e.is_pub as u8, e.name),
                );
                for v in &e.variants {
                    let mut l = format!("{} {}", head("VARIANT", v.span), v.name);
                    if let Some(n) = v.value {
                        l.push_str(&format!(" ={}", n));
                    }
                    self.line(d + 1, &l);
                }
            }
            Item::Union(u) => {
                self.line(
                    d,
                    &format!("{} {} {}", head("UNION", u.span), u.is_pub as u8, u.name),
                );
                for v in &u.variants {
                    self.line(d + 1, &format!("{} {}", head("UVAR", v.span), v.name));
                    self.ty(&v.payload, d + 2);
                }
            }
            Item::Import(i) => self.line(d, &head("IMPORT", i.span)),
            Item::ErrorSet(e) => {
                let mut l = format!("{} {} {}", head("ERRSET", e.span), e.is_pub as u8, e.name);
                for m in &e.members {
                    l.push(' ');
                    l.push_str(m);
                }
                self.line(d, &l);
            }
        }
    }

    fn func(&mut self, f: &Func, d: usize) {
        self.line(
            d,
            &format!("{} {} {}", head("FN", f.span), f.is_pub as u8, f.name),
        );
        for p in &f.params {
            self.line(
                d + 1,
                &format!("{} {} {}", head("PARAM", p.span), p.is_comptime as u8, p.name),
            );
            self.ty(&p.ty, d + 2);
        }
        self.ty(&f.ret, d + 1);
        self.block(&f.body, d + 1);
    }

    fn ty(&mut self, t: &TypeExpr, d: usize) {
        let mut l = format!("{} {}", head("TYPE", t.span), t.name);
        if t.optional {
            l.push_str(" opt");
        }
        if t.error_union {
            l.push_str(" err");
        }
        if let Some(s) = &t.error_set {
            l.push_str(&format!(" errset={}", s));
        }
        if t.pointer {
            l.push_str(" ptr");
        }
        if t.slice {
            l.push_str(" slice");
        }
        match &t.array_len {
            Some(ArraySize::Lit(n)) => l.push_str(&format!(" arr={}", n)),
            Some(ArraySize::Param(p)) => l.push_str(&format!(" arrp={}", p)),
            None => {}
        }
        if t.ctor_args.is_some() {
            l.push_str(" app");
        }
        self.line(d, &l);
        if let Some(args) = &t.ctor_args {
            for a in args {
                self.ty(a, d + 1);
            }
        }
    }

    fn block(&mut self, b: &Block, d: usize) {
        self.line(d, &head("BLOCK", b.span));
        for s in &b.stmts {
            self.stmt(s, d + 1);
        }
    }

    fn stmt(&mut self, s: &Stmt, d: usize) {
        match s {
            Stmt::Let {
                is_const,
                name,
                ty,
                value,
                span,
            } => {
                self.line(
                    d,
                    &format!("{} {} {}", head("LET", *span), *is_const as u8, name),
                );
                if let Some(t) = ty {
                    self.ty(t, d + 1);
                }
                self.expr(value, d + 1);
            }
            Stmt::Assign {
                name,
                op,
                value,
                span,
            } => {
                self.line(
                    d,
                    &format!("{} {} {}", head("ASSIGN", *span), name, assign_name(op)),
                );
                self.expr(value, d + 1);
            }
            Stmt::FieldAssign {
                place,
                op,
                value,
                span,
            } => {
                self.line(d, &format!("{} {}", head("PASSIGN", *span), assign_name(op)));
                self.expr(place, d + 1);
                self.expr(value, d + 1);
            }
            Stmt::Expr(e) => self.expr(e, d),
            Stmt::Return { value, span } => {
                self.line(d, &head("RETURN", *span));
                if let Some(v) = value {
                    self.expr(v, d + 1);
                }
            }
            Stmt::If {
                cond,
                capture,
                then,
                els,
                span,
            } => {
                let mut l = head("IF", *span);
                if let Some(c) = capture {
                    l.push_str(&format!(" cap={}", c));
                }
                self.line(d, &l);
                self.expr(cond, d + 1);
                self.block(then, d + 1);
                if let Some(e) = els {
                    self.stmt(e, d + 1);
                }
            }
            Stmt::While {
                cond,
                cont,
                body,
                label,
                span,
            } => {
                let mut l = head("WHILE", *span);
                if let Some(n) = label {
                    l.push_str(&format!(" label={}", n));
                }
                if cont.is_some() {
                    l.push_str(" cont");
                }
                self.line(d, &l);
                self.expr(cond, d + 1);
                if let Some(c) = cont {
                    self.stmt(c, d + 1);
                }
                self.block(body, d + 1);
            }
            Stmt::For {
                iter,
                elem,
                index,
                body,
                label,
                span,
            } => {
                let mut l = format!("{} {}", head("FOR", *span), elem);
                if let Some(i) = index {
                    l.push_str(&format!(" idx={}", i));
                }
                if let Some(n) = label {
                    l.push_str(&format!(" label={}", n));
                }
                self.line(d, &l);
                self.expr(iter, d + 1);
                self.block(body, d + 1);
            }
            Stmt::Break { target, span } => {
                let mut l = head("BREAK", *span);
                if let Some(n) = target {
                    l.push_str(&format!(" label={}", n));
                }
                self.line(d, &l);
            }
            Stmt::Continue { target, span } => {
                let mut l = head("CONTINUE", *span);
                if let Some(n) = target {
                    l.push_str(&format!(" label={}", n));
                }
                self.line(d, &l);
            }
            Stmt::Defer { stmt, span } => {
                self.line(d, &head("DEFER", *span));
                self.stmt(stmt, d + 1);
            }
            Stmt::ErrDefer { stmt, span } => {
                self.line(d, &head("ERRDEFER", *span));
                self.stmt(stmt, d + 1);
            }
            Stmt::Block(b) => self.block(b, d),
            Stmt::Switch {
                scrutinee,
                arms,
                default,
                span,
            } => {
                self.line(d, &head("SWITCH", *span));
                self.expr(scrutinee, d + 1);
                for arm in arms {
                    self.arm(arm, d + 1);
                }
                if let Some(b) = default {
                    self.block(b, d + 1);
                }
            }
        }
    }

    fn arm(&mut self, arm: &SwitchArm, d: usize) {
        let mut l = head("ARM", arm.span);
        if let Some(c) = &arm.capture {
            l.push_str(&format!(" cap={}", c));
        }
        for (lo, hi) in &arm.ranges {
            l.push_str(&format!(" r={}..{}", lo, hi));
        }
        self.line(d, &l);
        for label in &arm.labels {
            self.expr(label, d + 1);
        }
        self.block(&arm.body, d + 1);
    }

    fn expr(&mut self, e: &Expr, d: usize) {
        match e {
            Expr::Int { value, span } => {
                self.line(d, &format!("{} {}", head("INT", *span), value));
            }
            Expr::Float { span, .. } => self.line(d, &head("FLOAT", *span)),
            Expr::Bool { value, span } => {
                self.line(d, &format!("{} {}", head("BOOL", *span), *value as u8));
            }
            Expr::Ident { name, span } => {
                self.line(d, &format!("{} {}", head("IDENT", *span), name));
            }
            Expr::Unary { op, expr, span } => {
                self.line(d, &format!("{} {}", head("UNARY", *span), un_name(*op)));
                self.expr(expr, d + 1);
            }
            Expr::Binary { op, lhs, rhs, span } => {
                self.line(d, &format!("{} {}", head("BIN", *span), bin_name(*op)));
                self.expr(lhs, d + 1);
                self.expr(rhs, d + 1);
            }
            Expr::Call { callee, args, span } => {
                self.line(d, &format!("{} {}", head("CALL", *span), callee));
                for a in args {
                    self.expr(a, d + 1);
                }
            }
            Expr::Comptime { expr, span } => {
                self.line(d, &head("COMPTIME", *span));
                self.expr(expr, d + 1);
            }
            Expr::StructLit { name, fields, span } => {
                self.line(d, &format!("{} {}", head("SLIT", *span), name));
                for f in fields {
                    self.line(d + 1, &format!("{} {}", head("FINIT", f.span), f.name));
                    self.expr(&f.value, d + 2);
                }
            }
            Expr::Field { base, field, span } => {
                self.line(d, &format!("{} {}", head("FIELD", *span), field));
                self.expr(base, d + 1);
            }
            Expr::StrLit { span, .. } => self.line(d, &head("STR", *span)),
            Expr::Unreachable { span } => self.line(d, &head("UNREACHABLE", *span)),
            Expr::Builtin { name, args, span } => {
                self.line(d, &format!("{} {}", head("BUILTIN", *span), name));
                for a in args {
                    self.expr(a, d + 1);
                }
            }
            Expr::StructType {
                fields,
                methods,
                span,
            } => {
                self.line(d, &head("STRUCTTYPE", *span));
                for f in fields {
                    self.line(d + 1, &format!("{} {}", head("SFIELD", f.span), f.name));
                    self.ty(&f.ty, d + 2);
                }
                for m in methods {
                    self.func(m, d + 1);
                }
            }
            Expr::MethodCall {
                receiver,
                method,
                args,
                span,
            } => {
                self.line(d, &format!("{} {}", head("MCALL", *span), method));
                self.expr(receiver, d + 1);
                for a in args {
                    self.expr(a, d + 1);
                }
            }
            Expr::Null { span } => self.line(d, &head("NULL", *span)),
            Expr::Orelse { lhs, rhs, span } => {
                self.line(d, &head("ORELSE", *span));
                self.expr(lhs, d + 1);
                self.expr(rhs, d + 1);
            }
            Expr::Unwrap { expr, span } => {
                self.line(d, &head("UNWRAP", *span));
                self.expr(expr, d + 1);
            }
            Expr::ErrorLit { name, span } => {
                self.line(d, &format!("{} {}", head("ERRLIT", *span), name));
            }
            Expr::EnumLit { variant, span } => {
                self.line(d, &format!("{} {}", head("ENUMLIT", *span), variant));
            }
            Expr::ArrayLit { elem, elems, span } => {
                self.line(d, &head("ALIT", *span));
                self.ty(elem, d + 1);
                for e in elems {
                    self.expr(e, d + 1);
                }
            }
            Expr::Index { base, index, span } => {
                self.line(d, &head("INDEX", *span));
                self.expr(base, d + 1);
                self.expr(index, d + 1);
            }
            Expr::AddrOf { place, span } => {
                self.line(d, &head("ADDROF", *span));
                self.expr(place, d + 1);
            }
            Expr::Deref { expr, span } => {
                self.line(d, &head("DEREF", *span));
                self.expr(expr, d + 1);
            }
            Expr::SliceExpr { base, lo, hi, span } => {
                self.line(d, &head("SLICE", *span));
                self.expr(base, d + 1);
                self.expr(lo, d + 1);
                self.expr(hi, d + 1);
            }
            Expr::Try { expr, span } => {
                self.line(d, &head("TRY", *span));
                self.expr(expr, d + 1);
            }
            Expr::Catch {
                expr,
                capture,
                default,
                span,
            } => {
                let mut l = head("CATCH", *span);
                if let Some(c) = capture {
                    l.push_str(&format!(" cap={}", c));
                }
                self.line(d, &l);
                self.expr(expr, d + 1);
                self.expr(default, d + 1);
            }
        }
    }
}

/// The reference dump: lex + parse `src` with the Rust toolchain and render
/// the canonical format (module docs). For an erroneous input this is the
/// single `ERROR <code> <pos>` line built from the FIRST diagnostic.
fn rust_dump(src: &str) -> String {
    let tokens = match kardc::lexer::lex(src) {
        Ok(t) => t,
        Err(diags) => {
            let d = &diags[0];
            let code = match d.code {
                "E0001" => 1,
                "E0002" => 2,
                other => panic!("unexpected lexer diagnostic code {other}"),
            };
            return format!("ERROR {} {}\n", code, d.span.start);
        }
    };
    match kardc::parser::parse(&tokens) {
        Ok(module) => {
            let mut dumper = Dumper { out: String::new() };
            dumper.module(&module);
            dumper.out
        }
        Err(diags) => {
            let d = &diags[0];
            let code = match d.code {
                "E0200" => 200,
                "E0201" => 201,
                other => panic!("unexpected parser diagnostic code {other}"),
            };
            format!("ERROR {} {}\n", code, d.span.start)
        }
    }
}

// ---- harness ----------------------------------------------------------------

/// Compile `selfhost/astdump.ks` (program mode, `-O0`) to a temp executable.
fn build_astdump() -> PathBuf {
    let src = repo_root().join("selfhost/astdump.ks");
    let c = kardc::compile_program(&src, EmitMode::Program).unwrap_or_else(|diags| {
        let text = std::fs::read_to_string(&src).unwrap_or_default();
        panic!(
            "selfhost/astdump.ks failed to compile:\n{}",
            kardc::diag::render_all(&diags, &src.display().to_string(), &text)
        )
    });
    let exe = temp_path("astdump");
    let opts = BuildOptions {
        opt: OptLevel::O0,
        ..BuildOptions::default()
    };
    kardc::backend::cc_build(&c, &exe, &opts).expect("cc should build astdump");
    exe
}

/// Recursively collect every `.ks` file under `dir` (fixtures included).
fn collect_ks(dir: &Path, out: &mut Vec<PathBuf>) {
    let entries = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("cannot read corpus dir {}: {e}", dir.display()));
    for entry in entries {
        let path = entry.expect("dir entry").path();
        if path.is_dir() {
            collect_ks(&path, out);
        } else if path.extension().is_some_and(|x| x == "ks") {
            out.push(path);
        }
    }
}

/// True if the reference dump uses a node kind the selfhost parser has
/// declared unimplemented (see [`DECLARED_UNIMPLEMENTED`]).
fn uses_unimplemented(expected: &str) -> bool {
    if DECLARED_UNIMPLEMENTED.is_empty() {
        return false;
    }
    expected.lines().any(|l| {
        let kind = l.trim_start().split(' ').next().unwrap_or("");
        DECLARED_UNIMPLEMENTED.contains(&kind)
    })
}

/// Run the astdump binary on `input` and diff its stdout against
/// [`rust_dump`]. `Ok(lines)` is the number of dump lines compared.
fn diff_one(exe: &Path, input: &Path, src: &str) -> Result<usize, String> {
    let expected = rust_dump(src);
    let out = Command::new(exe)
        .arg(input)
        .output()
        .unwrap_or_else(|e| panic!("failed to run astdump on {}: {e}", input.display()));
    if out.status.code() != Some(0) {
        return Err(format!(
            "{}: astdump exited {:?}\n--- stderr ---\n{}",
            input.display(),
            out.status.code(),
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    let got = String::from_utf8_lossy(&out.stdout);
    if got != expected {
        // Name the first divergent line so a corpus-wide failure is readable.
        let g: Vec<&str> = got.lines().collect();
        let e: Vec<&str> = expected.lines().collect();
        let mut i = 0;
        while i < g.len() && i < e.len() && g[i] == e[i] {
            i += 1;
        }
        return Err(format!(
            "{}: dump mismatch at line {} — rust `{}` vs selfhost `{}` ({} vs {} lines)",
            input.display(),
            i + 1,
            e.get(i).unwrap_or(&"<eof>"),
            g.get(i).unwrap_or(&"<eof>"),
            e.len(),
            g.len()
        ));
    }
    Ok(expected.lines().count())
}

/// (a) The full-repository differential corpus: every real `.ks` source in
/// the repo, byte-for-byte. One shared `-O0` astdump build, one subprocess
/// execution per file, so the corpus is NOT capped.
#[test]
fn selfhost_parser_differential_corpus() {
    let root = repo_root();
    let exe = build_astdump();

    let mut corpus: Vec<PathBuf> = Vec::new();
    collect_ks(&root.join("tests/spec"), &mut corpus);
    collect_ks(&root.join("tests/std"), &mut corpus);
    collect_ks(&root.join("tests/selfhost"), &mut corpus);
    collect_ks(&root.join("examples"), &mut corpus);
    collect_ks(&root.join("selfhost"), &mut corpus);
    corpus.push(root.join("crates/kardc/src/std.ks"));
    corpus.sort();
    corpus.dedup();
    assert!(
        corpus.len() >= 300,
        "differential corpus shrank to {} files — expected the full tree (650+)",
        corpus.len()
    );

    let mut failures: Vec<String> = Vec::new();
    let mut skipped: Vec<String> = Vec::new();
    let mut lines_total = 0usize;
    for file in &corpus {
        let src = match std::fs::read_to_string(file) {
            Ok(s) => s,
            Err(e) => {
                failures.push(format!("{}: unreadable corpus file: {e}", file.display()));
                continue;
            }
        };
        if uses_unimplemented(&rust_dump(&src)) {
            skipped.push(file.display().to_string());
            continue;
        }
        match diff_one(&exe, file, &src) {
            Ok(lines) => lines_total += lines,
            Err(msg) => failures.push(msg),
        }
    }
    let _ = std::fs::remove_file(&exe);

    assert!(
        skipped.len() <= MAX_DECLARED_SKIPS,
        "{} files skipped via DECLARED_UNIMPLEMENTED (bound {}):\n{}",
        skipped.len(),
        MAX_DECLARED_SKIPS,
        skipped.join("\n")
    );
    assert!(
        failures.is_empty(),
        "{} of {} corpus files mismatched the Rust parser:\n{}",
        failures.len(),
        corpus.len(),
        failures.join("\n")
    );
    println!(
        "selfhost parser differential: {} files ({} skipped), {} dump lines byte-identical",
        corpus.len(),
        skipped.len(),
        lines_total
    );
}

/// (b) Targeted parse-error / edge inputs (written to temp files): both
/// parsers must agree on the single `ERROR <code> <pos>` line — and on the
/// tricky clean inputs' dumps.
#[test]
fn selfhost_parser_differential_error_inputs() {
    let exe = build_astdump();
    let cases: &[(&str, &str)] = &[
        // Parse errors: code 200 at an exact position.
        ("stray_top_level", "fn f() void {}\n+"),
        ("missing_semi", "fn f() void { return 1 }"),
        ("missing_body", "fn f() void return;"),
        ("pub_test", "pub test \"t\" {}"),
        ("pub_test_then_error", "pub test \"t\" {}\nfn f( void {}"),
        ("error_before_pub_test", "fn f( void {}\npub test \"t\" {}"),
        ("bad_type_prefix", "fn f() ?!i32 {}"),
        ("for_nonzero_index", "fn f(xs: []i32) void { for (xs, 1..) |x, i| {} }"),
        ("for_arity_one", "fn f(xs: []i32) void { for (xs, 0..) |x| {} }"),
        ("for_arity_two", "fn f(xs: []i32) void { for (xs) |x, i| {} }"),
        ("at_not_import", "@notimport(\"x\");"),
        ("this_with_args", "fn f() void { var x: @This()(i32) = 0; }"),
        ("eof_mid_item", "pub fn f("),
        ("empty", ""),
        // Lex errors surface through the same ERROR contract.
        ("lex_unterminated", "fn f() void { var s = \"oops; }"),
        ("lex_overflow", "const X = 99999999999999999999;"),
        // Tricky clean inputs.
        ("else_if_chain", "fn f(x: i32) void { if (x == 1) {} else if (x == 2) {} else {} }"),
        ("orelse_catch_mix", "fn f(o: ?i32, e: !i32) void { var x = o orelse 1 catch 2; }"),
        (
            "switch_ranges_multi",
            "fn f(x: i32) void { switch (x) { 1, 2 => {}, 3..9, 12 => {}, else => {} } }",
        ),
        ("paren_int_range", "fn f(x: i32) void { switch (x) { (1)..3 => {}, else => {} } }"),
        ("trailing_commas", "fn g(a: i32,) void { h(1,); var p = P{ .x = 1, }; }"),
        ("ctor_app_nested", "fn f(comptime T: type) void { var x: Map(K(i32), V()) = undefinedz(); }"),
        ("deref_chain_assign", "fn f(p: *P) void { p.*.x.y[0] += 2; }"),
        ("labeled_loops", "fn f() void { outer: while (true) { inner: for (xs) |x| { break :outer; } } }"),
        ("set_err_union", "const E = error{ A, B };\nfn f() E!i32 { return 1; }"),
        ("comptime_expr", "fn f() void { var x = comptime g(1) + 2; }"),
        ("leading_zeros_int", "const X = 000123;"),
        ("anon_struct_value", "fn F(comptime T: type) type { return struct { v: T, fn get(self: Self) T { return self.v; } }; }"),
    ];
    let mut failures: Vec<String> = Vec::new();
    for (tag, src) in cases {
        let input = temp_path(&format!("perr_{tag}"));
        std::fs::write(&input, src).expect("write temp error input");
        if let Err(msg) = diff_one(&exe, &input, src) {
            failures.push(format!("[{tag}] {msg}"));
        }
        let _ = std::fs::remove_file(&input);
    }
    let _ = std::fs::remove_file(&exe);
    assert!(
        failures.is_empty(),
        "{} targeted inputs mismatched:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// (c) The in-language suite: `tests/selfhost/parser_suite.ks` must compile
/// in test mode and report every test passing (exit code 0 = failure count).
#[test]
fn selfhost_parser_suite_passes() {
    let suite = repo_root().join("tests/selfhost/parser_suite.ks");
    let c = kardc::compile_program(&suite, EmitMode::Test).unwrap_or_else(|diags| {
        let text = std::fs::read_to_string(&suite).unwrap_or_default();
        panic!(
            "parser_suite.ks failed to compile:\n{}",
            kardc::diag::render_all(&diags, &suite.display().to_string(), &text)
        )
    });
    let exe = temp_path("psuite");
    let opts = BuildOptions {
        opt: OptLevel::O0,
        ..BuildOptions::default()
    };
    kardc::backend::cc_build(&c, &exe, &opts).expect("cc should build the suite harness");
    let output = Command::new(&exe).output().expect("should run the harness");
    let _ = std::fs::remove_file(&exe);
    assert_eq!(
        output.status.code(),
        Some(0),
        "parser_suite.ks had failing tests:\n--- stderr ---\n{}\n--- stdout ---\n{}",
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout)
    );
}
