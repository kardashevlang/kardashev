//! The canonical source formatter (`kard fmt`): lex, parse, then pretty-print
//! the AST with standard spacing and indentation (SPEC §6).
//!
//! The formatter is a pure function of the parsed [`Module`]: it never inspects
//! the original byte layout. The public entry point [`format_source`] runs the
//! front-end ([`crate::lexer::lex`] + [`crate::parser::parse`]) and delegates to
//! the pure printer [`print_module`].
//!
//! ## Canonical style
//!
//! - 4-space indentation, one statement per line.
//! - Items separated by a single blank line; file ends with one newline.
//! - `pub fn name(a: T, b: T) R { … }` — return type follows the parens, no
//!   arrow (Zig style). A compile-time type parameter prints with a leading
//!   `comptime ` keyword — `fn max(comptime T: type, a: T, b: T) T { … }`
//!   (SPEC §17).
//! - Spaces around every binary operator (`a + b`, `a and b`).
//! - `if (cond) { … } else if (cond) { … } else { … }`. The optional-payload
//!   form binds the unwrapped value: `if (opt) |v| { … } else { … }` (SPEC §21).
//! - `while (cond) { … }` / `while (cond) : (cont) { … }`. A loop may carry a
//!   label (SPEC §40): `label: while (cond) { … }` / `label: for (…) |x| { … }`
//!   print the `label: ` before the loop keyword. `break;` / `continue;` are the
//!   unlabeled forms; `break :label;` / `continue :label;` target the named
//!   enclosing loop. An unlabeled loop or break/continue is unchanged.
//! - `const NAME: T = expr;` / `var name: T = expr;` / `return expr;`. The type
//!   annotation is optional (SPEC §18): an inferred binding prints with no
//!   `: T` — `const NAME = expr;` / `var name = expr;`.
//! - `defer <stmt>`; `errdefer <stmt>` — the latter mirrors `defer`'s printing
//!   but runs only on error-return paths (SPEC §21); `test "name" { … }`.
//! - `const Name = struct { f: T, … };` — one field per line, 4-space indent,
//!   trailing comma on each; an empty struct prints `const Name = struct {};`.
//!   Struct literals print `Name{ .f = e, … }` and field access `base.field`
//!   (SPEC §9).
//! - `const Name = enum { A, B, … };` — one variant per line, 4-space indent,
//!   trailing comma on each; an empty enum prints `const Name = enum {};`. A
//!   variant carrying an explicit integer value prints `A = N,` (SPEC §37); a
//!   variant without one keeps the bare `A,`. An unqualified enum literal prints
//!   `.Variant`; the qualified form reuses field access (`Enum.Variant`). A
//!   `switch` prints with each arm `labels => { … }` indented one level, arms
//!   comma-terminated, an `else` arm last (SPEC §13). The `@intFromEnum(e)` /
//!   `@enumFromInt(E, n)` conversions (SPEC §37) are `Expr::Builtin`s and print
//!   through the shared builtin printer as `@intFromEnum(<e>)` /
//!   `@enumFromInt(<E>, <n>)`.
//! - `const Name = union(enum) { v: T, … };` — a tagged union, one `v: T` per
//!   line, 4-space indent, trailing comma on each, mirroring the struct form;
//!   an empty union prints `const Name = union(enum) {};`. Union construction
//!   reuses the struct-literal form (`Name{ .v = e }`). A `switch` arm that
//!   binds the matched variant's payload prints `labels => |cap| { … }` (SPEC
//!   §20).
//! - `const Name = error{ A, B };` — a named error set (SPEC §34), printed on a
//!   single line with its members joined by `, ` inside `error{ … }` (one space
//!   just inside each brace, no trailing comma); an empty set prints `const Name
//!   = error{};`. A named error union over such a set prints the set name before
//!   the `!` — `E!i32` — while the implicit global `!i32` is unchanged.
//! - `unreachable` (SPEC §35) — the diverging runtime-safety primitive prints as
//!   the bare keyword, both as a statement (`unreachable;`) and in a switch arm
//!   body. `@panic(msg)` is an `Expr::Builtin` and prints through the shared
//!   builtin printer as `@panic(<msg>)`, its `[]u8` argument a string literal.
//! - `ArrayList(i32)` — a generic type-constructor application directly in type
//!   position (SPEC §42.1) prints as `Name(a1, a2)` with each argument rendered
//!   recursively (`ArrayList(ArrayList(i32))`), composing with the `?`/`!`/
//!   `*`/`[]`/`[N]` prefix forms through the single [`fmt_type`] spelling.
//!
//! ## Idempotence
//!
//! Parenthesisation is precedence-driven and minimal, so re-formatting the
//! canonical output produces byte-identical text.

use crate::ast::{
    ArraySize, BinOp, Block, ConstDecl, EnumDecl, ErrorSetDecl, Expr, Func, Item, Module, Stmt,
    StructDecl, SwitchArm, TestBlock, TypeExpr, UnOp, UnionDecl,
};
use crate::diag::Diagnostic;

/// Parse `src` and re-emit it in canonical form.
///
/// Returns the formatted source on success, or every diagnostic gathered by the
/// lexer / parser.
pub fn format_source(src: &str) -> Result<String, Vec<Diagnostic>> {
    let tokens = crate::lexer::lex(src)?;
    let module = crate::parser::parse(&tokens)?;
    Ok(print_module(&module))
}

/// Pretty-print a whole [`Module`] to canonical source text.
///
/// Pure: depends only on the AST, so the output is deterministic and the
/// front-end is not involved.
pub fn print_module(module: &Module) -> String {
    let mut p = Printer::new();
    for (i, item) in module.items.iter().enumerate() {
        if i > 0 {
            // A single blank line between top-level items. The previous item
            // already ended with a newline, so one more produces the blank line.
            p.out.push('\n');
        }
        match item {
            Item::Func(f) => p.print_func(f),
            Item::Const(c) => p.print_const(c),
            Item::Test(t) => p.print_test(t),
            Item::Struct(s) => p.print_struct(s),
            Item::Enum(e) => p.print_enum(e),
            Item::Union(u) => p.print_union(u),
            Item::ErrorSet(es) => p.print_error_set(es),
            Item::Import(im) => {
                p.out
                    .push_str(&format!("@import({});\n", escape_string(&im.path)));
            }
        }
    }
    p.out
}

/// Accumulates the formatted text while tracking the current indent depth.
struct Printer {
    out: String,
    indent: usize,
    /// When set, the next `write_indent` is suppressed (used so a `defer`
    /// keyword and the statement it guards share a line).
    suppress_indent: bool,
}

impl Printer {
    fn new() -> Printer {
        Printer {
            out: String::new(),
            indent: 0,
            suppress_indent: false,
        }
    }

    /// Emit the leading indentation for a new line, unless suppressed.
    fn write_indent(&mut self) {
        if self.suppress_indent {
            self.suppress_indent = false;
            return;
        }
        for _ in 0..self.indent {
            self.out.push_str("    ");
        }
    }

    // ----- top-level items -------------------------------------------------

    fn print_func(&mut self, f: &Func) {
        self.write_indent();
        // The `pub? fn name(params) ret` signature spelling is shared with the
        // inline printer via [`fmt_func_sig`]; only the layout differs.
        self.out.push_str(&fmt_func_sig(f));
        self.out.push_str(" {\n");
        self.print_block_body(&f.body);
        self.write_indent();
        self.out.push_str("}\n");
    }

    fn print_const(&mut self, c: &ConstDecl) {
        self.write_indent();
        if c.is_pub {
            self.out.push_str("pub ");
        }
        self.out.push_str("const ");
        self.out.push_str(&c.name);
        // The type annotation is optional (SPEC §18): `const NAME: T = expr;`
        // when present, `const NAME = expr;` when inferred from the initializer.
        if let Some(ty) = &c.ty {
            self.out.push_str(": ");
            self.out.push_str(&fmt_type(ty));
        }
        self.out.push_str(" = ");
        self.out.push_str(&fmt_expr(&c.value));
        self.out.push_str(";\n");
    }

    /// Print a struct declaration (SPEC §9/§10). One `    field: Type,` per line
    /// with a trailing comma on every field; then, after the fields, each
    /// method / associated function (`pub? fn …`) printed one indent deep with
    /// the ordinary function printer, separated by a single blank line (SPEC
    /// §10). An empty struct — no fields *and* no methods — collapses to
    /// `const Name = struct {};` on a single line.
    fn print_struct(&mut self, s: &StructDecl) {
        self.write_indent();
        if s.is_pub {
            self.out.push_str("pub ");
        }
        self.out.push_str("const ");
        self.out.push_str(&s.name);
        self.out.push_str(" = struct {");
        if s.fields.is_empty() && s.methods.is_empty() {
            self.out.push_str("};\n");
            return;
        }
        self.out.push('\n');
        self.indent += 1;
        for field in &s.fields {
            self.write_indent();
            self.out.push_str(&field.name);
            self.out.push_str(": ");
            self.out.push_str(&fmt_type(&field.ty));
            self.out.push_str(",\n");
        }
        // Each method is a `pub? fn …` printed at the struct body's indent
        // using the same printer as a top-level function. A single blank line
        // separates the fields from the first method and each method from the
        // next; the previous line already ends in `\n`, so one extra `\n`
        // yields the blank line.
        for (i, method) in s.methods.iter().enumerate() {
            if i > 0 || !s.fields.is_empty() {
                self.out.push('\n');
            }
            self.print_func(method);
        }
        self.indent -= 1;
        self.write_indent();
        self.out.push_str("};\n");
    }

    /// Print an enum declaration (SPEC §13/§37). One `    Variant,` per line with
    /// a 4-space indent and a trailing comma on every variant, then `};` to close.
    /// A variant carrying an explicit integer value (`EnumVariant.value =
    /// Some(N)`, SPEC §37) prints `    Variant = N,`; a variant without one
    /// (`None`, the auto-incrementing case) prints the bare `    Variant,`,
    /// exactly as in v0.116. An empty enum — no variants — collapses to
    /// `const Name = enum {};` on a single line. A `pub` enum keeps its leading
    /// `pub`. The printed form re-lexes to the same `EnumDecl` (the parser reads
    /// the optional `= N` per variant), so re-formatting is idempotent.
    fn print_enum(&mut self, e: &EnumDecl) {
        self.write_indent();
        if e.is_pub {
            self.out.push_str("pub ");
        }
        self.out.push_str("const ");
        self.out.push_str(&e.name);
        self.out.push_str(" = enum {");
        if e.variants.is_empty() {
            self.out.push_str("};\n");
            return;
        }
        self.out.push('\n');
        self.indent += 1;
        for variant in &e.variants {
            self.write_indent();
            self.out.push_str(&variant.name);
            // An explicit value (`= N`, SPEC §37) prints after the variant name
            // with single spaces around the `=`; a `None` value (auto-increment)
            // prints nothing extra, leaving the v0.116 bare-`Variant` form.
            if let Some(n) = variant.value {
                self.out.push_str(" = ");
                self.out.push_str(&n.to_string());
            }
            self.out.push_str(",\n");
        }
        self.indent -= 1;
        self.write_indent();
        self.out.push_str("};\n");
    }

    /// Print a tagged-union declaration (SPEC §20). Mirrors the struct printer:
    /// one `    variant: PayloadType,` per line with a 4-space indent and a
    /// trailing comma on every variant, wrapped in `union(enum) { … };`. A
    /// variant-less union — which the grammar never produces, since every
    /// `union(enum)` variant carries a payload — collapses to
    /// `const Name = union(enum) {};` on a single line, matching the empty
    /// struct/enum forms and keeping the printer total. A `pub` union keeps its
    /// leading `pub`.
    fn print_union(&mut self, u: &UnionDecl) {
        self.write_indent();
        if u.is_pub {
            self.out.push_str("pub ");
        }
        self.out.push_str("const ");
        self.out.push_str(&u.name);
        self.out.push_str(" = union(enum) {");
        if u.variants.is_empty() {
            self.out.push_str("};\n");
            return;
        }
        self.out.push('\n');
        self.indent += 1;
        for variant in &u.variants {
            self.write_indent();
            self.out.push_str(&variant.name);
            self.out.push_str(": ");
            self.out.push_str(&fmt_type(&variant.payload));
            self.out.push_str(",\n");
        }
        self.indent -= 1;
        self.write_indent();
        self.out.push_str("};\n");
    }

    /// Print a named error-set declaration (SPEC §34): `pub? const Name = error{
    /// A, B };`. Unlike the multi-line struct/enum/union forms, an error set
    /// prints on a **single line** with its members joined by `, ` inside
    /// `error{ … }` — a brace-delimited list with one space just inside each
    /// brace, no trailing comma — mirroring the `error.X` literal spelling
    /// (SPEC §12). An empty set collapses to `const Name = error{};`, matching
    /// the empty struct/enum/union forms and keeping the printer total. A `pub`
    /// set keeps its leading `pub`. The printed form re-lexes to the same
    /// `Item::ErrorSet`, so re-formatting is idempotent.
    fn print_error_set(&mut self, es: &ErrorSetDecl) {
        self.write_indent();
        if es.is_pub {
            self.out.push_str("pub ");
        }
        self.out.push_str("const ");
        self.out.push_str(&es.name);
        self.out.push_str(" = error{");
        if es.members.is_empty() {
            self.out.push_str("};\n");
            return;
        }
        self.out.push(' ');
        for (i, member) in es.members.iter().enumerate() {
            if i > 0 {
                self.out.push_str(", ");
            }
            self.out.push_str(member);
        }
        self.out.push_str(" };\n");
    }

    fn print_test(&mut self, t: &TestBlock) {
        self.write_indent();
        self.out.push_str("test ");
        self.out.push_str(&escape_string(&t.name));
        self.out.push_str(" {\n");
        self.print_block_body(&t.body);
        self.write_indent();
        self.out.push_str("}\n");
    }

    // ----- statements ------------------------------------------------------

    /// Print every statement of `block` at one deeper indent level. Does not
    /// emit the surrounding braces.
    fn print_block_body(&mut self, block: &Block) {
        self.indent += 1;
        for stmt in &block.stmts {
            self.print_stmt(stmt);
        }
        self.indent -= 1;
    }

    fn print_stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::Let {
                is_const,
                name,
                ty,
                value,
                ..
            } => {
                self.write_indent();
                self.out.push_str(&fmt_let_core(*is_const, name, ty, value));
                self.out.push('\n');
            }
            // `name = expr;`, or a compound `name op= expr;` (SPEC §27): `op`
            // selects the operator spelling — `=` when `None`, `+= -= *= /= %=`
            // when `Some` — with a single space on each side either way.
            Stmt::Assign {
                name, op, value, ..
            } => {
                self.write_indent();
                self.out.push_str(&fmt_assign_core(name, *op, value));
                self.out.push('\n');
            }
            // `place = expr;`, or a compound `place op= expr;` (SPEC §27), where
            // `place` is a field-access / index / deref chain. The operator
            // spelling follows `op`, exactly as for [`Stmt::Assign`].
            Stmt::FieldAssign {
                place, op, value, ..
            } => {
                self.write_indent();
                self.out
                    .push_str(&fmt_assign_core(&fmt_expr(place), *op, value));
                self.out.push('\n');
            }
            Stmt::Expr(e) => {
                self.write_indent();
                self.out.push_str(&fmt_expr(e));
                self.out.push_str(";\n");
            }
            Stmt::Return { value, .. } => {
                self.write_indent();
                match value {
                    Some(e) => {
                        self.out.push_str("return ");
                        self.out.push_str(&fmt_expr(e));
                        self.out.push_str(";\n");
                    }
                    None => self.out.push_str("return;\n"),
                }
            }
            Stmt::If {
                cond,
                capture,
                then,
                els,
                ..
            } => self.print_if(cond, capture, then, els),
            // `while (cond) { … }` / `while (cond) : (cont) { … }`, optionally
            // labeled (SPEC §40): a `Some(name)` label prints `name: ` before
            // the `while` keyword — `outer: while (cond) { … }` — and is then
            // targetable by `break :name` / `continue :name`. An unlabeled loop
            // (`label == None`) is byte-for-byte the pre-v0.147 form.
            Stmt::While {
                cond,
                cont,
                body,
                label,
                ..
            } => {
                self.write_indent();
                self.out.push_str(&fmt_while_header(cond, cont, label));
                self.out.push_str(" {\n");
                self.print_block_body(body);
                self.write_indent();
                self.out.push_str("}\n");
            }
            // `for (<iter>) |elem| { … }` — iterate an array/slice, binding each
            // element by value (SPEC §29). The `, 0..` index form additionally
            // binds a 0-based `usize` and prints `for (<iter>, 0..) |elem, index|
            // { … }`: the `, 0..` follows the iterable inside the parens and a
            // second `, index` capture is appended between the pipes. `index =
            // None` is the plain form, exactly `for (<iter>) |elem| { … }`. The
            // body prints one indent deeper, like `while`. A `Some(name)` label
            // (SPEC §40) prints `name: ` before the `for` keyword — `outer: for
            // (<iter>) |elem| { … }` — exactly as for a labeled `while`; an
            // unlabeled loop (`label == None`) is unchanged.
            Stmt::For {
                iter,
                elem,
                index,
                body,
                label,
                ..
            } => {
                self.write_indent();
                self.out.push_str(&fmt_for_header(iter, elem, index, label));
                self.out.push_str(" {\n");
                self.print_block_body(body);
                self.write_indent();
                self.out.push_str("}\n");
            }
            // `break;` / `break :label;` (SPEC §40). An unlabeled `break`
            // (`target == None`) is the unchanged bare `break;`; a labeled one
            // (`target == Some(label)`) targets the named enclosing loop and
            // prints `break :label;`. (See [`fmt_break`].)
            Stmt::Break { target, .. } => {
                self.write_indent();
                self.out.push_str(&fmt_break(target));
                self.out.push('\n');
            }
            // `continue;` / `continue :label;` (SPEC §40), mirroring `break`.
            Stmt::Continue { target, .. } => {
                self.write_indent();
                self.out.push_str(&fmt_continue(target));
                self.out.push('\n');
            }
            Stmt::Defer { stmt, .. } => {
                self.write_indent();
                self.out.push_str("defer ");
                // The guarded statement shares the `defer` line: suppress the
                // indent it would otherwise emit for its first line.
                self.suppress_indent = true;
                self.print_stmt(stmt);
            }
            // `errdefer <stmt>` (SPEC §21.2) prints exactly like `defer`, with
            // the `errdefer` keyword: the keyword and the guarded statement
            // share one line, so the statement's leading indent is suppressed.
            Stmt::ErrDefer { stmt, .. } => {
                self.write_indent();
                self.out.push_str("errdefer ");
                self.suppress_indent = true;
                self.print_stmt(stmt);
            }
            Stmt::Block(b) => {
                self.write_indent();
                self.out.push_str("{\n");
                self.print_block_body(b);
                self.write_indent();
                self.out.push_str("}\n");
            }
            Stmt::Switch {
                scrutinee,
                arms,
                default,
                ..
            } => self.print_switch(scrutinee, arms, default),
        }
    }

    /// Print a `switch` statement (SPEC §13/§20). The header is
    /// `switch (<scrutinee>) {`; each arm is printed one indent deeper as
    /// `<labels> => {` (labels joined with `, `) followed by its body and a
    /// closing `},`. A tagged-union arm that binds the matched variant's payload
    /// (`SwitchArm.capture = Some(name)`, SPEC §20) prints the capture between
    /// the arrow and the block — `<labels> => |<name>| {`. The optional `else`
    /// arm prints last as `else => { … },` (it never carries a capture). Every
    /// arm — the `else` included — ends with a trailing comma (the parser accepts
    /// a trailing comma after a `}` block), which keeps the canonical form
    /// uniform and idempotent.
    fn print_switch(&mut self, scrutinee: &Expr, arms: &[SwitchArm], default: &Option<Block>) {
        self.write_indent();
        self.out.push_str("switch (");
        self.out.push_str(&fmt_expr(scrutinee));
        self.out.push_str(") {\n");
        self.indent += 1;
        for arm in arms {
            self.write_indent();
            // The `<labels> => |cap|?` arm opening is shared with the inline
            // printer via [`fmt_switch_arm_open`]; only the layout differs.
            self.out.push_str(&fmt_switch_arm_open(arm));
            self.out.push_str(" {\n");
            self.print_block_body(&arm.body);
            self.write_indent();
            self.out.push_str("},\n");
        }
        if let Some(block) = default {
            self.write_indent();
            self.out.push_str("else => {\n");
            self.print_block_body(block);
            self.write_indent();
            self.out.push_str("},\n");
        }
        self.indent -= 1;
        self.write_indent();
        self.out.push_str("}\n");
    }

    /// Print an `if`/`else if`/`else` chain. `cond`/`capture`/`then` are this
    /// `if`'s condition, optional payload capture and body; `els` is its optional
    /// trailing branch. With a capture (SPEC §21.1) the header prints
    /// `if (<cond>) |<name>| {`; without one it is the unchanged `if (<cond>) {`.
    /// An `else if` whose own `if` carries a capture chains the same way
    /// (`} else if (<cond>) |<name>| {`). The `if (<cond>) |<name>|?` header
    /// spelling is shared with the inline printer via [`fmt_if_header`]; only
    /// the brace/indent layout differs.
    fn print_if(
        &mut self,
        cond: &Expr,
        capture: &Option<String>,
        then: &Block,
        els: &Option<Box<Stmt>>,
    ) {
        self.write_indent();
        self.out.push_str(&fmt_if_header(cond, capture));
        self.out.push_str(" {\n");
        self.print_block_body(then);

        let mut els = els;
        loop {
            match els {
                None => {
                    self.write_indent();
                    self.out.push_str("}\n");
                    return;
                }
                Some(boxed) => match boxed.as_ref() {
                    Stmt::If {
                        cond: c2,
                        capture: cap2,
                        then: t2,
                        els: e2,
                        ..
                    } => {
                        self.write_indent();
                        self.out.push_str("} else ");
                        self.out.push_str(&fmt_if_header(c2, cap2));
                        self.out.push_str(" {\n");
                        self.print_block_body(t2);
                        els = e2;
                    }
                    Stmt::Block(b) => {
                        self.write_indent();
                        self.out.push_str("} else {\n");
                        self.print_block_body(b);
                        self.write_indent();
                        self.out.push_str("}\n");
                        return;
                    }
                    // The AST only ever stores an `If` or a `Block` here, but
                    // remain total: wrap any other statement in an else block.
                    other => {
                        self.write_indent();
                        self.out.push_str("} else {\n");
                        self.indent += 1;
                        self.print_stmt(other);
                        self.indent -= 1;
                        self.write_indent();
                        self.out.push_str("}\n");
                        return;
                    }
                },
            }
        }
    }
}

// ----- types ----------------------------------------------------------------

/// Format a type reference (SPEC §11.1 / §12.1 / §14.1 / §15 / §24 / §34 /
/// §42.1). The *base* spelling is the bare type name or, when `ctor_args` is
/// `Some(args)` (a generic type-constructor application, v0.152, SPEC §42.1),
/// the application `Name(a1, a2)` with each argument rendered recursively — so
/// a nested application (`ArrayList(ArrayList(i32))`) prints, and `Some(vec![])`
/// prints `Name()` (it re-lexes to the same `TypeExpr`; sema rejects it). A
/// pointer (`TypeExpr.pointer`) prints with a leading `*` — e.g. `*i32` — a slice
/// (`TypeExpr.slice`) with a leading `[]` — e.g. `[]i32` — an array
/// (`TypeExpr.array_len = Some(..)`) with a leading `[N]` whose `N` is either a
/// literal size (`ArraySize::Lit`, e.g. `[3]i32`, v0.117) or a comptime
/// value-parameter name (`ArraySize::Param`, e.g. `[n]i32`, v0.128) — an
/// optional type (`TypeExpr.optional`) with a leading `?` — e.g. `?i32` — an
/// error union (`TypeExpr.error_union`) with a leading `!` — e.g. `!i32` — or, for
/// a *named* error union over the error set `Set` (`TypeExpr.error_set =
/// Some("Set")`, v0.139), the set name before the `!` — e.g. `E!i32` — and a
/// plain type as its base spelling. The qualifiers are mutually exclusive
/// (v0.115: `?` and `!` are never combined; v0.117: `[N]` is not combined with
/// either; v0.118: `*`/`[]` are not combined with the others), so at most one
/// prefix is emitted; each wraps the base spelling, so an application composes
/// with every prefix form (`?Name(A)`, `*Name(A)`, …, SPEC §42.1). Used
/// wherever a type appears: params, return types, `var`/`const` annotations and
/// struct fields.
fn fmt_type(ty: &TypeExpr) -> String {
    // The base spelling (SPEC §42.1): the bare name, or — for a generic
    // type-constructor application (v0.152) — `Name(a1, a2)` with the
    // arguments `, `-joined and each rendered recursively (a nested
    // application recurses through `fmt_type`). Every prefix branch below
    // wraps this one spelling. The error-*set* name in `Set!T` is never an
    // application (SPEC §42.1), so only the payload spelling is involved.
    let base = match &ty.ctor_args {
        Some(args) => {
            let parts: Vec<String> = args.iter().map(fmt_type).collect();
            format!("{}({})", ty.name, parts.join(", "))
        }
        None => ty.name.clone(),
    };
    if ty.pointer {
        format!("*{}", base)
    } else if ty.slice {
        format!("[]{}", base)
    } else if let Some(size) = &ty.array_len {
        // A fixed-size array `[N]T`: the size prints inside the `[…]` prefix
        // before the element type. `ArraySize::Lit(n)` prints the literal
        // (`[3]i32`, v0.117); `ArraySize::Param(name)` prints the comptime
        // value-parameter name (`[n]i32`, v0.128). Both forms re-lex to the
        // same `TypeExpr`, so re-formatting is idempotent.
        match size {
            ArraySize::Lit(n) => format!("[{}]{}", n, base),
            ArraySize::Param(name) => format!("[{}]{}", name, base),
        }
    } else if ty.optional {
        format!("?{}", base)
    } else if ty.error_union {
        // An error union (SPEC §12/§34). The global form `!T` (`error_set ==
        // None`) prints with a bare leading `!` — `!i32` — unchanged from
        // v0.115. A **named** error union `Set!T` (`error_set == Some("Set")`,
        // v0.139) prints the set name before the `!` — `Set!i32` — so the set
        // constraint is visible and the printed form re-lexes to the same
        // `TypeExpr` (the parser reads a base type name `Set` followed by `!` as
        // a named error union).
        match &ty.error_set {
            Some(set) => format!("{}!{}", set, base),
            None => format!("!{}", base),
        }
    } else {
        base
    }
}

