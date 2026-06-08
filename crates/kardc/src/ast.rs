//! The abstract syntax tree.
//!
//! This is the central contract between the compiler stages: the parser
//! produces it, semantic analysis validates it, and the C backend / formatter
//! consume it. Keep it stable — every other module depends on these shapes.

use crate::span::Span;

/// A whole source file.
#[derive(Clone, Debug)]
pub struct Module {
    pub items: Vec<Item>,
}

/// A top-level item.
#[derive(Clone, Debug)]
pub enum Item {
    Func(Func),
    Const(ConstDecl),
    Test(TestBlock),
    Struct(StructDecl),
    Enum(EnumDecl),
}

/// An enum declaration: `pub? const Name = enum { A, B, C };` (v0.116).
/// Plain (C-like) enums; tagged-union payloads are a later roadmap item.
#[derive(Clone, Debug)]
pub struct EnumDecl {
    pub is_pub: bool,
    pub name: String,
    pub variants: Vec<String>,
    pub span: Span,
}

/// A struct declaration: `pub? const Name = struct { f: T, ... };` (v0.112).
/// Data only — methods / associated functions are a later roadmap version.
#[derive(Clone, Debug)]
pub struct StructDecl {
    pub is_pub: bool,
    pub name: String,
    pub fields: Vec<FieldDecl>,
    /// Methods and associated functions declared in the struct body (v0.113).
    /// A function whose first parameter is named `self` is a method (callable
    /// `instance.m(..)`); otherwise it is an associated function (`Name.f(..)`).
    pub methods: Vec<Func>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct FieldDecl {
    pub name: String,
    pub ty: TypeExpr,
    pub span: Span,
}

/// A function definition: `pub fn name(params) RetType { body }`.
///
/// Note the Zig-style return type: it follows the parameter list directly,
/// with no `->` arrow.
#[derive(Clone, Debug)]
pub struct Func {
    pub is_pub: bool,
    pub name: String,
    pub params: Vec<Param>,
    pub ret: TypeExpr,
    pub body: Block,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct Param {
    pub name: String,
    pub ty: TypeExpr,
    pub span: Span,
}

/// A `const NAME: T = <comptime-expr>;` declaration. Valid at the top level
/// and (without `pub`) inside a function body.
#[derive(Clone, Debug)]
pub struct ConstDecl {
    pub is_pub: bool,
    pub name: String,
    pub ty: TypeExpr,
    pub value: Expr,
    pub span: Span,
}

/// A `test "name" { ... }` block — a first-class testing construct.
#[derive(Clone, Debug)]
pub struct TestBlock {
    pub name: String,
    pub body: Block,
    pub span: Span,
}

/// A named type reference, e.g. `i32`. Resolved to a [`crate::types::Type`]
/// during semantic analysis.
#[derive(Clone, Debug)]
pub struct TypeExpr {
    pub name: String,
    /// True if written as `?T` (an optional). v0.114: no nesting (`??T`).
    pub optional: bool,
    /// True if written as `!T` (an error union). v0.115: implicit global error
    /// set; not combined with `optional`.
    pub error_union: bool,
    pub span: Span,
}

/// A brace-delimited sequence of statements that introduces a new scope.
#[derive(Clone, Debug)]
pub struct Block {
    pub stmts: Vec<Stmt>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub enum Stmt {
    /// `var name: T = expr;` (is_const = false) or `const name: T = expr;`.
    Let {
        is_const: bool,
        name: String,
        ty: TypeExpr,
        value: Expr,
        span: Span,
    },
    /// `name = expr;`
    Assign {
        name: String,
        value: Expr,
        span: Span,
    },
    /// `place = expr;` where `place` is a field-access chain (`a.b.c`).
    /// Simple `name = expr;` uses [`Stmt::Assign`] instead.
    FieldAssign {
        place: Expr,
        value: Expr,
        span: Span,
    },
    /// An expression evaluated for its effect, e.g. `print(x);`.
    Expr(Expr),
    /// `return expr;` or `return;`
    Return {
        value: Option<Expr>,
        span: Span,
    },
    /// `if (cond) then [else els]`. `els` is another statement so that
    /// `else if` chains and `else { ... }` blocks are both representable.
    If {
        cond: Expr,
        then: Block,
        els: Option<Box<Stmt>>,
        span: Span,
    },
    /// `while (cond) { body }` or `while (cond) : (cont) { body }`.
    ///
    /// The continue-clause `cont` is a *statement* — typically an assignment
    /// like `i = i + 1` — because assignment is a statement, not an expression,
    /// in this language. The parser restricts it to an assignment or an
    /// expression statement.
    While {
        cond: Expr,
        cont: Option<Box<Stmt>>,
        body: Block,
        span: Span,
    },
    Break(Span),
    Continue(Span),
    /// `defer stmt;` — runs `stmt` at scope exit, in LIFO order.
    Defer {
        stmt: Box<Stmt>,
        span: Span,
    },
    /// A bare nested block `{ ... }`.
    Block(Block),
    /// `switch (scrutinee) { labels => body, ..., else => body }`.
    Switch {
        scrutinee: Expr,
        arms: Vec<SwitchArm>,
        default: Option<Block>,
        span: Span,
    },
}

/// One `labels => body` arm of a `switch`. `labels` are constant patterns
/// (enum literals `.V` / `Enum.V`, or integer literals); multiple labels share
/// one body (`.A, .B => …`).
#[derive(Clone, Debug)]
pub struct SwitchArm {
    pub labels: Vec<Expr>,
    pub body: Block,
    pub span: Span,
}

impl Stmt {
    pub fn span(&self) -> Span {
        match self {
            Stmt::Let { span, .. } => *span,
            Stmt::Assign { span, .. } => *span,
            Stmt::FieldAssign { span, .. } => *span,
            Stmt::Expr(e) => e.span(),
            Stmt::Return { span, .. } => *span,
            Stmt::If { span, .. } => *span,
            Stmt::While { span, .. } => *span,
            Stmt::Break(s) => *s,
            Stmt::Continue(s) => *s,
            Stmt::Defer { span, .. } => *span,
            Stmt::Block(b) => b.span,
            Stmt::Switch { span, .. } => *span,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnOp {
    Neg, // -x
    Not, // !x
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
}

impl BinOp {
    /// True for comparison/logical operators (which yield `bool`).
    pub fn is_bool_result(self) -> bool {
        matches!(
            self,
            BinOp::Eq
                | BinOp::Ne
                | BinOp::Lt
                | BinOp::Le
                | BinOp::Gt
                | BinOp::Ge
                | BinOp::And
                | BinOp::Or
        )
    }

    /// The C operator spelling.
    pub fn c_op(self) -> &'static str {
        match self {
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
            BinOp::And => "&&",
            BinOp::Or => "||",
        }
    }
}

#[derive(Clone, Debug)]
pub enum Expr {
    Int {
        value: i64,
        span: Span,
    },
    Bool {
        value: bool,
        span: Span,
    },
    Ident {
        name: String,
        span: Span,
    },
    Unary {
        op: UnOp,
        expr: Box<Expr>,
        span: Span,
    },
    Binary {
        op: BinOp,
        lhs: Box<Expr>,
        rhs: Box<Expr>,
        span: Span,
    },
    Call {
        callee: String,
        args: Vec<Expr>,
        span: Span,
    },
    /// `comptime <expr>` — must be evaluable at compile time.
    Comptime {
        expr: Box<Expr>,
        span: Span,
    },
    /// A struct literal: `Name{ .f1 = e1, .f2 = e2 }`.
    StructLit {
        name: String,
        fields: Vec<FieldInit>,
        span: Span,
    },
    /// Field access: `base.field`.
    Field {
        base: Box<Expr>,
        field: String,
        span: Span,
    },
    /// A method / associated-function call: `receiver.method(args)`.
    /// `receiver` is either a struct value (method; `self` is prepended) or an
    /// `Ident` naming a struct type (associated call). Resolved in sema.
    MethodCall {
        receiver: Box<Expr>,
        method: String,
        args: Vec<Expr>,
        span: Span,
    },
    /// The `null` literal (the empty optional). Its `?T` type comes from context.
    Null { span: Span },
    /// `lhs orelse rhs` — unwrap the optional `lhs`, or evaluate `rhs` if null.
    Orelse {
        lhs: Box<Expr>,
        rhs: Box<Expr>,
        span: Span,
    },
    /// `expr.?` — force-unwrap an optional; panics (exit 101) if null.
    Unwrap { expr: Box<Expr>, span: Span },
    /// `error.Name` — an error value from the (implicit global) error set.
    ErrorLit { name: String, span: Span },
    /// `.Variant` — an unqualified enum literal; its enum type comes from
    /// context. (The qualified form `Enum.Variant` reuses [`Expr::Field`].)
    EnumLit { variant: String, span: Span },
    /// `try expr` — unwrap an error union `!T`, or propagate the error by
    /// returning it from the enclosing `!U` function. v0.115: statement-level.
    Try { expr: Box<Expr>, span: Span },
    /// `expr catch default` — unwrap `!T`, or evaluate `default` (a `T`) on error.
    Catch {
        expr: Box<Expr>,
        default: Box<Expr>,
        span: Span,
    },
}

/// One `.name = value` initializer inside a struct literal.
#[derive(Clone, Debug)]
pub struct FieldInit {
    pub name: String,
    pub value: Expr,
    pub span: Span,
}

impl Expr {
    pub fn span(&self) -> Span {
        match self {
            Expr::Int { span, .. } => *span,
            Expr::Bool { span, .. } => *span,
            Expr::Ident { span, .. } => *span,
            Expr::Unary { span, .. } => *span,
            Expr::Binary { span, .. } => *span,
            Expr::Call { span, .. } => *span,
            Expr::Comptime { span, .. } => *span,
            Expr::StructLit { span, .. } => *span,
            Expr::Field { span, .. } => *span,
            Expr::MethodCall { span, .. } => *span,
            Expr::Null { span } => *span,
            Expr::Orelse { span, .. } => *span,
            Expr::Unwrap { span, .. } => *span,
            Expr::ErrorLit { span, .. } => *span,
            Expr::Try { span, .. } => *span,
            Expr::Catch { span, .. } => *span,
            Expr::EnumLit { span, .. } => *span,
        }
    }
}
