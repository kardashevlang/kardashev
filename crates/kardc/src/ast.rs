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
    Union(UnionDecl),
    /// `@import("path.ks");` — a top-level import (v0.126). Resolved and erased
    /// by the module flattener before sema/emit; a residual one is an error.
    Import(ImportDecl),
    /// `pub? const Name = error{ A, B, … };` — a named error set (v0.139).
    ErrorSet(ErrorSetDecl),
}

/// A named error set `const Name = error{ A, B };` (v0.139).
#[derive(Clone, Debug)]
pub struct ErrorSetDecl {
    pub is_pub: bool,
    pub name: String,
    pub members: Vec<String>,
    pub span: Span,
}

/// A `@import("path");` declaration (v0.126).
#[derive(Clone, Debug)]
pub struct ImportDecl {
    pub path: String,
    pub span: Span,
}

/// A tagged union: `pub? const Name = union(enum) { v: T, ... };` (v0.124).
/// Each variant carries a payload type. Lowered to a tagged C struct.
#[derive(Clone, Debug)]
pub struct UnionDecl {
    pub is_pub: bool,
    pub name: String,
    pub variants: Vec<UnionVariant>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct UnionVariant {
    pub name: String,
    pub payload: TypeExpr,
    pub span: Span,
}

/// An enum declaration: `pub? const Name = enum { A, B, C };` (v0.116).
/// Plain (C-like) enums; tagged-union payloads are a later roadmap item.
#[derive(Clone, Debug)]
pub struct EnumDecl {
    pub is_pub: bool,
    pub name: String,
    pub variants: Vec<EnumVariant>,
    pub span: Span,
}

/// One enum variant, optionally with an explicit integer value `A = 1` (v0.143).
/// A `None` value auto-increments from the previous (C rules: first is 0).
#[derive(Clone, Debug)]
pub struct EnumVariant {
    pub name: String,
    pub value: Option<i64>,
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
    /// True for a `comptime IDENT: type` parameter (a compile-time type
    /// parameter, v0.120). A function with any such parameter is generic and is
    /// monomorphised per concrete type argument.
    pub is_comptime: bool,
    pub span: Span,
}

/// A `const NAME: T = <comptime-expr>;` declaration. Valid at the top level
/// and (without `pub`) inside a function body.
#[derive(Clone, Debug)]
pub struct ConstDecl {
    pub is_pub: bool,
    pub name: String,
    /// Optional type annotation (v0.121); `None` infers from `value`.
    pub ty: Option<TypeExpr>,
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
    /// `Some(name)` if written as `Set!T` (an error union over the *named* error
    /// set `Set`, v0.139); `None` for the implicit global `!T`. Only meaningful
    /// when `error_union` is true.
    pub error_set: Option<String>,
    /// `Some(..)` if written as `[N]T` (a fixed-size array); `name` is then the
    /// element type. The size is a literal (`[3]T`, v0.117) or a comptime
    /// value-parameter name (`[n]T`, v0.128). Not combined with `?`/`!`.
    pub array_len: Option<ArraySize>,
    /// True if written as `*T` (a single pointer to `T`). v0.118.
    pub pointer: bool,
    /// True if written as `[]T` (a slice of `T`). v0.118.
    pub slice: bool,
    /// `Some(args)` if written as `Name(A, B, …)` — a **generic type-constructor
    /// application** directly in type position (v0.152, SPEC §31.4); `name` is
    /// then the constructor. Each argument is itself a base type reference: a
    /// bare name or a nested application (no `?`/`!`/`*`/`[]`/`[N]` argument
    /// forms — the same restriction as alias arguments, §25.2). `None` for a
    /// plain named type. Composes with the prefix forms (`?Name(A)`, `!Name(A)`,
    /// `*Name(A)`, `[]Name(A)`, `[N]Name(A)`); never combined with the named
    /// error-set form (`Set` in `Set!T` is always a plain name).
    pub ctor_args: Option<Vec<TypeExpr>>,
    pub span: Span,
}

/// The size of an array type `[N]T`: a literal (`[3]T`) or a comptime
/// value-parameter name (`[n]T`, resolved per monomorphisation, v0.128).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ArraySize {
    Lit(i64),
    Param(String),
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
    /// The type annotation is optional (v0.121): `var name = expr;` infers the
    /// type from `expr`.
    Let {
        is_const: bool,
        name: String,
        ty: Option<TypeExpr>,
        value: Expr,
        span: Span,
    },
    /// `name = expr;`, or a compound assignment `name op= expr;` (v0.131) when
    /// `op` is `Some` (`+= -= *= /= %=` → `Add`/`Sub`/`Mul`/`Div`/`Rem`),
    /// which means `name = name op expr` (the place is read once).
    Assign {
        name: String,
        op: Option<BinOp>,
        value: Expr,
        span: Span,
    },
    /// `place = expr;` (or a compound `place op= expr;`, v0.131) where `place` is
    /// a field-access / index chain (`a.b.c`, `a[i]`). Simple `name = expr;`
    /// uses [`Stmt::Assign`] instead. For a compound assignment the place is
    /// evaluated once.
    FieldAssign {
        place: Expr,
        op: Option<BinOp>,
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
    ///
    /// `capture` is `Some(name)` for the optional-payload form
    /// `if (opt) |name| { … } else { … }` (v0.125): `cond` is an optional, and
    /// `name` binds the unwrapped value in `then`; `els` runs when it is null.
    If {
        cond: Expr,
        capture: Option<String>,
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
        /// `Some(name)` for a labeled loop `name: while (…)` (v0.147), targetable
        /// by `break :name` / `continue :name`.
        label: Option<String>,
        span: Span,
    },
    /// `for (iter) |elem| { body }` or `for (iter, 0..) |elem, index| { body }`
    /// (v0.133) — iterate the elements of an array/slice. `index` is `Some(name)`
    /// for the `, 0..` index-capture form. Lowered to an indexed `while`.
    For {
        iter: Expr,
        elem: String,
        index: Option<String>,
        body: Block,
        /// `Some(name)` for a labeled loop `name: for (…)` (v0.147).
        label: Option<String>,
        span: Span,
    },
    /// `break;` or `break :label;` (v0.147). `target` is `Some(label)` to break
    /// out of the enclosing loop with that label, else the innermost loop.
    Break {
        target: Option<String>,
        span: Span,
    },
    /// `continue;` or `continue :label;` (v0.147).
    Continue {
        target: Option<String>,
        span: Span,
    },
    /// `defer stmt;` — runs `stmt` at scope exit, in LIFO order.
    Defer {
        stmt: Box<Stmt>,
        span: Span,
    },
    /// `errdefer stmt;` — like `defer`, but runs (LIFO) only on **error-return**
    /// paths (a `try` propagation or `return error.X`), not on normal exit
    /// (v0.125).
    ErrDefer {
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
    /// Inclusive integer-range labels `lo..hi` (v0.146): the arm matches when the
    /// scrutinee is in `[lo, hi]`. Bounds are integer literals. Combine freely
    /// with `labels` (an arm matches any label OR any range).
    pub ranges: Vec<(i64, i64)>,
    /// `|name|` payload capture (v0.124, tagged-union switch): binds the matched
    /// variant's payload in the arm body. `None` for enum/integer switches.
    pub capture: Option<String>,
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
            Stmt::For { span, .. } => *span,
            Stmt::Break { span, .. } => *span,
            Stmt::Continue { span, .. } => *span,
            Stmt::Defer { span, .. } => *span,
            Stmt::ErrDefer { span, .. } => *span,
            Stmt::Block(b) => b.span,
            Stmt::Switch { span, .. } => *span,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnOp {
    Neg,    // -x
    Not,    // !x
    BitNot, // ~x  (bitwise complement, v0.132)
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
    // Bitwise / shift (v0.132); integer operands, integer result.
    BitAnd,
    BitOr,
    BitXor,
    Shl,
    Shr,
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
            BinOp::BitAnd => "&",
            BinOp::BitOr => "|",
            BinOp::BitXor => "^",
            BinOp::Shl => "<<",
            BinOp::Shr => ">>",
        }
    }
}

#[derive(Clone, Debug)]
pub enum Expr {
    Int {
        value: i64,
        span: Span,
    },
    /// A floating-point literal `3.14` of type `f64` (v0.144).
    Float {
        value: f64,
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
    /// A string literal `"…"` as a value of type `[]u8` (v0.127): a slice over
    /// static bytes. `value` is the decoded (unescaped) contents.
    StrLit { value: String, span: Span },
    /// `unreachable` (v0.141) — a diverging expression/statement asserting a
    /// path is impossible; traps (exit 101) if reached.
    Unreachable { span: Span },
    /// A comptime builtin call `@name(args)` in expression position (v0.136):
    /// `@sizeOf(T)` → `usize`, `@typeName(T)` → `[]u8`. (`@This()` is a *type*,
    /// handled in `TypeExpr`, and `@import` is a top-level item.)
    Builtin {
        name: String,
        args: Vec<Expr>,
        span: Span,
    },
    /// An anonymous `struct { fields [methods] }` **type value** (v0.129) — only
    /// valid as the body of a type-returning function `fn F(comptime T: type)
    /// type`. `methods` (v0.130) are monomorphised per instantiation and use
    /// `Self` (the instantiated struct) and the type parameter in their bodies.
    StructType {
        fields: Vec<FieldDecl>,
        methods: Vec<Func>,
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
    /// An array literal `[N]T{ e0, e1, … }` with exactly `N` elements.
    ArrayLit {
        elem: TypeExpr,
        elems: Vec<Expr>,
        span: Span,
    },
    /// Indexing `base[index]` (read). Index assignment reuses
    /// [`Stmt::FieldAssign`] with an `Index` place.
    Index {
        base: Box<Expr>,
        index: Box<Expr>,
        span: Span,
    },
    /// `&place` — address-of an lvalue, yielding `*T` (v0.118).
    AddrOf { place: Box<Expr>, span: Span },
    /// `expr.*` — pointer dereference (read). Deref assignment reuses
    /// [`Stmt::FieldAssign`] with a `Deref` place.
    Deref { expr: Box<Expr>, span: Span },
    /// `base[lo..hi]` — slice an array (or slice), yielding `[]T` (v0.118).
    SliceExpr {
        base: Box<Expr>,
        lo: Box<Expr>,
        hi: Box<Expr>,
        span: Span,
    },
    /// `try expr` — unwrap an error union `!T`, or propagate the error by
    /// returning it from the enclosing `!U` function. v0.115: statement-level.
    Try { expr: Box<Expr>, span: Span },
    /// `expr catch default` — unwrap `!T`, or evaluate `default` (a `T`) on
    /// error. `capture` is `Some(name)` for the capturing form `expr catch
    /// |name| default` (v0.142): `name` binds the error code (`i32`) and
    /// `default` is evaluated only on the error path with it in scope.
    Catch {
        expr: Box<Expr>,
        capture: Option<String>,
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
            Expr::Float { span, .. } => *span,
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
            Expr::ArrayLit { span, .. } => *span,
            Expr::Index { span, .. } => *span,
            Expr::StrLit { span, .. } => *span,
            Expr::Builtin { span, .. } => *span,
            Expr::Unreachable { span } => *span,
            Expr::StructType { span, .. } => *span,
            Expr::AddrOf { span, .. } => *span,
            Expr::Deref { span, .. } => *span,
            Expr::SliceExpr { span, .. } => *span,
        }
    }
}

/// Shared test-fixture constructors for the sema / fmt / emit_c test modules.
///
/// These build the AST shapes the unit tests exercise, with every flag
/// defaulted and [`Span::DUMMY`] spans. The full set of [`TypeExpr`] fields is
/// spelled exactly once, in [`fixtures::ty`]; every variant constructor is a
/// functional-record-update over it, so adding a field to [`TypeExpr`] only
/// touches `ty`. The parser's exhaustive `TypeExpr` literals (with real merged
/// spans) are intentionally *not* routed through here.
#[cfg(test)]
pub(crate) mod fixtures {
    use super::{ArraySize, BinOp, Block, Expr, Stmt, TypeExpr};
    use crate::span::Span;