// ----- expressions ---------------------------------------------------------

/// Binding-power of an expression, used to insert minimal parentheses. Higher
/// binds tighter. Mirrors the grammar in SPEC §2 / §11 / §28.1.
///
/// The full ladder (loosest → tightest), matching the parser's
/// precedence-climbing chain:
///
/// ```text
///  0  orelse / catch
///  1  or
///  2  and
///  3  |   (BitOr)
///  4  ^   (BitXor)
///  5  &   (BitAnd)
///  6  == != (equality)
///  7  < <= > >= (relational)
///  8  << >> (shift)
///  9  + -  (additive)
/// 10  * / % (multiplicative)
/// 11  - ! ~ / try / &  (unary prefix)
/// 12  comptime
/// 13  primaries & postfix
/// ```
fn expr_prec(e: &Expr) -> u8 {
    match e {
        // Primaries and postfix forms (calls, struct literals, field access,
        // `null`, and the `.?` unwrap) bind tightest.
        Expr::Int { .. }
        // A floating-point literal `3.14` is a `f64` value (SPEC §38). Like an
        // integer literal it is an atomic primary, so it binds tightest.
        | Expr::Float { .. }
        | Expr::Bool { .. }
        | Expr::Ident { .. }
        | Expr::Call { .. }
        // A comptime reflection builtin `@name(args)` (SPEC §32.1) — e.g.
        // `@sizeOf(T)` / `@typeName(T)`. It is a call-shaped primary, so it
        // binds tightest like an ordinary `Expr::Call`.
        | Expr::Builtin { .. }
        | Expr::StructLit { .. }
        // An anonymous `struct { … }` **type value** (SPEC §25). It only ever
        // appears as the whole body of a type-returning function's `return`, so
        // it is never actually a sub-operand; treating it as an atomic primary
        // (like a struct literal) keeps the printer total and idempotent.
        | Expr::StructType { .. }
        | Expr::Field { .. }
        | Expr::MethodCall { .. }
        | Expr::Null { .. }
        // `error.Name` is atomic — a bare error literal binds as a primary.
        | Expr::ErrorLit { .. }
        // `.Variant` is atomic — an unqualified enum literal binds as a primary.
        | Expr::EnumLit { .. }
        // An array literal `[N]T{ … }` is a primary; indexing `a[i]` is postfix.
        // Both bind tightest (SPEC §14.1).
        | Expr::ArrayLit { .. }
        | Expr::Index { .. }
        // A string literal `"…"` is an atomic primary — a `[]u8` value over
        // static bytes (SPEC §23). It binds tightest, like an integer literal.
        | Expr::StrLit { .. }
        // `unreachable` (SPEC §35) — a diverging runtime-safety primitive that
        // adopts the expected type and never returns. It is a bare keyword
        // primary, so it binds tightest like a literal.
        | Expr::Unreachable { .. }
        // `expr.*` (deref) and `base[lo..hi]` (slice) are postfix forms and bind
        // as primaries, like `.?` and `a[i]` (SPEC §15).
        | Expr::Deref { .. }
        | Expr::SliceExpr { .. }
        | Expr::Unwrap { .. } => 13,
        Expr::Comptime { .. } => 12,
        // `try expr` is a prefix form (SPEC §12.1), at the same binding power as
        // the other prefixes (`-`/`!`/`~`); `&place` (address-of) is likewise a
        // prefix (SPEC §15.1 / §28). v0.115 only ever produces `try` at a
        // statement position, so it is rarely a sub-operand; this keeps the
        // printer total.
        Expr::Unary { .. } | Expr::Try { .. } | Expr::AddrOf { .. } => 11,
        // Binary operators (SPEC §28.1). Bitwise `& | ^` and the shifts `<< >>`
        // slot into the C-like ladder: `|` < `^` < `&` < equality < relational
        // < shift < additive. Equality and relational are now distinct levels
        // (shift binds tighter than both).
        Expr::Binary { op, .. } => match op {
            BinOp::Mul | BinOp::Div | BinOp::Rem => 10,
            BinOp::Add | BinOp::Sub => 9,
            BinOp::Shl | BinOp::Shr => 8,
            BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => 7,
            BinOp::Eq | BinOp::Ne => 6,
            BinOp::BitAnd => 5,
            BinOp::BitXor => 4,
            BinOp::BitOr => 3,
            BinOp::And => 2,
            BinOp::Or => 1,
        },
        // `orelse` and `catch` are the loosest operators (SPEC §12.1 places
        // `catch` beside `orelse`): the right-hand fallback is an ordinary `T`
        // expression, so `head orelse a + b` / `head catch a + b` read with no
        // parentheses around the fallback.
        Expr::Orelse { .. } | Expr::Catch { .. } => 0,
    }
}

/// The kardashev *source* spelling of a binary operator. Unlike
/// [`BinOp::c_op`], the logical operators spell as `and`/`or`, not `&&`/`||`.
fn binop_src(op: BinOp) -> &'static str {
    match op {
        BinOp::Add => "+",
        BinOp::Sub => "-",
        BinOp::Mul => "*",
        BinOp::Div => "/",
        BinOp::Rem => "%",
        BinOp::Eq => "==",
        BinOp::Ne => "!=",
        BinOp::Lt => "<",
        BinOp::Le => "<=",
        BinOp::Gt => ">",
        BinOp::Ge => ">=",
        BinOp::And => "and",
        BinOp::Or => "or",
        // Bitwise & shift (SPEC §28). The source spelling matches `c_op`'s here,
        // since C uses the same `& | ^ << >>` characters; only the logical
        // `and`/`or` differ from C's `&&`/`||`.
        BinOp::BitAnd => "&",
        BinOp::BitOr => "|",
        BinOp::BitXor => "^",
        BinOp::Shl => "<<",
        BinOp::Shr => ">>",
    }
}

/// The source spelling of an assignment operator (SPEC §27). A plain `=` for
/// `None`; a compound `op=` for `Some(binop)` — `+= -= *= /= %=` from the
/// arithmetic [`BinOp`]s, reusing [`binop_src`] for the operator character. The
/// returned text is the operator alone (no surrounding spaces); callers add the
/// single spaces, so a plain assignment stays `<lhs> = <rhs>` and a compound one
/// is `<lhs> += <rhs>` (SPEC §27.1). Re-lexes to the same token, so re-formatting
/// is idempotent.
fn assign_op_src(op: Option<BinOp>) -> String {
    match op {
        None => "=".to_string(),
        Some(b) => format!("{}=", binop_src(b)),
    }
}

/// Format a `while` continue-clause statement (an assignment or expression)
/// inline, with no trailing semicolon — e.g. `i = i + 1`.
fn fmt_cont(s: &Stmt) -> String {
    match s {
        // The continue-clause may itself be a compound assignment (SPEC §27); the
        // operator spelling follows `op` (`=` / `+=` / …), with no trailing `;`.
        Stmt::Assign {
            name, op, value, ..
        } => format!("{} {} {}", name, assign_op_src(*op), fmt_expr(value)),
        Stmt::Expr(e) => fmt_expr(e),
        // The parser only produces Assign/Expr in this position.
        _ => String::new(),
    }
}

/// The source prefix for a loop's optional label (SPEC §40). A labeled loop
/// (`Some(name)`) prints `name: ` before the `while`/`for` keyword — e.g.
/// `outer: while (…) { … }` — so the loop can be targeted by `break :name` /
/// `continue :name`. An unlabeled loop (`None`) yields the empty string, leaving
/// the pre-v0.147 form byte-for-byte unchanged. Shared by the multi-line and
/// inline printers, and re-lexes to the same `label` so re-formatting is
/// idempotent.
fn fmt_loop_label(label: &Option<String>) -> String {
    match label {
        Some(name) => format!("{}: ", name),
        None => String::new(),
    }
}

/// The source spelling of a `break` statement (SPEC §40), including its trailing
/// `;` but no newline. An unlabeled `break` (`target == None`) prints the bare
/// `break;`, unchanged from pre-v0.147; a labeled one (`target == Some(label)`)
/// targets the named enclosing loop and prints `break :label;`. Shared by the
/// multi-line and inline printers; re-lexes to the same `target`.
fn fmt_break(target: &Option<String>) -> String {
    match target {
        Some(label) => format!("break :{};", label),
        None => "break;".to_string(),
    }
}

/// The source spelling of a `continue` statement (SPEC §40), mirroring
/// [`fmt_break`]: `continue;` when unlabeled, `continue :label;` when targeting
/// the named enclosing loop.
fn fmt_continue(target: &Option<String>) -> String {
    match target {
        Some(label) => format!("continue :{};", label),
        None => "continue;".to_string(),
    }
}

/// The source spelling of a function signature (SPEC §3/§17): `pub? fn
/// name(comptime? p: T, …) ret` — an optional `pub `, the parameter list with a
/// `comptime ` prefix on compile-time type parameters and `name: Type` entries
/// joined with `, `, then the Zig-style return type after the parens. Stops
/// before the body's opening brace, so the multi-line and inline printers share
/// one spelling and differ only in the brace/indent layout that follows. Shared
/// by [`Printer::print_func`] and [`fmt_func_inline`].
fn fmt_func_sig(f: &Func) -> String {
    let mut s = String::new();
    if f.is_pub {
        s.push_str("pub ");
    }
    s.push_str("fn ");
    s.push_str(&f.name);
    s.push('(');
    for (i, param) in f.params.iter().enumerate() {
        if i > 0 {
            s.push_str(", ");
        }
        // A compile-time type parameter (`comptime T: type`, SPEC §17.1)
        // prints with a leading `comptime ` keyword; everything else about
        // the parameter list is unchanged.
        if param.is_comptime {
            s.push_str("comptime ");
        }
        s.push_str(&param.name);
        s.push_str(": ");
        s.push_str(&fmt_type(&param.ty));
    }
    s.push_str(") ");
    s.push_str(&fmt_type(&f.ret));
    s
}

/// The source spelling of a `const`/`var` binding (SPEC §18), trailing `;`
/// included: `const NAME: T = expr;` / `var name = expr;` — `const` vs `var`
/// per `is_const`, with the `: T` annotation only when present (an inferred
/// binding prints none). Shared by [`Printer::print_stmt`] and
/// [`fmt_stmt_inline`]; the callers add only the indent / newline layout.
fn fmt_let_core(is_const: bool, name: &str, ty: &Option<TypeExpr>, value: &Expr) -> String {
    let mut s = String::new();
    s.push_str(if is_const { "const " } else { "var " });
    s.push_str(name);
    // The type annotation is optional (SPEC §18): `var name: T = …;` when
    // present, `var name = …;` when inferred from the value.
    if let Some(ty) = ty {
        s.push_str(": ");
        s.push_str(&fmt_type(ty));
    }
    s.push_str(" = ");
    s.push_str(&fmt_expr(value));
    s.push(';');
    s
}

/// The source spelling of an assignment statement (SPEC §27), trailing `;`
/// included: `<lhs> = <rhs>;`, or compound `<lhs> += <rhs>;` per `op` (see
/// [`assign_op_src`]), with a single space on each side of the operator. The
/// left-hand side arrives pre-rendered so one helper serves both
/// [`Stmt::Assign`] (a bare name) and [`Stmt::FieldAssign`] (a field-access /
/// index / deref chain rendered by [`fmt_expr`]). Shared by
/// [`Printer::print_stmt`] and [`fmt_stmt_inline`].
fn fmt_assign_core(lhs: &str, op: Option<BinOp>, value: &Expr) -> String {
    format!("{} {} {};", lhs, assign_op_src(op), fmt_expr(value))
}

/// The source spelling of a `while` header (SPEC §5/§40): the optional
/// `label: ` prefix (see [`fmt_loop_label`]), `while (<cond>)`, then the
/// optional ` : (<cont>)` continue clause (see [`fmt_cont`]). Stops before the
/// body's opening brace, so the multi-line and inline printers share one
/// spelling and differ only in the layout that follows. Shared by
/// [`Printer::print_stmt`] and [`fmt_stmt_inline`].
fn fmt_while_header(cond: &Expr, cont: &Option<Box<Stmt>>, label: &Option<String>) -> String {
    let mut s = fmt_loop_label(label);
    s.push_str("while (");
    s.push_str(&fmt_expr(cond));
    s.push(')');
    if let Some(c) = cont {
        s.push_str(" : (");
        s.push_str(&fmt_cont(c));
        s.push(')');
    }
    s
}

/// The source spelling of a `for` header (SPEC §29/§40): the optional `label: `
/// prefix, `for (<iter>)` with `, 0..` appended inside the parens when an index
/// is captured, then the `|elem|` / `|elem, index|` capture list up to and
/// including the closing `|`. Stops before the body's opening brace, so the
/// multi-line and inline printers share one spelling and differ only in the
/// layout that follows. Shared by [`Printer::print_stmt`] and
/// [`fmt_stmt_inline`].
fn fmt_for_header(
    iter: &Expr,
    elem: &str,
    index: &Option<String>,
    label: &Option<String>,
) -> String {
    let mut s = fmt_loop_label(label);
    s.push_str("for (");
    s.push_str(&fmt_expr(iter));
    if index.is_some() {
        s.push_str(", 0..");
    }
    s.push_str(") |");
    s.push_str(elem);
    if let Some(idx) = index {
        s.push_str(", ");
        s.push_str(idx);
    }
    s.push('|');
    s
}

/// The source spelling of a `switch` arm's opening (SPEC §13/§20/§39):
/// `<labels> =>` — the label list rendered by [`fmt_arm_labels`] (value labels
/// first, then ranges) and the arrow — with an optional ` |<cap>|` payload
/// capture appended (SPEC §20). Stops before the arm body's opening brace, so
/// the multi-line and inline printers share one spelling and differ only in the
/// layout that follows. Shared by [`Printer::print_switch`] and
/// [`fmt_stmt_inline`].
fn fmt_switch_arm_open(arm: &SwitchArm) -> String {
    let mut s = fmt_arm_labels(arm);
    s.push_str(" =>");
    if let Some(cap) = &arm.capture {
        s.push_str(" |");
        s.push_str(cap);
        s.push('|');
    }
    s
}

/// The source spelling of an `if` / `else if` header (SPEC §5/§21.1):
/// `if (<cond>)` with an optional ` |<name>|` payload capture appended. The
/// same text serves the chain's initial `if` and — after a `} else ` / ` else `
/// prefix from the caller — every `else if`, stopping before the branch body's
/// opening brace so the multi-line and inline printers share one spelling and
/// differ only in the layout that follows. Shared by [`Printer::print_if`] and
/// [`fmt_if_inline`].
fn fmt_if_header(cond: &Expr, capture: &Option<String>) -> String {
    let mut s = String::from("if (");
    s.push_str(&fmt_expr(cond));
    s.push(')');
    if let Some(name) = capture {
        s.push_str(" |");
        s.push_str(name);
        s.push('|');
    }
    s
}

/// Format an expression with no surrounding parentheses.
fn fmt_expr(e: &Expr) -> String {
    match e {
        Expr::Int { value, .. } => value.to_string(),
        // A floating-point literal `3.14` of type `f64` (SPEC §38). Print with
        // Rust's `{:?}` (Debug) formatting, which emits the shortest decimal that
        // round-trips and always keeps a decimal point on a finite value — `3.0`
        // stays `3.0` (not `3`), `3.14` stays `3.14`. That keeps it lexable as a
        // float (the lexer requires a digit on both sides of the `.`, SPEC §1)
        // and distinct from an integer literal, so the printed form re-lexes to
        // the same `Expr::Float` and re-formatting is idempotent.
        Expr::Float { value, .. } => format!("{:?}", value),
        Expr::Bool { value, .. } => if *value { "true" } else { "false" }.to_string(),
        Expr::Ident { name, .. } => name.clone(),
        // A string literal `"…"` — a `[]u8` value over static bytes (SPEC §23).
        // Re-emit as a double-quoted literal, re-escaping `\n \t \" \\` (and any
        // other special characters) via the same [`escape_string`] helper used
        // for `test` names, so the printed form re-lexes to the same bytes.
        Expr::StrLit { value, .. } => escape_string(value),
        // `unreachable` (SPEC §35) — a diverging runtime-safety primitive that
        // asserts a path is impossible (traps with `exit(101)` if reached) and
        // adopts the expected type in a value position. It prints as the bare
        // `unreachable` keyword and re-lexes to the same `Expr::Unreachable`,
        // both as a statement (`unreachable;`) and inside a switch arm, so
        // re-formatting is idempotent. (`@panic(msg)` is an `Expr::Builtin` and
        // prints via the builtin arm below as `@panic(<msg>)`.)
        Expr::Unreachable { .. } => "unreachable".to_string(),
        Expr::Unary { op, expr, .. } => {
            let ops = match op {
                UnOp::Neg => "-",
                UnOp::Not => "!",
                // `~x` — bitwise complement (SPEC §28). A prefix like `-`/`!`.
                UnOp::BitNot => "~",
            };
            // A unary operand may be a unary/comptime/primary but never a bare
            // binary (grammar: `unary := ("-"|"!"|"~") unary | comptime_expr`),
            // so parenthesise binaries (precedence < unary).
            if expr_prec(expr) < 11 {
                format!("{}({})", ops, fmt_expr(expr))
            } else {
                format!("{}{}", ops, fmt_expr(expr))
            }
        }
        Expr::Binary { op, lhs, rhs, .. } => {
            let p = expr_prec(e);
            let l = fmt_operand(lhs, p, false);
            let r = fmt_operand(rhs, p, true);
            format!("{} {} {}", l, binop_src(*op), r)
        }
        Expr::Call { callee, args, .. } => {
            let mut s = String::new();
            s.push_str(callee);
            s.push('(');
            for (i, arg) in args.iter().enumerate() {
                if i > 0 {
                    s.push_str(", ");
                }
                s.push_str(&fmt_expr(arg));
            }
            s.push(')');
            s
        }
        // A comptime reflection builtin `@name(args)` (SPEC §32.1) — e.g.
        // `@sizeOf(i32)` / `@typeName(Point)`. Prints `@` then the builtin name,
        // then the argument list in parentheses (each argument a full
        // expression, joined with `, `), mirroring an ordinary [`Expr::Call`]
        // with a leading `@`. The single argument is the type-naming `Ident`,
        // which prints as its bare name, so `@sizeOf(i32)` round-trips. (Note
        // `@import` is a top-level item and `@This()` is a *type*, both handled
        // elsewhere; this arm is only for expression-position builtins.) The
        // printed form re-lexes to the same `Expr::Builtin`, so re-formatting is
        // idempotent.
        Expr::Builtin { name, args, .. } => {
            let mut s = String::new();
            s.push('@');
            s.push_str(name);
            s.push('(');
            for (i, arg) in args.iter().enumerate() {
                if i > 0 {
                    s.push_str(", ");
                }
                s.push_str(&fmt_expr(arg));
            }
            s.push(')');
            s
        }
        Expr::Comptime { expr, .. } => {
            // `comptime` binds a single primary; wrap anything that is not a
            // primary (Int/Bool/Ident/Call) in parentheses.
            if expr_prec(expr) >= 13 {
                format!("comptime {}", fmt_expr(expr))
            } else {
                format!("comptime ({})", fmt_expr(expr))
            }
        }
        Expr::StructLit { name, fields, .. } => {
            // `Name{}` when empty, else `Name{ .f = e, .g = e }` with a single
            // space inside the braces and `, ` between initializers.
            if fields.is_empty() {
                return format!("{}{{}}", name);
            }
            let mut s = String::new();
            s.push_str(name);
            s.push_str("{ ");
            for (i, init) in fields.iter().enumerate() {
                if i > 0 {
                    s.push_str(", ");
                }
                s.push('.');
                s.push_str(&init.name);
                s.push_str(" = ");
                s.push_str(&fmt_expr(&init.value));
            }
            s.push_str(" }");
            s
        }
        // An anonymous `struct { f: T, …, fn m(…) … { … } }` **type value** (SPEC
        // §25/§26) — the body of a type-returning function `fn F(comptime T:
        // type) type`, e.g. `return struct { v: T };` or, with methods (v0.130),
        // `return struct { v: T, fn get(self: Self) T { return self.v; } };`.
        // Prints inline on a single line, mirroring the struct-literal spacing:
        // one space just inside the braces, then the fields THEN the methods,
        // matching a named struct's field-then-method order (SPEC §10/§26). Each
        // field is `name: Type` (the field-decl spelling) and the fields are
        // joined with `, `; each method is the ordinary `pub? fn …` spelling laid
        // out inline by [`fmt_func_inline`] and the methods are joined with a
        // single space (the parser accepts an optional trailing comma after the
        // fields but **no** comma between methods, so a comma separates the field
        // block from the first method while the methods themselves are
        // space-separated). An anonymous struct type with neither fields nor
        // methods collapses to `struct {}`; a fields-only one (no methods) prints
        // exactly as in v0.129. The per-line / trailing-comma layout a
        // *top-level* `const Name = struct {…};` declaration uses is not used here
        // because this is an inline expression in value/return position. The
        // printed form re-lexes to the same `Expr::StructType`, so re-formatting
        // is idempotent.
        Expr::StructType {
            fields, methods, ..
        } => {
            if fields.is_empty() && methods.is_empty() {
                return "struct {}".to_string();
            }
            let field_parts: Vec<String> = fields
                .iter()
                .map(|f| format!("{}: {}", f.name, fmt_type(&f.ty)))
                .collect();
            let method_parts: Vec<String> = methods.iter().map(fmt_func_inline).collect();
            // Fields are comma-separated; methods are space-separated; a comma
            // (the optional trailing comma after the last field) joins the two
            // blocks when both are present.
            let mut inner = field_parts.join(", ");
            if !field_parts.is_empty() && !method_parts.is_empty() {
                inner.push_str(", ");
            }
            inner.push_str(&method_parts.join(" "));
            format!("struct {{ {} }}", inner)
        }
        Expr::Field { base, field, .. } => {
            // `base.field` with no spaces. Field access is postfix (binds as a
            // primary), so a base that is not itself primary/postfix is
            // parenthesised. The parser never produces such a base, but this
            // keeps the printer total and idempotent.
            if expr_prec(base) >= 13 {
                format!("{}.{}", fmt_expr(base), field)
            } else {
                format!("({}).{}", fmt_expr(base), field)
            }
        }
        Expr::MethodCall {
            receiver,
            method,
            args,
            ..
        } => {
            // `receiver.method(arg, ...)`. The receiver is a postfix base —
            // either a struct value or an `Ident` naming a struct type
            // (associated call). Parenthesise any non-primary/non-postfix
            // receiver to stay total and idempotent; the parser never produces
            // one.
            let mut s = String::new();
            if expr_prec(receiver) >= 13 {
                s.push_str(&fmt_expr(receiver));
            } else {
                s.push('(');
                s.push_str(&fmt_expr(receiver));
                s.push(')');
            }
            s.push('.');
            s.push_str(method);
            s.push('(');
            for (i, arg) in args.iter().enumerate() {
                if i > 0 {
                    s.push_str(", ");
                }
                s.push_str(&fmt_expr(arg));
            }
            s.push(')');
            s
        }
        // The `null` literal (SPEC §11.1). Its `?T` type comes from context.
        Expr::Null { .. } => "null".to_string(),
        // `lhs orelse rhs` (SPEC §11.1). `orelse` is the loosest operator, so
        // its left operand never needs parentheses and only a right operand of
        // equal precedence (another `orelse`) does — yielding the left-
        // associative `a orelse b orelse c` and the explicit
        // `a orelse (b orelse c)`.
        Expr::Orelse { lhs, rhs, .. } => {
            let p = expr_prec(e);
            let l = fmt_operand(lhs, p, false);
            let r = fmt_operand(rhs, p, true);
            format!("{} orelse {}", l, r)
        }
        // `expr.?` — postfix force-unwrap (SPEC §11.1). Like field access it
        // binds as a primary, so a non-primary/non-postfix operand (e.g. an
        // `orelse`) is parenthesised to stay total and idempotent.
        Expr::Unwrap { expr, .. } => {
            if expr_prec(expr) >= 13 {
                format!("{}.?", fmt_expr(expr))
            } else {
                format!("({}).?", fmt_expr(expr))
            }
        }
        // `error.Name` — an error value from the implicit global error set
        // (SPEC §12.1). Atomic, like a literal.
        Expr::ErrorLit { name, .. } => format!("error.{}", name),
        // `.Variant` — an unqualified enum literal (SPEC §13); its enum type
        // comes from context. The qualified form `Enum.Variant` is an
        // [`Expr::Field`] off an `Ident` base and prints there. Atomic.
        Expr::EnumLit { variant, .. } => format!(".{}", variant),
        // An array literal `[N]T{ e0, e1, … }` (SPEC §14.1). The `elem` field is
        // the array's own `[N]T` type expression, so its [`fmt_type`] rendering
        // already carries the `[N]` prefix and the element name. Elements join
        // with `, ` inside `{ … }`, mirroring struct-literal spacing; an empty
        // literal collapses to `[N]T{}`.
        Expr::ArrayLit { elem, elems, .. } => {
            let head = fmt_type(elem);
            if elems.is_empty() {
                return format!("{}{{}}", head);
            }
            let mut s = String::new();
            s.push_str(&head);
            s.push_str("{ ");
            for (i, e) in elems.iter().enumerate() {
                if i > 0 {
                    s.push_str(", ");
                }
                s.push_str(&fmt_expr(e));
            }
            s.push_str(" }");
            s
        }
        // Indexing `base[index]` (SPEC §14.1). Indexing is postfix and binds as a
        // primary, so a base that is not itself primary/postfix is parenthesised
        // to stay total and idempotent; the parser never produces such a base.
        // The index is a full expression and prints bare inside the brackets.
        Expr::Index { base, index, .. } => {
            if expr_prec(base) >= 13 {
                format!("{}[{}]", fmt_expr(base), fmt_expr(index))
            } else {
                format!("({})[{}]", fmt_expr(base), fmt_expr(index))
            }
        }
        // `&place` — address-of an lvalue, yielding `*T` (SPEC §15.1). `&` is a
        // prefix; the place is an lvalue (an `Ident`, a field chain, an index or
        // a `Deref`), all of which bind as primaries/postfix and so print bare.
        // Anything looser is parenthesised to keep the printer total and
        // idempotent; the parser never produces such a place.
        Expr::AddrOf { place, .. } => {
            if expr_prec(place) >= 13 {
                format!("&{}", fmt_expr(place))
            } else {
                format!("&({})", fmt_expr(place))
            }
        }
        // `expr.*` — postfix pointer dereference (SPEC §15.1). Like the `.?`
        // unwrap it binds as a primary, so a non-primary/non-postfix operand
        // (e.g. an `orelse`) is parenthesised to stay total and idempotent.
        Expr::Deref { expr, .. } => {
            if expr_prec(expr) >= 13 {
                format!("{}.*", fmt_expr(expr))
            } else {
                format!("({}).*", fmt_expr(expr))
            }
        }
        // `base[lo..hi]` — slice an array (or slice), yielding `[]T` (SPEC §15.2).
        // Postfix, like indexing: a base that is not itself primary/postfix is
        // parenthesised; the bounds are full expressions and print bare, joined
        // by `..` inside the brackets.
        Expr::SliceExpr { base, lo, hi, .. } => {
            let b = if expr_prec(base) >= 13 {
                fmt_expr(base)
            } else {
                format!("({})", fmt_expr(base))
            };
            format!("{}[{}..{}]", b, fmt_expr(lo), fmt_expr(hi))
        }
        // `try expr` — statement-level error-union propagation (SPEC §12.1). The
        // operand stands at a value position, so a primary/postfix operand
        // prints bare (`try parse(s)`) while anything looser is parenthesised
        // (`try (a + b)`). The parentheses make the result re-parse to the same
        // AST whether `try` is read as a prefix (binding tighter than `+`) or as
        // consuming the whole following expression — both accept `try (e)`.
        Expr::Try { expr, .. } => {
            if expr_prec(expr) >= 13 {
                format!("try {}", fmt_expr(expr))
            } else {
                format!("try ({})", fmt_expr(expr))
            }
        }
        // `expr catch default` (SPEC §12.1) — like `orelse`, the loosest
        // operator, so its left operand never needs parentheses and only an
        // equal-precedence right operand (another `catch`/`orelse`) does. This
        // yields the left-associative `a catch b catch c` and the explicit
        // `a catch (b catch c)`. The capturing form (SPEC §36, v0.142) prints
        // the binding between `catch` and the fallback as `<expr> catch |e|
        // <default>`; `|e|` is purely a binder, so the operand
        // parenthesisation is identical to the non-capturing form.
        Expr::Catch {
            expr,
            capture,
            default,
            ..
        } => {
            let p = expr_prec(e);
            let l = fmt_operand(expr, p, false);
            let r = fmt_operand(default, p, true);
            match capture {
                Some(name) => format!("{} catch |{}| {}", l, name, r),
                None => format!("{} catch {}", l, r),
            }
        }
    }
}

