//! Semantic analysis: name resolution + type checking + comptime validation.
//!
//! `check` runs a single pass over the module with a stack of lexical scopes.
//! It first collects every struct declaration (interning ids and resolving
//! field types), then every top-level function signature (so calls may refer to
//! functions defined later), then folds the top-level constants in source order
//! via [`const_eval`], then type-checks every function and test body. All
//! diagnostics are collected — analysis never stops at the first error. On
//! success it returns the built [`StructTable`] for the backend.
//!
//! Error codes (SPEC §3, §9.4):
//! - `E0100` — unknown name (value, callee, or type name).
//! - `E0101` — redefining a builtin (`print` / `expect`).
//! - `E0110` — a type mismatch (the general sema type-error code).
//! - `E0120` — `break` / `continue` outside a loop.
//! - `E0130` / `E0131` / `E0132` — non-constant / unknown-const / type error
//!   in a `comptime` or top-level `const` initializer (raised by `const_eval`).
//! - `E0140` — `expect` called outside a `test` block.
//! - `E0160` — forward / cyclic struct reference in a field type.
//! - `E0161` — unknown type name in a struct field.
//! - `E0162` — duplicate field name within a struct.
//! - `E0163` — struct literal of a name that is not a struct.
//! - `E0164` — missing / extra / duplicate field in a struct literal.
//! - `E0165` — field access on a non-struct value.
//! - `E0166` — access of a field the struct does not have.
//! - `E0167` — field-assignment target not rooted in an assignable `var`.
//! - `E0168` — `==` / `!=` on struct types.
//! - `E0170` — call of a method / associated function the struct does not have.
//! - `E0171` — wrong number of arguments to a method / associated function.
//! - `E0172` — calling a method statically without the `self` argument, or an
//!   associated function on a value.
//! - `E0180` — a bare `null` with no expected optional type at its position.
//! - `E0181` — `orelse` whose left operand is not an optional (`?T`).
//! - `E0182` — `.?` (force-unwrap) whose operand is not an optional (`?T`).

use std::collections::{HashMap, HashSet};

use crate::ast::{
    BinOp, Block, Expr, FieldInit, Func, Item, Module, Stmt, StructDecl, TestBlock, TypeExpr, UnOp,
};
use crate::const_eval::{self, ConstVal};
use crate::diag::Diagnostic;
use crate::span::Span;
use crate::types::{StructTable, Type};