    /// A bare named type expression `name` — the ONLY place that spells all
    /// `TypeExpr` fields and their defaults.
    pub fn ty(name: &str) -> TypeExpr {
        TypeExpr {
            name: name.to_string(),
            optional: false,
            error_union: false,
            error_set: None,
            array_len: None,
            pointer: false,
            slice: false,
            ctor_args: None,
            span: Span::DUMMY,
        }
    }

    /// An optional type expression `?name` (v0.114).
    /// A generic type-constructor application `Name(A, B, …)` in type position
    /// (v0.152, SPEC §42.1).
    pub fn app_ty(name: &str, args: Vec<TypeExpr>) -> TypeExpr {
        TypeExpr {
            ctor_args: Some(args),
            ..ty(name)
        }
    }

    pub fn opt_ty(name: &str) -> TypeExpr {
        TypeExpr {
            optional: true,
            ..ty(name)
        }
    }

    /// An error-union type expression `!name` over the implicit global error
    /// set (v0.115).
    pub fn err_ty(name: &str) -> TypeExpr {
        TypeExpr {
            error_union: true,
            ..ty(name)
        }
    }

    /// A *named* error-union type expression `set!name` — the error union over
    /// the named error set `set` with payload type `name` (v0.139). Its runtime
    /// representation is identical to [`err_ty`]; the set name is a pure sema
    /// constraint.
    pub fn set_err_ty(set: &str, name: &str) -> TypeExpr {
        TypeExpr {
            error_union: true,
            error_set: Some(set.to_string()),
            ..ty(name)
        }
    }