/// Format a binary operand, parenthesising only when precedence /
/// left-associativity requires it. All grammar binaries are left-associative,
/// so an equal-precedence right operand needs parentheses while an
/// equal-precedence left operand does not.
fn fmt_operand(e: &Expr, parent_prec: u8, is_right: bool) -> String {
    let p = expr_prec(e);
    let needs_parens = if is_right {
        p <= parent_prec
    } else {
        p < parent_prec
    };
    if needs_parens {
        format!("({})", fmt_expr(e))
    } else {
        fmt_expr(e)
    }
}

// ----- inline function / statement printing (SPEC §26) ---------------------
//
// A generic-struct method (a `Func` inside an `Expr::StructType` type value,
// v0.130) is printed **inline** — on the single line of the enclosing anonymous
// struct-type value — rather than with the multi-line, indentation-driven
// [`Printer`] layout used for top-level / named-struct functions. This keeps the
// `struct { … }` type value a single expression (matching the inline v0.129
// fields-only form) and keeps `fmt_expr` — the one total entry point for every
// expression, including a nested `StructType` — free of any indent context. The
// spelling of every construct (the `pub? fn name(params) ret` signature with a
// `comptime ` prefix, `name: Type` fields, `if`/`while`/`switch`/`defer`/…
// statements) is shared with the multi-line printers **by construction**: both
// call the same pure spelling helpers ([`fmt_func_sig`], [`fmt_let_core`],
// [`fmt_assign_core`], [`fmt_while_header`], [`fmt_for_header`],
// [`fmt_switch_arm_open`], [`fmt_if_header`], [`fmt_break`], …) and differ only
// in the brace/indent/newline layout they wrap around them, so the inline form
// re-lexes to the same AST and re-formatting is idempotent.

/// Format a function on a single line (SPEC §26): `pub? fn name(params) ret {
/// body }` — the `pub? fn name(params) ret` signature spelling shared with
/// [`Printer::print_func`] via [`fmt_func_sig`], laid out inline. Used for the
/// methods of an [`Expr::StructType`] type value.
fn fmt_func_inline(f: &Func) -> String {
    let mut s = fmt_func_sig(f);
    s.push(' ');
    s.push_str(&fmt_block_inline(&f.body));
    s
}

/// Format a block inline (SPEC §26): `{}` when empty, else `{ <stmt> <stmt> … }`
/// with each statement in its own inline form (terminated by `;`, or `}` for a
/// nested block / control-flow statement) and a single space separating them.
fn fmt_block_inline(block: &Block) -> String {
    if block.stmts.is_empty() {
        return "{}".to_string();
    }
    let parts: Vec<String> = block.stmts.iter().map(fmt_stmt_inline).collect();
    format!("{{ {} }}", parts.join(" "))
}

/// Format a single statement inline (SPEC §26) — no leading indent and no
/// trailing newline. Mirrors [`Printer::print_stmt`] construct-for-construct,
/// differing only in whitespace, so the result re-lexes to the same `Stmt`.
fn fmt_stmt_inline(stmt: &Stmt) -> String {
    match stmt {
        Stmt::Let {
            is_const,
            name,
            ty,
            value,
            ..
        } => fmt_let_core(*is_const, name, ty, value),
        // `name = expr;` / compound `name op= expr;` (SPEC §27), inline.
        Stmt::Assign {
            name, op, value, ..
        } => fmt_assign_core(name, *op, value),
        // `place = expr;` / compound `place op= expr;` (SPEC §27), inline.
        Stmt::FieldAssign {
            place, op, value, ..
        } => fmt_assign_core(&fmt_expr(place), *op, value),
        Stmt::Expr(e) => format!("{};", fmt_expr(e)),
        Stmt::Return { value, .. } => match value {
            Some(e) => format!("return {};", fmt_expr(e)),
            None => "return;".to_string(),
        },
        Stmt::If {
            cond,
            capture,
            then,
            els,
            ..
        } => fmt_if_inline(cond, capture, then, els),
        Stmt::While {
            cond,
            cont,
            body,
            label,
            ..
        } => {
            // The `label: while (cond) : (cont)?` header spelling is shared
            // with the multi-line printer via [`fmt_while_header`].
            let mut s = fmt_while_header(cond, cont, label);
            s.push(' ');
            s.push_str(&fmt_block_inline(body));
            s
        }
        // `for (<iter>) |elem| { … }` / `for (<iter>, 0..) |elem, index| { … }`
        // (SPEC §29), inline. The header spelling — label, `, 0..` and captures
        // — is shared with the multi-line printer via [`fmt_for_header`], so
        // the result re-lexes to the same `Stmt::For`.
        Stmt::For {
            iter,
            elem,
            index,
            body,
            label,
            ..
        } => {
            let mut s = fmt_for_header(iter, elem, index, label);
            s.push(' ');
            s.push_str(&fmt_block_inline(body));
            s
        }
        // `break;` / `break :label;` and `continue;` / `continue :label;`
        // (SPEC §40), inline — the same spelling as the multi-line printer.
        Stmt::Break { target, .. } => fmt_break(target),
        Stmt::Continue { target, .. } => fmt_continue(target),
        Stmt::Defer { stmt, .. } => format!("defer {}", fmt_stmt_inline(stmt)),
        Stmt::ErrDefer { stmt, .. } => format!("errdefer {}", fmt_stmt_inline(stmt)),
        Stmt::Block(b) => fmt_block_inline(b),
        Stmt::Switch {
            scrutinee,
            arms,
            default,
            ..
        } => {
            let mut s = String::from("switch (");
            s.push_str(&fmt_expr(scrutinee));
            s.push_str(") {");
            for arm in arms {
                s.push(' ');
                // The `<labels> => |cap|?` arm opening is shared with the
                // multi-line printer via [`fmt_switch_arm_open`].
                s.push_str(&fmt_switch_arm_open(arm));
                s.push(' ');
                s.push_str(&fmt_block_inline(&arm.body));
                s.push(',');
            }
            if let Some(block) = default {
                s.push_str(" else => ");
                s.push_str(&fmt_block_inline(block));
                s.push(',');
            }
            s.push_str(" }");
            s
        }
    }
}

/// Render the label list of one `switch` arm (everything before `=>`), per
/// SPEC §39. Value labels (`SwitchArm.labels` — enum literals / integer
/// literals) print first, then inclusive integer-range labels
/// (`SwitchArm.ranges`) as `lo..hi`; all parts are joined with `, ` in this
/// stable order. With no ranges this is byte-for-byte the pre-v0.146 output
/// (value labels joined with `, `), so value- and multi-label switches are
/// unchanged and round-tripping stays idempotent.
fn fmt_arm_labels(arm: &SwitchArm) -> String {
    let mut parts: Vec<String> = arm.labels.iter().map(fmt_expr).collect();
    for &(lo, hi) in &arm.ranges {
        parts.push(format!("{lo}..{hi}"));
    }
    parts.join(", ")
}

/// Format an `if`/`else if`/`else` chain inline (SPEC §26). With a capture the
/// header is `if (<cond>) |<name>| { … }`; without one it is `if (<cond>) { …
/// }`. The `if (<cond>) |<name>|?` header spelling is shared with
/// [`Printer::print_if`] via [`fmt_if_header`]; the chaining (`} else if …` /
/// `} else …`) mirrors it, inline.
fn fmt_if_inline(
    cond: &Expr,
    capture: &Option<String>,
    then: &Block,
    els: &Option<Box<Stmt>>,
) -> String {
    let mut s = fmt_if_header(cond, capture);
    s.push(' ');
    s.push_str(&fmt_block_inline(then));

    let mut els = els;
    loop {
        match els {
            None => return s,
            Some(boxed) => match boxed.as_ref() {
                Stmt::If {
                    cond: c2,
                    capture: cap2,
                    then: t2,
                    els: e2,
                    ..
                } => {
                    s.push_str(" else ");
                    s.push_str(&fmt_if_header(c2, cap2));
                    s.push(' ');
                    s.push_str(&fmt_block_inline(t2));
                    els = e2;
                }
                Stmt::Block(b) => {
                    s.push_str(" else ");
                    s.push_str(&fmt_block_inline(b));
                    return s;
                }
                // The AST only ever stores an `If` or a `Block` here, but stay
                // total: wrap any other statement in an inline else block.
                other => {
                    s.push_str(" else { ");
                    s.push_str(&fmt_stmt_inline(other));
                    s.push_str(" }");
                    return s;
                }
            },
        }
    }
}

