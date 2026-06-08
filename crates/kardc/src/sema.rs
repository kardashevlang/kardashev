//! Semantic analysis: name resolution + type checking + comptime validation.
//!
//! `check` runs a single pass over the module with a stack of lexical scopes.
//! It first collects every top-level function signature (so calls may refer to
//! functions defined later), then folds the top-level constants in source order
//! via [`const_eval`], then type-checks every function and test body. All
//! diagnostics are collected — analysis never stops at the first error.
//!
//! Error codes (SPEC §3):
//! - `E0100` — unknown name (value, callee, or type name).
//! - `E0101` — redefining a builtin (`print` / `expect`).
//! - `E0110` — a type mismatch (the general sema type-error code).
//! - `E0120` — `break` / `continue` outside a loop.
//! - `E0130` / `E0131` / `E0132` — non-constant / unknown-const / type error
//!   in a `comptime` or top-level `const` initializer (raised by `const_eval`).
//! - `E0140` — `expect` called outside a `test` block.

use std::collections::HashMap;

use crate::ast::{BinOp, Block, Expr, Func, Item, Module, Stmt, TestBlock, TypeExpr, UnOp};
use crate::const_eval::{self, ConstVal};
use crate::diag::Diagnostic;
use crate::span::Span;
use crate::types::Type;

/// One-pass semantic check of a whole module.
pub fn check(module: &Module) -> Result<(), Vec<Diagnostic>> {
    let mut cx = Checker::new();
    cx.check_module(module);
    if cx.diags.is_empty() {
        Ok(())
    } else {
        Err(cx.diags)
    }
}

/// A resolved function signature, used to type-check call sites.
#[derive(Clone)]
struct FuncSig {
    params: Vec<Type>,
    ret: Type,
}

/// A lexical binding: its type and whether it is immutable (a `const` or a
/// parameter — only `var` locals may be assigned to).
type Binding = (Type, bool);

struct Checker {
    diags: Vec<Diagnostic>,
    /// Folded values of top-level consts, in source order so far.
    consts: HashMap<String, ConstVal>,
    /// Declared types of top-level consts.
    const_types: HashMap<String, Type>,
    /// All user function signatures (collected up front).
    funcs: HashMap<String, FuncSig>,
    /// Lexical scope stack; the back is the innermost scope.
    scopes: Vec<HashMap<String, Binding>>,
    /// Whether we are currently inside a `test` block (gates `expect`).
    in_test: bool,
    /// Nesting depth of enclosing `while` loops (gates `break`/`continue`).
    loop_depth: usize,
    /// Return type of the function/test currently being checked.
    ret_type: Type,
}

impl Checker {
    fn new() -> Checker {
        Checker {
            diags: Vec::new(),
            consts: HashMap::new(),
            const_types: HashMap::new(),
            funcs: HashMap::new(),
            scopes: Vec::new(),
            in_test: false,
            loop_depth: 0,
            ret_type: Type::Void,
        }
    }

    fn error(&mut self, span: Span, code: &'static str, message: impl Into<String>) {
        self.diags.push(Diagnostic::error(span, code, message));
    }

    // ---- top-level driving ------------------------------------------------