    /// A fixed-size array type expression `[len]name` with a literal length
    /// (v0.117).
    pub fn arr_ty(name: &str, len: i64) -> TypeExpr {
        TypeExpr {
            array_len: Some(ArraySize::Lit(len)),
            ..ty(name)
        }
    }

    /// An array type expression `[param]name` whose length is the comptime
    /// value-parameter `param` (v0.128).
    pub fn arr_param_ty(name: &str, param: &str) -> TypeExpr {
        TypeExpr {
            array_len: Some(ArraySize::Param(param.to_string())),
            ..ty(name)
        }
    }

    /// A pointer type expression `*name` (v0.118).
    pub fn ptr_ty(name: &str) -> TypeExpr {
        TypeExpr {
            pointer: true,
            ..ty(name)
        }
    }

    /// A slice type expression `[]name` (v0.118).
    pub fn slice_ty(name: &str) -> TypeExpr {
        TypeExpr {
            slice: true,
            ..ty(name)
        }
    }

    /// An identifier expression `name`.
    pub fn ident(name: &str) -> Expr {
        Expr::Ident {
            name: name.to_string(),
            span: Span::DUMMY,
        }
    }

    /// An integer literal expression.
    pub fn int(value: i64) -> Expr {
        Expr::Int {
            value,
            span: Span::DUMMY,
        }
    }

