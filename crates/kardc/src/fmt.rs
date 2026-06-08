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
//! - `if (cond) { … } else if (cond) { … } else { … }`.
//! - `while (cond) { … }` / `while (cond) : (cont) { … }`.
//! - `const NAME: T = expr;` / `var name: T = expr;` / `return expr;`. The type
//!   annotation is optional (SPEC §18): an inferred binding prints with no
//!   `: T` — `const NAME = expr;` / `var name = expr;`.
//! - `defer <stmt>`; `test "name" { … }`.
//! - `const Name = struct { f: T, … };` — one field per line, 4-space indent,
//!   trailing comma on each; an empty struct prints `const Name = struct {};`.
//!   Struct literals print `Name{ .f = e, … }` and field access `base.field`
//!   (SPEC §9).
//! - `const Name = enum { A, B, … };` — one variant per line, 4-space indent,
//!   trailing comma on each; an empty enum prints `const Name = enum {};`. An
//!   unqualified enum literal prints `.Variant`; the qualified form reuses field
//!   access (`Enum.Variant`). A `switch` prints with each arm `labels => { … }`
//!   indented one level, arms comma-terminated, an `else` arm last (SPEC §13).
//!
//! ## Idempotence
//!
//! Parenthesisation is precedence-driven and minimal, so re-formatting the
//! canonical output produces byte-identical text.