    fn check_module(&mut self, m: &Module) {
        // Pass 1: collect function signatures so calls can forward-reference.
        for item in &m.items {
            if let Item::Func(f) = item {
                if f.name == "print" || f.name == "expect" {
                    self.error(
                        f.span,
                        "E0101",
                        format!("cannot redefine builtin `{}`", f.name),
                    );
                }
                let params = f
                    .params
                    .iter()
                    .map(|p| Type::from_name(&p.ty.name).unwrap_or(Type::I64))
                    .collect();
                let ret = Type::from_name(&f.ret.name).unwrap_or(Type::Void);
                self.funcs.insert(f.name.clone(), FuncSig { params, ret });
            }
        }

        // Pass 2: fold top-level consts in source order.
        for item in &m.items {
            if let Item::Const(c) = item {
                let declared = Type::from_name(&c.ty.name);
                if declared.is_none() {
                    self.error(
                        c.ty.span,
                        "E0100",
                        format!("unknown type `{}`", c.ty.name),
                    );
                }
                match const_eval::eval(&c.value, &self.consts) {
                    Ok(val) => {
                        if let Some(dt) = declared {
                            let ok = match val {
                                ConstVal::Int(_) => dt.is_int(),
                                ConstVal::Bool(_) => dt == Type::Bool,
                            };
                            if !ok {
                                let found = match val {
                                    ConstVal::Int(_) => "integer",
                                    ConstVal::Bool(_) => "bool",
                                };
                                self.error(
                                    c.value.span(),
                                    "E0110",
                                    format!(
                                        "constant initializer type mismatch: expected `{}`, found `{}`",
                                        dt.name(),
                                        found
                                    ),
                                );
                            }
                        }
                        self.consts.insert(c.name.clone(), val);
                        let ty = declared.unwrap_or(match val {
                            ConstVal::Int(_) => Type::I64,
                            ConstVal::Bool(_) => Type::Bool,
                        });
                        self.const_types.insert(c.name.clone(), ty);
                    }
                    Err(d) => {
                        self.diags.push(d);
                        // Record the declared type so later references resolve
                        // by name (its value stays unknown, so a later const
                        // referencing it still reports E0131).
                        if let Some(dt) = declared {
                            self.const_types.insert(c.name.clone(), dt);
                        }
                    }
                }
            }
        }

        // Pass 3: type-check function and test bodies.
        for item in &m.items {
            match item {
                Item::Func(f) => self.check_func(f),
                Item::Test(t) => self.check_test(t),
                Item::Const(_) => {}
            }
        }
    }

    fn check_func(&mut self, f: &Func) {
        self.ret_type = self.resolve_type(&f.ret).unwrap_or(Type::Void);
        self.in_test = false;
        self.loop_depth = 0;
        self.scopes.push(HashMap::new());
        for p in &f.params {
            let pt = self.resolve_type(&p.ty).unwrap_or(Type::I64);
            // Parameters are immutable bindings.
            self.define(&p.name, pt, true);
        }
        self.check_block(&f.body);
        self.scopes.pop();
    }

    fn check_test(&mut self, t: &TestBlock) {
        // A test body behaves like a `void` function for return purposes.
        self.ret_type = Type::Void;
        self.in_test = true;
        self.loop_depth = 0;
        self.check_block(&t.body);
        self.in_test = false;
    }

    // ---- scope helpers ----------------------------------------------------