    /// A binary expression `lhs op rhs`.
    pub fn bin(op: BinOp, lhs: Expr, rhs: Expr) -> Expr {
        Expr::Binary {
            op,
            lhs: Box::new(lhs),
            rhs: Box::new(rhs),
            span: Span::DUMMY,
        }
    }

    /// A call expression `callee(args…)`.
    pub fn call(callee: &str, args: Vec<Expr>) -> Expr {
        Expr::Call {
            callee: callee.to_string(),
            args,
            span: Span::DUMMY,
        }
    }

    /// The `null` literal (v0.114).
    pub fn null() -> Expr {
        Expr::Null { span: Span::DUMMY }
    }

    /// `lhs orelse rhs` — optional defaulting (v0.114).
    pub fn orelse(lhs: Expr, rhs: Expr) -> Expr {
        Expr::Orelse {
            lhs: Box::new(lhs),
            rhs: Box::new(rhs),
            span: Span::DUMMY,
        }
    }

    /// `expr.?` — optional unwrap (v0.114).
    pub fn unwrap(expr: Expr) -> Expr {
        Expr::Unwrap {
            expr: Box::new(expr),
            span: Span::DUMMY,
        }
    }

    /// An error literal `error.name` (v0.115).
    pub fn error_lit(name: &str) -> Expr {
        Expr::ErrorLit {
            name: name.to_string(),
            span: Span::DUMMY,
        }
    }

    /// `try expr` — error propagation (v0.115).
    pub fn try_expr(expr: Expr) -> Expr {
        Expr::Try {
            expr: Box::new(expr),
            span: Span::DUMMY,
        }
    }

    /// `expr catch default` — the non-capturing handler form (v0.115).
    pub fn catch_expr(expr: Expr, default: Expr) -> Expr {
        Expr::Catch {
            expr: Box::new(expr),
            capture: None,
            default: Box::new(default),
            span: Span::DUMMY,
        }
    }

    /// `expr catch |name| default` — the capturing handler form (v0.142, §36).
    pub fn catch_capture_expr(expr: Expr, name: &str, default: Expr) -> Expr {
        Expr::Catch {
            expr: Box::new(expr),
            capture: Some(name.to_string()),
            default: Box::new(default),
            span: Span::DUMMY,
        }
    }

    /// A block `{ stmts… }`.
    pub fn block(stmts: Vec<Stmt>) -> Block {
        Block {
            stmts,
            span: Span::DUMMY,
        }
    }
}
