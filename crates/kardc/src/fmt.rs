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
//!   arrow (Zig style).
//! - Spaces around every binary operator (`a + b`, `a and b`).
//! - `if (cond) { … } else if (cond) { … } else { … }`.
//! - `while (cond) { … }` / `while (cond) : (cont) { … }`.
//! - `const NAME: T = expr;` / `var name: T = expr;` / `return expr;`.
//! - `defer <stmt>`; `test "name" { … }`.
//! - `const Name = struct { f: T, … };` — one field per line, 4-space indent,
//!   trailing comma on each; an empty struct prints `const Name = struct {};`.
//!   Struct literals print `Name{ .f = e, … }` and field access `base.field`
//!   (SPEC §9).
//!
//! ## Idempotence
//!
//! Parenthesisation is precedence-driven and minimal, so re-formatting the
//! canonical output produces byte-identical text.

use crate::ast::{
    BinOp, Block, ConstDecl, Expr, Func, Item, Module, Stmt, StructDecl, TestBlock, TypeExpr, UnOp,
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
        self.out.push_str(": ");
        self.out.push_str(&fmt_type(&c.ty));
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
                self.out.push_str(": ");
                self.out.push_str(&fmt_type(ty));
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
        }
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

/// Format a type reference (SPEC §11.1). An optional type (`TypeExpr.optional`)
/// prints with a leading `?` — e.g. `?i32` — and a plain type prints as its bare
/// name. Used wherever a type appears: params, return types, `var`/`const`
/// annotations and struct fields.
fn fmt_type(ty: &TypeExpr) -> String {
    if ty.optional {
        format!("?{}", ty.name)
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
        | Expr::Unwrap { .. } => 8,
        Expr::Comptime { .. } => 7,
        Expr::Unary { .. } => 6,
        Expr::Binary { op, .. } => match op {
            BinOp::Mul | BinOp::Div | BinOp::Rem => 5,
            BinOp::Add | BinOp::Sub => 4,
            BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => 3,
            BinOp::And => 2,
            BinOp::Or => 1,
        },
        // `orelse` is the loosest operator: its right-hand fallback is an
        // ordinary `T` expression, so `head orelse a + b` reads as
        // `head orelse (a + b)` with no parentheses.
        Expr::Orelse { .. } => 0,
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
            span: D,
        }
    }

    /// An optional type `?name` (`TypeExpr.optional = true`).
    fn opt_ty(name: &str) -> TypeExpr {
        TypeExpr {
            name: name.to_string(),
            optional: true,
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
                        span: D,
                    },
                    Param {
                        name: "b".to_string(),
                        ty: ty("i32"),
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
                    ty: ty("i32"),
                    value: int(10),
                    span: D,
                }),
                Item::Const(ConstDecl {
                    is_pub: false,
                    name: "MIN".to_string(),
                    ty: ty("i32"),
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
                            ty: ty("i64"),
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
                                ty: ty("Point"),
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
            ty: opt_ty("i32"),
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
                span: D,
            }],
            ret: opt_ty("i32"),
            body: Block {
                stmts: vec![Stmt::Let {
                    is_const: false,
                    name: "y".to_string(),
                    ty: opt_ty("i32"),
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
                    span: D,
                }],
                ret: ty("i32"),
                body: Block {
                    stmts: vec![
                        Stmt::Let {
                            is_const: false,
                            name: "y".to_string(),
                            ty: opt_ty("i32"),
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
}