    fn define(&mut self, name: &str, ty: Type, is_const: bool) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name.to_string(), (ty, is_const));
        }
    }

    /// Resolve a value name to its `(type, is_const)`, searching inner scopes
    /// first, then falling back to top-level consts.
    fn lookup(&self, name: &str) -> Option<Binding> {
        for scope in self.scopes.iter().rev() {
            if let Some(&b) = scope.get(name) {
                return Some(b);
            }
        }
        self.const_types.get(name).map(|&t| (t, true))
    }

    fn resolve_type(&mut self, te: &TypeExpr) -> Option<Type> {
        match Type::from_name(&te.name) {
            Some(t) => Some(t),
            None => {
                self.error(te.span, "E0100", format!("unknown type `{}`", te.name));
                None
            }
        }
    }

    // ---- statements -------------------------------------------------------

    fn check_block(&mut self, block: &Block) {
        self.scopes.push(HashMap::new());
        for stmt in &block.stmts {
            self.check_stmt(stmt);
        }
        self.scopes.pop();
    }

    fn check_stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::Let {
                is_const,
                name,
                ty,
                value,
                ..
            } => {
                let declared = self.resolve_type(ty);
                let vt = self.check_expr(value, declared);
                if let (Some(dt), Some(vt)) = (declared, vt) {
                    if dt != vt {
                        self.error(
                            value.span(),
                            "E0110",
                            format!(
                                "initializer type mismatch: expected `{}`, found `{}`",
                                dt.name(),
                                vt.name()
                            ),
                        );
                    }
                }
                let bind_ty = declared.unwrap_or(Type::I64);
                self.define(name, bind_ty, *is_const);
            }
            Stmt::Assign { name, value, span } => match self.lookup(name) {
                Some((ty, is_const)) => {
                    if is_const {
                        self.error(
                            *span,
                            "E0110",
                            format!("cannot assign to immutable binding `{}`", name),
                        );
                        self.check_expr(value, Some(ty));
                    } else {
                        let vt = self.check_expr(value, Some(ty));
                        if let Some(vt) = vt {
                            if vt != ty {
                                self.error(
                                    value.span(),
                                    "E0110",
                                    format!(
                                        "cannot assign value of type `{}` to `{}` of type `{}`",
                                        vt.name(),
                                        name,
                                        ty.name()
                                    ),
                                );
                            }
                        }
                    }
                }
                None => {
                    self.error(*span, "E0100", format!("unknown name `{}`", name));
                    self.check_expr(value, None);
                }
            },
            Stmt::Expr(e) => {
                self.check_expr(e, None);
            }
            Stmt::Return { value, span } => match value {
                Some(e) => {
                    if self.ret_type == Type::Void {
                        self.error(
                            *span,
                            "E0110",
                            "cannot return a value from a `void` function",
                        );
                        self.check_expr(e, None);
                    } else {
                        let expected = self.ret_type;
                        let vt = self.check_expr(e, Some(expected));
                        if let Some(vt) = vt {
                            if vt != expected {
                                self.error(
                                    e.span(),
                                    "E0110",
                                    format!(
                                        "return type mismatch: expected `{}`, found `{}`",
                                        expected.name(),
                                        vt.name()
                                    ),
                                );
                            }
                        }
                    }
                }
                None => {
                    if self.ret_type != Type::Void {
                        self.error(
                            *span,
                            "E0110",
                            format!(
                                "`return;` is only valid in a `void` function, found return type `{}`",
                                self.ret_type.name()
                            ),
                        );
                    }
                }
            },
            Stmt::If {
                cond, then, els, ..
            } => {
                self.check_condition(cond, "if");
                self.check_block(then);
                if let Some(els) = els {
                    self.check_stmt(els);
                }
            }
            Stmt::While {
                cond, cont, body, ..
            } => {
                self.check_condition(cond, "while");
                // The continue-clause statement runs in the loop's outer scope.
                if let Some(c) = cont {
                    self.check_stmt(c);
                }
                self.loop_depth += 1;
                self.check_block(body);
                self.loop_depth -= 1;
            }
            Stmt::Break(span) => {
                if self.loop_depth == 0 {
                    self.error(*span, "E0120", "`break` is only valid inside a loop");
                }
            }
            Stmt::Continue(span) => {
                if self.loop_depth == 0 {
                    self.error(*span, "E0120", "`continue` is only valid inside a loop");
                }
            }
            Stmt::Defer { stmt, .. } => {
                self.check_stmt(stmt);
            }
            Stmt::Block(b) => {
                self.check_block(b);
            }
        }
    }

    fn check_condition(&mut self, cond: &Expr, kw: &str) {
        if let Some(t) = self.check_expr(cond, Some(Type::Bool)) {
            if t != Type::Bool {
                self.error(
                    cond.span(),
                    "E0110",
                    format!("`{}` condition must be `bool`, found `{}`", kw, t.name()),
                );
            }
        }
    }

    // ---- expressions ------------------------------------------------------

    /// Type-check `expr`. `expected` carries a contextual type hint used for
    /// integer-literal polymorphism. Returns the inferred type, or `None` if a
    /// diagnostic was emitted (used to avoid cascading errors).
    fn check_expr(&mut self, expr: &Expr, expected: Option<Type>) -> Option<Type> {
        match expr {
            Expr::Int { .. } => Some(match expected {
                Some(t) if t.is_int() => t,
                _ => Type::I64,
            }),
            Expr::Bool { .. } => Some(Type::Bool),
            Expr::Ident { name, span } => match self.lookup(name) {
                Some((t, _)) => Some(t),
                None => {
                    self.error(*span, "E0100", format!("unknown name `{}`", name));
                    None
                }
            },
            Expr::Unary { op, expr: inner, span } => self.check_unary(*op, inner, *span, expected),
            Expr::Binary { op, lhs, rhs, span } => {
                self.check_binary(*op, lhs, rhs, *span, expected)
            }
            Expr::Call { callee, args, span } => self.check_call(callee, args, *span),
            Expr::Comptime { expr: inner, .. } => {
                // A `comptime` expression must be const-evaluable over the
                // top-level consts. Its type follows the folded value (with
                // integer-literal polymorphism applied to int results).
                match const_eval::eval(inner, &self.consts) {
                    Ok(ConstVal::Int(_)) => Some(match expected {
                        Some(t) if t.is_int() => t,
                        _ => Type::I64,
                    }),
                    Ok(ConstVal::Bool(_)) => Some(Type::Bool),
                    Err(d) => {
                        self.diags.push(d);
                        None
                    }
                }
            }
        }
    }

    fn check_unary(
        &mut self,
        op: UnOp,
        inner: &Expr,
        span: Span,
        expected: Option<Type>,
    ) -> Option<Type> {
        match op {
            UnOp::Neg => {
                let t = self.check_expr(inner, expected)?;
                if t.is_int() && t.is_signed() {
                    Some(t)
                } else {
                    self.error(
                        span,
                        "E0110",
                        format!("unary `-` requires a signed integer, found `{}`", t.name()),
                    );
                    None
                }
            }
            UnOp::Not => {
                let t = self.check_expr(inner, Some(Type::Bool))?;
                if t == Type::Bool {
                    Some(Type::Bool)
                } else {
                    self.error(
                        span,
                        "E0110",
                        format!("unary `!` requires a `bool`, found `{}`", t.name()),
                    );
                    None
                }
            }
        }
    }

    fn check_binary(
        &mut self,
        op: BinOp,
        lhs: &Expr,
        rhs: &Expr,
        span: Span,
        expected: Option<Type>,
    ) -> Option<Type> {
        match op {
            BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Rem => {
                let (lt, rt) = self.check_int_operands(lhs, rhs, expected.filter(|t| t.is_int()));
                let lt = lt?;
                let rt = rt?;
                if !lt.is_int() {
                    self.error(
                        lhs.span(),
                        "E0110",
                        format!("arithmetic operand must be an integer, found `{}`", lt.name()),
                    );
                    return None;
                }
                if !rt.is_int() {
                    self.error(
                        rhs.span(),
                        "E0110",
                        format!("arithmetic operand must be an integer, found `{}`", rt.name()),
                    );
                    return None;
                }
                if lt != rt {
                    self.error(
                        span,
                        "E0110",
                        format!(
                            "arithmetic operands must have the same type, found `{}` and `{}`",
                            lt.name(),
                            rt.name()
                        ),
                    );
                    return None;
                }
                Some(lt)
            }
            BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => {
                // Operands may be int or bool, but must be the same type.
                let (lt, rt) = self.check_int_operands(lhs, rhs, None);
                let lt = lt?;
                let rt = rt?;
                if lt != rt {
                    self.error(
                        span,
                        "E0110",
                        format!(
                            "comparison operands must have the same type, found `{}` and `{}`",
                            lt.name(),
                            rt.name()
                        ),
                    );
                    return None;
                }
                Some(Type::Bool)
            }
            BinOp::And | BinOp::Or => {
                let lt = self.check_expr(lhs, Some(Type::Bool));
                let rt = self.check_expr(rhs, Some(Type::Bool));
                let lt = lt?;
                let rt = rt?;
                let mut ok = true;
                if lt != Type::Bool {
                    self.error(
                        lhs.span(),
                        "E0110",
                        format!("`{}` requires `bool` operands, found `{}`", op.c_op(), lt.name()),
                    );
                    ok = false;
                }
                if rt != Type::Bool {
                    self.error(
                        rhs.span(),
                        "E0110",
                        format!("`{}` requires `bool` operands, found `{}`", op.c_op(), rt.name()),
                    );
                    ok = false;
                }
                if ok {
                    Some(Type::Bool)
                } else {
                    None
                }
            }
        }
    }

    /// Check two operands that should share a type, applying integer-literal
    /// polymorphism: a flexible literal adopts `expected` if given, otherwise
    /// the concrete type of the other operand, otherwise `i64`.
    fn check_int_operands(
        &mut self,
        lhs: &Expr,
        rhs: &Expr,
        expected: Option<Type>,
    ) -> (Option<Type>, Option<Type>) {
        if let Some(t) = expected {
            let lt = self.check_expr(lhs, Some(t));
            let rt = self.check_expr(rhs, Some(t));
            (lt, rt)
        } else if !is_flex_int_literal(lhs) {
            // Anchor on the concrete left operand.
            let lt = self.check_expr(lhs, None);
            let rt = self.check_expr(rhs, lt.filter(|t| t.is_int()));
            (lt, rt)
        } else if !is_flex_int_literal(rhs) {
            // Anchor on the concrete right operand.
            let rt = self.check_expr(rhs, None);
            let lt = self.check_expr(lhs, rt.filter(|t| t.is_int()));
            (lt, rt)
        } else {
            // Both operands are flexible integer literals: default to i64.
            let lt = self.check_expr(lhs, Some(Type::I64));
            let rt = self.check_expr(rhs, Some(Type::I64));
            (lt, rt)
        }
    }

    fn check_call(&mut self, callee: &str, args: &[Expr], span: Span) -> Option<Type> {
        match callee {
            "print" => {
                if args.len() != 1 {
                    self.error(
                        span,
                        "E0110",
                        format!("`print` takes exactly 1 argument, found {}", args.len()),
                    );
                    for a in args {
                        self.check_expr(a, None);
                    }
                    return Some(Type::Void);
                }
                if let Some(t) = self.check_expr(&args[0], None) {
                    if !t.is_int() {
                        self.error(
                            args[0].span(),
                            "E0110",
                            format!("`print` requires an integer argument, found `{}`", t.name()),
                        );
                    }
                }
                Some(Type::Void)
            }
            "expect" => {
                if !self.in_test {
                    self.error(
                        span,
                        "E0140",
                        "`expect` may only be called inside a `test` block",
                    );
                }
                if args.len() != 1 {
                    self.error(
                        span,
                        "E0110",
                        format!("`expect` takes exactly 1 argument, found {}", args.len()),
                    );
                    for a in args {
                        self.check_expr(a, Some(Type::Bool));
                    }
                    return Some(Type::Void);
                }
                if let Some(t) = self.check_expr(&args[0], Some(Type::Bool)) {
                    if t != Type::Bool {
                        self.error(
                            args[0].span(),
                            "E0110",
                            format!("`expect` requires a `bool` argument, found `{}`", t.name()),
                        );
                    }
                }
                Some(Type::Void)
            }
            _ => {
                if let Some(sig) = self.funcs.get(callee).cloned() {
                    if args.len() != sig.params.len() {
                        self.error(
                            span,
                            "E0110",
                            format!(
                                "`{}` takes {} argument(s), found {}",
                                callee,
                                sig.params.len(),
                                args.len()
                            ),
                        );
                        for a in args {
                            self.check_expr(a, None);
                        }
                        return Some(sig.ret);
                    }
                    for (a, &pt) in args.iter().zip(sig.params.iter()) {
                        if let Some(at) = self.check_expr(a, Some(pt)) {
                            if at != pt {
                                self.error(
                                    a.span(),
                                    "E0110",
                                    format!(
                                        "argument type mismatch: expected `{}`, found `{}`",
                                        pt.name(),
                                        at.name()
                                    ),
                                );
                            }
                        }
                    }
                    Some(sig.ret)
                } else {
                    self.error(span, "E0100", format!("unknown function `{}`", callee));
                    for a in args {
                        self.check_expr(a, None);
                    }
                    None
                }
            }
        }
    }
}