use crate::ast::{
    BinOp, Block, ConstDecl, EnumDecl, Expr, Func, Item, Module, Stmt, StructDecl, SwitchArm,
    TestBlock, TypeExpr, UnOp,
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
        if f.is_pub {
            self.out.push_str("pub ");
        }
        self.out.push_str("fn ");
        self.out.push_str(&f.name);
        self.out.push('(');
        for (i, param) in f.params.iter().enumerate() {
            if i > 0 {
                self.out.push_str(", ");
            }
            // A compile-time type parameter (`comptime T: type`, SPEC §17.1)
            // prints with a leading `comptime ` keyword; everything else about
            // the parameter list is unchanged.
            if param.is_comptime {
                self.out.push_str("comptime ");
            }
            self.out.push_str(&param.name);
            self.out.push_str(": ");
            self.out.push_str(&fmt_type(&param.ty));
        }
        self.out.push_str(") ");
        self.out.push_str(&fmt_type(&f.ret));
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

    /// Print an enum declaration (SPEC §13). One `    Variant,` per line with a
    /// 4-space indent and a trailing comma on every variant, then `};` to close.
    /// An empty enum — no variants — collapses to `const Name = enum {};` on a
    /// single line. A `pub` enum keeps its leading `pub`.
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
            self.out.push_str(variant);
            self.out.push_str(",\n");
        }
        self.indent -= 1;
        self.write_indent();
        self.out.push_str("};\n");
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
                self.out.push_str(if *is_const { "const " } else { "var " });
                self.out.push_str(name);
                // The type annotation is optional (SPEC §18): `var name: T = …;`
                // when present, `var name = …;` when inferred from the value.
                if let Some(ty) = ty {
                    self.out.push_str(": ");
                    self.out.push_str(&fmt_type(ty));
                }
                self.out.push_str(" = ");
                self.out.push_str(&fmt_expr(value));
                self.out.push_str(";\n");
            }
            Stmt::Assign { name, value, .. } => {
                self.write_indent();
                self.out.push_str(name);
                self.out.push_str(" = ");
                self.out.push_str(&fmt_expr(value));
                self.out.push_str(";\n");
            }
            Stmt::FieldAssign { place, value, .. } => {
                self.write_indent();
                self.out.push_str(&fmt_expr(place));
                self.out.push_str(" = ");
                self.out.push_str(&fmt_expr(value));
                self.out.push_str(";\n");
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
                cond, then, els, ..
            } => self.print_if(cond, then, els),
            Stmt::While {
                cond, cont, body, ..
            } => {
                self.write_indent();
                self.out.push_str("while (");
                self.out.push_str(&fmt_expr(cond));
                self.out.push(')');
                if let Some(c) = cont {
                    self.out.push_str(" : (");
                    self.out.push_str(&fmt_cont(c));
                    self.out.push(')');
                }
                self.out.push_str(" {\n");
                self.print_block_body(body);
                self.write_indent();
                self.out.push_str("}\n");
            }
            Stmt::Break(_) => {
                self.write_indent();
                self.out.push_str("break;\n");
            }
            Stmt::Continue(_) => {
                self.write_indent();
                self.out.push_str("continue;\n");
            }
            Stmt::Defer { stmt, .. } => {
                self.write_indent();
                self.out.push_str("defer ");
                // The guarded statement shares the `defer` line: suppress the
                // indent it would otherwise emit for its first line.
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

    /// Print a `switch` statement (SPEC §13). The header is
    /// `switch (<scrutinee>) {`; each arm is printed one indent deeper as
    /// `<labels> => {` (labels joined with `, `) followed by its body and a
    /// closing `},`. The optional `else` arm prints last as `else => { … },`.
    /// Every arm — the `else` included — ends with a trailing comma (the parser
    /// accepts a trailing comma after a `}` block), which keeps the canonical
    /// form uniform and idempotent.
    fn print_switch(&mut self, scrutinee: &Expr, arms: &[SwitchArm], default: &Option<Block>) {
        self.write_indent();
        self.out.push_str("switch (");
        self.out.push_str(&fmt_expr(scrutinee));
        self.out.push_str(") {\n");
        self.indent += 1;
        for arm in arms {
            self.write_indent();
            for (i, label) in arm.labels.iter().enumerate() {
                if i > 0 {
                    self.out.push_str(", ");
                }
                self.out.push_str(&fmt_expr(label));
            }
            self.out.push_str(" => {\n");
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

    /// Print an `if`/`else if`/`else` chain. `cond`/`then` are this `if`'s
    /// condition and body; `els` is its optional trailing branch.
    fn print_if(&mut self, cond: &Expr, then: &Block, els: &Option<Box<Stmt>>) {
        self.write_indent();
        self.out.push_str("if (");
        self.out.push_str(&fmt_expr(cond));
        self.out.push_str(") {\n");
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
                        then: t2,
                        els: e2,
                        ..
                    } => {
                        self.write_indent();
                        self.out.push_str("} else if (");
                        self.out.push_str(&fmt_expr(c2));
                        self.out.push_str(") {\n");
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

/// Format a type reference (SPEC §11.1 / §12.1 / §14.1 / §15). A pointer
/// (`TypeExpr.pointer`) prints with a leading `*` — e.g. `*i32` — a slice
/// (`TypeExpr.slice`) with a leading `[]` — e.g. `[]i32` — a fixed-size array
/// (`TypeExpr.array_len = Some(N)`) with a leading `[N]` — e.g. `[3]i32` — an
/// optional type (`TypeExpr.optional`) with a leading `?` — e.g. `?i32` — an
/// error union (`TypeExpr.error_union`) with a leading `!` — e.g. `!i32` — and a
/// plain type as its bare name. The qualifiers are mutually exclusive (v0.115:
/// `?` and `!` are never combined; v0.117: `[N]` is not combined with either;
/// v0.118: `*`/`[]` are not combined with the others), so at most one prefix is
/// emitted. Used wherever a type appears: params, return types, `var`/`const`
/// annotations and struct fields.
fn fmt_type(ty: &TypeExpr) -> String {
    if ty.pointer {
        format!("*{}", ty.name)
    } else if ty.slice {
        format!("[]{}", ty.name)
    } else if let Some(n) = ty.array_len {
        format!("[{}]{}", n, ty.name)
    } else if ty.optional {
        format!("?{}", ty.name)
    } else if ty.error_union {
        format!("!{}", ty.name)
    } else {
        ty.name.clone()
    }
}

// ----- expressions ---------------------------------------------------------

/// Binding-power of an expression, used to insert minimal parentheses. Higher
/// binds tighter. Mirrors the grammar in SPEC §2 / §11.
fn expr_prec(e: &Expr) -> u8 {
    match e {
        // Primaries and postfix forms (calls, struct literals, field access,
        // `null`, and the `.?` unwrap) bind tightest.
        Expr::Int { .. }
        | Expr::Bool { .. }
        | Expr::Ident { .. }
        | Expr::Call { .. }
        | Expr::StructLit { .. }
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
        // `expr.*` (deref) and `base[lo..hi]` (slice) are postfix forms and bind
        // as primaries, like `.?` and `a[i]` (SPEC §15).
        | Expr::Deref { .. }
        | Expr::SliceExpr { .. }
        | Expr::Unwrap { .. } => 8,
        Expr::Comptime { .. } => 7,
        // `try expr` is a prefix form (SPEC §12.1), at the same binding power as
        // the other prefixes (`-`/`!`); `&place` (address-of) is likewise a
        // prefix (SPEC §15.1). v0.115 only ever produces `try` at a statement
        // position, so it is rarely a sub-operand; this keeps the printer total.
        Expr::Unary { .. } | Expr::Try { .. } | Expr::AddrOf { .. } => 6,
        Expr::Binary { op, .. } => match op {
            BinOp::Mul | BinOp::Div | BinOp::Rem => 5,
            BinOp::Add | BinOp::Sub => 4,
            BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => 3,
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
    }
}

/// Format a `while` continue-clause statement (an assignment or expression)
/// inline, with no trailing semicolon — e.g. `i = i + 1`.
fn fmt_cont(s: &Stmt) -> String {
    match s {
        Stmt::Assign { name, value, .. } => format!("{} = {}", name, fmt_expr(value)),
        Stmt::Expr(e) => fmt_expr(e),
        // The parser only produces Assign/Expr in this position.
        _ => String::new(),
    }
}

/// Format an expression with no surrounding parentheses.
fn fmt_expr(e: &Expr) -> String {
    match e {
        Expr::Int { value, .. } => value.to_string(),
        Expr::Bool { value, .. } => if *value { "true" } else { "false" }.to_string(),
        Expr::Ident { name, .. } => name.clone(),
        Expr::Unary { op, expr, .. } => {
            let ops = match op {
                UnOp::Neg => "-",
                UnOp::Not => "!",
            };
            // A unary operand may be a unary/comptime/primary but never a bare
            // binary (grammar: `unary := ("-"|"!") unary | comptime_expr`), so
            // parenthesise binaries (precedence < unary).
            if expr_prec(expr) < 6 {
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
        Expr::Comptime { expr, .. } => {
            // `comptime` binds a single primary; wrap anything that is not a
            // primary (Int/Bool/Ident/Call) in parentheses.
            if expr_prec(expr) >= 8 {
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
        Expr::Field { base, field, .. } => {
            // `base.field` with no spaces. Field access is postfix (binds as a
            // primary), so a base that is not itself primary/postfix is
            // parenthesised. The parser never produces such a base, but this
            // keeps the printer total and idempotent.
            if expr_prec(base) >= 8 {
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
            if expr_prec(receiver) >= 8 {
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
            if expr_prec(expr) >= 8 {
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
            if expr_prec(base) >= 8 {
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
            if expr_prec(place) >= 8 {
                format!("&{}", fmt_expr(place))
            } else {
                format!("&({})", fmt_expr(place))
            }
        }
        // `expr.*` — postfix pointer dereference (SPEC §15.1). Like the `.?`
        // unwrap it binds as a primary, so a non-primary/non-postfix operand
        // (e.g. an `orelse`) is parenthesised to stay total and idempotent.
        Expr::Deref { expr, .. } => {
            if expr_prec(expr) >= 8 {
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
            let b = if expr_prec(base) >= 8 {
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
            if expr_prec(expr) >= 8 {
                format!("try {}", fmt_expr(expr))
            } else {
                format!("try ({})", fmt_expr(expr))
            }
        }
        // `expr catch default` (SPEC §12.1) — like `orelse`, the loosest
        // operator, so its left operand never needs parentheses and only an
        // equal-precedence right operand (another `catch`/`orelse`) does. This
        // yields the left-associative `a catch b catch c` and the explicit
        // `a catch (b catch c)`.
        Expr::Catch { expr, default, .. } => {
            let p = expr_prec(e);
            let l = fmt_operand(expr, p, false);
            let r = fmt_operand(default, p, true);
            format!("{} catch {}", l, r)
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
    use crate::ast::{FieldDecl, FieldInit, Param, TypeExpr};
    use crate::span::Span;

    const D: Span = Span::DUMMY;

    fn ty(name: &str) -> TypeExpr {
        TypeExpr {
            name: name.to_string(),
            optional: false,
            error_union: false,
            array_len: None,
            pointer: false,
            slice: false,
            span: D,
        }
    }

    /// An optional type `?name` (`TypeExpr.optional = true`).
    fn opt_ty(name: &str) -> TypeExpr {
        TypeExpr {
            name: name.to_string(),
            optional: true,
            error_union: false,
            array_len: None,
            pointer: false,
            slice: false,
            span: D,
        }
    }

    /// An error-union type `!name` (`TypeExpr.error_union = true`; v0.115).
    fn err_ty(name: &str) -> TypeExpr {
        TypeExpr {
            name: name.to_string(),
            optional: false,
            error_union: true,
            array_len: None,
            pointer: false,
            slice: false,
            span: D,
        }
    }

    /// A fixed-size array type `[len]name` (`TypeExpr.array_len = Some(len)`;
    /// v0.117).
    fn arr_ty(name: &str, len: i64) -> TypeExpr {
        TypeExpr {
            name: name.to_string(),
            optional: false,
            error_union: false,
            array_len: Some(len),
            pointer: false,
            slice: false,
            span: D,
        }
    }

    /// A pointer type `*name` (`TypeExpr.pointer = true`; v0.118).
    fn ptr_ty(name: &str) -> TypeExpr {
        TypeExpr {
            name: name.to_string(),
            optional: false,
            error_union: false,
            array_len: None,
            pointer: true,
            slice: false,
            span: D,
        }
    }

    /// A slice type `[]name` (`TypeExpr.slice = true`; v0.118).
    fn slice_ty(name: &str) -> TypeExpr {
        TypeExpr {
            name: name.to_string(),
            optional: false,
            error_union: false,
            array_len: None,
            pointer: false,
            slice: true,
            span: D,
        }
    }

    fn error_lit(name: &str) -> Expr {
        Expr::ErrorLit {
            name: name.to_string(),
            span: D,
        }
    }

    fn try_(expr: Expr) -> Expr {
        Expr::Try {
            expr: Box::new(expr),
            span: D,
        }
    }

    fn catch_(expr: Expr, default: Expr) -> Expr {
        Expr::Catch {
            expr: Box::new(expr),
            default: Box::new(default),
            span: D,
        }
    }

    fn call(callee: &str, args: Vec<Expr>) -> Expr {
        Expr::Call {
            callee: callee.to_string(),
            args,
            span: D,
        }
    }

    fn null() -> Expr {
        Expr::Null { span: D }
    }

    fn orelse(lhs: Expr, rhs: Expr) -> Expr {
        Expr::Orelse {
            lhs: Box::new(lhs),
            rhs: Box::new(rhs),
            span: D,
        }
    }

    fn unwrap(expr: Expr) -> Expr {
        Expr::Unwrap {
            expr: Box::new(expr),
            span: D,
        }
    }

    fn ident(name: &str) -> Expr {
        Expr::Ident {
            name: name.to_string(),
            span: D,
        }
    }

    fn int(value: i64) -> Expr {
        Expr::Int { value, span: D }
    }

    fn bin(op: BinOp, lhs: Expr, rhs: Expr) -> Expr {
        Expr::Binary {
            op,
            lhs: Box::new(lhs),
            rhs: Box::new(rhs),
            span: D,
        }
    }

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
                Stmt::Break(D),
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
                            value: bin(BinOp::Add, ident("i"), int(1)),
                            span: D,
                        })),
                        body,
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

    fn arm(labels: Vec<Expr>, body: Vec<Stmt>) -> SwitchArm {
        SwitchArm {
            labels,
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
                variants: vec!["Red".to_string(), "Green".to_string(), "Blue".to_string()],
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
}