/// One-pass semantic check of a whole module. On success, returns the resolved
/// [`StructTable`] (consumed by the backend); on failure, every diagnostic.
pub fn check(module: &Module) -> Result<StructTable, Vec<Diagnostic>> {
    let mut cx = Checker::new();
    cx.check_module(module);
    if cx.diags.is_empty() {
        Ok(cx.structs)
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

/// A resolved signature for a struct's method or associated function (SPEC §10).
///
/// `params` lists every parameter type in declaration order — including the
/// leading `self` (whose type is the enclosing struct) when `is_method` is true.
/// `is_method` records whether the first parameter is named `self`, which
/// decides whether the function may be invoked on a value (`v.m(..)`) or only
/// statically (`Name.f(..)`).
#[derive(Clone)]
struct StructFn {
    params: Vec<Type>,
    ret: Type,
    is_method: bool,
}

/// A lexical binding: its type and whether it is immutable (a `const` or a
/// parameter — only `var` locals may be assigned to).
type Binding = (Type, bool);

struct Checker {
    diags: Vec<Diagnostic>,
    /// All struct types, interned in declaration order.
    structs: StructTable,
    /// Folded values of top-level consts, in source order so far.
    consts: HashMap<String, ConstVal>,
    /// Declared types of top-level consts.
    const_types: HashMap<String, Type>,
    /// All user function signatures (collected up front).
    funcs: HashMap<String, FuncSig>,
    /// Per-struct method / associated-function signatures, keyed by struct id
    /// then function name (collected up front, so method calls resolve).
    struct_funcs: HashMap<u32, HashMap<String, StructFn>>,
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
            structs: StructTable::new(),
            consts: HashMap::new(),
            const_types: HashMap::new(),
            funcs: HashMap::new(),
            struct_funcs: HashMap::new(),
            scopes: Vec::new(),
            in_test: false,
            loop_depth: 0,
            ret_type: Type::Void,
        }
    }

    fn error(&mut self, span: Span, code: &'static str, message: impl Into<String>) {
        self.diags.push(Diagnostic::error(span, code, message));
    }

    /// The source spelling of a type for diagnostics — struct types are named
    /// via the [`StructTable`] (their declared name), everything else via
    /// [`Type::name`].
    fn type_name(&self, t: Type) -> String {
        match t {
            Type::Struct(id) => self.structs.get(id).name.clone(),
            Type::Optional(id) => {
                format!("?{}", self.type_name(self.structs.optional_inner(id)))
            }
            other => other.name().to_string(),
        }
    }

    // ---- top-level driving ------------------------------------------------

    fn check_module(&mut self, m: &Module) {
        // Pass 0a: intern every struct name first so that field types and
        // signatures may refer to any struct (forward references in signatures
        // are fine; forward references in *field types* are caught below).
        for item in &m.items {
            if let Item::Struct(s) = item {
                self.structs.intern(&s.name);
            }
        }

        // Pass 0b: resolve struct field types. A field type resolves to a
        // builtin or to a struct declared *earlier* in source order; a
        // reference to a not-yet-declared struct (forward/cyclic) is E0160, an
        // unknown name is E0161, a duplicate field is E0162.
        let mut declared: HashSet<String> = HashSet::new();
        for item in &m.items {
            if let Item::Struct(s) = item {
                let id = match self.structs.id_of(&s.name) {
                    Some(id) => id,
                    None => continue, // unreachable: interned in pass 0a
                };
                let mut fields: Vec<(String, Type)> = Vec::new();
                let mut seen: HashSet<String> = HashSet::new();
                for f in &s.fields {
                    if !seen.insert(f.name.clone()) {
                        let msg = format!("duplicate field `{}` in struct `{}`", f.name, s.name);
                        self.error(f.span, "E0162", msg);
                        continue;
                    }
                    // Unresolved field types fall back to `i64` so downstream
                    // field-access checks still recognise the field name.
                    let fty = self
                        .resolve_field_type(&f.ty, &declared, &s.name)
                        .unwrap_or(Type::I64);
                    fields.push((f.name.clone(), fty));
                }
                self.structs.set_fields(id, fields);
                declared.insert(s.name.clone());
            }
        }

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
                    .map(|p| self.resolve_type_opt(&p.ty).unwrap_or(Type::I64))
                    .collect();
                let ret = self.resolve_type_opt(&f.ret).unwrap_or(Type::Void);
                self.funcs.insert(f.name.clone(), FuncSig { params, ret });
            }
        }

        // Pass 1b: collect struct method / associated-function signatures so
        // that method calls resolve regardless of declaration order. `self`'s
        // type is always the enclosing struct (SPEC §10); other parameter and
        // return types resolve to builtins or any interned struct (diagnostics
        // for ill-typed parameters are raised when the body is checked).
        for item in &m.items {
            if let Item::Struct(s) = item {
                let id = match self.structs.id_of(&s.name) {
                    Some(id) => id,
                    None => continue, // unreachable: interned in pass 0a
                };
                let mut map: HashMap<String, StructFn> = HashMap::new();
                for f in &s.methods {
                    let is_method = f.params.first().map_or(false, |p| p.name == "self");
                    let params = f
                        .params
                        .iter()
                        .enumerate()
                        .map(|(i, p)| {
                            if i == 0 && is_method {
                                Type::Struct(id)
                            } else {
                                self.resolve_type_opt(&p.ty).unwrap_or(Type::I64)
                            }
                        })
                        .collect();
                    let ret = self.resolve_type_opt(&f.ret).unwrap_or(Type::Void);
                    // A duplicate function name keeps the last declaration; the
                    // grammar does not define a diagnostic for it.
                    map.insert(
                        f.name.clone(),
                        StructFn {
                            params,
                            ret,
                            is_method,
                        },
                    );
                }
                self.struct_funcs.insert(id, map);
            }
        }

        // Pass 2: fold top-level consts in source order.
        for item in &m.items {
            if let Item::Const(c) = item {
                let declared = self.resolve_type_opt(&c.ty);
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
                                let msg = format!(
                                    "constant initializer type mismatch: expected `{}`, found `{}`",
                                    self.type_name(dt),
                                    found
                                );
                                self.error(c.value.span(), "E0110", msg);
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
                Item::Struct(s) => self.check_struct_methods(s),
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

    /// Type-check every method / associated-function body of a struct (SPEC
    /// §10). Each body is checked exactly like a free function, except that a
    /// leading `self` parameter is bound to the enclosing struct type.
    fn check_struct_methods(&mut self, s: &StructDecl) {
        let id = match self.structs.id_of(&s.name) {
            Some(id) => id,
            None => return, // unreachable: interned in pass 0a
        };
        for f in &s.methods {
            self.check_struct_func(f, id);
        }
    }

    /// Type-check one struct function body. `struct_id` is the enclosing struct,
    /// used as the type of a leading `self` parameter.
    fn check_struct_func(&mut self, f: &Func, struct_id: u32) {
        self.ret_type = self.resolve_type(&f.ret).unwrap_or(Type::Void);
        self.in_test = false;
        self.loop_depth = 0;
        self.scopes.push(HashMap::new());
        let is_method = f.params.first().map_or(false, |p| p.name == "self");
        for (i, p) in f.params.iter().enumerate() {
            // The receiver `self` always has the enclosing struct type; other
            // parameters resolve normally (emitting `E0100` for unknown types).
            let pt = if i == 0 && is_method {
                Type::Struct(struct_id)
            } else {
                self.resolve_type(&p.ty).unwrap_or(Type::I64)
            };
            // Parameters (including `self`) are immutable bindings.
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

    /// Resolve a type name to a builtin or a registered struct, without
    /// emitting a diagnostic. Returns `None` for an unknown name.
    ///
    /// When the [`TypeExpr`] is written `?T` (`optional`), the inner type is
    /// resolved first and the result is `Type::Optional(intern_optional(inner))`
    /// — so optional types are interned the moment a signature, field or local
    /// declaration mentions them (SPEC §11.1).
    fn resolve_type_opt(&mut self, te: &TypeExpr) -> Option<Type> {
        let inner =
            Type::from_name(&te.name).or_else(|| self.structs.id_of(&te.name).map(Type::Struct))?;
        if te.optional {
            Some(Type::Optional(self.structs.intern_optional(inner)))
        } else {
            Some(inner)
        }
    }

    /// Resolve a type name to a builtin or a registered struct, emitting
    /// `E0100` for an unknown name.
    fn resolve_type(&mut self, te: &TypeExpr) -> Option<Type> {
        match self.resolve_type_opt(te) {
            Some(t) => Some(t),
            None => {
                self.error(te.span, "E0100", format!("unknown type `{}`", te.name));
                None
            }
        }
    }

    /// Resolve a *struct field* type: a builtin, or a struct declared earlier
    /// (tracked by `declared`). A reference to a struct not yet declared is a
    /// forward/cyclic reference (`E0160`); an unknown name is `E0161`.
    ///
    /// A field written `?T` resolves its inner `T` by these same rules and
    /// returns `Type::Optional(intern_optional(inner))` (SPEC §11.1).
    fn resolve_field_type(
        &mut self,
        te: &TypeExpr,
        declared: &HashSet<String>,
        owner: &str,
    ) -> Option<Type> {
        let inner = if let Some(t) = Type::from_name(&te.name) {
            t
        } else if let Some(id) = self.structs.id_of(&te.name) {
            if declared.contains(&te.name) {
                Type::Struct(id)
            } else {
                let msg = format!(
                    "field of struct `{}` refers to struct `{}` before it is declared (forward or cyclic reference)",
                    owner, te.name
                );
                self.error(te.span, "E0160", msg);
                return None;
            }
        } else {
            self.error(te.span, "E0161", format!("unknown type `{}`", te.name));
            return None;
        };
        if te.optional {
            Some(Type::Optional(self.structs.intern_optional(inner)))
        } else {
            Some(inner)
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
                // With a known annotation, apply optional coercion (§11.2): a
                // `T` value or `null` widens to an expected `?T`.
                let vt = match declared {
                    Some(dt) => self.check_coerce(value, dt),
                    None => self.check_expr(value, None),
                };
                if let (Some(dt), Some(vt)) = (declared, vt) {
                    if dt != vt {
                        let msg = format!(
                            "initializer type mismatch: expected `{}`, found `{}`",
                            self.type_name(dt),
                            self.type_name(vt)
                        );
                        self.error(value.span(), "E0110", msg);
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
                        // Optional coercion (§11.2): assigning a `T` value or
                        // `null` to a `?T` place widens implicitly.
                        let vt = self.check_coerce(value, ty);
                        if let Some(vt) = vt {
                            if vt != ty {
                                let msg = format!(
                                    "cannot assign value of type `{}` to `{}` of type `{}`",
                                    self.type_name(vt),
                                    name,
                                    self.type_name(ty)
                                );
                                self.error(value.span(), "E0110", msg);
                            }
                        }
                    }
                }
                None => {
                    self.error(*span, "E0100", format!("unknown name `{}`", name));
                    self.check_expr(value, None);
                }
            },
            Stmt::FieldAssign { place, value, .. } => {
                if let Some(pt) = self.resolve_place(place) {
                    // Optional coercion (§11.2): a `T`/`null` widens to a `?T` field.
                    if let Some(vt) = self.check_coerce(value, pt) {
                        if vt != pt {
                            let msg = format!(
                                "cannot assign value of type `{}` to field of type `{}`",
                                self.type_name(vt),
                                self.type_name(pt)
                            );
                            self.error(value.span(), "E0110", msg);
                        }
                    }
                } else {
                    self.check_expr(value, None);
                }
            }
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
                        // Optional coercion (§11.2): `return e` widens `T`/`null`
                        // to a `?T` function return type.
                        let vt = self.check_coerce(e, expected);
                        if let Some(vt) = vt {
                            if vt != expected {
                                let msg = format!(
                                    "return type mismatch: expected `{}`, found `{}`",
                                    self.type_name(expected),
                                    self.type_name(vt)
                                );
                                self.error(e.span(), "E0110", msg);
                            }
                        }
                    }
                }
                None => {
                    if self.ret_type != Type::Void {
                        let msg = format!(
                            "`return;` is only valid in a `void` function, found return type `{}`",
                            self.type_name(self.ret_type)
                        );
                        self.error(*span, "E0110", msg);
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
                let msg = format!("`{}` condition must be `bool`, found `{}`", kw, self.type_name(t));
                self.error(cond.span(), "E0110", msg);
            }
        }
    }

    /// Resolve the type of an assignment place (a field-access chain) and
    /// verify that its root is an assignable `var` local. Emits `E0167` if the
    /// root is a `const`/parameter (or the place is not a chain), and
    /// `E0165`/`E0166` for an ill-typed chain. Returns the place's type.
    fn resolve_place(&mut self, place: &Expr) -> Option<Type> {
        match place {
            Expr::Ident { name, span } => match self.lookup(name) {
                Some((ty, is_const)) => {
                    if is_const {
                        let msg = format!(
                            "cannot assign through immutable binding `{}` (only `var` locals are assignable)",
                            name
                        );
                        self.error(*span, "E0167", msg);
                    }
                    Some(ty)
                }
                None => {
                    self.error(*span, "E0100", format!("unknown name `{}`", name));
                    None
                }
            },
            Expr::Field { base, field, span } => {
                let bt = self.resolve_place(base)?;
                self.field_type_of(bt, field, *span)
            }
            _ => {
                self.error(
                    place.span(),
                    "E0167",
                    "assignment target must be a `var` local or a field of one",
                );
                self.check_expr(place, None);
                None
            }
        }
    }

    /// Resolve `<base type>.field`, emitting `E0165` if `base` is not a struct
    /// or `E0166` if it has no such field. Returns the field's type.
    fn field_type_of(&mut self, base: Type, field: &str, span: Span) -> Option<Type> {
        match base {
            Type::Struct(id) => match self.structs.get(id).field_type(field) {
                Some(t) => Some(t),
                None => {
                    let sname = self.structs.get(id).name.clone();
                    let msg = format!("struct `{}` has no field `{}`", sname, field);
                    self.error(span, "E0166", msg);
                    None
                }
            },
            other => {
                let msg = format!(
                    "cannot access field `{}` of non-struct type `{}`",
                    field,
                    self.type_name(other)
                );
                self.error(span, "E0165", msg);
                None
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
            Expr::StructLit { name, fields, span } => self.check_struct_lit(name, fields, *span),
            Expr::Field { base, field, span } => {
                let bt = self.check_expr(base, None)?;
                self.field_type_of(bt, field, *span)
            }
            Expr::MethodCall {
                receiver,
                method,
                args,
                span,
            } => self.check_method_call(receiver, method, args, *span),
            // `null` takes its `?T` type from the expected type at this position
            // (SPEC §11.1); with no expected optional type it is `E0180`.
            Expr::Null { span } => match expected {
                Some(Type::Optional(id)) => Some(Type::Optional(id)),
                _ => {
                    self.error(
                        *span,
                        "E0180",
                        "`null` has no expected optional type here; annotate the target as `?T`",
                    );
                    None
                }
            },
            // `lhs orelse rhs`: `lhs` must be `?T` (else `E0181`); `rhs` must be
            // `T`; the result is `T` (SPEC §11.1).
            Expr::Orelse { lhs, rhs, span } => {
                let lhs_expected = self.as_optional_expectation(expected);
                match self.check_expr(lhs, lhs_expected) {
                    Some(Type::Optional(id)) => {
                        let inner = self.structs.optional_inner(id);
                        if let Some(rt) = self.check_expr(rhs, Some(inner)) {
                            if rt != inner {
                                let msg = format!(
                                    "`orelse` alternative type mismatch: expected `{}`, found `{}`",
                                    self.type_name(inner),
                                    self.type_name(rt)
                                );
                                self.error(rhs.span(), "E0110", msg);
                            }
                        }
                        Some(inner)
                    }
                    Some(other) => {
                        let msg = format!(
                            "`orelse` requires an optional (`?T`) left operand, found `{}`",
                            self.type_name(other)
                        );
                        self.error(*span, "E0181", msg);
                        // Still check the alternative to surface its own errors.
                        self.check_expr(rhs, None);
                        None
                    }
                    None => {
                        self.check_expr(rhs, None);
                        None
                    }
                }
            }
            // `expr.?`: `expr` must be `?T` (else `E0182`); the result is `T`
            // (a null unwrap panics at run time — that is the backend's job).
            Expr::Unwrap { expr: inner, span } => {
                let inner_expected = self.as_optional_expectation(expected);
                match self.check_expr(inner, inner_expected) {
                    Some(Type::Optional(id)) => Some(self.structs.optional_inner(id)),
                    Some(other) => {
                        let msg = format!(
                            "`.?` requires an optional (`?T`) operand, found `{}`",
                            self.type_name(other)
                        );
                        self.error(*span, "E0182", msg);
                        None
                    }
                    None => None,
                }
            }
        }
    }

    /// Derive the optional type a sub-expression should be checked against when
    /// its *result* is expected to be `T`. For `x orelse y` and `x.?` the
    /// operand is `?T`: an expected inner `T` becomes `?T` (interning it if
    /// necessary), an already-optional expectation is kept, and no expectation
    /// stays `None`. This lets a bare `null` operand (whose type comes from
    /// context) resolve, e.g. `var v: i32 = (null orelse 0);`.
    fn as_optional_expectation(&mut self, expected: Option<Type>) -> Option<Type> {
        match expected {
            Some(t @ Type::Optional(_)) => Some(t),
            Some(other) => Some(Type::Optional(self.structs.intern_optional(other))),
            None => None,
        }
    }

    /// Type-check `expr` against an expected type, applying optional coercion
    /// (SPEC §11.2). When `expected` is `?T`, this accepts:
    /// - a `null` literal (which adopts `?T`),
    /// - a value whose type is the inner `T` (which widens to `?T`), or
    /// - a value already of type `?T`,
    /// returning the expected `?T` in each accepting case so the caller's
    /// equality check passes. Any other type is returned unchanged for the
    /// caller to report as `E0110`. For a non-optional `expected`, this is just
    /// [`check_expr`] with that expectation.
    fn check_coerce(&mut self, expr: &Expr, expected: Type) -> Option<Type> {
        if let Type::Optional(id) = expected {
            // `null` adopts the optional type directly.
            if matches!(expr, Expr::Null { .. }) {
                return self.check_expr(expr, Some(expected));
            }
            // Otherwise check against the inner `T` so that integer literals and
            // nested constructs adopt it, then accept either `T` (coerces) or an
            // already-`?T` value.
            let inner = self.structs.optional_inner(id);
            let vt = self.check_expr(expr, Some(inner))?;
            if vt == inner || vt == expected {
                Some(expected)
            } else {
                Some(vt)
            }
        } else {
            self.check_expr(expr, Some(expected))
        }
    }

    /// Type-check a method / associated-function call `receiver.method(args)`
    /// (SPEC §10). Resolution has two shapes:
    ///
    /// - **(b) associated/static call** — `receiver` is an [`Expr::Ident`] that
    ///   names a struct *type* and is not a value in scope: bind `args` to *all*
    ///   of the function's parameters (so `Counter.get(c)` is the explicit-self
    ///   form and `Counter.zero()` the static form).
    /// - **(a) method call** — otherwise `receiver` is evaluated as a value; it
    ///   must have a struct type, the resolved function must be a method (first
    ///   parameter `self`), and `args` bind to the parameters *after* `self`.
    fn check_method_call(
        &mut self,
        receiver: &Expr,
        method: &str,
        args: &[Expr],
        span: Span,
    ) -> Option<Type> {
        // Case (b): an identifier that names a struct type and is not shadowed
        // by a value in scope → associated / static call.
        if let Expr::Ident { name, .. } = receiver {
            if self.lookup(name).is_none() {
                if let Some(id) = self.structs.id_of(name) {
                    return self.check_static_call(id, name, method, args, span);
                }
            }
        }
        // Case (a): evaluate the receiver as a value; it must be a struct.
        let recv_ty = self.check_expr(receiver, None)?;
        let id = match recv_ty {
            Type::Struct(id) => id,
            other => {
                let msg = format!(
                    "type `{}` has no method `{}` (method calls require a struct receiver)",
                    self.type_name(other),
                    method
                );
                self.error(span, "E0170", msg);
                for a in args {
                    self.check_expr(a, None);
                }
                return None;
            }
        };
        self.check_value_method_call(id, method, args, span)
    }

    /// Resolve `value.method(args)` — a method call on a struct value (case a).
    fn check_value_method_call(
        &mut self,
        id: u32,
        method: &str,
        args: &[Expr],
        span: Span,
    ) -> Option<Type> {
        let sf = match self.struct_func(id, method) {
            Some(sf) => sf,
            None => {
                let sname = self.structs.get(id).name.clone();
                self.error(
                    span,
                    "E0170",
                    format!("struct `{}` has no method `{}`", sname, method),
                );
                for a in args {
                    self.check_expr(a, None);
                }
                return None;
            }
        };
        if !sf.is_method {
            // An associated function (no `self`) cannot be invoked on a value.
            let sname = self.structs.get(id).name.clone();
            let msg = format!(
                "`{}` is an associated function of `{}`; call it as `{}.{}(...)`, not on a value",
                method, sname, sname, method
            );
            self.error(span, "E0172", msg);
            for a in args {
                self.check_expr(a, None);
            }
            return None;
        }
        // The receiver supplies `self`; the remaining parameters bind `args`.
        let expected: Vec<Type> = sf.params[1..].to_vec();
        if args.len() != expected.len() {
            let sname = self.structs.get(id).name.clone();
            self.error(
                span,
                "E0171",
                format!(
                    "method `{}` of `{}` takes {} argument(s), found {}",
                    method,
                    sname,
                    expected.len(),
                    args.len()
                ),
            );
            for a in args {
                self.check_expr(a, None);
            }
            return Some(sf.ret);
        }
        self.check_arg_types(args, &expected);
        Some(sf.ret)
    }

    /// Resolve `Name.method(args)` — an associated / static call (case b).
    fn check_static_call(
        &mut self,
        id: u32,
        sname: &str,
        method: &str,
        args: &[Expr],
        span: Span,
    ) -> Option<Type> {
        let sf = match self.struct_func(id, method) {
            Some(sf) => sf,
            None => {
                self.error(
                    span,
                    "E0170",
                    format!(
                        "struct `{}` has no method or associated function `{}`",
                        sname, method
                    ),
                );
                for a in args {
                    self.check_expr(a, None);
                }
                return None;
            }
        };
        // The static form binds `args` to *all* parameters (including an
        // explicit `self` for methods).
        let params: Vec<Type> = sf.params.clone();
        if args.len() != params.len() {
            // A method invoked statically with all of its post-`self` arguments
            // but no explicit `self` receiver is the dedicated `E0172`; any other
            // count is a plain arity error.
            if sf.is_method && args.len() == params.len().saturating_sub(1) {
                self.error(
                    span,
                    "E0172",
                    format!(
                        "method `{}` of `{}` called statically without the `self` argument; \
                         pass the receiver explicitly, e.g. `{}.{}(value, ...)`",
                        method, sname, sname, method
                    ),
                );
            } else {
                self.error(
                    span,
                    "E0171",
                    format!(
                        "`{}.{}` takes {} argument(s), found {}",
                        sname,
                        method,
                        params.len(),
                        args.len()
                    ),
                );
            }
            for a in args {
                self.check_expr(a, None);
            }
            return Some(sf.ret);
        }
        self.check_arg_types(args, &params);
        Some(sf.ret)
    }

    /// Type-check each argument against its expected parameter type, reusing the
    /// general type-mismatch code `E0110`. Caller guarantees equal lengths.
    fn check_arg_types(&mut self, args: &[Expr], params: &[Type]) {
        for (a, &pt) in args.iter().zip(params.iter()) {
            // Optional coercion (§11.2): a `T`/`null` argument widens to a `?T` param.
            if let Some(at) = self.check_coerce(a, pt) {
                if at != pt {
                    let msg = format!(
                        "argument type mismatch: expected `{}`, found `{}`",
                        self.type_name(pt),
                        self.type_name(at)
                    );
                    self.error(a.span(), "E0110", msg);
                }
            }
        }
    }

    /// Look up a struct's method / associated function by id and name.
    fn struct_func(&self, id: u32, method: &str) -> Option<StructFn> {
        self.struct_funcs
            .get(&id)
            .and_then(|m| m.get(method))
            .cloned()
    }

    /// Type-check a struct literal `Name{ .f = e, ... }`.
    fn check_struct_lit(&mut self, name: &str, inits: &[FieldInit], span: Span) -> Option<Type> {
        let id = match self.structs.id_of(name) {
            Some(id) => id,
            None => {
                self.error(span, "E0163", format!("`{}` is not a struct", name));
                for fi in inits {
                    self.check_expr(&fi.value, None);
                }
                return None;
            }
        };
        // Own the field list so we may freely call `&mut self` checks below.
        let decl_fields = self.structs.get(id).fields.clone();
        let mut inited: HashSet<String> = HashSet::new();
        for fi in inits {
            match decl_fields.iter().find(|(n, _)| n == &fi.name) {
                Some((_, fty)) => {
                    let fty = *fty;
                    if !inited.insert(fi.name.clone()) {
                        let msg = format!(
                            "field `{}` initialized more than once in `{}` literal",
                            fi.name, name
                        );
                        self.error(fi.span, "E0164", msg);
                    }
                    // Optional coercion (§11.2): a `T`/`null` widens to a `?T` field.
                    if let Some(vt) = self.check_coerce(&fi.value, fty) {
                        if vt != fty {
                            let msg = format!(
                                "field `{}` type mismatch: expected `{}`, found `{}`",
                                fi.name,
                                self.type_name(fty),
                                self.type_name(vt)
                            );
                            self.error(fi.value.span(), "E0110", msg);
                        }
                    }
                }
                None => {
                    let msg = format!("`{}` has no field `{}`", name, fi.name);
                    self.error(fi.span, "E0164", msg);
                    self.check_expr(&fi.value, None);
                }
            }
        }
        for (fname, _) in &decl_fields {
            if !inited.contains(fname) {
                let msg = format!("missing field `{}` in `{}` literal", fname, name);
                self.error(span, "E0164", msg);
            }
        }
        Some(Type::Struct(id))
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
                    let msg = format!(
                        "unary `-` requires a signed integer, found `{}`",
                        self.type_name(t)
                    );
                    self.error(span, "E0110", msg);
                    None
                }
            }
            UnOp::Not => {
                let t = self.check_expr(inner, Some(Type::Bool))?;
                if t == Type::Bool {
                    Some(Type::Bool)
                } else {
                    let msg = format!("unary `!` requires a `bool`, found `{}`", self.type_name(t));
                    self.error(span, "E0110", msg);
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
                    let msg = format!(
                        "arithmetic operand must be an integer, found `{}`",
                        self.type_name(lt)
                    );
                    self.error(lhs.span(), "E0110", msg);
                    return None;
                }
                if !rt.is_int() {
                    let msg = format!(
                        "arithmetic operand must be an integer, found `{}`",
                        self.type_name(rt)
                    );
                    self.error(rhs.span(), "E0110", msg);
                    return None;
                }
                if lt != rt {
                    let msg = format!(
                        "arithmetic operands must have the same type, found `{}` and `{}`",
                        self.type_name(lt),
                        self.type_name(rt)
                    );
                    self.error(span, "E0110", msg);
                    return None;
                }
                Some(lt)
            }
            BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => {
                // Operands may be int or bool, but must be the same type.
                // Struct types are never comparable.
                let (lt, rt) = self.check_int_operands(lhs, rhs, None);
                let lt = lt?;
                let rt = rt?;
                if matches!(lt, Type::Struct(_)) || matches!(rt, Type::Struct(_)) {
                    if matches!(op, BinOp::Eq | BinOp::Ne) {
                        self.error(
                            span,
                            "E0168",
                            "struct values do not support `==` / `!=` comparison",
                        );
                    } else {
                        self.error(
                            span,
                            "E0110",
                            "struct values do not support ordering comparisons",
                        );
                    }
                    return None;
                }
                if lt != rt {
                    let msg = format!(
                        "comparison operands must have the same type, found `{}` and `{}`",
                        self.type_name(lt),
                        self.type_name(rt)
                    );
                    self.error(span, "E0110", msg);
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
                    let msg = format!(
                        "`{}` requires `bool` operands, found `{}`",
                        op.c_op(),
                        self.type_name(lt)
                    );
                    self.error(lhs.span(), "E0110", msg);
                    ok = false;
                }
                if rt != Type::Bool {
                    let msg = format!(
                        "`{}` requires `bool` operands, found `{}`",
                        op.c_op(),
                        self.type_name(rt)
                    );
                    self.error(rhs.span(), "E0110", msg);
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
                        let msg = format!(
                            "`print` requires an integer argument, found `{}`",
                            self.type_name(t)
                        );
                        self.error(args[0].span(), "E0110", msg);
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
                        let msg = format!(
                            "`expect` requires a `bool` argument, found `{}`",
                            self.type_name(t)
                        );
                        self.error(args[0].span(), "E0110", msg);
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
                        // Optional coercion (§11.2): a `T`/`null` argument widens
                        // to a `?T` parameter.
                        if let Some(at) = self.check_coerce(a, pt) {
                            if at != pt {
                                let msg = format!(
                                    "argument type mismatch: expected `{}`, found `{}`",
                                    self.type_name(pt),
                                    self.type_name(at)
                                );
                                self.error(a.span(), "E0110", msg);
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
    use crate::ast::{ConstDecl, FieldDecl, FieldInit, Func, Param, StructDecl, TestBlock};

    fn sp() -> Span {
        Span::DUMMY
    }
    fn te(name: &str) -> TypeExpr {
        TypeExpr {
            name: name.into(),
            optional: false,
            span: sp(),
        }
    }
    /// An optional type expression `?name`.
    fn te_opt(name: &str) -> TypeExpr {
        TypeExpr {
            name: name.into(),
            optional: true,
            span: sp(),
        }
    }
    fn null_lit() -> Expr {
        Expr::Null { span: sp() }
    }
    fn orelse(lhs: Expr, rhs: Expr) -> Expr {
        Expr::Orelse {
            lhs: Box::new(lhs),
            rhs: Box::new(rhs),
            span: sp(),
        }
    }
    fn unwrap(e: Expr) -> Expr {
        Expr::Unwrap {
            expr: Box::new(e),
            span: sp(),
        }
    }
    fn param_opt(name: &str, inner: &str) -> Param {
        Param {
            name: name.into(),
            ty: te_opt(inner),
            span: sp(),
        }
    }
    /// `var name: ?inner = value;`
    fn let_var_opt(name: &str, inner: &str, value: Expr) -> Stmt {
        Stmt::Let {
            is_const: false,
            name: name.into(),
            ty: te_opt(inner),
            value,
            span: sp(),
        }
    }
    /// A struct with a single optional field `field: ?inner`.
    fn struct_item_optfield(name: &str, field: &str, inner: &str) -> Item {
        Item::Struct(StructDecl {
            is_pub: false,
            name: name.into(),
            fields: vec![FieldDecl {
                name: field.into(),
                ty: te_opt(inner),
                span: sp(),
            }],
            methods: Vec::new(),
            span: sp(),
        })
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
    fn raw_func(name: &str, params: Vec<Param>, ret: &str, body: Vec<Stmt>) -> Func {
        Func {
            is_pub: false,
            name: name.into(),
            params,
            ret: te(ret),
            body: block(body),
            span: sp(),
        }
    }
    fn func(name: &str, params: Vec<Param>, ret: &str, body: Vec<Stmt>) -> Item {
        Item::Func(raw_func(name, params, ret, body))
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
    fn field_decls(fields: Vec<(&str, &str)>) -> Vec<FieldDecl> {
        fields
            .into_iter()
            .map(|(n, t)| FieldDecl {
                name: n.into(),
                ty: te(t),
                span: sp(),
            })
            .collect()
    }
    fn struct_item(name: &str, fields: Vec<(&str, &str)>) -> Item {
        Item::Struct(StructDecl {
            is_pub: false,
            name: name.into(),
            fields: field_decls(fields),
            methods: Vec::new(),
            span: sp(),
        })
    }
    /// A struct with both fields and methods / associated functions (v0.113).
    fn struct_item_m(name: &str, fields: Vec<(&str, &str)>, methods: Vec<Func>) -> Item {
        Item::Struct(StructDecl {
            is_pub: false,
            name: name.into(),
            fields: field_decls(fields),
            methods,
            span: sp(),
        })
    }
    fn method_call(receiver: Expr, method: &str, args: Vec<Expr>) -> Expr {
        Expr::MethodCall {
            receiver: Box::new(receiver),
            method: method.into(),
            args,
            span: sp(),
        }
    }
    fn struct_lit(name: &str, inits: Vec<(&str, Expr)>) -> Expr {
        Expr::StructLit {
            name: name.into(),
            fields: inits
                .into_iter()
                .map(|(n, v)| FieldInit {
                    name: n.into(),
                    value: v,
                    span: sp(),
                })
                .collect(),
            span: sp(),
        }
    }
    fn field(base: Expr, f: &str) -> Expr {
        Expr::Field {
            base: Box::new(base),
            field: f.into(),
            span: sp(),
        }
    }
    fn field_assign(place: Expr, value: Expr) -> Stmt {
        Stmt::FieldAssign {
            place,
            value,
            span: sp(),
        }
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
            Ok(_) => vec![],
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

    // ---- struct tests (v0.112) -------------------------------------------

    #[test]
    fn good_struct_program_passes_and_returns_table() {
        // const Point = struct { x: i32, y: i32 };
        // fn make() Point { return Point{ .x = 1, .y = 2 }; }
        // fn getx(p: Point) i32 { return p.x; }
        // fn main() void {
        //     var p: Point = make();
        //     p.x = 5;
        //     print(p.x);
        //     print(getx(p));
        // }
        let items = vec![
            struct_item("Point", vec![("x", "i32"), ("y", "i32")]),
            func(
                "make",
                vec![],
                "Point",
                vec![ret(Some(struct_lit(
                    "Point",
                    vec![("x", int(1)), ("y", int(2))],
                )))],
            ),
            func(
                "getx",
                vec![param("p", "Point")],
                "i32",
                vec![ret(Some(field(ident("p"), "x")))],
            ),
            func(
                "main",
                vec![],
                "void",
                vec![
                    let_var("p", "Point", call("make", vec![])),
                    field_assign(field(ident("p"), "x"), int(5)),
                    Stmt::Expr(call("print", vec![field(ident("p"), "x")])),
                    Stmt::Expr(call("print", vec![call("getx", vec![ident("p")])])),
                ],
            ),
        ];
        let m = Module { items };
        let table = check(&m).expect("struct program should type-check");
        let id = table.id_of("Point").expect("Point should be registered");
        let info = table.get(id);
        assert_eq!(info.name, "Point");
        assert_eq!(
            info.fields,
            vec![
                ("x".to_string(), Type::I32),
                ("y".to_string(), Type::I32),
            ]
        );
    }

    #[test]
    fn unknown_field_access_is_e0166() {
        // const Point = struct { x: i32 };
        // fn f(p: Point) i32 { return p.y; }
        let items = vec![
            struct_item("Point", vec![("x", "i32")]),
            func(
                "f",
                vec![param("p", "Point")],
                "i32",
                vec![ret(Some(field(ident("p"), "y")))],
            ),
        ];
        assert!(codes(items).contains(&"E0166"));
    }

    #[test]
    fn missing_field_in_literal_is_e0164() {
        // const Point = struct { x: i32, y: i32 };
        // fn f() Point { return Point{ .x = 1 }; }   // missing y
        let items = vec![
            struct_item("Point", vec![("x", "i32"), ("y", "i32")]),
            func(
                "f",
                vec![],
                "Point",
                vec![ret(Some(struct_lit("Point", vec![("x", int(1))])))],
            ),
        ];
        assert!(codes(items).contains(&"E0164"));
    }

    #[test]
    fn extra_field_in_literal_is_e0164() {
        // const Point = struct { x: i32 };
        // fn f() Point { return Point{ .x = 1, .z = 2 }; }
        let items = vec![
            struct_item("Point", vec![("x", "i32")]),
            func(
                "f",
                vec![],
                "Point",
                vec![ret(Some(struct_lit(
                    "Point",
                    vec![("x", int(1)), ("z", int(2))],
                )))],
            ),
        ];
        assert!(codes(items).contains(&"E0164"));
    }

    #[test]
    fn type_mismatch_in_field_is_e0110() {
        // const Point = struct { x: i32 };
        // fn f() Point { return Point{ .x = true }; }
        let items = vec![
            struct_item("Point", vec![("x", "i32")]),
            func(
                "f",
                vec![],
                "Point",
                vec![ret(Some(struct_lit("Point", vec![("x", boolean(true))])))],
            ),
        ];
        assert!(codes(items).contains(&"E0110"));
    }

    #[test]
    fn forward_struct_ref_is_e0160() {
        // const A = struct { b: B };   const B = struct { x: i32 };
        let items = vec![
            struct_item("A", vec![("b", "B")]),
            struct_item("B", vec![("x", "i32")]),
        ];
        assert!(codes(items).contains(&"E0160"));
    }

    #[test]
    fn back_reference_between_structs_is_ok() {
        // const B = struct { x: i32 };   const A = struct { b: B };
        let items = vec![
            struct_item("B", vec![("x", "i32")]),
            struct_item("A", vec![("b", "B")]),
        ];
        assert_eq!(codes(items), Vec::<&str>::new());
    }

    #[test]
    fn unknown_field_type_is_e0161() {
        // const A = struct { x: Nope };
        let items = vec![struct_item("A", vec![("x", "Nope")])];
        assert!(codes(items).contains(&"E0161"));
    }

    #[test]
    fn duplicate_field_decl_is_e0162() {
        // const A = struct { x: i32, x: i32 };
        let items = vec![struct_item("A", vec![("x", "i32"), ("x", "i32")])];
        assert!(codes(items).contains(&"E0162"));
    }

    #[test]
    fn literal_of_non_struct_is_e0163() {
        // fn f() i32 { return Nope{ .x = 1 }; }
        let items = vec![func(
            "f",
            vec![],
            "i32",
            vec![ret(Some(struct_lit("Nope", vec![("x", int(1))])))],
        )];
        assert!(codes(items).contains(&"E0163"));
    }

    #[test]
    fn field_access_on_non_struct_is_e0165() {
        // fn f(x: i32) i32 { return x.foo; }
        let items = vec![func(
            "f",
            vec![param("x", "i32")],
            "i32",
            vec![ret(Some(field(ident("x"), "foo")))],
        )];
        assert!(codes(items).contains(&"E0165"));
    }

    #[test]
    fn assign_through_immutable_field_is_e0167() {
        // const Point = struct { x: i32 };
        // fn f(p: Point) void { p.x = 5; }   // p is a parameter (immutable)
        let items = vec![
            struct_item("Point", vec![("x", "i32")]),
            func(
                "f",
                vec![param("p", "Point")],
                "void",
                vec![field_assign(field(ident("p"), "x"), int(5))],
            ),
        ];
        assert!(codes(items).contains(&"E0167"));
    }

    #[test]
    fn struct_eq_struct_is_e0168() {
        // const Point = struct { x: i32 };
        // fn f(p: Point, q: Point) bool { return p == q; }
        let items = vec![
            struct_item("Point", vec![("x", "i32")]),
            func(
                "f",
                vec![param("p", "Point"), param("q", "Point")],
                "bool",
                vec![ret(Some(bin(BinOp::Eq, ident("p"), ident("q"))))],
            ),
        ];
        assert!(codes(items).contains(&"E0168"));
    }

    #[test]
    fn field_assign_type_mismatch_is_e0110() {
        // const Point = struct { x: i32 };
        // fn f() void { var p: Point = Point{ .x = 1 }; p.x = true; }
        let items = vec![
            struct_item("Point", vec![("x", "i32")]),
            func(
                "f",
                vec![],
                "void",
                vec![
                    let_var("p", "Point", struct_lit("Point", vec![("x", int(1))])),
                    field_assign(field(ident("p"), "x"), boolean(true)),
                ],
            ),
        ];
        assert!(codes(items).contains(&"E0110"));
    }

    #[test]
    fn nested_struct_field_access_ok() {
        // const Inner = struct { v: i32 };
        // const Outer = struct { inner: Inner };
        // fn get(o: Outer) i32 { return o.inner.v; }
        let items = vec![
            struct_item("Inner", vec![("v", "i32")]),
            struct_item("Outer", vec![("inner", "Inner")]),
            func(
                "get",
                vec![param("o", "Outer")],
                "i32",
                vec![ret(Some(field(field(ident("o"), "inner"), "v")))],
            ),
        ];
        assert_eq!(codes(items), Vec::<&str>::new());
    }

    // ---- method / associated-function tests (v0.113) ---------------------

    /// The canonical `Counter` struct from SPEC §10:
    /// ```text
    /// const Counter = struct {
    ///     n: i32,
    ///     fn get(self: Counter) i32 { return self.n; }
    ///     fn bumped(self: Counter, by: i32) Counter { return Counter{ .n = self.n + by }; }
    ///     fn zero() Counter { return Counter{ .n = 0 }; }   // associated (no self)
    /// };
    /// ```
    fn counter_struct() -> Item {
        let get = raw_func(
            "get",
            vec![param("self", "Counter")],
            "i32",
            vec![ret(Some(field(ident("self"), "n")))],
        );
        let bumped = raw_func(
            "bumped",
            vec![param("self", "Counter"), param("by", "i32")],
            "Counter",
            vec![ret(Some(struct_lit(
                "Counter",
                vec![(
                    "n",
                    bin(BinOp::Add, field(ident("self"), "n"), ident("by")),
                )],
            )))],
        );
        let zero = raw_func(
            "zero",
            vec![],
            "Counter",
            vec![ret(Some(struct_lit("Counter", vec![("n", int(0))])))],
        );
        struct_item_m("Counter", vec![("n", "i32")], vec![get, bumped, zero])
    }

    #[test]
    fn method_and_assoc_calls_typecheck_with_result_types() {
        // fn main() void {
        //     var c: Counter = Counter.zero();   // associated fn  -> Counter
        //     var d: Counter = c.bumped(5);      // method + arg   -> Counter
        //     var r: i32 = d.get();              // method         -> i32
        //     print(r);
        // }
        // The `var T = ...` declarations pin each call's result type, so a
        // clean run also proves the inferred result types.
        let items = vec![
            counter_struct(),
            func(
                "main",
                vec![],
                "void",
                vec![
                    let_var("c", "Counter", method_call(ident("Counter"), "zero", vec![])),
                    let_var("d", "Counter", method_call(ident("c"), "bumped", vec![int(5)])),
                    let_var("r", "i32", method_call(ident("d"), "get", vec![])),
                    Stmt::Expr(call("print", vec![ident("r")])),
                ],
            ),
        ];
        assert_eq!(codes(items), Vec::<&str>::new());
    }

    #[test]
    fn associated_and_explicit_self_static_calls_ok() {
        // fn main() void {
        //     var c: Counter = Counter.zero();   // static form
        //     var r: i32 = Counter.get(c);       // explicit-self static form
        //     print(r);
        // }
        let items = vec![
            counter_struct(),
            func(
                "main",
                vec![],
                "void",
                vec![
                    let_var("c", "Counter", method_call(ident("Counter"), "zero", vec![])),
                    let_var(
                        "r",
                        "i32",
                        method_call(ident("Counter"), "get", vec![ident("c")]),
                    ),
                    Stmt::Expr(call("print", vec![ident("r")])),
                ],
            ),
        ];
        assert_eq!(codes(items), Vec::<&str>::new());
    }

    #[test]
    fn unknown_method_is_e0170() {
        // fn f(c: Counter) i32 { return c.nope(); }
        let items = vec![
            counter_struct(),
            func(
                "f",
                vec![param("c", "Counter")],
                "i32",
                vec![ret(Some(method_call(ident("c"), "nope", vec![])))],
            ),
        ];
        assert!(codes(items).contains(&"E0170"));
    }

    #[test]
    fn unknown_static_method_is_e0170() {
        // fn f() void { Counter.nope(); }
        let items = vec![
            counter_struct(),
            func(
                "f",
                vec![],
                "void",
                vec![Stmt::Expr(method_call(ident("Counter"), "nope", vec![]))],
            ),
        ];
        assert!(codes(items).contains(&"E0170"));
    }

    #[test]
    fn method_on_non_struct_value_is_e0170() {
        // fn f(x: i32) i32 { return x.foo(); }
        let items = vec![func(
            "f",
            vec![param("x", "i32")],
            "i32",
            vec![ret(Some(method_call(ident("x"), "foo", vec![])))],
        )];
        assert!(codes(items).contains(&"E0170"));
    }

    #[test]
    fn method_arity_mismatch_is_e0171() {
        // fn f(c: Counter) Counter { return c.bumped(); }   // bumped needs 1 arg
        let items = vec![
            counter_struct(),
            func(
                "f",
                vec![param("c", "Counter")],
                "Counter",
                vec![ret(Some(method_call(ident("c"), "bumped", vec![])))],
            ),
        ];
        assert!(codes(items).contains(&"E0171"));
    }

    #[test]
    fn assoc_fn_called_on_value_is_e0172() {
        // fn f(c: Counter) Counter { return c.zero(); }   // zero is associated
        let items = vec![
            counter_struct(),
            func(
                "f",
                vec![param("c", "Counter")],
                "Counter",
                vec![ret(Some(method_call(ident("c"), "zero", vec![])))],
            ),
        ];
        assert!(codes(items).contains(&"E0172"));
    }

    #[test]
    fn method_called_statically_without_self_is_e0172() {
        // fn f() i32 { return Counter.get(); }   // get is a method, no self passed
        let items = vec![
            counter_struct(),
            func(
                "f",
                vec![],
                "i32",
                vec![ret(Some(method_call(ident("Counter"), "get", vec![])))],
            ),
        ];
        assert!(codes(items).contains(&"E0172"));
    }

    #[test]
    fn method_arg_type_mismatch_is_e0110() {
        // fn f(c: Counter) Counter { return c.bumped(true); }   // bumped wants i32
        let items = vec![
            counter_struct(),
            func(
                "f",
                vec![param("c", "Counter")],
                "Counter",
                vec![ret(Some(method_call(
                    ident("c"),
                    "bumped",
                    vec![boolean(true)],
                )))],
            ),
        ];
        assert!(codes(items).contains(&"E0110"));
    }

    #[test]
    fn method_body_return_type_is_checked_e0110() {
        // const Counter = struct { n: i32, fn bad(self: Counter) bool { return self.n; } };
        // The body returns `self.n` (i32) where `bool` is declared.
        let bad = raw_func(
            "bad",
            vec![param("self", "Counter")],
            "bool",
            vec![ret(Some(field(ident("self"), "n")))],
        );
        let items = vec![struct_item_m("Counter", vec![("n", "i32")], vec![bad])];
        assert!(codes(items).contains(&"E0110"));
    }

    #[test]
    fn method_body_self_field_access_ok() {
        // A method body type-checks `self.<field>` against the enclosing struct.
        // const Counter = struct { n: i32, fn get(self: Counter) i32 { return self.n; } };
        let get = raw_func(
            "get",
            vec![param("self", "Counter")],
            "i32",
            vec![ret(Some(field(ident("self"), "n")))],
        );
        let items = vec![struct_item_m("Counter", vec![("n", "i32")], vec![get])];
        assert_eq!(codes(items), Vec::<&str>::new());
    }

    // ---- optional tests (v0.114) -----------------------------------------

    #[test]
    fn optional_null_and_coercion_ok() {
        // fn main() void { var x: ?i32 = null; x = 5; }
        // `null` adopts `?i32`; the bare `5` coerces `i32 -> ?i32` on assign.
        let items = vec![func(
            "main",
            vec![],
            "void",
            vec![
                let_var_opt("x", "i32", null_lit()),
                assign("x", int(5)),
            ],
        )];
        assert_eq!(codes(items), Vec::<&str>::new());
    }

    #[test]
    fn optional_value_initializer_coerces_ok() {
        // fn main() void { var x: ?i32 = 7; }  (T coerces to ?T)
        let items = vec![func(
            "main",
            vec![],
            "void",
            vec![let_var_opt("x", "i32", int(7))],
        )];
        assert_eq!(codes(items), Vec::<&str>::new());
    }

    #[test]
    fn orelse_yields_inner_type_ok() {
        // fn f(opt: ?i32) void { var v: i32 = opt orelse 0; print(v); }
        let items = vec![func(
            "f",
            vec![param_opt("opt", "i32")],
            "void",
            vec![
                let_var("v", "i32", orelse(ident("opt"), int(0))),
                Stmt::Expr(call("print", vec![ident("v")])),
            ],
        )];
        assert_eq!(codes(items), Vec::<&str>::new());
    }

    #[test]
    fn unwrap_yields_inner_type_ok() {
        // fn f(opt: ?i32) void { var v: i32 = opt.?; print(v); }
        let items = vec![func(
            "f",
            vec![param_opt("opt", "i32")],
            "void",
            vec![
                let_var("v", "i32", unwrap(ident("opt"))),
                Stmt::Expr(call("print", vec![ident("v")])),
            ],
        )];
        assert_eq!(codes(items), Vec::<&str>::new());
    }

    #[test]
    fn orelse_alternative_type_mismatch_is_e0110() {
        // fn f(opt: ?i32) void { var v: i32 = opt orelse true; }  // alt is bool
        let items = vec![func(
            "f",
            vec![param_opt("opt", "i32")],
            "void",
            vec![let_var("v", "i32", orelse(ident("opt"), boolean(true)))],
        )];
        assert!(codes(items).contains(&"E0110"));
    }

    #[test]
    fn bare_null_without_expected_optional_is_e0180() {
        // fn main() void { var x: i32 = null; }  // i32 is not optional
        let items = vec![func(
            "main",
            vec![],
            "void",
            vec![let_var("x", "i32", null_lit())],
        )];
        assert!(codes(items).contains(&"E0180"));
    }

    #[test]
    fn orelse_on_non_optional_is_e0181() {
        // fn f(x: i32) void { var v: i32 = x orelse 0; }  // x is not optional
        let items = vec![func(
            "f",
            vec![param("x", "i32")],
            "void",
            vec![let_var("v", "i32", orelse(ident("x"), int(0)))],
        )];
        assert!(codes(items).contains(&"E0181"));
    }

    #[test]
    fn unwrap_on_non_optional_is_e0182() {
        // fn f(x: i32) void { var v: i32 = x.?; }  // x is not optional
        let items = vec![func(
            "f",
            vec![param("x", "i32")],
            "void",
            vec![let_var("v", "i32", unwrap(ident("x")))],
        )];
        assert!(codes(items).contains(&"E0182"));
    }

    #[test]
    fn optional_struct_field_ok_and_interned() {
        // const Box = struct { val: ?i32 };
        // fn mk() Box { return Box{ .val = null }; }       // null in field
        // fn set() Box { return Box{ .val = 9 }; }         // T coerces in field
        // fn get(b: Box) i32 { return b.val orelse 0; }    // field is ?i32
        let items = vec![
            struct_item_optfield("Box", "val", "i32"),
            func(
                "mk",
                vec![],
                "Box",
                vec![ret(Some(struct_lit("Box", vec![("val", null_lit())])))],
            ),
            func(
                "set",
                vec![],
                "Box",
                vec![ret(Some(struct_lit("Box", vec![("val", int(9))])))],
            ),
            func(
                "get",
                vec![param("b", "Box")],
                "i32",
                vec![ret(Some(orelse(field(ident("b"), "val"), int(0))))],
            ),
        ];
        let m = Module { items };
        let table = check(&m).expect("optional-field program should type-check");
        // The `?i32` field interned exactly one optional whose inner is `i32`.
        let opts: Vec<Type> = table.optionals().map(|(_, t)| t).collect();
        assert_eq!(opts, vec![Type::I32]);
        let id = table.id_of("Box").unwrap();
        assert_eq!(
            table.get(id).fields,
            vec![("val".to_string(), Type::Optional(0))]
        );
    }

    #[test]
    fn return_value_coerces_to_optional_ok() {
        // fn f() ?i32 { return 3; }   // T coerces to ?T on return
        // fn g() ?i32 { return null; }
        let items = vec![
            Item::Func(Func {
                is_pub: false,
                name: "f".into(),
                params: vec![],
                ret: te_opt("i32"),
                body: block(vec![ret(Some(int(3)))]),
                span: sp(),
            }),
            Item::Func(Func {
                is_pub: false,
                name: "g".into(),
                params: vec![],
                ret: te_opt("i32"),
                body: block(vec![ret(Some(null_lit()))]),
                span: sp(),
            }),
        ];
        assert_eq!(codes(items), Vec::<&str>::new());
    }

    #[test]
    fn optional_arg_coercion_to_param_ok() {
        // fn takes(o: ?i32) void {}
        // fn main() void { takes(5); takes(null); }
        let items = vec![
            func("takes", vec![param_opt("o", "i32")], "void", vec![]),
            func(
                "main",
                vec![],
                "void",
                vec![
                    Stmt::Expr(call("takes", vec![int(5)])),
                    Stmt::Expr(call("takes", vec![null_lit()])),
                ],
            ),
        ];
        assert_eq!(codes(items), Vec::<&str>::new());
    }
}