/// A "flexible" integer literal whose type is determined solely by context: a
/// bare integer literal, or unary negation of one.
fn is_flex_int_literal(e: &Expr) -> bool {
    match e {
        Expr::Int { .. } => true,
        Expr::Unary {
            op: UnOp::Neg,
            expr,
            ..
        } => is_flex_int_literal(expr),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{ConstDecl, Func, Param, TestBlock};

    fn sp() -> Span {
        Span::DUMMY
    }
    fn te(name: &str) -> TypeExpr {
        TypeExpr {
            name: name.into(),
            span: sp(),
        }
    }
    fn int(v: i64) -> Expr {
        Expr::Int { value: v, span: sp() }
    }
    fn boolean(v: bool) -> Expr {
        Expr::Bool { value: v, span: sp() }
    }
    fn ident(n: &str) -> Expr {
        Expr::Ident {
            name: n.into(),
            span: sp(),
        }
    }
    fn call(c: &str, args: Vec<Expr>) -> Expr {
        Expr::Call {
            callee: c.into(),
            args,
            span: sp(),
        }
    }
    fn bin(op: BinOp, l: Expr, r: Expr) -> Expr {
        Expr::Binary {
            op,
            lhs: Box::new(l),
            rhs: Box::new(r),
            span: sp(),
        }
    }
    fn block(stmts: Vec<Stmt>) -> Block {
        Block { stmts, span: sp() }
    }
    fn param(name: &str, ty: &str) -> Param {
        Param {
            name: name.into(),
            ty: te(ty),
            span: sp(),
        }
    }
    fn func(name: &str, params: Vec<Param>, ret: &str, body: Vec<Stmt>) -> Item {
        Item::Func(Func {
            is_pub: false,
            name: name.into(),
            params,
            ret: te(ret),
            body: block(body),
            span: sp(),
        })
    }
    fn const_item(name: &str, ty: &str, value: Expr) -> Item {
        Item::Const(ConstDecl {
            is_pub: false,
            name: name.into(),
            ty: te(ty),
            value,
            span: sp(),
        })
    }
    fn test_block(name: &str, body: Vec<Stmt>) -> Item {
        Item::Test(TestBlock {
            name: name.into(),
            body: block(body),
            span: sp(),
        })
    }
    fn let_var(name: &str, ty: &str, value: Expr) -> Stmt {
        Stmt::Let {
            is_const: false,
            name: name.into(),
            ty: te(ty),
            value,
            span: sp(),
        }
    }
    fn let_const(name: &str, ty: &str, value: Expr) -> Stmt {
        Stmt::Let {
            is_const: true,
            name: name.into(),
            ty: te(ty),
            value,
            span: sp(),
        }
    }
    fn assign(name: &str, value: Expr) -> Stmt {
        Stmt::Assign {
            name: name.into(),
            value,
            span: sp(),
        }
    }
    fn ret(value: Option<Expr>) -> Stmt {
        Stmt::Return { value, span: sp() }
    }

    fn codes(items: Vec<Item>) -> Vec<&'static str> {
        let m = Module { items };
        match check(&m) {
            Ok(()) => vec![],
            Err(ds) => ds.iter().map(|d| d.code).collect(),
        }
    }

    #[test]
    fn good_program_passes() {
        // fn add(a: i32, b: i32) i32 { return a + b; }
        // const MAX: i32 = 10 + 5;
        // fn main() void { var x: i32 = add(1, 2); print(x); }
        // test "eq" { expect(1 == 1); }
        let items = vec![
            func(
                "add",
                vec![param("a", "i32"), param("b", "i32")],
                "i32",
                vec![ret(Some(bin(BinOp::Add, ident("a"), ident("b"))))],
            ),
            const_item("MAX", "i32", bin(BinOp::Add, int(10), int(5))),
            func(
                "main",
                vec![],
                "void",
                vec![
                    let_var("x", "i32", call("add", vec![int(1), int(2)])),
                    Stmt::Expr(call("print", vec![ident("x")])),
                ],
            ),
            test_block(
                "eq",
                vec![Stmt::Expr(call("expect", vec![bin(BinOp::Eq, int(1), int(1))]))],
            ),
        ];
        assert_eq!(codes(items), Vec::<&str>::new());
    }

    #[test]
    fn unknown_name_is_e0100() {
        let items = vec![func(
            "main",
            vec![],
            "void",
            vec![Stmt::Expr(call("print", vec![ident("y")]))],
        )];
        assert!(codes(items).contains(&"E0100"));
    }

    #[test]
    fn type_mismatch_is_e0110() {
        // var x: bool = 1;
        let items = vec![func(
            "main",
            vec![],
            "void",
            vec![let_var("x", "bool", int(1))],
        )];
        assert!(codes(items).contains(&"E0110"));
    }

    #[test]
    fn expect_outside_test_is_e0140() {
        let items = vec![func(
            "main",
            vec![],
            "void",
            vec![Stmt::Expr(call("expect", vec![boolean(true)]))],
        )];
        assert!(codes(items).contains(&"E0140"));
    }

    #[test]
    fn break_outside_loop_is_e0120() {
        let items = vec![func(
            "main",
            vec![],
            "void",
            vec![Stmt::Break(sp())],
        )];
        assert!(codes(items).contains(&"E0120"));
    }

    #[test]
    fn assign_to_const_is_e0110() {
        // fn main() void { const c: i32 = 5; c = 6; }
        let items = vec![func(
            "main",
            vec![],
            "void",
            vec![let_const("c", "i32", int(5)), assign("c", int(6))],
        )];
        assert!(codes(items).contains(&"E0110"));
    }

    #[test]
    fn non_constant_top_level_const_is_e0130() {
        // const X: i32 = bar();
        let items = vec![const_item("X", "i32", call("bar", vec![]))];
        assert!(codes(items).contains(&"E0130"));
    }

    #[test]
    fn const_referencing_later_const_is_e0131() {
        // const A: i32 = B;  const B: i32 = 1;
        let items = vec![
            const_item("A", "i32", ident("B")),
            const_item("B", "i32", int(1)),
        ];
        assert!(codes(items).contains(&"E0131"));
    }

    #[test]
    fn redefining_builtin_is_e0101() {
        let items = vec![func("print", vec![param("x", "i32")], "void", vec![])];
        assert!(codes(items).contains(&"E0101"));
    }

    #[test]
    fn continue_outside_loop_is_e0120() {
        let items = vec![func(
            "main",
            vec![],
            "void",
            vec![Stmt::Continue(sp())],
        )];
        assert!(codes(items).contains(&"E0120"));
    }

    #[test]
    fn break_inside_while_is_ok() {
        // fn main() void { while (true) { break; } }
        let items = vec![func(
            "main",
            vec![],
            "void",
            vec![Stmt::While {
                cond: boolean(true),
                cont: None,
                body: block(vec![Stmt::Break(sp())]),
                span: sp(),
            }],
        )];
        assert_eq!(codes(items), Vec::<&str>::new());
    }

    #[test]
    fn return_type_mismatch_is_e0110() {
        // fn f() i32 { return true; }
        let items = vec![func("f", vec![], "i32", vec![ret(Some(boolean(true)))])];
        assert!(codes(items).contains(&"E0110"));
    }

    #[test]
    fn int_literal_adopts_param_type() {
        // fn f(x: u8) void {}  fn main() void { f(7); }
        let items = vec![
            func("f", vec![param("x", "u8")], "void", vec![]),
            func(
                "main",
                vec![],
                "void",
                vec![Stmt::Expr(call("f", vec![int(7)]))],
            ),
        ];
        assert_eq!(codes(items), Vec::<&str>::new());
    }

    #[test]
    fn unknown_callee_is_e0100() {
        let items = vec![func(
            "main",
            vec![],
            "void",
            vec![Stmt::Expr(call("nope", vec![]))],
        )];
        assert!(codes(items).contains(&"E0100"));
    }
}