/// Re-escape a string value into a double-quoted literal (SPEC §1 escapes).
fn escape_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for ch in s.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::fixtures::{
        app_ty, arr_param_ty as arr_ty_param, arr_ty, bin, call, catch_capture_expr as catch_cap,
        catch_expr as catch_, err_ty, error_lit, ident, int, null, opt_ty, orelse, ptr_ty,
        set_err_ty, slice_ty, try_expr as try_, ty, unwrap,
    };
    use crate::ast::{EnumVariant, FieldDecl, FieldInit, Param, TypeExpr, UnionVariant};
    use crate::span::Span;

    const D: Span = Span::DUMMY;

    #[test]
    fn function_with_params_and_return() {
        let m = Module {
            items: vec![Item::Func(Func {
                is_pub: true,
                name: "add".to_string(),
                params: vec![
                    Param {
                        name: "a".to_string(),
                        ty: ty("i32"),
                        is_comptime: false,
                        span: D,
                    },
                    Param {
                        name: "b".to_string(),
                        ty: ty("i32"),
                        is_comptime: false,
                        span: D,
                    },
                ],
                ret: ty("i32"),
                body: Block {
                    stmts: vec![Stmt::Return {
                        value: Some(bin(BinOp::Add, ident("a"), ident("b"))),
                        span: D,
                    }],
                    span: D,
                },
                span: D,
            })],
        };
        let expected = "pub fn add(a: i32, b: i32) i32 {\n    return a + b;\n}\n";
        assert_eq!(print_module(&m), expected);
    }

    #[test]
    fn non_pub_func_void_empty_body() {
        let m = Module {
            items: vec![Item::Func(Func {
                is_pub: false,
                name: "main".to_string(),
                params: vec![],
                ret: ty("void"),
                body: Block {
                    stmts: vec![],
                    span: D,
                },
                span: D,
            })],
        };
        assert_eq!(print_module(&m), "fn main() void {\n}\n");
    }

    #[test]
    fn top_level_const_and_blank_line_between_items() {
        let m = Module {
            items: vec![
                Item::Const(ConstDecl {
                    is_pub: true,
                    name: "MAX".to_string(),
                    ty: Some(ty("i32")),
                    value: int(10),
                    span: D,
                }),
                Item::Const(ConstDecl {
                    is_pub: false,
                    name: "MIN".to_string(),
                    ty: Some(ty("i32")),
                    value: int(0),
                    span: D,
                }),
            ],
        };
        assert_eq!(
            print_module(&m),
            "pub const MAX: i32 = 10;\n\nconst MIN: i32 = 0;\n"
        );
    }

    #[test]
    fn let_assign_print_statements() {
        let m = Module {
            items: vec![Item::Func(Func {
                is_pub: false,
                name: "f".to_string(),
                params: vec![],
                ret: ty("void"),
                body: Block {
                    stmts: vec![
                        Stmt::Let {
                            is_const: false,
                            name: "x".to_string(),
                            ty: Some(ty("i64")),
                            value: int(1),
                            span: D,
                        },
                        Stmt::Assign {
                            name: "x".to_string(),
                            op: None,
                            value: bin(BinOp::Add, ident("x"), int(2)),
                            span: D,
                        },
                        Stmt::Expr(Expr::Call {
                            callee: "print".to_string(),
                            args: vec![ident("x")],
                            span: D,
                        }),
                    ],
                    span: D,
                },
                span: D,
            })],
        };
        let expected = "fn f() void {\n    var x: i64 = 1;\n    x = x + 2;\n    print(x);\n}\n";
        assert_eq!(print_module(&m), expected);
    }

    #[test]
    fn if_else_if_else_chain() {
        // if (a) { return 1; } else if (b) { return 2; } else { return 3; }
        let inner_else = Stmt::Block(Block {
            stmts: vec![Stmt::Return {
                value: Some(int(3)),
                span: D,
            }],
            span: D,
        });
        let else_if = Stmt::If {
            cond: ident("b"),
            capture: None,
            then: Block {
                stmts: vec![Stmt::Return {
                    value: Some(int(2)),
                    span: D,
                }],
                span: D,
            },
            els: Some(Box::new(inner_else)),
            span: D,
        };
        let top_if = Stmt::If {
            cond: ident("a"),
            capture: None,
            then: Block {
                stmts: vec![Stmt::Return {
                    value: Some(int(1)),
                    span: D,
                }],
                span: D,
            },
            els: Some(Box::new(else_if)),
            span: D,
        };
        let m = Module {
            items: vec![Item::Func(Func {
                is_pub: false,
                name: "g".to_string(),
                params: vec![],
                ret: ty("i32"),
                body: Block {
                    stmts: vec![top_if],
                    span: D,
                },
                span: D,
            })],
        };
        let expected = concat!(
            "fn g() i32 {\n",
            "    if (a) {\n",
            "        return 1;\n",
            "    } else if (b) {\n",
            "        return 2;\n",
            "    } else {\n",
            "        return 3;\n",
            "    }\n",
            "}\n"
        );
        assert_eq!(print_module(&m), expected);
    }

    #[test]
    fn while_with_continue_expr_and_defer() {
        // The continue-clause is a statement (typically an assignment). Here we
        // exercise the canonical `i = i + 1` increment in the `) : (cont) {`
        // form.
        let body = Block {
            stmts: vec![
                Stmt::Defer {
                    stmt: Box::new(Stmt::Expr(Expr::Call {
                        callee: "print".to_string(),
                        args: vec![ident("i")],
                        span: D,
                    })),
                    span: D,
                },
                Stmt::Break {
                    target: None,
                    span: D,
                },
            ],
            span: D,
        };
        let m = Module {
            items: vec![Item::Func(Func {
                is_pub: false,
                name: "loopy".to_string(),
                params: vec![],
                ret: ty("void"),
                body: Block {
                    stmts: vec![Stmt::While {
                        cond: bin(BinOp::Lt, ident("i"), int(10)),
                        cont: Some(Box::new(Stmt::Assign {
                            name: "i".to_string(),
                            op: None,
                            value: bin(BinOp::Add, ident("i"), int(1)),
                            span: D,
                        })),
                        body,
                        label: None,
                        span: D,
                    }],
                    span: D,
                },
                span: D,
            })],
        };
        let expected = concat!(
            "fn loopy() void {\n",
            "    while (i < 10) : (i = i + 1) {\n",
            "        defer print(i);\n",
            "        break;\n",
            "    }\n",
            "}\n"
        );
        assert_eq!(print_module(&m), expected);
    }

    #[test]
    fn test_block_with_escaped_name() {
        let m = Module {
            items: vec![Item::Test(TestBlock {
                name: "a \"quoted\"\tname".to_string(),
                body: Block {
                    stmts: vec![Stmt::Expr(Expr::Call {
                        callee: "expect".to_string(),
                        args: vec![Expr::Bool {
                            value: true,
                            span: D,
                        }],
                        span: D,
                    })],
                    span: D,
                },
                span: D,
            })],
        };
        let expected = "test \"a \\\"quoted\\\"\\tname\" {\n    expect(true);\n}\n";
        assert_eq!(print_module(&m), expected);
    }

    #[test]
    fn precedence_parenthesisation() {
        // (a + b) * c  — multiplication of a lower-precedence sum.
        let e1 = bin(BinOp::Mul, bin(BinOp::Add, ident("a"), ident("b")), ident("c"));
        assert_eq!(fmt_expr(&e1), "(a + b) * c");

        // a + b * c — natural precedence needs no parentheses.
        let e2 = bin(BinOp::Add, ident("a"), bin(BinOp::Mul, ident("b"), ident("c")));
        assert_eq!(fmt_expr(&e2), "a + b * c");

        // a - (b - c) — right operand at equal precedence keeps its parens
        // because subtraction is left-associative.
        let e3 = bin(BinOp::Sub, ident("a"), bin(BinOp::Sub, ident("b"), ident("c")));
        assert_eq!(fmt_expr(&e3), "a - (b - c)");

        // a - b - c — left-associative chain, no parens.
        let e4 = bin(BinOp::Sub, bin(BinOp::Sub, ident("a"), ident("b")), ident("c"));
        assert_eq!(fmt_expr(&e4), "a - b - c");

        // a and b or c — `or` binds looser than `and`.
        let e5 = bin(BinOp::Or, bin(BinOp::And, ident("a"), ident("b")), ident("c"));
        assert_eq!(fmt_expr(&e5), "a and b or c");
    }

    #[test]
    fn unary_and_comptime() {
        // -(a + b): unary over a binary parenthesises.
        let neg = Expr::Unary {
            op: UnOp::Neg,
            expr: Box::new(bin(BinOp::Add, ident("a"), ident("b"))),
            span: D,
        };
        assert_eq!(fmt_expr(&neg), "-(a + b)");

        // !x: unary over a primary does not.
        let not = Expr::Unary {
            op: UnOp::Not,
            expr: Box::new(ident("x")),
            span: D,
        };
        assert_eq!(fmt_expr(&not), "!x");

        // comptime (2 + 3): non-primary operand parenthesised.
        let ct = Expr::Comptime {
            expr: Box::new(bin(BinOp::Add, int(2), int(3))),
            span: D,
        };
        assert_eq!(fmt_expr(&ct), "comptime (2 + 3)");

        // comptime x: primary operand bare.
        let ct2 = Expr::Comptime {
            expr: Box::new(ident("x")),
            span: D,
        };
        assert_eq!(fmt_expr(&ct2), "comptime x");
    }

    #[test]
    fn output_is_deterministic() {
        // We cannot re-parse here (the parser is a stub during this module's
        // isolated build), so idempotence is checked as determinism: printing
        // the same Module twice yields byte-identical text.
        let m = Module {
            items: vec![Item::Func(Func {
                is_pub: false,
                name: "f".to_string(),
                params: vec![Param {
                    name: "n".to_string(),
                    ty: ty("i64"),
                    is_comptime: false,
                    span: D,
                }],
                ret: ty("i64"),
                body: Block {
                    stmts: vec![Stmt::Return {
                        value: Some(bin(BinOp::Mul, ident("n"), int(2))),
                        span: D,
                    }],
                    span: D,
                },
                span: D,
            })],
        };
        let a = print_module(&m);
        let b = print_module(&m);
        assert_eq!(a, b);
        assert!(a.ends_with('\n'));
    }

    // ----- structs (v0.112) -----------------------------------------------

    fn field_decl(name: &str, type_name: &str) -> FieldDecl {
        FieldDecl {
            name: name.to_string(),
            ty: ty(type_name),
            span: D,
        }
    }

    fn field(base: Expr, name: &str) -> Expr {
        Expr::Field {
            base: Box::new(base),
            field: name.to_string(),
            span: D,
        }
    }

    fn field_init(name: &str, value: Expr) -> FieldInit {
        FieldInit {
            name: name.to_string(),
            value,
            span: D,
        }
    }

    #[test]
    fn struct_decl_canonical_form() {
        // One field per line, 4-space indent, trailing comma on each, `};` to
        // close. A `pub` struct keeps its leading `pub`.
        let m = Module {
            items: vec![Item::Struct(StructDecl {
                is_pub: true,
                name: "Point".to_string(),
                fields: vec![field_decl("x", "i32"), field_decl("y", "i32")],
                methods: vec![],
                span: D,
            })],
        };
        let expected = "pub const Point = struct {\n    x: i32,\n    y: i32,\n};\n";
        assert_eq!(print_module(&m), expected);
        // Idempotence: re-printing the same AST is byte-identical.
        assert_eq!(print_module(&m), expected);
    }

    #[test]
    fn empty_struct_decl_is_single_line() {
        let m = Module {
            items: vec![Item::Struct(StructDecl {
                is_pub: false,
                name: "Empty".to_string(),
                fields: vec![],
                methods: vec![],
                span: D,
            })],
        };
        assert_eq!(print_module(&m), "const Empty = struct {};\n");
    }

    #[test]
    fn struct_literal_field_access_and_assign() {
        // A struct decl followed (blank-line separated) by a function that uses
        // a struct literal, a field assignment chain and a field access.
        let lit = Expr::StructLit {
            name: "Point".to_string(),
            fields: vec![field_init("x", int(1)), field_init("y", int(2))],
            span: D,
        };
        let m = Module {
            items: vec![
                Item::Struct(StructDecl {
                    is_pub: false,
                    name: "Point".to_string(),
                    fields: vec![field_decl("x", "i32"), field_decl("y", "i32")],
                    methods: vec![],
                    span: D,
                }),
                Item::Func(Func {
                    is_pub: false,
                    name: "f".to_string(),
                    params: vec![],
                    ret: ty("void"),
                    body: Block {
                        stmts: vec![
                            Stmt::Let {
                                is_const: false,
                                name: "p".to_string(),
                                ty: Some(ty("Point")),
                                value: lit,
                                span: D,
                            },
                            Stmt::FieldAssign {
                                place: field(ident("p"), "x"),
                                op: None,
                                value: field(ident("p"), "y"),
                                span: D,
                            },
                            Stmt::Expr(Expr::Call {
                                callee: "print".to_string(),
                                args: vec![field(ident("p"), "x")],
                                span: D,
                            }),
                        ],
                        span: D,
                    },
                    span: D,
                }),
            ],
        };
        let expected = concat!(
            "const Point = struct {\n",
            "    x: i32,\n",
            "    y: i32,\n",
            "};\n",
            "\n",
            "fn f() void {\n",
            "    var p: Point = Point{ .x = 1, .y = 2 };\n",
            "    p.x = p.y;\n",
            "    print(p.x);\n",
            "}\n",
        );
        let printed = print_module(&m);
        assert_eq!(printed, expected);
        // Idempotence (as determinism): the pure printer re-prints identically.
        assert_eq!(print_module(&m), printed);
    }

    #[test]
    fn empty_struct_literal_and_field_chains() {
        // Empty struct literal: `Empty{}`.
        let empty = Expr::StructLit {
            name: "Empty".to_string(),
            fields: vec![],
            span: D,
        };
        assert_eq!(fmt_expr(&empty), "Empty{}");

        // Chained field access prints with no spaces: `a.b.c`.
        let chain = field(field(ident("a"), "b"), "c");
        assert_eq!(fmt_expr(&chain), "a.b.c");

        // Field access directly off a struct literal needs no parentheses,
        // because both bind as primaries: `Point{ .x = 1 }.x`.
        let lit = Expr::StructLit {
            name: "Point".to_string(),
            fields: vec![field_init("x", int(1))],
            span: D,
        };
        assert_eq!(fmt_expr(&field(lit, "x")), "Point{ .x = 1 }.x");
    }

    // ----- methods & associated functions (v0.113) -------------------------

    /// `pub fn get(self: Counter) i32 { return self.<field>; }` helper.
    fn method_get(field_name: &str) -> Func {
        Func {
            is_pub: true,
            name: "get".to_string(),
            params: vec![Param {
                name: "self".to_string(),
                ty: ty("Counter"),
                is_comptime: false,
                span: D,
            }],
            ret: ty("i32"),
            body: Block {
                stmts: vec![Stmt::Return {
                    value: Some(field(ident("self"), field_name)),
                    span: D,
                }],
                span: D,
            },
            span: D,
        }
    }

    #[test]
    fn struct_with_method_canonical_form() {
        // The SPEC §10 example: fields first, a blank line, then each method
        // printed one indent deep with the ordinary `pub? fn …` printer.
        let m = Module {
            items: vec![Item::Struct(StructDecl {
                is_pub: false,
                name: "Counter".to_string(),
                fields: vec![field_decl("n", "i32")],
                methods: vec![method_get("n")],
                span: D,
            })],
        };
        let expected = concat!(
            "const Counter = struct {\n",
            "    n: i32,\n",
            "\n",
            "    pub fn get(self: Counter) i32 {\n",
            "        return self.n;\n",
            "    }\n",
            "};\n",
        );
        let printed = print_module(&m);
        assert_eq!(printed, expected);
        // Idempotence (as determinism): the pure printer re-prints identically.
        assert_eq!(print_module(&m), printed);
    }

    #[test]
    fn struct_with_field_and_two_methods_blank_lines() {
        // A blank line separates fields from the first method and each method
        // from the next. The second item is an associated function (no `self`,
        // not `pub`) to exercise both flavours of the function printer.
        let zero = Func {
            is_pub: false,
            name: "zero".to_string(),
            params: vec![],
            ret: ty("i32"),
            body: Block {
                stmts: vec![Stmt::Return {
                    value: Some(int(0)),
                    span: D,
                }],
                span: D,
            },
            span: D,
        };
        let m = Module {
            items: vec![Item::Struct(StructDecl {
                is_pub: false,
                name: "Counter".to_string(),
                fields: vec![field_decl("n", "i32")],
                methods: vec![method_get("n"), zero],
                span: D,
            })],
        };
        let expected = concat!(
            "const Counter = struct {\n",
            "    n: i32,\n",
            "\n",
            "    pub fn get(self: Counter) i32 {\n",
            "        return self.n;\n",
            "    }\n",
            "\n",
            "    fn zero() i32 {\n",
            "        return 0;\n",
            "    }\n",
            "};\n",
        );
        assert_eq!(print_module(&m), expected);
    }

    #[test]
    fn methods_only_struct_has_no_leading_blank_line() {
        // With no fields, the first method follows the opening brace directly
        // (no leading blank line), and the struct does not collapse to one
        // line because it is not truly empty.
        let zero = Func {
            is_pub: true,
            name: "zero".to_string(),
            params: vec![],
            ret: ty("i32"),
            body: Block {
                stmts: vec![Stmt::Return {
                    value: Some(int(0)),
                    span: D,
                }],
                span: D,
            },
            span: D,
        };
        let m = Module {
            items: vec![Item::Struct(StructDecl {
                is_pub: false,
                name: "Util".to_string(),
                fields: vec![],
                methods: vec![zero],
                span: D,
            })],
        };
        let expected = concat!(
            "const Util = struct {\n",
            "    pub fn zero() i32 {\n",
            "        return 0;\n",
            "    }\n",
            "};\n",
        );
        assert_eq!(print_module(&m), expected);
    }

    #[test]
    fn method_call_prints_receiver_method_args() {
        // Method call on a value: receiver.method(arg, ...).
        let mc = Expr::MethodCall {
            receiver: Box::new(ident("c")),
            method: "bumped".to_string(),
            args: vec![int(1), int(2)],
            span: D,
        };
        assert_eq!(fmt_expr(&mc), "c.bumped(1, 2)");

        // No-argument method call: receiver.method().
        let mc0 = Expr::MethodCall {
            receiver: Box::new(ident("c")),
            method: "get".to_string(),
            args: vec![],
            span: D,
        };
        assert_eq!(fmt_expr(&mc0), "c.get()");

        // Associated-call form (receiver is an Ident naming the type):
        // Counter.zero().
        let assoc = Expr::MethodCall {
            receiver: Box::new(ident("Counter")),
            method: "zero".to_string(),
            args: vec![],
            span: D,
        };
        assert_eq!(fmt_expr(&assoc), "Counter.zero()");

        // A method call directly off a field-access receiver: a.b.get().
        let chained = Expr::MethodCall {
            receiver: Box::new(field(ident("a"), "b")),
            method: "get".to_string(),
            args: vec![],
            span: D,
        };
        assert_eq!(fmt_expr(&chained), "a.b.get()");
    }

    // ----- optionals (v0.114) ----------------------------------------------

    #[test]
    fn optional_type_prints_with_leading_question() {
        // The bare type helper still formats as the plain name; the optional
        // helper prepends `?`.
        assert_eq!(fmt_type(&ty("i32")), "i32");
        assert_eq!(fmt_type(&opt_ty("i32")), "?i32");
        // A struct optional formats the same way: `?Point`.
        assert_eq!(fmt_type(&opt_ty("Point")), "?Point");
    }

    #[test]
    fn optional_type_in_every_position() {
        // `?T` must print wherever a type appears: a top-level `const`, a
        // function's params and return, a `var`/`const` local annotation, and a
        // struct field.
        let const_decl = Item::Const(ConstDecl {
            is_pub: true,
            name: "NONE".to_string(),
            ty: Some(opt_ty("i32")),
            value: null(),
            span: D,
        });
        let strukt = Item::Struct(StructDecl {
            is_pub: false,
            name: "Node".to_string(),
            fields: vec![FieldDecl {
                name: "next".to_string(),
                ty: opt_ty("i32"),
                span: D,
            }],
            methods: vec![],
            span: D,
        });
        let func = Item::Func(Func {
            is_pub: false,
            name: "f".to_string(),
            params: vec![Param {
                name: "x".to_string(),
                ty: opt_ty("i32"),
                is_comptime: false,
                span: D,
            }],
            ret: opt_ty("i32"),
            body: Block {
                stmts: vec![Stmt::Let {
                    is_const: false,
                    name: "y".to_string(),
                    ty: Some(opt_ty("i32")),
                    value: null(),
                    span: D,
                }],
                span: D,
            },
            span: D,
        });
        let m = Module {
            items: vec![const_decl, strukt, func],
        };
        let expected = concat!(
            "pub const NONE: ?i32 = null;\n",
            "\n",
            "const Node = struct {\n",
            "    next: ?i32,\n",
            "};\n",
            "\n",
            "fn f(x: ?i32) ?i32 {\n",
            "    var y: ?i32 = null;\n",
            "}\n",
        );
        assert_eq!(print_module(&m), expected);
    }

    #[test]
    fn null_orelse_unwrap_expr_print() {
        // The bare `null` literal.
        assert_eq!(fmt_expr(&null()), "null");

        // `x orelse y`: both operands are primaries, so no parentheses.
        assert_eq!(fmt_expr(&orelse(ident("x"), ident("y"))), "x orelse y");

        // `x.?`: postfix unwrap of a primary.
        assert_eq!(fmt_expr(&unwrap(ident("x"))), "x.?");

        // `null orelse 0`: the fallback follows `orelse`.
        assert_eq!(fmt_expr(&orelse(null(), int(0))), "null orelse 0");
    }

    #[test]
    fn orelse_precedence_and_associativity() {
        // `orelse` is the loosest operator: the fallback `a + b` needs no
        // parentheses — `head orelse a + b`.
        let e = orelse(ident("head"), bin(BinOp::Add, ident("a"), ident("b")));
        assert_eq!(fmt_expr(&e), "head orelse a + b");

        // Left-associative chain prints without parentheses.
        let left = orelse(orelse(ident("a"), ident("b")), ident("c"));
        assert_eq!(fmt_expr(&left), "a orelse b orelse c");

        // A right-nested `orelse` keeps its parentheses (equal precedence on the
        // right of a left-associative operator).
        let right = orelse(ident("a"), orelse(ident("b"), ident("c")));
        assert_eq!(fmt_expr(&right), "a orelse (b orelse c)");

        // Unwrapping an `orelse` parenthesises it: `(a orelse b).?`.
        let uw = unwrap(orelse(ident("a"), ident("b")));
        assert_eq!(fmt_expr(&uw), "(a orelse b).?");

        // `.?` binds tightest, so an `orelse` over an unwrap needs no parens on
        // the unwrap: `x.? orelse 0`.
        let chained = orelse(unwrap(ident("x")), int(0));
        assert_eq!(fmt_expr(&chained), "x.? orelse 0");
    }

    #[test]
    fn optional_sample_is_idempotent() {
        // A whole function exercising an optional local, `orelse` and `.?`. The
        // pure printer is deterministic (idempotence here is checked as
        // re-printing the same AST byte-identically, since the parser is not
        // involved in this isolated unit).
        let m = Module {
            items: vec![Item::Func(Func {
                is_pub: false,
                name: "f".to_string(),
                params: vec![Param {
                    name: "x".to_string(),
                    ty: opt_ty("i32"),
                    is_comptime: false,
                    span: D,
                }],
                ret: ty("i32"),
                body: Block {
                    stmts: vec![
                        Stmt::Let {
                            is_const: false,
                            name: "y".to_string(),
                            ty: Some(opt_ty("i32")),
                            value: null(),
                            span: D,
                        },
                        Stmt::Return {
                            value: Some(orelse(ident("x"), unwrap(ident("y")))),
                            span: D,
                        },
                    ],
                    span: D,
                },
                span: D,
            })],
        };
        let expected = concat!(
            "fn f(x: ?i32) i32 {\n",
            "    var y: ?i32 = null;\n",
            "    return x orelse y.?;\n",
            "}\n",
        );
        let printed = print_module(&m);
        assert_eq!(printed, expected);
        // Idempotence as determinism: re-printing yields identical bytes.
        assert_eq!(print_module(&m), printed);
    }

    // ----- error unions (v0.115) -------------------------------------------

    #[test]
    fn error_union_type_prints_with_leading_bang() {
        // The error-union helper prepends `!`; the bare and optional helpers are
        // unaffected, and `!` and `?` are never combined.
        assert_eq!(fmt_type(&ty("i32")), "i32");
        assert_eq!(fmt_type(&err_ty("i32")), "!i32");
        // A struct error union formats the same way: `!Point`.
        assert_eq!(fmt_type(&err_ty("Point")), "!Point");
        // The optional prefix is independent and unchanged.
        assert_eq!(fmt_type(&opt_ty("i32")), "?i32");
    }

    #[test]
    fn error_lit_try_catch_expr_print() {
        // `error.Name` is a bare error literal.
        assert_eq!(fmt_expr(&error_lit("Oops")), "error.Oops");

        // `try parse(s)`: the call operand is a primary, so it prints bare.
        assert_eq!(
            fmt_expr(&try_(call("parse", vec![ident("s")]))),
            "try parse(s)"
        );

        // `parse(s) catch 0`: both operands are primaries, so no parentheses.
        assert_eq!(
            fmt_expr(&catch_(call("parse", vec![ident("s")]), int(0))),
            "parse(s) catch 0"
        );

        // `error.Bad` as a catch fallback: still atomic.
        assert_eq!(
            fmt_expr(&catch_(call("parse", vec![ident("s")]), error_lit("Bad"))),
            "parse(s) catch error.Bad"
        );
    }

    #[test]
    fn try_and_catch_precedence_and_associativity() {
        // `catch` is the loosest operator: the fallback `a + b` needs no
        // parentheses — `head catch a + b`.
        let e = catch_(ident("head"), bin(BinOp::Add, ident("a"), ident("b")));
        assert_eq!(fmt_expr(&e), "head catch a + b");

        // Left-associative chain prints without parentheses.
        let left = catch_(catch_(ident("a"), ident("b")), ident("c"));
        assert_eq!(fmt_expr(&left), "a catch b catch c");

        // A right-nested `catch` keeps its parentheses (equal precedence on the
        // right of a left-associative operator).
        let right = catch_(ident("a"), catch_(ident("b"), ident("c")));
        assert_eq!(fmt_expr(&right), "a catch (b catch c)");

        // `try` over a non-primary operand parenthesises it so it re-parses to
        // the same node regardless of how tightly `try` binds.
        let t = try_(bin(BinOp::Add, ident("a"), ident("b")));
        assert_eq!(fmt_expr(&t), "try (a + b)");

        // `try` over a primary (field access binds as a primary) does not.
        let tf = try_(field(ident("a"), "b"));
        assert_eq!(fmt_expr(&tf), "try a.b");
    }

    #[test]
    fn capturing_catch_prints_binder(/* SPEC §36, v0.142 */) {
        // The capturing form spells the binder between `catch` and the fallback:
        // `parse(s) catch |e| e`. Both the left operand and the `default` keep the
        // same parenthesisation as the non-capturing `catch` (the `|e|` binder
        // does not change precedence).
        assert_eq!(
            fmt_expr(&catch_cap(call("parse", vec![ident("s")]), "e", ident("e"))),
            "parse(s) catch |e| e"
        );

        // A non-trivial fallback that references the captured error code still
        // needs no parentheses — `catch` is the loosest operator.
        assert_eq!(
            fmt_expr(&catch_cap(
                ident("head"),
                "e",
                bin(BinOp::Add, ident("e"), int(1))
            )),
            "head catch |e| e + 1"
        );

        // A right-nested `catch` fallback is still parenthesised (equal
        // precedence on the right of the left-associative `catch`), exactly as in
        // the non-capturing form.
        assert_eq!(
            fmt_expr(&catch_cap(ident("a"), "e", catch_(ident("b"), ident("c")))),
            "a catch |e| (b catch c)"
        );

        // The non-capturing form is byte-for-byte unchanged.
        assert_eq!(
            fmt_expr(&catch_(call("parse", vec![ident("s")]), int(0))),
            "parse(s) catch 0"
        );
    }

    #[test]
    fn catch_roundtrips_via_source() {
        // Full lex + parse + print round-trip (`format_source` runs the lexer and
        // parser): both the non-capturing `f() catch 0` and the capturing
        // `f() catch |e| e` reach canonical form, and re-formatting is idempotent.
        let src = concat!(
            "fn g() i32 {\n",
            "    const a: i32 = f() catch 0;\n",
            "    const b: i32 = f() catch |e| e;\n",
            "    return a + b;\n",
            "}\n",
        );
        let once = format_source(src).expect("source formats");
        assert_eq!(once, src, "both catch forms reach canonical form");
        let twice = format_source(&once).expect("canonical source re-formats");
        assert_eq!(once, twice, "re-formatting is idempotent");
    }

    #[test]
    fn error_union_type_in_every_position() {
        // `!T` must print wherever a type appears: a top-level `const`, a struct
        // field, a function's params and return, and a `var`/`const` local
        // annotation. `error.Name` supplies the values.
        let const_decl = Item::Const(ConstDecl {
            is_pub: true,
            name: "NIL".to_string(),
            ty: Some(err_ty("i32")),
            value: error_lit("Oops"),
            span: D,
        });
        let strukt = Item::Struct(StructDecl {
            is_pub: false,
            name: "Box".to_string(),
            fields: vec![FieldDecl {
                name: "payload".to_string(),
                ty: err_ty("i32"),
                span: D,
            }],
            methods: vec![],
            span: D,
        });
        let func = Item::Func(Func {
            is_pub: false,
            name: "f".to_string(),
            params: vec![Param {
                name: "x".to_string(),
                ty: err_ty("i32"),
                is_comptime: false,
                span: D,
            }],
            ret: err_ty("i32"),
            body: Block {
                stmts: vec![Stmt::Let {
                    is_const: false,
                    name: "y".to_string(),
                    ty: Some(err_ty("i32")),
                    value: error_lit("Bad"),
                    span: D,
                }],
                span: D,
            },
            span: D,
        });
        let m = Module {
            items: vec![const_decl, strukt, func],
        };
        let expected = concat!(
            "pub const NIL: !i32 = error.Oops;\n",
            "\n",
            "const Box = struct {\n",
            "    payload: !i32,\n",
            "};\n",
            "\n",
            "fn f(x: !i32) !i32 {\n",
            "    var y: !i32 = error.Bad;\n",
            "}\n",
        );
        assert_eq!(print_module(&m), expected);
    }

    // ----- named error sets (SPEC §34, v0.139) ----------------------------

    #[test]
    fn named_error_set_decl_prints() {
        // `const E = error{ A, B };` — a named error set prints on one line with
        // its members joined by `, ` inside `error{ … }`, one space just inside
        // each brace, no trailing comma. (Parser-independent: the AST is built
        // directly so only the printer is exercised.)
        let m = Module {
            items: vec![Item::ErrorSet(ErrorSetDecl {
                is_pub: false,
                name: "E".to_string(),
                members: vec!["A".to_string(), "B".to_string()],
                span: D,
            })],
        };
        assert_eq!(print_module(&m), "const E = error{ A, B };\n");
    }

    #[test]
    fn named_error_set_pub_and_empty_and_single() {
        // A `pub` set keeps its leading `pub`.
        let pub_set = Module {
            items: vec![Item::ErrorSet(ErrorSetDecl {
                is_pub: true,
                name: "FileErr".to_string(),
                members: vec!["NotFound".to_string(), "Denied".to_string()],
                span: D,
            })],
        };
        assert_eq!(
            print_module(&pub_set),
            "pub const FileErr = error{ NotFound, Denied };\n"
        );

        // A single-member set: no trailing comma, spaces inside the braces.
        let single = Module {
            items: vec![Item::ErrorSet(ErrorSetDecl {
                is_pub: false,
                name: "E".to_string(),
                members: vec!["Only".to_string()],
                span: D,
            })],
        };
        assert_eq!(print_module(&single), "const E = error{ Only };\n");

        // An empty set collapses to `const Name = error{};`, mirroring the empty
        // struct/enum/union forms (keeps the printer total).
        let empty = Module {
            items: vec![Item::ErrorSet(ErrorSetDecl {
                is_pub: false,
                name: "E".to_string(),
                members: vec![],
                span: D,
            })],
        };
        assert_eq!(print_module(&empty), "const E = error{};\n");
    }

    #[test]
    fn named_error_union_type_prints_with_set_prefix() {
        // A named error union `Set!T` (`error_set = Some("Set")`, v0.139) prints
        // the set name before the `!`.
        assert_eq!(fmt_type(&set_err_ty("E", "i32")), "E!i32");
        assert_eq!(fmt_type(&set_err_ty("FileErr", "Point")), "FileErr!Point");

        // The global form `!T` (`error_set = None`, v0.115) is unchanged — a bare
        // leading `!`, with no set name.
        assert_eq!(fmt_type(&err_ty("i32")), "!i32");
        assert_eq!(fmt_type(&err_ty("Point")), "!Point");
    }

    #[test]
    fn named_error_union_in_every_type_position() {
        // `Set!T` must print wherever a type appears: a top-level `const`
        // annotation, a struct field, a function's params and return, and a local
        // `var`/`const` annotation — mirroring the global `!T` coverage test.
        let set = Item::ErrorSet(ErrorSetDecl {
            is_pub: false,
            name: "E".to_string(),
            members: vec!["A".to_string(), "B".to_string()],
            span: D,
        });
        let const_decl = Item::Const(ConstDecl {
            is_pub: true,
            name: "NIL".to_string(),
            ty: Some(set_err_ty("E", "i32")),
            value: error_lit("A"),
            span: D,
        });
        let strukt = Item::Struct(StructDecl {
            is_pub: false,
            name: "Box".to_string(),
            fields: vec![FieldDecl {
                name: "payload".to_string(),
                ty: set_err_ty("E", "i32"),
                span: D,
            }],
            methods: vec![],
            span: D,
        });
        let func = Item::Func(Func {
            is_pub: false,
            name: "f".to_string(),
            params: vec![Param {
                name: "x".to_string(),
                ty: set_err_ty("E", "i32"),
                is_comptime: false,
                span: D,
            }],
            ret: set_err_ty("E", "i32"),
            body: Block {
                stmts: vec![Stmt::Let {
                    is_const: false,
                    name: "y".to_string(),
                    ty: Some(set_err_ty("E", "i32")),
                    value: error_lit("B"),
                    span: D,
                }],
                span: D,
            },
            span: D,
        });
        let m = Module {
            items: vec![set, const_decl, strukt, func],
        };
        let expected = concat!(
            "const E = error{ A, B };\n",
            "\n",
            "pub const NIL: E!i32 = error.A;\n",
            "\n",
            "const Box = struct {\n",
            "    payload: E!i32,\n",
            "};\n",
            "\n",
            "fn f(x: E!i32) E!i32 {\n",
            "    var y: E!i32 = error.B;\n",
            "}\n",
        );
        assert_eq!(print_module(&m), expected);
    }

    #[test]
    fn named_error_set_roundtrips_via_source() {
        // Full lex + parse + print round-trip (`format_source` runs the lexer and
        // parser, not sema): a named error-set declaration and a function whose
        // return type is the named error union `E!i32` both reach canonical form,
        // and re-formatting is idempotent.
        let src = "const E = error{ A, B };\n\nfn f() E!i32 {\n    return error.A;\n}\n";
        let once = format_source(src).expect("source formats");
        assert_eq!(once, src, "named error set + E!i32 reaches canonical form");
        let twice = format_source(&once).expect("canonical source re-formats");
        assert_eq!(once, twice, "re-formatting is idempotent");
    }

    #[test]
    fn plain_error_union_still_roundtrips_unchanged() {
        // The global `!T` form is untouched by named sets: a fn returning `!i32`
        // round-trips to a bare `!i32`, exactly as in v0.115.
        let src = "fn f() !i32 {\n    return error.Oops;\n}\n";
        let once = format_source(src).expect("source formats");
        assert_eq!(once, src, "plain !i32 reaches canonical form");
        let twice = format_source(&once).expect("canonical source re-formats");
        assert_eq!(once, twice, "re-formatting is idempotent");
    }

    #[test]
    fn error_union_sample_is_idempotent() {
        // A whole function returning `!i32` that uses `try` (statement-level, as
        // a `const` initializer) and `catch` (as an expression). The pure
        // printer is deterministic, so idempotence is checked here as
        // re-printing the same AST byte-identically (the parser is not involved
        // in this isolated unit).
        let m = Module {
            items: vec![Item::Func(Func {
                is_pub: false,
                name: "f".to_string(),
                params: vec![Param {
                    name: "s".to_string(),
                    ty: ty("i32"),
                    is_comptime: false,
                    span: D,
                }],
                ret: err_ty("i32"),
                body: Block {
                    stmts: vec![
                        Stmt::Let {
                            is_const: true,
                            name: "x".to_string(),
                            ty: Some(ty("i32")),
                            value: catch_(call("parse", vec![ident("s")]), int(0)),
                            span: D,
                        },
                        Stmt::Let {
                            is_const: true,
                            name: "y".to_string(),
                            ty: Some(ty("i32")),
                            value: try_(call("parse", vec![ident("s")])),
                            span: D,
                        },
                        Stmt::Return {
                            value: Some(ident("y")),
                            span: D,
                        },
                    ],
                    span: D,
                },
                span: D,
            })],
        };
        let expected = concat!(
            "fn f(s: i32) !i32 {\n",
            "    const x: i32 = parse(s) catch 0;\n",
            "    const y: i32 = try parse(s);\n",
            "    return y;\n",
            "}\n",
        );
        let printed = print_module(&m);
        assert_eq!(printed, expected);
        // Idempotence as determinism: re-printing yields identical bytes.
        assert_eq!(print_module(&m), printed);
    }

    // ----- enums & switch (v0.116) -----------------------------------------

    fn enum_lit(variant: &str) -> Expr {
        Expr::EnumLit {
            variant: variant.to_string(),
            span: D,
        }
    }

    /// An enum variant for an [`EnumDecl`] (SPEC §13/§37): `name` with an
    /// optional explicit integer value (`Some(N)` → `name = N`, `None` → the
    /// bare auto-incrementing `name`).
    fn enum_variant(name: &str, value: Option<i64>) -> EnumVariant {
        EnumVariant {
            name: name.to_string(),
            value,
            span: D,
        }
    }

    fn arm(labels: Vec<Expr>, body: Vec<Stmt>) -> SwitchArm {
        SwitchArm {
            labels,
            ranges: vec![],
            capture: None,
            body: Block { stmts: body, span: D },
            span: D,
        }
    }

    /// A switch arm carrying inclusive integer-range labels `lo..hi` (SPEC §39),
    /// optionally alongside value `labels`. Value labels print first, then the
    /// ranges, joined with `, `.
    fn arm_ranges(labels: Vec<Expr>, ranges: Vec<(i64, i64)>, body: Vec<Stmt>) -> SwitchArm {
        SwitchArm {
            labels,
            ranges,
            capture: None,
            body: Block { stmts: body, span: D },
            span: D,
        }
    }

    /// A tagged-union switch arm `labels => |cap| { … }` (SPEC §20): like
    /// [`arm`], but binds the matched variant's payload via `capture`.
    fn arm_capture(labels: Vec<Expr>, capture: &str, body: Vec<Stmt>) -> SwitchArm {
        SwitchArm {
            labels,
            ranges: vec![],
            capture: Some(capture.to_string()),
            body: Block { stmts: body, span: D },
            span: D,
        }
    }

    fn call_stmt(callee: &str, args: Vec<Expr>) -> Stmt {
        Stmt::Expr(call(callee, args))
    }

    #[test]
    fn enum_decl_canonical_form() {
        // One variant per line, 4-space indent, trailing comma on each, `};` to
        // close. A `pub` enum keeps its leading `pub`.
        let m = Module {
            items: vec![Item::Enum(EnumDecl {
                is_pub: true,
                name: "Color".to_string(),
                variants: vec![
                    enum_variant("Red", None),
                    enum_variant("Green", None),
                    enum_variant("Blue", None),
                ],
                span: D,
            })],
        };
        let expected = "pub const Color = enum {\n    Red,\n    Green,\n    Blue,\n};\n";
        let printed = print_module(&m);
        assert_eq!(printed, expected);
        // Idempotence as determinism: re-printing yields identical bytes.
        assert_eq!(print_module(&m), printed);
    }

    #[test]
    fn empty_enum_decl_is_single_line() {
        let m = Module {
            items: vec![Item::Enum(EnumDecl {
                is_pub: false,
                name: "Never".to_string(),
                variants: vec![],
                span: D,
            })],
        };
        assert_eq!(print_module(&m), "const Never = enum {};\n");
    }

    #[test]
    fn enum_decl_explicit_values_print() {
        // Explicit values (SPEC §37): a variant with `value = Some(N)` prints
        // `Variant = N,`; one with `value = None` (auto-increment) keeps the
        // bare `Variant,`. Mixed forms in one enum print each per its own value.
        let m = Module {
            items: vec![Item::Enum(EnumDecl {
                is_pub: false,
                name: "Color".to_string(),
                variants: vec![
                    enum_variant("Red", Some(1)),
                    enum_variant("Green", None),
                    enum_variant("Blue", Some(10)),
                ],
                span: D,
            })],
        };
        let expected = "const Color = enum {\n    Red = 1,\n    Green,\n    Blue = 10,\n};\n";
        let printed = print_module(&m);
        assert_eq!(printed, expected);
        // Idempotence as determinism: re-printing yields identical bytes.
        assert_eq!(print_module(&m), printed);
    }

    #[test]
    fn enum_decl_no_values_round_trips() {
        // End-to-end (lex → parse → print), SPEC §13/§37: an enum whose variants
        // carry no explicit value is canonical as the bare-`Variant` form,
        // exactly as in v0.116 — formatting reproduces the source byte-for-byte
        // and re-formatting is idempotent (no spurious `= 0,1,2`).
        let src = "const Dir = enum {\n    A,\n    B,\n};\n";
        let once = format_source(src).expect("source formats");
        assert_eq!(once, src);
        let twice = format_source(&once).expect("canonical source re-formats");
        assert_eq!(twice, once);
    }

    #[test]
    fn enum_decl_explicit_values_round_trip_preserves_values() {
        // End-to-end (lex → parse → print), SPEC §37: explicit values survive a
        // format round-trip — `A = 1` and `C = 10` keep their `= N`, while the
        // value-less `B` stays bare (it auto-increments at the semantic layer,
        // not in the printed surface). The canonical form is already minimal, so
        // formatting is byte-stable and re-formatting is idempotent.
        let src = "const Color = enum {\n    A = 1,\n    B,\n    C = 10,\n};\n";
        let once = format_source(src).expect("source formats");
        assert_eq!(once, src);
        let twice = format_source(&once).expect("canonical source re-formats");
        assert_eq!(twice, once);
    }

    #[test]
    fn int_from_enum_builtin_prints_and_round_trips() {
        // `@intFromEnum(e)` (SPEC §37) is an `Expr::Builtin` and prints through
        // the shared builtin printer: `@` + name + parenthesised args. A single
        // ident argument prints bare, so `@intFromEnum(x)` reproduces exactly.
        assert_eq!(
            fmt_expr(&builtin("intFromEnum", vec![ident("x")])),
            "@intFromEnum(x)"
        );
        // End-to-end round-trip: the builtin form is already canonical.
        let src = "fn f() void {\n    var n = @intFromEnum(x);\n}\n";
        let once = format_source(src).expect("source formats");
        assert_eq!(once, src);
        let twice = format_source(&once).expect("canonical source re-formats");
        assert_eq!(twice, once);
    }

    #[test]
    fn enum_from_int_builtin_prints() {
        // `@enumFromInt(E, n)` (SPEC §37) — a two-argument `Expr::Builtin`. The
        // shared builtin printer joins the type-naming ident and the integer
        // value with `, ` inside the parens: `@enumFromInt(Color, n)`.
        assert_eq!(
            fmt_expr(&builtin("enumFromInt", vec![ident("Color"), ident("n")])),
            "@enumFromInt(Color, n)"
        );
    }

    #[test]
    fn enum_literal_qualified_and_unqualified_print() {
        // Unqualified `.Variant` (an `EnumLit`).
        assert_eq!(fmt_expr(&enum_lit("Red")), ".Red");

        // Qualified `Enum.Variant` reuses field access off an `Ident` base.
        assert_eq!(fmt_expr(&field(ident("Color"), "Red")), "Color.Red");

        // `.Variant` binds as a primary: a method call directly off it needs no
        // parentheses, and an `orelse` over it keeps the literal bare.
        assert_eq!(
            fmt_expr(&orelse(enum_lit("Red"), enum_lit("Blue"))),
            ".Red orelse .Blue"
        );
    }

    #[test]
    fn switch_with_else_is_idempotent() {
        // A `switch` with a multi-label arm and an `else` arm. Arms are printed
        // one indent deep, labels joined with `, `, every arm (else included)
        // ends with a trailing comma.
        let m = Module {
            items: vec![Item::Func(Func {
                is_pub: false,
                name: "f".to_string(),
                params: vec![Param {
                    name: "c".to_string(),
                    ty: ty("Color"),
                    is_comptime: false,
                    span: D,
                }],
                ret: ty("void"),
                body: Block {
                    stmts: vec![Stmt::Switch {
                        scrutinee: ident("c"),
                        arms: vec![arm(
                            vec![enum_lit("Red"), enum_lit("Green")],
                            vec![call_stmt("print", vec![int(1)])],
                        )],
                        default: Some(Block {
                            stmts: vec![call_stmt("print", vec![int(0)])],
                            span: D,
                        }),
                        span: D,
                    }],
                    span: D,
                },
                span: D,
            })],
        };
        let expected = concat!(
            "fn f(c: Color) void {\n",
            "    switch (c) {\n",
            "        .Red, .Green => {\n",
            "            print(1);\n",
            "        },\n",
            "        else => {\n",
            "            print(0);\n",
            "        },\n",
            "    }\n",
            "}\n",
        );
        let printed = print_module(&m);
        assert_eq!(printed, expected);
        // Idempotence as determinism: re-printing yields identical bytes.
        assert_eq!(print_module(&m), printed);
    }

    #[test]
    fn exhaustive_switch_without_else_prints_no_else_arm() {
        // An enum switch covering every variant has no `else` arm
        // (`default = None`); the printer emits only the explicit arms.
        let m = Module {
            items: vec![Item::Func(Func {
                is_pub: false,
                name: "g".to_string(),
                params: vec![Param {
                    name: "c".to_string(),
                    ty: ty("Color"),
                    is_comptime: false,
                    span: D,
                }],
                ret: ty("i32"),
                body: Block {
                    stmts: vec![Stmt::Switch {
                        scrutinee: ident("c"),
                        arms: vec![
                            arm(
                                vec![enum_lit("Red")],
                                vec![Stmt::Return {
                                    value: Some(int(1)),
                                    span: D,
                                }],
                            ),
                            arm(
                                vec![enum_lit("Green")],
                                vec![Stmt::Return {
                                    value: Some(int(2)),
                                    span: D,
                                }],
                            ),
                        ],
                        default: None,
                        span: D,
                    }],
                    span: D,
                },
                span: D,
            })],
        };
        let expected = concat!(
            "fn g(c: Color) i32 {\n",
            "    switch (c) {\n",
            "        .Red => {\n",
            "            return 1;\n",
            "        },\n",
            "        .Green => {\n",
            "            return 2;\n",
            "        },\n",
            "    }\n",
            "}\n",
        );
        let printed = print_module(&m);
        assert_eq!(printed, expected);
        assert_eq!(print_module(&m), printed);
    }

    #[test]
    fn integer_switch_with_else_prints() {
        // An integer scrutinee with integer labels and a required `else` arm.
        let m = Module {
            items: vec![Item::Func(Func {
                is_pub: false,
                name: "h".to_string(),
                params: vec![Param {
                    name: "n".to_string(),
                    ty: ty("i32"),
                    is_comptime: false,
                    span: D,
                }],
                ret: ty("void"),
                body: Block {
                    stmts: vec![Stmt::Switch {
                        scrutinee: ident("n"),
                        arms: vec![arm(
                            vec![int(0), int(1)],
                            vec![call_stmt("print", vec![ident("n")])],
                        )],
                        default: Some(Block {
                            stmts: vec![],
                            span: D,
                        }),
                        span: D,
                    }],
                    span: D,
                },
                span: D,
            })],
        };
        let expected = concat!(
            "fn h(n: i32) void {\n",
            "    switch (n) {\n",
            "        0, 1 => {\n",
            "            print(n);\n",
            "        },\n",
            "        else => {\n",
            "        },\n",
            "    }\n",
            "}\n",
        );
        let printed = print_module(&m);
        assert_eq!(printed, expected);
        assert_eq!(print_module(&m), printed);
    }

    // ----- switch range labels (v0.146, SPEC §39) --------------------------

    #[test]
    fn switch_single_range_label_round_trips() {
        // SPEC §39: an inclusive integer-range label `lo..hi` prints in the
        // label slot before `=>`. A range alone (no value labels) prints just
        // the range; an integer switch still requires an `else` arm.
        let m = Module {
            items: vec![Item::Func(Func {
                is_pub: false,
                name: "h".to_string(),
                params: vec![Param {
                    name: "n".to_string(),
                    ty: ty("i32"),
                    is_comptime: false,
                    span: D,
                }],
                ret: ty("void"),
                body: Block {
                    stmts: vec![Stmt::Switch {
                        scrutinee: ident("n"),
                        arms: vec![arm_ranges(
                            vec![],
                            vec![(1, 5)],
                            vec![call_stmt("print", vec![ident("n")])],
                        )],
                        default: Some(Block {
                            stmts: vec![],
                            span: D,
                        }),
                        span: D,
                    }],
                    span: D,
                },
                span: D,
            })],
        };
        let expected = concat!(
            "fn h(n: i32) void {\n",
            "    switch (n) {\n",
            "        1..5 => {\n",
            "            print(n);\n",
            "        },\n",
            "        else => {\n",
            "        },\n",
            "    }\n",
            "}\n",
        );
        let printed = print_module(&m);
        assert_eq!(printed, expected);
        // Idempotence: re-printing yields identical bytes.
        assert_eq!(print_module(&m), printed);
    }

    #[test]
    fn switch_value_and_range_labels_round_trips() {
        // SPEC §39: value labels and ranges combine in one arm. Value labels
        // print first, then ranges, joined with `, ` — here `0, 10..20`.
        let m = Module {
            items: vec![Item::Func(Func {
                is_pub: false,
                name: "h".to_string(),
                params: vec![Param {
                    name: "n".to_string(),
                    ty: ty("i32"),
                    is_comptime: false,
                    span: D,
                }],
                ret: ty("void"),
                body: Block {
                    stmts: vec![Stmt::Switch {
                        scrutinee: ident("n"),
                        arms: vec![arm_ranges(
                            vec![int(0)],
                            vec![(10, 20)],
                            vec![call_stmt("print", vec![ident("n")])],
                        )],
                        default: Some(Block {
                            stmts: vec![],
                            span: D,
                        }),
                        span: D,
                    }],
                    span: D,
                },
                span: D,
            })],
        };
        let expected = concat!(
            "fn h(n: i32) void {\n",
            "    switch (n) {\n",
            "        0, 10..20 => {\n",
            "            print(n);\n",
            "        },\n",
            "        else => {\n",
            "        },\n",
            "    }\n",
            "}\n",
        );
        let printed = print_module(&m);
        assert_eq!(printed, expected);
        assert_eq!(print_module(&m), printed);
    }

    #[test]
    fn switch_label_list_helper_handles_ranges_and_plain_labels() {
        // With no ranges the label list is byte-for-byte the pre-v0.146 output:
        // value labels joined with `, `, nothing extra (regression guard for
        // value- and multi-label switches).
        assert_eq!(fmt_arm_labels(&arm(vec![int(0), int(1)], vec![])), "0, 1");
        assert_eq!(fmt_arm_labels(&arm(vec![ident("x")], vec![])), "x");
        // A range arm appends `lo..hi` after the value labels, comma-joined.
        assert_eq!(
            fmt_arm_labels(&arm_ranges(vec![int(0)], vec![(10, 20)], vec![])),
            "0, 10..20"
        );
        // A range alone prints just the range.
        assert_eq!(fmt_arm_labels(&arm_ranges(vec![], vec![(1, 5)], vec![])), "1..5");
        // Multiple ranges keep their order after the value labels.
        assert_eq!(
            fmt_arm_labels(&arm_ranges(vec![], vec![(1, 5), (8, 9)], vec![])),
            "1..5, 8..9"
        );
        // Negative bounds print verbatim.
        assert_eq!(fmt_arm_labels(&arm_ranges(vec![], vec![(-3, -1)], vec![])), "-3..-1");
    }

    // ----- tagged unions & capture (v0.124) --------------------------------

    /// A union variant `name: PayloadType` (SPEC §20).
    fn union_variant(name: &str, payload: &str) -> UnionVariant {
        UnionVariant {
            name: name.to_string(),
            payload: ty(payload),
            span: D,
        }
    }

    #[test]
    fn union_decl_canonical_form() {
        // One `variant: Type,` per line, 4-space indent, trailing comma on each,
        // `};` to close — mirroring the struct form. A `pub` union keeps `pub`.
        let m = Module {
            items: vec![Item::Union(UnionDecl {
                is_pub: true,
                name: "Value".to_string(),
                variants: vec![union_variant("int", "i64"), union_variant("flag", "bool")],
                span: D,
            })],
        };
        let expected = "pub const Value = union(enum) {\n    int: i64,\n    flag: bool,\n};\n";
        let printed = print_module(&m);
        assert_eq!(printed, expected);
        // Idempotence as determinism: re-printing yields identical bytes.
        assert_eq!(print_module(&m), printed);
    }

    #[test]
    fn empty_union_decl_is_single_line() {
        // A variant-less union collapses to a single line, like an empty
        // struct/enum. (The grammar never produces one, but the printer stays
        // total and idempotent.)
        let m = Module {
            items: vec![Item::Union(UnionDecl {
                is_pub: false,
                name: "Empty".to_string(),
                variants: vec![],
                span: D,
            })],
        };
        assert_eq!(print_module(&m), "const Empty = union(enum) {};\n");
    }

    #[test]
    fn union_construction_reuses_struct_literal() {
        // Construction `Name{ .v = e }` is an ordinary `Expr::StructLit` (SPEC
        // §20.1): exactly one field naming a variant. No special printing.
        let lit = Expr::StructLit {
            name: "Value".to_string(),
            fields: vec![field_init("int", int(5))],
            span: D,
        };
        assert_eq!(fmt_expr(&lit), "Value{ .int = 5 }");
    }

    #[test]
    fn switch_with_capture_arm_round_trips() {
        // A tagged-union `switch`: each arm binds the matched variant's payload
        // with `=> |cap| {`. Arms are one indent deep, comma-terminated. A union
        // switch covering every variant needs no `else` arm (`default = None`).
        let m = Module {
            items: vec![Item::Func(Func {
                is_pub: false,
                name: "f".to_string(),
                params: vec![Param {
                    name: "v".to_string(),
                    ty: ty("Value"),
                    is_comptime: false,
                    span: D,
                }],
                ret: ty("void"),
                body: Block {
                    stmts: vec![Stmt::Switch {
                        scrutinee: ident("v"),
                        arms: vec![
                            arm_capture(
                                vec![enum_lit("int")],
                                "x",
                                vec![call_stmt("print", vec![ident("x")])],
                            ),
                            arm_capture(
                                vec![enum_lit("flag")],
                                "b",
                                vec![call_stmt("print", vec![int(0)])],
                            ),
                        ],
                        default: None,
                        span: D,
                    }],
                    span: D,
                },
                span: D,
            })],
        };
        let expected = concat!(
            "fn f(v: Value) void {\n",
            "    switch (v) {\n",
            "        .int => |x| {\n",
            "            print(x);\n",
            "        },\n",
            "        .flag => |b| {\n",
            "            print(0);\n",
            "        },\n",
            "    }\n",
            "}\n",
        );
        let printed = print_module(&m);
        assert_eq!(printed, expected);
        // Idempotence as determinism: re-printing yields identical bytes.
        assert_eq!(print_module(&m), printed);
    }

    #[test]
    fn switch_capture_and_non_capture_arms_mix() {
        // A capture arm and a plain `else` arm in the same switch: the capture
        // arm prints `=> |cap| {`, the `else` (which never captures) prints
        // `else => {` as before.
        let m = Module {
            items: vec![Item::Func(Func {
                is_pub: false,
                name: "g".to_string(),
                params: vec![Param {
                    name: "v".to_string(),
                    ty: ty("Value"),
                    is_comptime: false,
                    span: D,
                }],
                ret: ty("void"),
                body: Block {
                    stmts: vec![Stmt::Switch {
                        scrutinee: ident("v"),
                        arms: vec![arm_capture(
                            vec![enum_lit("int")],
                            "x",
                            vec![call_stmt("print", vec![ident("x")])],
                        )],
                        default: Some(Block {
                            stmts: vec![call_stmt("print", vec![int(0)])],
                            span: D,
                        }),
                        span: D,
                    }],
                    span: D,
                },
                span: D,
            })],
        };
        let expected = concat!(
            "fn g(v: Value) void {\n",
            "    switch (v) {\n",
            "        .int => |x| {\n",
            "            print(x);\n",
            "        },\n",
            "        else => {\n",
            "            print(0);\n",
            "        },\n",
            "    }\n",
            "}\n",
        );
        let printed = print_module(&m);
        assert_eq!(printed, expected);
        assert_eq!(print_module(&m), printed);
    }

    // ----- fixed-size arrays (v0.117) --------------------------------------

    /// An array literal `[len]elem{ e0, e1, … }`. The `elem` field carries the
    /// array's own `[len]elem` type expression (SPEC §14.1).
    fn array_lit(elem: &str, len: i64, elems: Vec<Expr>) -> Expr {
        Expr::ArrayLit {
            elem: arr_ty(elem, len),
            elems,
            span: D,
        }
    }

    /// An index expression `base[index]`.
    fn index(base: Expr, idx: Expr) -> Expr {
        Expr::Index {
            base: Box::new(base),
            index: Box::new(idx),
            span: D,
        }
    }

    #[test]
    fn array_type_prints_with_length_prefix() {
        // The array helper renders `[N]T`; the bare/optional/error helpers are
        // unaffected, and `[N]` is never combined with `?`/`!`.
        assert_eq!(fmt_type(&arr_ty("i32", 3)), "[3]i32");
        // A length-zero array still prints its prefix.
        assert_eq!(fmt_type(&arr_ty("u8", 0)), "[0]u8");
        // An array of a struct element prints the struct name after the prefix.
        assert_eq!(fmt_type(&arr_ty("Point", 2)), "[2]Point");
        // The other type forms are unchanged.
        assert_eq!(fmt_type(&ty("i32")), "i32");
        assert_eq!(fmt_type(&opt_ty("i32")), "?i32");
        assert_eq!(fmt_type(&err_ty("i32")), "!i32");
    }

    #[test]
    fn array_type_in_every_position() {
        // `[N]T` must print wherever a type appears: a top-level `const`, a
        // struct field, a function's params and return, and a `var`/`const`
        // local annotation.
        let const_decl = Item::Const(ConstDecl {
            is_pub: true,
            name: "ZEROS".to_string(),
            ty: Some(arr_ty("i32", 3)),
            value: array_lit("i32", 3, vec![int(0), int(0), int(0)]),
            span: D,
        });
        let strukt = Item::Struct(StructDecl {
            is_pub: false,
            name: "Grid".to_string(),
            fields: vec![FieldDecl {
                name: "cells".to_string(),
                ty: arr_ty("i32", 4),
                span: D,
            }],
            methods: vec![],
            span: D,
        });
        let func = Item::Func(Func {
            is_pub: false,
            name: "f".to_string(),
            params: vec![Param {
                name: "a".to_string(),
                ty: arr_ty("i32", 2),
                is_comptime: false,
                span: D,
            }],
            ret: arr_ty("i32", 2),
            body: Block {
                stmts: vec![Stmt::Let {
                    is_const: false,
                    name: "b".to_string(),
                    ty: Some(arr_ty("i32", 2)),
                    value: array_lit("i32", 2, vec![int(1), int(2)]),
                    span: D,
                }],
                span: D,
            },
            span: D,
        });
        let m = Module {
            items: vec![const_decl, strukt, func],
        };
        let expected = concat!(
            "pub const ZEROS: [3]i32 = [3]i32{ 0, 0, 0 };\n",
            "\n",
            "const Grid = struct {\n",
            "    cells: [4]i32,\n",
            "};\n",
            "\n",
            "fn f(a: [2]i32) [2]i32 {\n",
            "    var b: [2]i32 = [2]i32{ 1, 2 };\n",
            "}\n",
        );
        assert_eq!(print_module(&m), expected);
    }

    #[test]
    fn array_literal_index_and_len_expr_print() {
        // An array literal prints its `[N]T` head and `, `-joined elements
        // inside `{ … }`.
        assert_eq!(
            fmt_expr(&array_lit("i32", 3, vec![int(1), int(2), int(3)])),
            "[3]i32{ 1, 2, 3 }"
        );
        // An empty array literal collapses to `[0]T{}`.
        assert_eq!(fmt_expr(&array_lit("i32", 0, vec![])), "[0]i32{}");

        // Indexing a simple base: `a[0]`.
        assert_eq!(fmt_expr(&index(ident("a"), int(0))), "a[0]");

        // The index is a full expression and prints bare inside the brackets.
        assert_eq!(
            fmt_expr(&index(ident("a"), bin(BinOp::Add, ident("i"), int(1)))),
            "a[i + 1]"
        );

        // `a.len` reuses field access on an array (SPEC §14.1) and prints bare.
        assert_eq!(fmt_expr(&field(ident("a"), "len")), "a.len");

        // Indexing directly off an array literal needs no parentheses (both bind
        // as primaries): `[2]i32{ 7, 8 }[0]`.
        assert_eq!(
            fmt_expr(&index(array_lit("i32", 2, vec![int(7), int(8)]), int(0))),
            "[2]i32{ 7, 8 }[0]"
        );

        // Indexing a non-primary base parenthesises it to stay total: `(a orelse b)[0]`.
        assert_eq!(
            fmt_expr(&index(orelse(ident("a"), ident("b")), int(0))),
            "(a orelse b)[0]"
        );
    }

    #[test]
    fn array_sample_is_idempotent() {
        // A whole function exercising an array local, an array literal, an index
        // read, an index assignment (`FieldAssign` with an `Index` place) and a
        // `.len` read. The pure printer is deterministic, so idempotence here is
        // checked as re-printing the same AST byte-identically (the parser is not
        // involved in this isolated unit).
        let m = Module {
            items: vec![Item::Func(Func {
                is_pub: false,
                name: "f".to_string(),
                params: vec![],
                ret: ty("void"),
                body: Block {
                    stmts: vec![
                        Stmt::Let {
                            is_const: false,
                            name: "a".to_string(),
                            ty: Some(arr_ty("i32", 3)),
                            value: array_lit("i32", 3, vec![int(1), int(2), int(3)]),
                            span: D,
                        },
                        Stmt::FieldAssign {
                            place: index(ident("a"), int(0)),
                            op: None,
                            value: int(5),
                            span: D,
                        },
                        Stmt::Expr(call("print", vec![index(ident("a"), int(1))])),
                        Stmt::Expr(call("print", vec![field(ident("a"), "len")])),
                    ],
                    span: D,
                },
                span: D,
            })],
        };
        let expected = concat!(
            "fn f() void {\n",
            "    var a: [3]i32 = [3]i32{ 1, 2, 3 };\n",
            "    a[0] = 5;\n",
            "    print(a[1]);\n",
            "    print(a.len);\n",
            "}\n",
        );
        let printed = print_module(&m);
        assert_eq!(printed, expected);
        // Idempotence as determinism: re-printing yields identical bytes.
        assert_eq!(print_module(&m), printed);
    }

    // ----- pointers & slices (v0.118) --------------------------------------

    /// `&place` — address-of an lvalue (SPEC §15.1).
    fn addr_of(place: Expr) -> Expr {
        Expr::AddrOf {
            place: Box::new(place),
            span: D,
        }
    }

    /// `expr.*` — pointer dereference (SPEC §15.1).
    fn deref(expr: Expr) -> Expr {
        Expr::Deref {
            expr: Box::new(expr),
            span: D,
        }
    }

    /// `base[lo..hi]` — slice an array or slice (SPEC §15.2).
    fn slice_expr(base: Expr, lo: Expr, hi: Expr) -> Expr {
        Expr::SliceExpr {
            base: Box::new(base),
            lo: Box::new(lo),
            hi: Box::new(hi),
            span: D,
        }
    }

    #[test]
    fn pointer_type_prints_with_leading_star() {
        // The pointer helper renders `*T`; the other type forms are unaffected,
        // and `*` is never combined with `?`/`!`/`[N]`/`[]`.
        assert_eq!(fmt_type(&ptr_ty("i32")), "*i32");
        // A pointer to a struct prints the struct name after the `*`.
        assert_eq!(fmt_type(&ptr_ty("Point")), "*Point");
        // The other type forms are unchanged.
        assert_eq!(fmt_type(&ty("i32")), "i32");
        assert_eq!(fmt_type(&opt_ty("i32")), "?i32");
        assert_eq!(fmt_type(&err_ty("i32")), "!i32");
        assert_eq!(fmt_type(&arr_ty("i32", 3)), "[3]i32");
    }

    #[test]
    fn slice_type_prints_with_leading_brackets() {
        // The slice helper renders `[]T`; a slice of a struct prints the struct
        // name after the `[]`. The other type forms are unaffected.
        assert_eq!(fmt_type(&slice_ty("i32")), "[]i32");
        assert_eq!(fmt_type(&slice_ty("Point")), "[]Point");
        assert_eq!(fmt_type(&ty("i32")), "i32");
        assert_eq!(fmt_type(&arr_ty("i32", 3)), "[3]i32");
    }

    #[test]
    fn pointer_and_slice_type_in_every_position() {
        // `*T` and `[]T` must print wherever a type appears: a struct field, a
        // function's params and return, and a `var`/`const` local annotation.
        let strukt = Item::Struct(StructDecl {
            is_pub: false,
            name: "View".to_string(),
            fields: vec![
                FieldDecl {
                    name: "p".to_string(),
                    ty: ptr_ty("i32"),
                    span: D,
                },
                FieldDecl {
                    name: "s".to_string(),
                    ty: slice_ty("i32"),
                    span: D,
                },
            ],
            methods: vec![],
            span: D,
        });
        let func = Item::Func(Func {
            is_pub: false,
            name: "f".to_string(),
            params: vec![Param {
                name: "p".to_string(),
                ty: ptr_ty("i32"),
                is_comptime: false,
                span: D,
            }],
            ret: slice_ty("i32"),
            body: Block {
                stmts: vec![Stmt::Let {
                    is_const: false,
                    name: "q".to_string(),
                    ty: Some(ptr_ty("i32")),
                    value: addr_of(ident("x")),
                    span: D,
                }],
                span: D,
            },
            span: D,
        });
        let m = Module {
            items: vec![strukt, func],
        };
        let expected = concat!(
            "const View = struct {\n",
            "    p: *i32,\n",
            "    s: []i32,\n",
            "};\n",
            "\n",
            "fn f(p: *i32) []i32 {\n",
            "    var q: *i32 = &x;\n",
            "}\n",
        );
        assert_eq!(print_module(&m), expected);
    }

    #[test]
    fn addr_of_deref_and_slice_expr_print() {
        // `&x` — address-of a simple lvalue prints bare.
        assert_eq!(fmt_expr(&addr_of(ident("x"))), "&x");

        // `&a.b` — the place is a field chain (a primary), so no parentheses.
        assert_eq!(fmt_expr(&addr_of(field(ident("a"), "b"))), "&a.b");

        // `&a[0]` — the place is an index (postfix), so no parentheses.
        assert_eq!(fmt_expr(&addr_of(index(ident("a"), int(0)))), "&a[0]");

        // `p.*` — postfix deref of a simple pointer.
        assert_eq!(fmt_expr(&deref(ident("p"))), "p.*");

        // `&p.*` — address-of a deref place (the place binds as a primary).
        assert_eq!(fmt_expr(&addr_of(deref(ident("p")))), "&p.*");

        // `s[lo..hi]` — slice with identifier bounds.
        assert_eq!(
            fmt_expr(&slice_expr(ident("s"), ident("lo"), ident("hi"))),
            "s[lo..hi]"
        );

        // The bounds are full expressions and print bare inside the brackets.
        assert_eq!(
            fmt_expr(&slice_expr(
                ident("a"),
                int(0),
                bin(BinOp::Sub, ident("n"), int(1)),
            )),
            "a[0..n - 1]"
        );

        // `s[i]` (index) on a slice and `s.len` (field) reuse the existing
        // postfix forms.
        assert_eq!(fmt_expr(&index(ident("s"), ident("i"))), "s[i]");
        assert_eq!(fmt_expr(&field(ident("s"), "len")), "s.len");
    }

    #[test]
    fn pointer_and_slice_postfix_parenthesisation() {
        // `.*` binds tightest, so chained deref needs no inner parens: `p.*.*`.
        assert_eq!(fmt_expr(&deref(deref(ident("p")))), "p.*.*");

        // A deref of a non-primary operand (an `orelse`) parenthesises it.
        assert_eq!(
            fmt_expr(&deref(orelse(ident("a"), ident("b")))),
            "(a orelse b).*"
        );

        // Address-of a non-lvalue (a binary) is parenthesised to stay total.
        assert_eq!(
            fmt_expr(&addr_of(bin(BinOp::Add, ident("a"), ident("b")))),
            "&(a + b)"
        );

        // Slicing a non-primary base parenthesises it.
        assert_eq!(
            fmt_expr(&slice_expr(orelse(ident("a"), ident("b")), int(0), int(1))),
            "(a orelse b)[0..1]"
        );

        // Field access off a deref needs no parentheses (both primaries): `p.*.f`.
        assert_eq!(fmt_expr(&field(deref(ident("p")), "f")), "p.*.f");
    }

    #[test]
    fn pointer_and_slice_sample_is_idempotent() {
        // A whole function exercising a pointer (`&` address-of and `.*` deref,
        // including a deref-assign as a `FieldAssign` with a `Deref` place) and a
        // slice (`a[lo..hi]` slice, `s[i]` index read and `s.len`). The pure
        // printer is deterministic, so idempotence here is checked as re-printing
        // the same AST byte-identically (the parser is not involved in this
        // isolated unit).
        let m = Module {
            items: vec![Item::Func(Func {
                is_pub: false,
                name: "f".to_string(),
                params: vec![],
                ret: ty("void"),
                body: Block {
                    stmts: vec![
                        Stmt::Let {
                            is_const: false,
                            name: "a".to_string(),
                            ty: Some(arr_ty("i32", 3)),
                            value: array_lit("i32", 3, vec![int(1), int(2), int(3)]),
                            span: D,
                        },
                        Stmt::Let {
                            is_const: false,
                            name: "p".to_string(),
                            ty: Some(ptr_ty("i32")),
                            value: addr_of(index(ident("a"), int(0))),
                            span: D,
                        },
                        Stmt::FieldAssign {
                            place: deref(ident("p")),
                            op: None,
                            value: int(9),
                            span: D,
                        },
                        Stmt::Let {
                            is_const: false,
                            name: "s".to_string(),
                            ty: Some(slice_ty("i32")),
                            value: slice_expr(ident("a"), int(0), int(2)),
                            span: D,
                        },
                        Stmt::Expr(call("print", vec![deref(ident("p"))])),
                        Stmt::Expr(call("print", vec![index(ident("s"), int(1))])),
                        Stmt::Expr(call("print", vec![field(ident("s"), "len")])),
                    ],
                    span: D,
                },
                span: D,
            })],
        };
        let expected = concat!(
            "fn f() void {\n",
            "    var a: [3]i32 = [3]i32{ 1, 2, 3 };\n",
            "    var p: *i32 = &a[0];\n",
            "    p.* = 9;\n",
            "    var s: []i32 = a[0..2];\n",
            "    print(p.*);\n",
            "    print(s[1]);\n",
            "    print(s.len);\n",
            "}\n",
        );
        let printed = print_module(&m);
        assert_eq!(printed, expected);
        // Idempotence as determinism: re-printing yields identical bytes.
        assert_eq!(print_module(&m), printed);
    }

    // ----- comptime generics (v0.120) --------------------------------------

    /// A compile-time type parameter `comptime <name>: type` (SPEC §17.1).
    fn comptime_param(name: &str) -> Param {
        Param {
            name: name.to_string(),
            ty: ty("type"),
            is_comptime: true,
            span: D,
        }
    }

    /// An ordinary runtime parameter `<name>: <ty>`.
    fn param(name: &str, t: TypeExpr) -> Param {
        Param {
            name: name.to_string(),
            ty: t,
            is_comptime: false,
            span: D,
        }
    }

    #[test]
    fn generic_fn_param_prints_comptime_prefix() {
        // The SPEC §17 example: `fn max(comptime T: type, a: T, b: T) T { … }`.
        // The leading comptime type parameter prints with a `comptime ` prefix;
        // the runtime params that use `T` as their type are unchanged, and the
        // type-parameter names appear bare wherever they are used as types
        // (params and return).
        let m = Module {
            items: vec![Item::Func(Func {
                is_pub: false,
                name: "max".to_string(),
                params: vec![
                    comptime_param("T"),
                    param("a", ty("T")),
                    param("b", ty("T")),
                ],
                ret: ty("T"),
                body: Block {
                    stmts: vec![Stmt::If {
                        cond: bin(BinOp::Gt, ident("a"), ident("b")),
                        capture: None,
                        then: Block {
                            stmts: vec![Stmt::Return {
                                value: Some(ident("a")),
                                span: D,
                            }],
                            span: D,
                        },
                        els: None,
                        span: D,
                    }],
                    span: D,
                },
                span: D,
            })],
        };
        let expected = concat!(
            "fn max(comptime T: type, a: T, b: T) T {\n",
            "    if (a > b) {\n",
            "        return a;\n",
            "    }\n",
            "}\n",
        );
        let printed = print_module(&m);
        assert_eq!(printed, expected);
        // Idempotence as determinism: re-printing yields identical bytes.
        assert_eq!(print_module(&m), printed);
    }

    #[test]
    fn pub_generic_fn_with_only_comptime_param() {
        // A `pub` generic whose sole parameter is a comptime type parameter: the
        // `pub`, the `comptime ` prefix and the `: type` annotation all print.
        let m = Module {
            items: vec![Item::Func(Func {
                is_pub: true,
                name: "sizeName".to_string(),
                params: vec![comptime_param("T")],
                ret: ty("usize"),
                body: Block {
                    stmts: vec![Stmt::Return {
                        value: Some(int(0)),
                        span: D,
                    }],
                    span: D,
                },
                span: D,
            })],
        };
        assert_eq!(
            print_module(&m),
            "pub fn sizeName(comptime T: type) usize {\n    return 0;\n}\n"
        );
    }

    #[test]
    fn multiple_comptime_params_each_get_prefix() {
        // More than one comptime type parameter: each leading param prints with
        // its own `comptime ` prefix, and the runtime params (which may use any
        // of the bound type names, including composite forms like `[]T`) are
        // unchanged.
        let m = Module {
            items: vec![Item::Func(Func {
                is_pub: false,
                name: "pair".to_string(),
                params: vec![
                    comptime_param("T"),
                    comptime_param("U"),
                    param("xs", slice_ty("T")),
                    param("y", ty("U")),
                ],
                ret: ty("U"),
                body: Block {
                    stmts: vec![Stmt::Return {
                        value: Some(ident("y")),
                        span: D,
                    }],
                    span: D,
                },
                span: D,
            })],
        };
        assert_eq!(
            print_module(&m),
            "fn pair(comptime T: type, comptime U: type, xs: []T, y: U) U {\n    return y;\n}\n"
        );
    }

    #[test]
    fn normal_fn_has_no_comptime_prefix() {
        // A function with only runtime parameters never gains a `comptime `
        // prefix — existing (non-generic) formatting is preserved exactly.
        let m = Module {
            items: vec![Item::Func(Func {
                is_pub: false,
                name: "id".to_string(),
                params: vec![param("x", ty("i32"))],
                ret: ty("i32"),
                body: Block {
                    stmts: vec![Stmt::Return {
                        value: Some(ident("x")),
                        span: D,
                    }],
                    span: D,
                },
                span: D,
            })],
        };
        assert_eq!(print_module(&m), "fn id(x: i32) i32 {\n    return x;\n}\n");
    }

    // ----- comptime value parameters (v0.128) ------------------------------

    #[test]
    fn array_size_literal_and_param_prints() {
        // SPEC §24: an array type prints its size inside the `[…]` prefix. A
        // literal size (`ArraySize::Lit`, v0.117) prints the integer; a comptime
        // value-parameter size (`ArraySize::Param`, v0.128) prints the parameter
        // name. Both forms compose with any element type.
        assert_eq!(fmt_type(&arr_ty("i32", 3)), "[3]i32");
        assert_eq!(fmt_type(&arr_ty_param("i32", "n")), "[n]i32");
        // A multi-letter parameter name and a struct element both print verbatim
        // after the prefix.
        assert_eq!(fmt_type(&arr_ty_param("Point", "len")), "[len]Point");
        // The other (non-array) type forms are unaffected.
        assert_eq!(fmt_type(&ty("usize")), "usize");
        assert_eq!(fmt_type(&slice_ty("i32")), "[]i32");
    }

    #[test]
    fn comptime_value_param_fn_prints_comptime_prefix_and_param_array() {
        // SPEC §24: `fn zeros(comptime n: usize) [n]i32 { … }`. A comptime value
        // parameter (`is_comptime = true` with a non-`type` annotation, here
        // `usize`) prints with the same leading `comptime ` keyword as a comptime
        // *type* parameter — confirming the existing `is_comptime` printing works
        // for non-type annotations — and the `[n]i32` return type prints the
        // parameter name inside the array prefix.
        let m = Module {
            items: vec![Item::Func(Func {
                is_pub: false,
                name: "zeros".to_string(),
                params: vec![Param {
                    name: "n".to_string(),
                    ty: ty("usize"),
                    is_comptime: true,
                    span: D,
                }],
                ret: arr_ty_param("i32", "n"),
                body: Block {
                    stmts: vec![],
                    span: D,
                },
                span: D,
            })],
        };
        let expected = "fn zeros(comptime n: usize) [n]i32 {\n}\n";
        let printed = print_module(&m);
        assert_eq!(printed, expected);
        // Idempotence as determinism: re-printing yields identical bytes.
        assert_eq!(print_module(&m), printed);
    }

    #[test]
    fn comptime_value_param_mixed_with_runtime_and_type_params() {
        // A generic mixing a comptime *type* parameter, a comptime *value*
        // parameter and a runtime parameter: each comptime param keeps its
        // `comptime ` prefix, the value param's `usize` annotation prints plainly,
        // and a `[n]T` parameter type uses the bound value-parameter name.
        let m = Module {
            items: vec![Item::Func(Func {
                is_pub: true,
                name: "fill".to_string(),
                params: vec![
                    comptime_param("T"),
                    Param {
                        name: "n".to_string(),
                        ty: ty("usize"),
                        is_comptime: true,
                        span: D,
                    },
                    param("buf", arr_ty_param("T", "n")),
                ],
                ret: ty("void"),
                body: Block {
                    stmts: vec![],
                    span: D,
                },
                span: D,
            })],
        };
        assert_eq!(
            print_module(&m),
            "pub fn fill(comptime T: type, comptime n: usize, buf: [n]T) void {\n}\n"
        );
    }

    #[test]
    fn array_size_forms_round_trip() {
        // End-to-end (lex → parse → print), SPEC §24: a comptime value parameter
        // (`comptime n: usize`) with a `[n]i32` return type, and an ordinary
        // literal-sized `[3]i32` return type, are both already canonical, so
        // formatting reproduces the source byte-for-byte and re-formatting that
        // output is byte-identical (idempotence).
        let src = concat!(
            "fn zeros(comptime n: usize) [n]i32 {\n",
            "}\n",
            "\n",
            "fn three() [3]i32 {\n",
            "}\n",
        );
        let once = format_source(src).expect("source formats");
        assert_eq!(once, src);
        let twice = format_source(&once).expect("canonical source re-formats");
        assert_eq!(twice, once);
    }

    // ----- type inference for var/const (v0.121) ---------------------------

    #[test]
    fn inferred_local_let_omits_type_annotation() {
        // An inferred binding (`ty: None`) prints with no `: T`: `var x = …;` /
        // `const y = …;`. The type is recovered from the initializer in sema;
        // the formatter simply omits the annotation (SPEC §18.3).
        let m = Module {
            items: vec![Item::Func(Func {
                is_pub: false,
                name: "f".to_string(),
                params: vec![],
                ret: ty("void"),
                body: Block {
                    stmts: vec![
                        Stmt::Let {
                            is_const: false,
                            name: "x".to_string(),
                            ty: None,
                            value: int(1),
                            span: D,
                        },
                        Stmt::Let {
                            is_const: true,
                            name: "y".to_string(),
                            ty: None,
                            value: bin(BinOp::Add, ident("x"), int(2)),
                            span: D,
                        },
                        Stmt::Expr(call("print", vec![ident("y")])),
                    ],
                    span: D,
                },
                span: D,
            })],
        };
        let expected = concat!(
            "fn f() void {\n",
            "    var x = 1;\n",
            "    const y = x + 2;\n",
            "    print(y);\n",
            "}\n",
        );
        let printed = print_module(&m);
        assert_eq!(printed, expected);
        // Idempotence as determinism: re-printing yields identical bytes.
        assert_eq!(print_module(&m), printed);
    }

    #[test]
    fn annotated_and_inferred_lets_coexist() {
        // Annotated and inferred locals interleave: the annotated ones keep
        // `: T`, the inferred ones drop it. Both `var` and `const` are covered,
        // so the optional annotation is independent of the binding kind.
        let m = Module {
            items: vec![Item::Func(Func {
                is_pub: false,
                name: "g".to_string(),
                params: vec![],
                ret: ty("void"),
                body: Block {
                    stmts: vec![
                        Stmt::Let {
                            is_const: false,
                            name: "a".to_string(),
                            ty: Some(ty("i64")),
                            value: int(1),
                            span: D,
                        },
                        Stmt::Let {
                            is_const: false,
                            name: "b".to_string(),
                            ty: None,
                            value: int(2),
                            span: D,
                        },
                        Stmt::Let {
                            is_const: true,
                            name: "c".to_string(),
                            ty: Some(ty("bool")),
                            value: Expr::Bool {
                                value: true,
                                span: D,
                            },
                            span: D,
                        },
                        Stmt::Let {
                            is_const: true,
                            name: "d".to_string(),
                            ty: None,
                            value: Expr::Bool {
                                value: false,
                                span: D,
                            },
                            span: D,
                        },
                    ],
                    span: D,
                },
                span: D,
            })],
        };
        let expected = concat!(
            "fn g() void {\n",
            "    var a: i64 = 1;\n",
            "    var b = 2;\n",
            "    const c: bool = true;\n",
            "    const d = false;\n",
            "}\n",
        );
        let printed = print_module(&m);
        assert_eq!(printed, expected);
        // Idempotence as determinism: re-printing yields identical bytes.
        assert_eq!(print_module(&m), printed);
    }

    #[test]
    fn inferred_top_level_const_omits_type_annotation() {
        // A top-level inferred `const X = expr;` (`ty: None`) prints with no
        // `: T`; the annotated form keeps it. `pub` is preserved on both, and a
        // single blank line still separates the items.
        let m = Module {
            items: vec![
                Item::Const(ConstDecl {
                    is_pub: false,
                    name: "A".to_string(),
                    ty: None,
                    value: int(10),
                    span: D,
                }),
                Item::Const(ConstDecl {
                    is_pub: true,
                    name: "B".to_string(),
                    ty: None,
                    value: Expr::Bool {
                        value: true,
                        span: D,
                    },
                    span: D,
                }),
                Item::Const(ConstDecl {
                    is_pub: true,
                    name: "C".to_string(),
                    ty: Some(ty("i32")),
                    value: int(0),
                    span: D,
                }),
            ],
        };
        let expected = concat!(
            "const A = 10;\n",
            "\n",
            "pub const B = true;\n",
            "\n",
            "pub const C: i32 = 0;\n",
        );
        let printed = print_module(&m);
        assert_eq!(printed, expected);
        // Idempotence as determinism: re-printing yields identical bytes.
        assert_eq!(print_module(&m), printed);
    }

    #[test]
    fn inferred_and_annotated_forms_round_trip() {
        // End-to-end (lex → parse → print): a source mixing inferred and
        // annotated bindings — a top-level inferred `const`, an annotated
        // top-level `const`, and inferred / annotated `var`/`const` locals — is
        // already canonical, so formatting reproduces it byte-for-byte, and
        // formatting that output again is byte-identical (idempotence, SPEC §18).
        let src = concat!(
            "const MAX = 10;\n",
            "\n",
            "pub const MIN: i32 = 0;\n",
            "\n",
            "fn f() void {\n",
            "    var a = 1;\n",
            "    const b: i64 = 2;\n",
            "    var c = a + b;\n",
            "    print(c);\n",
            "}\n",
        );
        let once = format_source(src).expect("source formats");
        assert_eq!(once, src);
        let twice = format_source(&once).expect("canonical source re-formats");
        assert_eq!(twice, once);
    }

    // ----- payload captures + errdefer (v0.125) ---------------------------

    #[test]
    fn if_with_optional_capture_prints_pipe_binding() {
        // SPEC §21.1: `if (opt) |v| { … } else { … }` — the capture binds the
        // unwrapped optional in the then-block. The header prints `|v|` between
        // the `)` and the `{`; the `else` branch is the usual block form.
        let m = Module {
            items: vec![Item::Func(Func {
                is_pub: false,
                name: "f".to_string(),
                params: vec![param("opt", opt_ty("i32"))],
                ret: ty("void"),
                body: Block {
                    stmts: vec![Stmt::If {
                        cond: ident("opt"),
                        capture: Some("v".to_string()),
                        then: Block {
                            stmts: vec![call_stmt("print", vec![ident("v")])],
                            span: D,
                        },
                        els: Some(Box::new(Stmt::Block(Block {
                            stmts: vec![call_stmt("print", vec![int(0)])],
                            span: D,
                        }))),
                        span: D,
                    }],
                    span: D,
                },
                span: D,
            })],
        };
        let expected = concat!(
            "fn f(opt: ?i32) void {\n",
            "    if (opt) |v| {\n",
            "        print(v);\n",
            "    } else {\n",
            "        print(0);\n",
            "    }\n",
            "}\n",
        );
        let printed = print_module(&m);
        assert_eq!(printed, expected);
        // Idempotence as determinism: re-printing yields identical bytes.
        assert_eq!(print_module(&m), printed);
    }

    #[test]
    fn plain_if_without_capture_is_unchanged() {
        // A capture-less `if` (capture = None) prints exactly as before:
        // `if (cond) {` with no `|…|` between the `)` and the `{`.
        let m = Module {
            items: vec![Item::Func(Func {
                is_pub: false,
                name: "f".to_string(),
                params: vec![param("c", ty("bool"))],
                ret: ty("void"),
                body: Block {
                    stmts: vec![Stmt::If {
                        cond: ident("c"),
                        capture: None,
                        then: Block {
                            stmts: vec![call_stmt("print", vec![int(1)])],
                            span: D,
                        },
                        els: None,
                        span: D,
                    }],
                    span: D,
                },
                span: D,
            })],
        };
        let expected = concat!(
            "fn f(c: bool) void {\n",
            "    if (c) {\n",
            "        print(1);\n",
            "    }\n",
            "}\n",
        );
        assert_eq!(print_module(&m), expected);
    }

    #[test]
    fn errdefer_prints_like_defer_with_keyword() {
        // SPEC §21.2: `errdefer <stmt>` mirrors `defer`'s printing — the keyword
        // and the guarded statement share one line — but spells `errdefer`.
        let m = Module {
            items: vec![Item::Func(Func {
                is_pub: false,
                name: "f".to_string(),
                params: vec![],
                ret: err_ty("void"),
                body: Block {
                    stmts: vec![Stmt::ErrDefer {
                        stmt: Box::new(call_stmt("print", vec![ident("x")])),
                        span: D,
                    }],
                    span: D,
                },
                span: D,
            })],
        };
        let expected = "fn f() !void {\n    errdefer print(x);\n}\n";
        let printed = print_module(&m);
        assert_eq!(printed, expected);
        // Idempotence as determinism: re-printing yields identical bytes.
        assert_eq!(print_module(&m), printed);
    }

    #[test]
    fn optional_capture_and_errdefer_round_trip() {
        // End-to-end (lex → parse → print), SPEC §21: a function mixing an
        // `errdefer` and an optional-payload `if` capture is already canonical,
        // so formatting reproduces it byte-for-byte and re-formatting that output
        // is byte-identical (idempotence).
        let src = concat!(
            "fn f(opt: ?i32) !void {\n",
            "    errdefer print(0);\n",
            "    if (opt) |v| {\n",
            "        print(v);\n",
            "    } else {\n",
            "        print(1);\n",
            "    }\n",
            "}\n",
        );
        let once = format_source(src).expect("source formats");
        assert_eq!(once, src);
        let twice = format_source(&once).expect("canonical source re-formats");
        assert_eq!(twice, once);
    }

    // ----- string literals (v0.127) ---------------------------------------

    /// A string literal `"…"` helper (SPEC §23). `value` holds the *decoded*
    /// bytes — the lexer resolves escapes, so a `\n` in source is a real newline
    /// here — matching what the parser stores in [`Expr::StrLit`].
    fn str_lit(value: &str) -> Expr {
        Expr::StrLit {
            value: value.to_string(),
            span: D,
        }
    }

    #[test]
    fn string_literal_prints_escaped() {
        // The printer re-escapes the decoded bytes into a double-quoted literal,
        // reusing the `escape_string` helper. A real newline byte prints as the
        // two-character escape `\n`.
        assert_eq!(fmt_expr(&str_lit("hi\n")), "\"hi\\n\"");

        // Backslash, quote and tab all re-escape; ordinary bytes pass through.
        assert_eq!(fmt_expr(&str_lit("a\\b\"c\td")), "\"a\\\\b\\\"c\\td\"");

        // The empty string round-trips to `""`.
        assert_eq!(fmt_expr(&str_lit("")), "\"\"");
    }

    #[test]
    fn string_literal_binds_as_primary() {
        // A string literal is an atomic primary (SPEC §23), so it never gets
        // wrapped in parentheses as an operand — `print("hi")` and `s.len`-style
        // postfix uses print bare. Here a call argument and a `+`-operand both
        // print the literal with no surrounding parens.
        let c = call("print", vec![str_lit("hi")]);
        assert_eq!(fmt_expr(&c), "print(\"hi\")");

        // As a postfix base (indexing `"abc"[0]`) it needs no parentheses.
        let idx = Expr::Index {
            base: Box::new(str_lit("abc")),
            index: Box::new(int(0)),
            span: D,
        };
        assert_eq!(fmt_expr(&idx), "\"abc\"[0]");
    }

    #[test]
    fn string_literal_in_let() {
        // `var s = "hi\n";` (SPEC §23): an inferred binding (SPEC §18, no `: T`)
        // whose value is a string literal. The StrLit prints as the re-escaped
        // double-quoted form `"hi\n"`.
        let m = Module {
            items: vec![Item::Func(Func {
                is_pub: false,
                name: "f".to_string(),
                params: vec![],
                ret: ty("void"),
                body: Block {
                    stmts: vec![Stmt::Let {
                        is_const: false,
                        name: "s".to_string(),
                        ty: None,
                        value: str_lit("hi\n"),
                        span: D,
                    }],
                    span: D,
                },
                span: D,
            })],
        };
        let expected = "fn f() void {\n    var s = \"hi\\n\";\n}\n";
        let printed = print_module(&m);
        assert_eq!(printed, expected);
        // Idempotence as determinism: re-printing yields identical bytes.
        assert_eq!(print_module(&m), printed);
    }

    #[test]
    fn string_literal_source_round_trips() {
        // End-to-end (lex → parse → print), SPEC §23: `var s = "hi\n";` is
        // already canonical — the lexer decodes `\n` to a newline, the parser
        // stores it in `Expr::StrLit`, and the printer re-escapes it back to
        // `\n`. So formatting reproduces the source byte-for-byte and
        // re-formatting that output is byte-identical (idempotence).
        let src = "fn f() void {\n    var s = \"hi\\n\";\n}\n";
        let once = format_source(src).expect("source formats");
        assert_eq!(once, src);
        let twice = format_source(&once).expect("canonical source re-formats");
        assert_eq!(twice, once);
    }

    // ----- generic structs / type-returning functions (v0.129) -------------

    #[test]
    fn struct_type_expr_inline_forms() {
        // An anonymous `struct { … }` type value (SPEC §25.1) prints inline as a
        // single line, mirroring struct-literal spacing.

        // Empty anonymous struct type (no fields *and* no methods) → `struct {}`.
        let empty = Expr::StructType {
            fields: vec![],
            methods: vec![],
            span: D,
        };
        assert_eq!(fmt_expr(&empty), "struct {}");

        // Single field → `struct { v: T }` (the field-decl `name: Type` style).
        let one = Expr::StructType {
            fields: vec![field_decl("v", "T")],
            methods: vec![],
            span: D,
        };
        assert_eq!(fmt_expr(&one), "struct { v: T }");

        // Multiple fields join with `, `; a composite (`[]T`) field type prints
        // through `fmt_type`, so its `[]` prefix is preserved.
        let many = Expr::StructType {
            fields: vec![
                FieldDecl {
                    name: "items".to_string(),
                    ty: slice_ty("T"),
                    span: D,
                },
                field_decl("len", "usize"),
            ],
            methods: vec![],
            span: D,
        };
        assert_eq!(fmt_expr(&many), "struct { items: []T, len: usize }");
    }

    #[test]
    fn type_constructor_fn_prints_structtype_in_return() {
        // `fn Box(comptime T: type) type { return struct { v: T }; }` — a
        // type-returning function (SPEC §25.1). The `comptime T: type` parameter
        // prints with the `comptime` keyword (SPEC §17), the return type is the
        // bare `type`, and the body's `return` carries an `Expr::StructType`
        // printed inline as `struct { v: T }`. No special-casing of the function
        // or its return type is needed — the existing printers handle both.
        let m = Module {
            items: vec![Item::Func(Func {
                is_pub: false,
                name: "Box".to_string(),
                params: vec![Param {
                    name: "T".to_string(),
                    ty: ty("type"),
                    is_comptime: true,
                    span: D,
                }],
                ret: ty("type"),
                body: Block {
                    stmts: vec![Stmt::Return {
                        value: Some(Expr::StructType {
                            fields: vec![field_decl("v", "T")],
                            methods: vec![],
                            span: D,
                        }),
                        span: D,
                    }],
                    span: D,
                },
                span: D,
            })],
        };
        let expected = "fn Box(comptime T: type) type {\n    return struct { v: T };\n}\n";
        let printed = print_module(&m);
        assert_eq!(printed, expected);
        // Idempotence as determinism: the pure printer re-prints identically.
        assert_eq!(print_module(&m), printed);
    }

    #[test]
    fn type_alias_const_prints_normally() {
        // `const IL = Box(i32);` — a type alias (SPEC §25.1) is an ordinary
        // inferred `const` whose initializer is a call to a type-constructor; it
        // needs no special-casing and prints via the normal const printer with no
        // `: T` annotation.
        let m = Module {
            items: vec![Item::Const(ConstDecl {
                is_pub: false,
                name: "IL".to_string(),
                ty: None,
                value: call("Box", vec![ident("i32")]),
                span: D,
            })],
        };
        let expected = "const IL = Box(i32);\n";
        let printed = print_module(&m);
        assert_eq!(printed, expected);
        assert_eq!(print_module(&m), printed);
    }

    #[test]
    fn type_constructor_and_alias_source_round_trip() {
        // End-to-end (lex → parse → print), SPEC §25: a type-constructor and the
        // type alias that instantiates it, in canonical form, re-format
        // byte-for-byte — the `Expr::StructType` prints in the `return` and the
        // alias `const` prints as an ordinary call. Re-formatting the output is
        // byte-identical (idempotence).
        let src = concat!(
            "fn Box(comptime T: type) type {\n",
            "    return struct { v: T };\n",
            "}\n",
            "\n",
            "const IL = Box(i32);\n",
        );
        let once = format_source(src).expect("source formats");
        assert_eq!(once, src);
        let twice = format_source(&once).expect("canonical source re-formats");
        assert_eq!(twice, once);
    }

    // ----- generic-struct methods (v0.130) ---------------------------------

    /// A method `fn get(self: Self) T { return self.v; }` — a `self: Self`
    /// receiver whose body returns a field. `Self` is an ordinary type name to
    /// the formatter (resolved in sema/emit); it prints via [`fmt_type`].
    fn self_method_get() -> Func {
        Func {
            is_pub: false,
            name: "get".to_string(),
            params: vec![Param {
                name: "self".to_string(),
                ty: ty("Self"),
                is_comptime: false,
                span: D,
            }],
            ret: ty("T"),
            body: Block {
                stmts: vec![Stmt::Return {
                    value: Some(field(ident("self"), "v")),
                    span: D,
                }],
                span: D,
            },
            span: D,
        }
    }

    #[test]
    fn struct_type_with_field_and_method_inline() {
        // An anonymous struct type with a field THEN a method (SPEC §26) prints
        // inline: the field block, a `, `, then the method's inline `pub? fn …`
        // spelling. `self: Self` and the return field access print through the
        // ordinary helpers.
        let st = Expr::StructType {
            fields: vec![field_decl("v", "T")],
            methods: vec![self_method_get()],
            span: D,
        };
        assert_eq!(
            fmt_expr(&st),
            "struct { v: T, fn get(self: Self) T { return self.v; } }"
        );
        // Idempotence as determinism: the pure printer re-prints identically.
        assert_eq!(fmt_expr(&st), fmt_expr(&st));
    }

    #[test]
    fn struct_type_multiple_methods_space_separated() {
        // Two methods are separated by a single space with **no** comma between
        // them (the parser rejects a comma between methods); only the field block
        // is comma-terminated before the first method.
        let set = Func {
            is_pub: true,
            name: "withV".to_string(),
            params: vec![
                Param {
                    name: "self".to_string(),
                    ty: ty("Self"),
                    is_comptime: false,
                    span: D,
                },
                Param {
                    name: "x".to_string(),
                    ty: ty("T"),
                    is_comptime: false,
                    span: D,
                },
            ],
            ret: ty("Self"),
            body: Block {
                stmts: vec![Stmt::Return {
                    value: Some(Expr::StructLit {
                        name: "Self".to_string(),
                        fields: vec![field_init("v", ident("x"))],
                        span: D,
                    }),
                    span: D,
                }],
                span: D,
            },
            span: D,
        };
        let st = Expr::StructType {
            fields: vec![field_decl("v", "T")],
            methods: vec![self_method_get(), set],
            span: D,
        };
        assert_eq!(
            fmt_expr(&st),
            "struct { v: T, fn get(self: Self) T { return self.v; } \
             pub fn withV(self: Self, x: T) Self { return Self{ .v = x }; } }"
        );
    }

    #[test]
    fn struct_type_methods_only_no_leading_comma() {
        // A method-only anonymous struct type (e.g. an associated function with no
        // `self`) has no fields, so there is no leading comma before the first
        // method.
        let zero = Func {
            is_pub: false,
            name: "zero".to_string(),
            params: vec![],
            ret: ty("Self"),
            body: Block {
                stmts: vec![Stmt::Return {
                    value: Some(Expr::StructLit {
                        name: "Self".to_string(),
                        fields: vec![field_init("v", int(0))],
                        span: D,
                    }),
                    span: D,
                }],
                span: D,
            },
            span: D,
        };
        let st = Expr::StructType {
            fields: vec![],
            methods: vec![zero],
            span: D,
        };
        assert_eq!(
            fmt_expr(&st),
            "struct { fn zero() Self { return Self{ .v = 0 }; } }"
        );
    }

    #[test]
    fn struct_type_pointer_self_receiver() {
        // A `*Self` (pointer) receiver prints with the `*` prefix via `fmt_type`.
        let bump = Func {
            is_pub: false,
            name: "bump".to_string(),
            params: vec![Param {
                name: "self".to_string(),
                ty: ptr_ty("Self"),
                is_comptime: false,
                span: D,
            }],
            ret: ty("void"),
            body: Block {
                stmts: vec![],
                span: D,
            },
            span: D,
        };
        let st = Expr::StructType {
            fields: vec![field_decl("v", "T")],
            methods: vec![bump],
            span: D,
        };
        assert_eq!(
            fmt_expr(&st),
            "struct { v: T, fn bump(self: *Self) void {} }"
        );
    }

    #[test]
    fn fields_only_struct_type_unchanged_from_v0_129() {
        // A fields-only `Expr::StructType` (methods empty) prints exactly as in
        // v0.129 — the v0.130 `methods` field, when empty, adds nothing.
        let st = Expr::StructType {
            fields: vec![field_decl("v", "T"), field_decl("n", "usize")],
            methods: vec![],
            span: D,
        };
        assert_eq!(fmt_expr(&st), "struct { v: T, n: usize }");

        let empty = Expr::StructType {
            fields: vec![],
            methods: vec![],
            span: D,
        };
        assert_eq!(fmt_expr(&empty), "struct {}");
    }

    #[test]
    fn type_constructor_with_method_source_round_trip() {
        // End-to-end (lex → parse → print), SPEC §26: a type-constructor whose
        // returned `struct` type value carries a method round-trips in canonical
        // form byte-for-byte (the method prints inline on the `return` line), and
        // re-formatting the output is byte-identical (idempotence).
        let src = concat!(
            "fn Box(comptime T: type) type {\n",
            "    return struct { v: T, fn get(self: Self) T { return self.v; } };\n",
            "}\n",
        );
        let once = format_source(src).expect("source formats");
        assert_eq!(once, src);
        let twice = format_source(&once).expect("canonical source re-formats");
        assert_eq!(twice, once);
    }

    #[test]
    fn struct_type_method_with_control_flow_round_trip() {
        // A method body with control flow (a `while` loop with a continue clause
        // and an `if`) prints inline and round-trips, exercising the inline
        // statement printers (SPEC §26). A generic-struct method like
        // `ArrayList(T).append` relies on this staying idempotent.
        let src = concat!(
            "fn Counter(comptime T: type) type {\n",
            "    return struct { n: T, ",
            "fn run(self: Self) T { ",
            "var i: T = 0; ",
            "while (i < self.n) : (i = i + 1) { ",
            "if (i == 0) { print(i); } else { print(self.n); } } ",
            "return i; } };\n",
            "}\n",
        );
        let once = format_source(src).expect("source formats");
        assert_eq!(once, src);
        let twice = format_source(&once).expect("canonical source re-formats");
        assert_eq!(twice, once);
    }

    #[test]
    fn struct_type_method_switch_capture_and_ranges_round_trip() {
        // A `switch` inside a generic-struct method prints inline (SPEC §26)
        // and round-trips byte-for-byte: a payload-capturing arm (`=> |x|`,
        // SPEC §20), a multi-label arm mixing value labels with an inclusive
        // range (`1, 2, 5..9` — values print before ranges, SPEC §39) and a
        // trailing `else` arm, every arm comma-terminated. Pins the inline
        // switch-arm spelling to the multi-line printer's.
        let src = concat!(
            "fn Box(comptime T: type) type {\n",
            "    return struct { v: T, ",
            "fn pick(self: Self, u: Shape) T { ",
            "switch (u) { ",
            ".val => |x| { print(x); }, ",
            "1, 2, 5..9 => { print(self.v); }, ",
            "else => { print(0); }, } ",
            "return self.v; } };\n",
            "}\n",
        );
        let once = format_source(src).expect("source formats");
        assert_eq!(once, src);
        let twice = format_source(&once).expect("canonical source re-formats");
        assert_eq!(twice, once);
    }

    #[test]
    fn struct_type_method_for_with_index_round_trip() {
        // A labeled, index-capturing `for` (SPEC §29/§40) inside a
        // generic-struct method prints inline and round-trips byte-for-byte:
        // the `name: ` label prefix, the `, 0..` after the iterable and the
        // `|elem, index|` capture pair all keep the multi-line spelling.
        let src = concat!(
            "fn List(comptime T: type) type {\n",
            "    return struct { len: usize, ",
            "fn each(self: Self, xs: []T) void { ",
            "outer: for (xs, 0..) |x, i| { print(i); print(x); } } };\n",
            "}\n",
        );
        let once = format_source(src).expect("source formats");
        assert_eq!(once, src);
        let twice = format_source(&once).expect("canonical source re-formats");
        assert_eq!(twice, once);
    }

    #[test]
    fn struct_type_method_defer_and_field_assign_round_trip() {
        // `defer` / `errdefer` (SPEC §21) and field assignments (SPEC §27)
        // inside a generic-struct method print inline and round-trip
        // byte-for-byte: the deferred statement shares the keyword's line, a
        // compound `self.n += 1;` keeps the `op=` spelling and a plain
        // `self.v = x;` is unchanged.
        let src = concat!(
            "fn Box(comptime T: type) type {\n",
            "    return struct { v: T, n: i32, ",
            "fn set(self: *Self, x: T) void { ",
            "defer self.n += 1; ",
            "errdefer print(0); ",
            "self.v = x; } };\n",
            "}\n",
        );
        let once = format_source(src).expect("source formats");
        assert_eq!(once, src);
        let twice = format_source(&once).expect("canonical source re-formats");
        assert_eq!(twice, once);
    }

    // ----- compound assignment (v0.131) ------------------------------------

    /// A compound `Stmt::Assign` (`op = Some(..)`) prints `<name> <op>= <rhs>;`
    /// for each of `+= -= *= /= %=` (SPEC §27.1), with a single space on each
    /// side of the operator, while a plain assignment (`op = None`) is unchanged
    /// (`<name> = <rhs>;`). Built from the AST directly so it pins the printer
    /// independently of the parser; re-printing yields identical bytes
    /// (idempotence as determinism).
    fn compound_assign(name: &str, op: Option<BinOp>, value: Expr) -> Stmt {
        Stmt::Assign {
            name: name.to_string(),
            op,
            value,
            span: D,
        }
    }

    #[test]
    fn compound_name_assign_print_each_operator() {
        let m = Module {
            items: vec![Item::Func(Func {
                is_pub: false,
                name: "f".to_string(),
                params: vec![],
                ret: ty("void"),
                body: Block {
                    stmts: vec![
                        compound_assign("x", None, int(1)),
                        compound_assign("x", Some(BinOp::Add), int(1)),
                        compound_assign("x", Some(BinOp::Sub), int(2)),
                        compound_assign("x", Some(BinOp::Mul), int(3)),
                        compound_assign("x", Some(BinOp::Div), int(4)),
                        compound_assign("x", Some(BinOp::Rem), int(5)),
                    ],
                    span: D,
                },
                span: D,
            })],
        };
        let expected = concat!(
            "fn f() void {\n",
            "    x = 1;\n",
            "    x += 1;\n",
            "    x -= 2;\n",
            "    x *= 3;\n",
            "    x /= 4;\n",
            "    x %= 5;\n",
            "}\n",
        );
        let printed = print_module(&m);
        assert_eq!(printed, expected);
        // Idempotence as determinism: the pure printer re-prints identically.
        assert_eq!(print_module(&m), printed);
    }

    #[test]
    fn compound_field_and_index_assign_print() {
        // `a[i] -= 2;` (an `Index` place) and `s.f %= 3;` (a `Field` place) print
        // with the compound operator spelling (SPEC §27.1); the place is rendered
        // by the ordinary expression printer, unchanged from a plain `=`.
        let m = Module {
            items: vec![Item::Func(Func {
                is_pub: false,
                name: "f".to_string(),
                params: vec![],
                ret: ty("void"),
                body: Block {
                    stmts: vec![
                        Stmt::FieldAssign {
                            place: index(ident("a"), ident("i")),
                            op: Some(BinOp::Sub),
                            value: int(2),
                            span: D,
                        },
                        Stmt::FieldAssign {
                            place: field(ident("s"), "f"),
                            op: Some(BinOp::Rem),
                            value: int(3),
                            span: D,
                        },
                        // A plain field assignment (`op = None`) is unchanged.
                        Stmt::FieldAssign {
                            place: field(ident("s"), "g"),
                            op: None,
                            value: int(4),
                            span: D,
                        },
                    ],
                    span: D,
                },
                span: D,
            })],
        };
        let expected = concat!(
            "fn f() void {\n",
            "    a[i] -= 2;\n",
            "    s.f %= 3;\n",
            "    s.g = 4;\n",
            "}\n",
        );
        let printed = print_module(&m);
        assert_eq!(printed, expected);
        // Idempotence as determinism: re-printing yields identical bytes.
        assert_eq!(print_module(&m), printed);
    }

    #[test]
    fn compound_assign_round_trip() {
        // End-to-end (lex → parse → print), SPEC §27: each compound form and a
        // plain `=` are already canonical, so formatting reproduces the source
        // byte-for-byte and re-formatting that output is byte-identical
        // (idempotence). Exercises a simple-name target (`Assign`) and a
        // field/index-chain target (`FieldAssign`).
        let src = concat!(
            "fn f() void {\n",
            "    x = 1;\n",
            "    x += 1;\n",
            "    a[i] -= 2;\n",
            "    s.f %= 3;\n",
            "    y *= 4;\n",
            "    z /= 5;\n",
            "}\n",
        );
        let once = format_source(src).expect("source formats");
        assert_eq!(once, src);
        let twice = format_source(&once).expect("canonical source re-formats");
        assert_eq!(twice, once);
    }

    // ----- bitwise & shift operators (v0.132) -----------------------------

    #[test]
    fn bitwise_and_shift_operator_spellings() {
        // Each binary bitwise/shift operator prints with a single space on each
        // side (SPEC §28); the spelling matches the source `&`/`|`/`^`/`<<`/`>>`.
        assert_eq!(
            fmt_expr(&bin(BinOp::BitAnd, ident("a"), ident("b"))),
            "a & b"
        );
        assert_eq!(
            fmt_expr(&bin(BinOp::BitOr, ident("a"), ident("b"))),
            "a | b"
        );
        assert_eq!(
            fmt_expr(&bin(BinOp::BitXor, ident("a"), ident("b"))),
            "a ^ b"
        );
        assert_eq!(fmt_expr(&bin(BinOp::Shl, ident("a"), int(2))), "a << 2");
        assert_eq!(fmt_expr(&bin(BinOp::Shr, ident("a"), int(1))), "a >> 1");

        // Unary bitwise complement `~` is a prefix like `-`/`!`: bare over a
        // primary, parenthesised over a (looser) binary operand.
        let bitnot = Expr::Unary {
            op: UnOp::BitNot,
            expr: Box::new(ident("a")),
            span: D,
        };
        assert_eq!(fmt_expr(&bitnot), "~a");
        let bitnot_bin = Expr::Unary {
            op: UnOp::BitNot,
            expr: Box::new(bin(BinOp::Add, ident("a"), ident("b"))),
            span: D,
        };
        assert_eq!(fmt_expr(&bitnot_bin), "~(a + b)");
    }

    #[test]
    fn bitwise_and_shift_precedence() {
        // SPEC §28.1 ladder: `|` < `^` < `&` < equality < relational < shift <
        // additive. The formatter inserts the minimal parentheses for each.

        // `&` binds tighter than `|`, so `a | b & c` needs no parentheses.
        let e1 = bin(
            BinOp::BitOr,
            ident("a"),
            bin(BinOp::BitAnd, ident("b"), ident("c")),
        );
        assert_eq!(fmt_expr(&e1), "a | b & c");

        // The reverse grouping is below `&`, so it is parenthesised.
        let e2 = bin(
            BinOp::BitAnd,
            bin(BinOp::BitOr, ident("a"), ident("b")),
            ident("c"),
        );
        assert_eq!(fmt_expr(&e2), "(a | b) & c");

        // `^` sits between `|` and `&`: `a ^ b | c` is `(a ^ b) | c`, no parens.
        let e3 = bin(
            BinOp::BitOr,
            bin(BinOp::BitXor, ident("a"), ident("b")),
            ident("c"),
        );
        assert_eq!(fmt_expr(&e3), "a ^ b | c");

        // Shift binds tighter than equality: `a == b << c` needs no parens.
        let e4 = bin(
            BinOp::Eq,
            ident("a"),
            bin(BinOp::Shl, ident("b"), ident("c")),
        );
        assert_eq!(fmt_expr(&e4), "a == b << c");

        // Additive binds tighter than shift, so neither natural grouping needs
        // parentheses: `(a + b) << c` and `a << (b + c)` print bare.
        let e5 = bin(
            BinOp::Shl,
            bin(BinOp::Add, ident("a"), ident("b")),
            ident("c"),
        );
        assert_eq!(fmt_expr(&e5), "a + b << c");
        let e6 = bin(
            BinOp::Shl,
            ident("a"),
            bin(BinOp::Add, ident("b"), ident("c")),
        );
        assert_eq!(fmt_expr(&e6), "a << b + c");

        // The SPEC §28.3 const example: shift is looser than subtraction, so the
        // left shift is parenthesised — `(1 << 8) - 1`.
        let mask = bin(BinOp::Sub, bin(BinOp::Shl, int(1), int(8)), int(1));
        assert_eq!(fmt_expr(&mask), "(1 << 8) - 1");

        // Equality is now a distinct, looser level than relational: `a == b < c`
        // is `a == (b < c)` and prints with no parentheses (relational binds
        // tighter).
        let e7 = bin(
            BinOp::Eq,
            ident("a"),
            bin(BinOp::Lt, ident("b"), ident("c")),
        );
        assert_eq!(fmt_expr(&e7), "a == b < c");
    }

    #[test]
    fn bitwise_and_shift_source_round_trips() {
        // End-to-end (lex → parse → print), SPEC §28. The canonical spacing for
        // the bitwise/shift operators and the prefix `~` is already minimal, so
        // formatting reproduces the source byte-for-byte and re-formatting is
        // idempotent. This also pins the grammar distinctions that must survive:
        // the prefix `&a` (address-of, §15.1) and the optional-payload capture
        // `|v|` (§21) are NOT read as the infix bitand/bitor that appear in the
        // same parse; `(a | b) & c` and `(1 << 8) - 1` exercise precedence parens.
        let src = concat!(
            "const MASK = (1 << 8) - 1;\n",
            "\n",
            "fn bits(a: i32, b: i32, opt: ?i32) i32 {\n",
            "    var addr = &a;\n",
            "    var c = a & b;\n",
            "    c = (a | b) & c;\n",
            "    c = c ^ b;\n",
            "    c = c << 2;\n",
            "    c = c >> 1;\n",
            "    if (opt) |v| {\n",
            "        c = c | v;\n",
            "    }\n",
            "    return ~c;\n",
            "}\n",
        );
        let once = format_source(src).expect("source formats");
        assert_eq!(once, src);
        let twice = format_source(&once).expect("canonical source re-formats");
        assert_eq!(twice, once);
    }

    // ----- for loops over arrays & slices (v0.133) -------------------------

    #[test]
    fn for_loop_basic_form_prints() {
        // `for (xs) |x| { print(x); }` — element-by-value iteration with no index
        // capture (SPEC §29). The plain `for (<iter>) |elem| { … }` form prints,
        // with the body one indent deeper, like `while`.
        let m = Module {
            items: vec![Item::Func(Func {
                is_pub: false,
                name: "f".to_string(),
                params: vec![Param {
                    name: "xs".to_string(),
                    ty: slice_ty("i32"),
                    is_comptime: false,
                    span: D,
                }],
                ret: ty("void"),
                body: Block {
                    stmts: vec![Stmt::For {
                        iter: ident("xs"),
                        elem: "x".to_string(),
                        index: None,
                        body: Block {
                            stmts: vec![Stmt::Expr(call("print", vec![ident("x")]))],
                            span: D,
                        },
                        label: None,
                        span: D,
                    }],
                    span: D,
                },
                span: D,
            })],
        };
        let expected = concat!(
            "fn f(xs: []i32) void {\n",
            "    for (xs) |x| {\n",
            "        print(x);\n",
            "    }\n",
            "}\n",
        );
        assert_eq!(print_module(&m), expected);
    }

    #[test]
    fn for_loop_index_form_prints() {
        // `for (xs, 0..) |x, i| { … }` — the index-capture form (SPEC §29): the
        // `, 0..` follows the iterable inside the parens and a second `, i`
        // capture is appended between the pipes.
        let m = Module {
            items: vec![Item::Func(Func {
                is_pub: false,
                name: "f".to_string(),
                params: vec![Param {
                    name: "xs".to_string(),
                    ty: slice_ty("i32"),
                    is_comptime: false,
                    span: D,
                }],
                ret: ty("void"),
                body: Block {
                    stmts: vec![Stmt::For {
                        iter: ident("xs"),
                        elem: "x".to_string(),
                        index: Some("i".to_string()),
                        body: Block {
                            stmts: vec![
                                Stmt::Expr(call("print", vec![ident("x")])),
                                Stmt::Expr(call("print", vec![ident("i")])),
                            ],
                            span: D,
                        },
                        label: None,
                        span: D,
                    }],
                    span: D,
                },
                span: D,
            })],
        };
        let expected = concat!(
            "fn f(xs: []i32) void {\n",
            "    for (xs, 0..) |x, i| {\n",
            "        print(x);\n",
            "        print(i);\n",
            "    }\n",
            "}\n",
        );
        assert_eq!(print_module(&m), expected);
    }

    #[test]
    fn for_loop_source_round_trips() {
        // End-to-end (lex → parse → print), SPEC §29: `for (xs) |x| { print(x); }`
        // is already canonical, so formatting reproduces the source byte-for-byte
        // and re-formatting that output is byte-identical (idempotence).
        let src = concat!(
            "fn f(xs: []i32) void {\n",
            "    for (xs) |x| {\n",
            "        print(x);\n",
            "    }\n",
            "}\n",
        );
        let once = format_source(src).expect("source formats");
        assert_eq!(once, src);
        let twice = format_source(&once).expect("canonical source re-formats");
        assert_eq!(twice, once);
    }

    #[test]
    fn for_loop_index_source_round_trips() {
        // End-to-end, SPEC §29: the `, 0..` index-capture form
        // `for (xs, 0..) |x, i| { … }` round-trips and is idempotent.
        let src = concat!(
            "fn f(xs: []i32) void {\n",
            "    for (xs, 0..) |x, i| {\n",
            "        print(x);\n",
            "        print(i);\n",
            "    }\n",
            "}\n",
        );
        let once = format_source(src).expect("source formats");
        assert_eq!(once, src);
        let twice = format_source(&once).expect("canonical source re-formats");
        assert_eq!(twice, once);
    }

    // ----- labeled break/continue (v0.147, SPEC §40) -----------------------

    #[test]
    fn loop_label_and_target_helpers() {
        // The shared spelling helpers, independent of the parser. A `None` label
        // prints nothing (unlabeled loop unchanged); a `Some` prints `name: `.
        assert_eq!(fmt_loop_label(&None), "");
        assert_eq!(fmt_loop_label(&Some("outer".to_string())), "outer: ");
        // `break`/`continue` print the bare keyword when unlabeled, and the
        // `:label` target when labeled — each with a trailing `;`, no newline.
        assert_eq!(fmt_break(&None), "break;");
        assert_eq!(fmt_break(&Some("outer".to_string())), "break :outer;");
        assert_eq!(fmt_continue(&None), "continue;");
        assert_eq!(fmt_continue(&Some("lp".to_string())), "continue :lp;");
    }

    #[test]
    fn labeled_while_with_break_prints() {
        // `outer: while (c) { break :outer; }` — a labeled `while` with a
        // labeled `break` (SPEC §40). The label prints `outer: ` before the
        // `while` keyword and the break prints `break :outer;`. Built directly
        // from the AST so the printer is exercised independently of the parser.
        let m = Module {
            items: vec![Item::Func(Func {
                is_pub: false,
                name: "f".to_string(),
                params: vec![],
                ret: ty("void"),
                body: Block {
                    stmts: vec![Stmt::While {
                        cond: ident("c"),
                        cont: None,
                        body: Block {
                            stmts: vec![Stmt::Break {
                                target: Some("outer".to_string()),
                                span: D,
                            }],
                            span: D,
                        },
                        label: Some("outer".to_string()),
                        span: D,
                    }],
                    span: D,
                },
                span: D,
            })],
        };
        let expected = concat!(
            "fn f() void {\n",
            "    outer: while (c) {\n",
            "        break :outer;\n",
            "    }\n",
            "}\n",
        );
        assert_eq!(print_module(&m), expected);
    }

    #[test]
    fn labeled_for_with_continue_prints() {
        // `outer: for (xs) |x| { continue :outer; }` — a labeled `for` with a
        // labeled `continue` (SPEC §40). The label prints before `for`.
        let m = Module {
            items: vec![Item::Func(Func {
                is_pub: false,
                name: "f".to_string(),
                params: vec![Param {
                    name: "xs".to_string(),
                    ty: slice_ty("i32"),
                    is_comptime: false,
                    span: D,
                }],
                ret: ty("void"),
                body: Block {
                    stmts: vec![Stmt::For {
                        iter: ident("xs"),
                        elem: "x".to_string(),
                        index: None,
                        body: Block {
                            stmts: vec![Stmt::Continue {
                                target: Some("outer".to_string()),
                                span: D,
                            }],
                            span: D,
                        },
                        label: Some("outer".to_string()),
                        span: D,
                    }],
                    span: D,
                },
                span: D,
            })],
        };
        let expected = concat!(
            "fn f(xs: []i32) void {\n",
            "    outer: for (xs) |x| {\n",
            "        continue :outer;\n",
            "    }\n",
            "}\n",
        );
        assert_eq!(print_module(&m), expected);
    }

    #[test]
    fn labeled_while_break_round_trips() {
        // End-to-end (lex → parse → print), SPEC §40: a labeled `while` with a
        // labeled `break` is already canonical, so formatting reproduces the
        // source byte-for-byte and re-formatting is byte-identical (idempotent).
        let src = concat!(
            "fn f() void {\n",
            "    outer: while (c) {\n",
            "        break :outer;\n",
            "    }\n",
            "}\n",
        );
        let once = format_source(src).expect("source formats");
        assert_eq!(once, src);
        let twice = format_source(&once).expect("canonical source re-formats");
        assert_eq!(twice, once);
    }

    #[test]
    fn labeled_continue_round_trips() {
        // SPEC §40: `continue :lp;` targeting a labeled `for` round-trips and is
        // idempotent. Exercises the `for` label + `continue :label` together.
        let src = concat!(
            "fn f(xs: []i32) void {\n",
            "    lp: for (xs) |x| {\n",
            "        continue :lp;\n",
            "    }\n",
            "}\n",
        );
        let once = format_source(src).expect("source formats");
        assert_eq!(once, src);
        let twice = format_source(&once).expect("canonical source re-formats");
        assert_eq!(twice, once);
    }

    #[test]
    fn unlabeled_break_continue_unchanged() {
        // SPEC §40: an unlabeled `break;` / `continue;` inside an unlabeled loop
        // prints exactly as before v0.147 (regression guard for the `None`
        // target / `None` label paths). Round-trips and is idempotent.
        let src = concat!(
            "fn f() void {\n",
            "    while (c) {\n",
            "        continue;\n",
            "        break;\n",
            "    }\n",
            "}\n",
        );
        let once = format_source(src).expect("source formats");
        assert_eq!(once, src);
        let twice = format_source(&once).expect("canonical source re-formats");
        assert_eq!(twice, once);
    }

    #[test]
    fn labeled_break_continue_inline() {
        // The inline statement printer (SPEC §26/§40), used inside a
        // generic-struct method's `struct { … }` type value, spells labeled
        // loops and break/continue identically to the multi-line printer.
        let labeled_while = Stmt::While {
            cond: ident("c"),
            cont: None,
            body: Block {
                stmts: vec![Stmt::Break {
                    target: Some("outer".to_string()),
                    span: D,
                }],
                span: D,
            },
            label: Some("outer".to_string()),
            span: D,
        };
        assert_eq!(
            fmt_stmt_inline(&labeled_while),
            "outer: while (c) { break :outer; }"
        );
        assert_eq!(
            fmt_stmt_inline(&Stmt::Continue {
                target: Some("lp".to_string()),
                span: D,
            }),
            "continue :lp;"
        );
        // The unlabeled inline forms are unchanged.
        assert_eq!(
            fmt_stmt_inline(&Stmt::Break {
                target: None,
                span: D,
            }),
            "break;"
        );
        assert_eq!(
            fmt_stmt_inline(&Stmt::Continue {
                target: None,
                span: D,
            }),
            "continue;"
        );
    }

    // ----- comptime reflection builtins (v0.136, SPEC §32) -----------------

    /// A comptime reflection builtin `@name(args)` expression (SPEC §32.1).
    fn builtin(name: &str, args: Vec<Expr>) -> Expr {
        Expr::Builtin {
            name: name.to_string(),
            args,
            span: D,
        }
    }

    #[test]
    fn builtin_sizeof_in_let_prints() {
        // `var n = @sizeOf(i32);` — a comptime reflection builtin (SPEC §32.1)
        // as an inferred `var` initializer. The builtin prints call-shaped with
        // a leading `@`: `@<name>(<args>)`; its single type argument is the bare
        // `i32` ident.
        let m = Module {
            items: vec![Item::Func(Func {
                is_pub: false,
                name: "f".to_string(),
                params: vec![],
                ret: ty("void"),
                body: Block {
                    stmts: vec![Stmt::Let {
                        is_const: false,
                        name: "n".to_string(),
                        ty: None,
                        value: builtin("sizeOf", vec![ident("i32")]),
                        span: D,
                    }],
                    span: D,
                },
                span: D,
            })],
        };
        let expected = "fn f() void {\n    var n = @sizeOf(i32);\n}\n";
        assert_eq!(print_module(&m), expected);
    }

    #[test]
    fn builtin_typename_prints() {
        // `@typeName(Point)` prints `@typeName(Point)` (SPEC §32.1): the `@`
        // prefix, the builtin name, then the type-naming ident bare in parens.
        assert_eq!(
            fmt_expr(&builtin("typeName", vec![ident("Point")])),
            "@typeName(Point)"
        );
    }

    #[test]
    fn builtin_binds_as_primary() {
        // A builtin is a call-shaped primary (binding power 13), so it never
        // takes parentheses as a binary operand: `@sizeOf(i32) + 1` reads with
        // no parens around the builtin (SPEC §32.1).
        let e = bin(BinOp::Add, builtin("sizeOf", vec![ident("i32")]), int(1));
        assert_eq!(fmt_expr(&e), "@sizeOf(i32) + 1");
    }

    #[test]
    fn builtin_source_round_trips() {
        // End-to-end (lex → parse → print), SPEC §32.1: `@sizeOf(i32)` and
        // `@typeName(Point)` in expression position are already canonical, so
        // formatting reproduces the source byte-for-byte and re-formatting that
        // output is byte-identical (idempotence).
        let src = concat!(
            "fn f() void {\n",
            "    var n = @sizeOf(i32);\n",
            "    var s = @typeName(Point);\n",
            "}\n",
        );
        let once = format_source(src).expect("source formats");
        assert_eq!(once, src);
        let twice = format_source(&once).expect("canonical source re-formats");
        assert_eq!(twice, once);
    }

    #[test]
    fn self_pointer_param_prints_as_self() {
        // A plain struct method whose receiver is `self: *Self` (the desugaring
        // of `*@This()`, SPEC §32.2) prints the param type as `*Self`: the
        // TypeExpr printer emits the `*` pointer prefix and the bare `Self` type
        // name with no special handling (the parser desugars `@This()` to
        // `Self`, so the formatter never sees `@This()` in type position).
        let m = Module {
            items: vec![Item::Struct(StructDecl {
                is_pub: false,
                name: "Point".to_string(),
                fields: vec![FieldDecl {
                    name: "x".to_string(),
                    ty: ty("i32"),
                    span: D,
                }],
                methods: vec![Func {
                    is_pub: false,
                    name: "translate".to_string(),
                    params: vec![
                        Param {
                            name: "self".to_string(),
                            ty: ptr_ty("Self"),
                            is_comptime: false,
                            span: D,
                        },
                        Param {
                            name: "dx".to_string(),
                            ty: ty("i32"),
                            is_comptime: false,
                            span: D,
                        },
                    ],
                    ret: ty("void"),
                    body: Block {
                        stmts: vec![],
                        span: D,
                    },
                    span: D,
                }],
                span: D,
            })],
        };
        let out = print_module(&m);
        assert!(
            out.contains("fn translate(self: *Self, dx: i32) void {"),
            "expected a `*Self` self param, got:\n{out}"
        );
    }

    #[test]
    fn self_pointer_param_source_round_trips() {
        // End-to-end (lex → parse → print), SPEC §32.2: a plain struct method
        // with a `self: *Self` receiver is already canonical, so formatting
        // reproduces the source byte-for-byte and re-formatting is idempotent.
        let src = concat!(
            "const Point = struct {\n",
            "    x: i32,\n",
            "\n",
            "    fn translate(self: *Self, dx: i32) void {\n",
            "        self.x += dx;\n",
            "    }\n",
            "};\n",
        );
        let once = format_source(src).expect("source formats");
        assert_eq!(once, src);
        let twice = format_source(&once).expect("canonical source re-formats");
        assert_eq!(twice, once);
    }

    // ----- @panic / unreachable (v0.141, SPEC §35) -------------------------

    /// `unreachable` (`Expr::Unreachable`) prints as the bare keyword.
    fn unreachable_expr() -> Expr {
        Expr::Unreachable { span: D }
    }

    /// `@panic(<arg>)` — an `Expr::Builtin { name: "panic" }` with one argument
    /// (SPEC §32/§35); reuses the existing builtin printer.
    fn panic_call(arg: Expr) -> Expr {
        Expr::Builtin {
            name: "panic".to_string(),
            args: vec![arg],
            span: D,
        }
    }

    #[test]
    fn unreachable_expr_prints_bare_keyword() {
        // The bare expression prints as `unreachable` (no parens, no args).
        assert_eq!(fmt_expr(&unreachable_expr()), "unreachable");
    }

    #[test]
    fn unreachable_statement_prints_with_semicolon() {
        // As a statement (`Stmt::Expr`), it prints `unreachable;` one indent deep
        // inside the function body, like any other expression statement.
        let m = Module {
            items: vec![Item::Func(Func {
                is_pub: false,
                name: "f".to_string(),
                params: vec![],
                ret: ty("void"),
                body: Block {
                    stmts: vec![Stmt::Expr(unreachable_expr())],
                    span: D,
                },
                span: D,
            })],
        };
        let expected = "fn f() void {\n    unreachable;\n}\n";
        let printed = print_module(&m);
        assert_eq!(printed, expected);
        // Idempotence as determinism: re-printing yields identical bytes.
        assert_eq!(print_module(&m), printed);
    }

    #[test]
    fn unreachable_in_switch_else_arm_prints() {
        // `unreachable` as the (only) statement of a switch `else` arm body:
        // `else => { unreachable; },` — the canonical arm layout (SPEC §13/§35).
        let m = Module {
            items: vec![Item::Func(Func {
                is_pub: false,
                name: "g".to_string(),
                params: vec![Param {
                    name: "c".to_string(),
                    ty: ty("Color"),
                    is_comptime: false,
                    span: D,
                }],
                ret: ty("void"),
                body: Block {
                    stmts: vec![Stmt::Switch {
                        scrutinee: ident("c"),
                        arms: vec![arm(
                            vec![enum_lit("Red")],
                            vec![call_stmt("print", vec![int(1)])],
                        )],
                        default: Some(Block {
                            stmts: vec![Stmt::Expr(unreachable_expr())],
                            span: D,
                        }),
                        span: D,
                    }],
                    span: D,
                },
                span: D,
            })],
        };
        let expected = concat!(
            "fn g(c: Color) void {\n",
            "    switch (c) {\n",
            "        .Red => {\n",
            "            print(1);\n",
            "        },\n",
            "        else => {\n",
            "            unreachable;\n",
            "        },\n",
            "    }\n",
            "}\n",
        );
        let printed = print_module(&m);
        assert_eq!(printed, expected);
        // Idempotence as determinism: re-printing yields identical bytes.
        assert_eq!(print_module(&m), printed);
    }

    #[test]
    fn panic_builtin_prints_via_builtin_printer() {
        // `@panic("boom")` round-trips through the existing `Expr::Builtin`
        // printer: `@` + name + parenthesised args, the single `[]u8` argument
        // printed as a re-escaped string literal.
        assert_eq!(fmt_expr(&panic_call(str_lit("boom"))), "@panic(\"boom\")");
        // A message with escapes re-escapes correctly inside the literal.
        assert_eq!(
            fmt_expr(&panic_call(str_lit("a\nb"))),
            "@panic(\"a\\nb\")"
        );
    }

    #[test]
    fn panic_statement_prints_with_semicolon() {
        // `@panic("x");` as a statement, one indent deep in the function body.
        let m = Module {
            items: vec![Item::Func(Func {
                is_pub: false,
                name: "f".to_string(),
                params: vec![],
                ret: ty("void"),
                body: Block {
                    stmts: vec![Stmt::Expr(panic_call(str_lit("x")))],
                    span: D,
                },
                span: D,
            })],
        };
        let expected = "fn f() void {\n    @panic(\"x\");\n}\n";
        let printed = print_module(&m);
        assert_eq!(printed, expected);
        // Idempotence as determinism: re-printing yields identical bytes.
        assert_eq!(print_module(&m), printed);
    }

    #[test]
    fn unreachable_and_panic_round_trip_via_source() {
        // Full lex → parse → print round-trip (`format_source`): a function with
        // an `unreachable;` statement and an `@panic("x");` statement is already
        // canonical, so formatting reproduces the source byte-for-byte and
        // re-formatting is idempotent (SPEC §35).
        let src = concat!(
            "fn f() void {\n",
            "    unreachable;\n",
            "    @panic(\"x\");\n",
            "}\n",
        );
        let once = format_source(src).expect("source formats");
        assert_eq!(once, src, "unreachable + @panic reach canonical form");
        let twice = format_source(&once).expect("canonical source re-formats");
        assert_eq!(twice, once, "re-formatting is idempotent");
    }

    #[test]
    fn unreachable_in_arm_round_trips_via_source() {
        // `unreachable` inside a switch `else` arm body, full source round-trip.
        let src = concat!(
            "fn g(c: Color) void {\n",
            "    switch (c) {\n",
            "        .Red => {\n",
            "            print(1);\n",
            "        },\n",
            "        else => {\n",
            "            unreachable;\n",
            "        },\n",
            "    }\n",
            "}\n",
        );
        let once = format_source(src).expect("source formats");
        assert_eq!(once, src, "unreachable in an arm reaches canonical form");
        let twice = format_source(&once).expect("canonical source re-formats");
        assert_eq!(twice, once, "re-formatting is idempotent");
    }

    // ----- floating point `f64` (v0.144) -----------------------------------

    /// A floating-point literal `Expr::Float` of type `f64` (SPEC §38).
    fn float(value: f64) -> Expr {
        Expr::Float { value, span: D }
    }

    #[test]
    fn float_literal_prints_with_decimal_point() {
        // A `f64` literal prints via Rust's `{:?}`, which always keeps a decimal
        // point on a finite value (SPEC §38): `3.14` stays `3.14`, and a whole
        // value `3.0` keeps its `.0` (it must not collapse to `3`, or it would
        // re-lex as an integer). The pure printer is deterministic, so printing
        // twice is byte-identical (idempotence as determinism).
        assert_eq!(fmt_expr(&float(3.14)), "3.14");
        assert_eq!(fmt_expr(&float(3.0)), "3.0");
        assert_eq!(fmt_expr(&float(3.14)), fmt_expr(&float(3.14)));
        // A float literal binds as a primary, like an integer literal, so it
        // never needs surrounding parentheses as a binary operand.
        assert_eq!(
            fmt_expr(&bin(BinOp::Add, float(1.5), float(2.5))),
            "1.5 + 2.5"
        );
    }

    #[test]
    fn integer_literal_still_prints_without_decimal_point() {
        // Integer literals are unchanged (SPEC §3): `3` prints `3`, never `3.0`.
        assert_eq!(fmt_expr(&int(3)), "3");
        assert_eq!(fmt_expr(&int(0)), "0");
    }

    #[test]
    fn direct_application_source_round_trips() {
        // End-to-end (lex → parse → print), SPEC §42.1: a direct generic-type
        // application in type position — composed with prefix forms, nested,
        // and as an assoc-call receiver — is already canonical, so formatting
        // reproduces the source byte-for-byte and is idempotent.
        let src = "fn f(a: Allocator) void {\n    var l: ArrayList(i32) = ArrayList(i32).init(a);\n    var m: Map(i32, i64) = Map(i32, i64).init(a);\n    var n: ?Box(Box(i32)) = null;\n}\n";
        let once = format_source(src).expect("source formats");
        assert_eq!(once, src);
        let twice = format_source(&once).expect("canonical source re-formats");
        assert_eq!(twice, once, "re-formatting is idempotent");
    }

    #[test]
    fn float_let_source_round_trips() {
        // End-to-end (lex → parse → print), SPEC §38: `var x = 3.14;` is already
        // canonical — the lexer decodes the `digits.digits` literal into a
        // `Float` token, the parser stores it in `Expr::Float`, and the printer
        // re-emits `3.14`. So formatting reproduces the source byte-for-byte and
        // re-formatting that output is byte-identical (idempotence).
        let src = "fn f() void {\n    var x = 3.14;\n}\n";
        let once = format_source(src).expect("source formats");
        assert_eq!(once, src);
        let twice = format_source(&once).expect("canonical source re-formats");
        assert_eq!(twice, once, "re-formatting is idempotent");
    }

    #[test]
    fn whole_float_keeps_decimal_point_round_trip() {
        // A whole-valued `f64` literal `3.0` must keep its `.0` through a full
        // source round-trip — collapsing to `3` would silently retype it as an
        // integer. The lexer requires a digit on both sides of the `.`, so `3.0`
        // is the canonical lexable form.
        let src = "fn f() void {\n    var x = 3.0;\n}\n";
        let once = format_source(src).expect("source formats");
        assert_eq!(once, src, "the .0 is preserved");
        let twice = format_source(&once).expect("canonical source re-formats");
        assert_eq!(twice, once, "re-formatting is idempotent");
    }

    #[test]
    fn integer_let_source_unchanged() {
        // The integer path is untouched by f64 support: `var x = 3;` still
        // formats to `3` (no spurious `.0`) and round-trips unchanged (SPEC §3).
        let src = "fn f() void {\n    var x = 3;\n}\n";
        let once = format_source(src).expect("source formats");
        assert_eq!(once, src, "integer literal stays an integer");
        let twice = format_source(&once).expect("canonical source re-formats");
        assert_eq!(twice, once, "re-formatting is idempotent");
    }

    // ----- direct generic-type application (v0.152, SPEC §42) ---------------

    /// Assert that `ty_expr` renders as `spelling` in BOTH printers (SPEC
    /// §42.1): as a `var` annotation in a multi-line function body
    /// ([`Printer::print_stmt`]) and as the same statement inline inside an
    /// [`Expr::StructType`] method body ([`fmt_stmt_inline`]) — pinning that
    /// the application spelling is single-sourced in [`fmt_type`], so both
    /// printers get it for free. The printer is syntax-only, so the dummy `0`
    /// initializer never matters.
    fn assert_type_spelling_in_both_printers(ty_expr: TypeExpr, spelling: &str) {
        let decl = |ty_expr: TypeExpr| Stmt::Let {
            is_const: false,
            name: "l".to_string(),
            ty: Some(ty_expr),
            value: int(0),
            span: D,
        };
        // Multi-line: a `var` decl in a function body, one indent deep.
        let m = Module {
            items: vec![Item::Func(Func {
                is_pub: false,
                name: "f".to_string(),
                params: vec![],
                ret: ty("void"),
                body: Block {
                    stmts: vec![decl(ty_expr.clone())],
                    span: D,
                },
                span: D,
            })],
        };
        assert_eq!(
            print_module(&m),
            format!("fn f() void {{\n    var l: {} = 0;\n}}\n", spelling),
            "multi-line printer spelling for `{spelling}`"
        );
        // Inline: the same statement inside an `Expr::StructType` method body
        // (the generic-struct method context, SPEC §26).
        let st = Expr::StructType {
            fields: vec![],
            methods: vec![Func {
                is_pub: false,
                name: "m".to_string(),
                params: vec![],
                ret: ty("void"),
                body: Block {
                    stmts: vec![decl(ty_expr)],
                    span: D,
                },
                span: D,
            }],
            span: D,
        };
        assert_eq!(
            fmt_expr(&st),
            format!("struct {{ fn m() void {{ var l: {} = 0; }} }}", spelling),
            "inline printer spelling for `{spelling}`"
        );
    }

    #[test]
    fn application_type_spelling() {
        // A generic type-constructor application in type position (SPEC §42.1)
        // prints as `Name(args…)`: single-arg, multi-arg (`, `-joined) and a
        // nested application (each argument renders recursively through
        // `fmt_type`).
        assert_eq!(
            fmt_type(&app_ty("ArrayList", vec![ty("i32")])),
            "ArrayList(i32)"
        );
        assert_eq!(
            fmt_type(&app_ty("Map", vec![ty("i64"), ty("Point")])),
            "Map(i64, Point)"
        );
        assert_eq!(
            fmt_type(&app_ty("ArrayList", vec![app_ty("ArrayList", vec![ty("i32")])])),
            "ArrayList(ArrayList(i32))"
        );
        // `Some(vec![])` prints `Name()` — it re-lexes to the same `TypeExpr`
        // (sema rejects the zero-arg application; the printer stays total).
        assert_eq!(fmt_type(&app_ty("ArrayList", vec![])), "ArrayList()");
        // A plain named type (`ctor_args == None`) is unchanged.
        assert_eq!(fmt_type(&ty("ArrayList")), "ArrayList");
    }

    #[test]
    fn application_type_composes_with_prefixes() {
        // The application composes with every prefix form (SPEC §42.1): each
        // prefix wraps the `Name(args)` base spelling exactly as it wraps a
        // bare name.
        let app = || app_ty("ArrayList", vec![ty("i32")]);
        assert_eq!(
            fmt_type(&TypeExpr {
                optional: true,
                ..app()
            }),
            "?ArrayList(i32)"
        );
        assert_eq!(
            fmt_type(&TypeExpr {
                error_union: true,
                ..app()
            }),
            "!ArrayList(i32)"
        );
        // A *named* error union `Set!T` (SPEC §34/§42.1): the set name is
        // always a plain name — only the payload may be an application.
        assert_eq!(
            fmt_type(&TypeExpr {
                error_union: true,
                error_set: Some("E".to_string()),
                ..app()
            }),
            "E!ArrayList(i32)"
        );
        assert_eq!(
            fmt_type(&TypeExpr {
                pointer: true,
                ..app()
            }),
            "*ArrayList(i32)"
        );
        assert_eq!(
            fmt_type(&TypeExpr {
                slice: true,
                ..app()
            }),
            "[]ArrayList(i32)"
        );
        assert_eq!(
            fmt_type(&TypeExpr {
                array_len: Some(ArraySize::Lit(3)),
                ..app()
            }),
            "[3]ArrayList(i32)"
        );
        assert_eq!(
            fmt_type(&TypeExpr {
                array_len: Some(ArraySize::Param("n".to_string())),
                ..app()
            }),
            "[n]ArrayList(i32)"
        );
    }

    #[test]
    fn application_type_in_both_printers_all_forms() {
        // Every §42.1 form — plain, multi-arg, nested and each prefix
        // composition — renders identically in the multi-line and inline
        // printers through the single `fmt_type` spelling (SPEC §42.1).
        let app = || app_ty("ArrayList", vec![ty("i32")]);
        assert_type_spelling_in_both_printers(app(), "ArrayList(i32)");
        assert_type_spelling_in_both_printers(
            app_ty("Map", vec![ty("i64"), ty("Point")]),
            "Map(i64, Point)",
        );
        assert_type_spelling_in_both_printers(
            app_ty("ArrayList", vec![app()]),
            "ArrayList(ArrayList(i32))",
        );
        assert_type_spelling_in_both_printers(
            TypeExpr {
                optional: true,
                ..app()
            },
            "?ArrayList(i32)",
        );
        assert_type_spelling_in_both_printers(
            TypeExpr {
                error_union: true,
                ..app()
            },
            "!ArrayList(i32)",
        );
        assert_type_spelling_in_both_printers(
            TypeExpr {
                error_union: true,
                error_set: Some("E".to_string()),
                ..app()
            },
            "E!ArrayList(i32)",
        );
        assert_type_spelling_in_both_printers(
            TypeExpr {
                pointer: true,
                ..app()
            },
            "*ArrayList(i32)",
        );
        assert_type_spelling_in_both_printers(
            TypeExpr {
                slice: true,
                ..app()
            },
            "[]ArrayList(i32)",
        );
        assert_type_spelling_in_both_printers(
            TypeExpr {
                array_len: Some(ArraySize::Lit(3)),
                ..app()
            },
            "[3]ArrayList(i32)",
        );
        assert_type_spelling_in_both_printers(
            TypeExpr {
                array_len: Some(ArraySize::Param("n".to_string())),
                ..app()
            },
            "[n]ArrayList(i32)",
        );
    }

    #[test]
    fn application_type_var_decl_with_assoc_init_prints() {
        // SPEC §42's flagship line, multi-line context:
        // `var l: ArrayList(i32) = ArrayList(i32).init(a);` — the annotation
        // prints through the shared `fmt_type` (§42.1) and the initializer is
        // the *existing* `Expr::MethodCall` over an `Expr::Call` receiver
        // (§42.1: no new expression syntax), which already prints
        // `ArrayList(i32).init(a)` unparenthesised (a call is a primary).
        let m = Module {
            items: vec![Item::Func(Func {
                is_pub: false,
                name: "f".to_string(),
                params: vec![Param {
                    name: "a".to_string(),
                    ty: ty("Allocator"),
                    is_comptime: false,
                    span: D,
                }],
                ret: ty("void"),
                body: Block {
                    stmts: vec![Stmt::Let {
                        is_const: false,
                        name: "l".to_string(),
                        ty: Some(app_ty("ArrayList", vec![ty("i32")])),
                        value: Expr::MethodCall {
                            receiver: Box::new(call("ArrayList", vec![ident("i32")])),
                            method: "init".to_string(),
                            args: vec![ident("a")],
                            span: D,
                        },
                        span: D,
                    }],
                    span: D,
                },
                span: D,
            })],
        };
        let expected = concat!(
            "fn f(a: Allocator) void {\n",
            "    var l: ArrayList(i32) = ArrayList(i32).init(a);\n",
            "}\n",
        );
        let printed = print_module(&m);
        assert_eq!(printed, expected);
        // Idempotence as determinism: the pure printer re-prints identically.
        assert_eq!(print_module(&m), printed);
    }

    #[test]
    fn application_type_in_struct_type_field_inline() {
        // Generic composition, inline context (SPEC §42.1): a field of
        // application type inside an anonymous `struct { … }` type value —
        // `fn Stack(comptime T: type) type { return struct { items:
        // ArrayList(T) }; }`'s payload — prints the application through the
        // shared `fmt_type` in the inline field-decl spelling.
        let st = Expr::StructType {
            fields: vec![FieldDecl {
                name: "items".to_string(),
                ty: app_ty("ArrayList", vec![ty("T")]),
                span: D,
            }],
            methods: vec![],
            span: D,
        };
        assert_eq!(fmt_expr(&st), "struct { items: ArrayList(T) }");
    }
}
