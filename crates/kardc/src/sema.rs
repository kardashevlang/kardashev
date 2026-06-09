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
//! - `E0101` — redefining a builtin (`print` / `expect` / `c_allocator` /
//!   `alloc` / `free`).
//! - `E0110` — a type mismatch (the general sema type-error code).
//! - `E0120` — `break` / `continue` outside a loop.
//! - `E0121` — a labeled `break :name` / `continue :name` whose `name` does not
//!   match any enclosing loop's label (SPEC §40.1).
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
//! - `E0190` — `try` whose operand is not an error union, or whose enclosing
//!   function does not return an error union (`!T`).
//! - `E0191` — `try` used outside a statement-level position (initializer,
//!   `return`, or expression statement).
//! - `E0192` — `catch` whose left operand is not an error union (`!T`).
//! - `E0193` — an `error.Name` value with no expected error-union (`!T`) type.
//! - `E0210` — a non-exhaustive `switch` on an enum (a variant is uncovered and
//!   there is no `else` arm).
//! - `E0211` — a duplicate enum variant in a declaration, or a duplicate
//!   `switch` label across arms.
//! - `E0212` — an enum variant name (`Enum.V` / `.V` / a `switch` label) that
//!   the enum does not declare, or a range label `lo..hi` on a `switch` whose
//!   scrutinee is not an integer type (a range is only a valid label for an
//!   integer `switch`, SPEC §39.1).
//! - `E0213` — a `switch` scrutinee whose type is neither an enum nor an
//!   integer.
//! - `E0214` — a non-exhaustive `switch` on an integer type (no `else` arm).
//! - `E0215` — an unqualified enum literal `.V` with no expected enum type at
//!   its position.
//! - `E0220` — indexing (`a[i]`) a value whose type is not a fixed-size array.
//! - `E0221` — an array literal whose element count does not equal its length.
//! - `E0223` — an index-assignment (`a[i] = e`) whose base is not a mutable
//!   `var` array (a `const`/parameter root, or a non-array base).
//! - `E0224` — a fixed-size array type `[N]T` with a negative (or otherwise
//!   absurd) length `N`.
//! - `E0230` — `.*` (dereference) of a value whose type is not a pointer (`*T`).
//! - `E0231` — `&` (address-of) applied to a non-lvalue expression.
//! - `E0232` — slicing (`a[lo..hi]`) a value that is neither a (addressable)
//!   array nor a slice.
//! - `E0241` — `alloc`'s second argument is not an identifier naming a type
//!   (a builtin, struct or enum) (SPEC §16.1).
//! - `E0242` — `free`'s second argument is not a slice (`[]T`) (SPEC §16.1).
//! - `E0250` — a `comptime` parameter whose annotation is not `type` (SPEC §17.2).
//! - `E0251` — a type argument to a generic-function call that does not name a
//!   concrete type (SPEC §17.2).
//! - `E0252` — too few type arguments for a generic-function call (SPEC §17.2).
//! - `E0260` — an un-annotated `var`/`const` whose initializer's type cannot be
//!   inferred without context (a bare `null` / `error.X` / `.Variant`) (§18.2).
//! - `E0270` — a tagged-union construction `Name{ … }` that does not have
//!   exactly one variant initializer (SPEC §20.2).
//! - `E0271` — a union construction field, or a union `switch` label, that does
//!   not name a variant of the union (SPEC §20.2).
//! - `E0272` — a payload capture (`|x|`) on a `switch` whose scrutinee is not a
//!   tagged union (an enum / integer / otherwise un-switchable type) (SPEC §20.2).
//! - `E0280` — an optional `if` capture (`if (cond) |v| { … }`) whose condition
//!   is not an optional (`?T`) (SPEC §21.1).
//! - `E0310` — a type-returning function (`fn Name(comptime T: type) type`) that
//!   is not a valid type-constructor: it lacks exactly one `comptime` type
//!   parameter, or its body is not a single `return struct { … };`; or a
//!   `struct { … }` type value used outside such a body (SPEC §25.2).
//! - `E0311` — instantiating in a type alias (`const Alias = Name(C);`) a callee
//!   that is not a type-constructor, or a type-constructor argument that does not
//!   name a concrete type (SPEC §25.2).

use std::collections::{HashMap, HashSet};

use crate::ast::{
    ArraySize, BinOp, Block, Expr, FieldDecl, FieldInit, Func, Item, Module, Param, Stmt,
    StructDecl, SwitchArm, TestBlock, TypeExpr, UnOp,
};
use crate::const_eval::{self, ConstVal};
use crate::diag::Diagnostic;
use crate::span::Span;
use crate::types::{ComptimeArg, StructTable, Type};

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
/// leading `self` (whose type is the enclosing struct, or a pointer to it for a
/// pointer receiver) when `is_method` is true. `is_method` records whether the
/// first parameter is named `self`, which decides whether the function may be
/// invoked on a value (`v.m(..)`) or only statically (`Name.f(..)`).
///
/// `is_ptr_receiver` records whether that `self` is a **pointer receiver**
/// (`self: *Struct` / `self: *Self`, SPEC §30): such a method mutates the
/// receiver in place, so `params[0]` is `Ptr(Struct)` and a value-receiver call
/// `obj.m(..)` auto-refs `&obj` (the receiver must be an addressable lvalue). A
/// value receiver (`self: Struct` / `self: Self`) keeps `is_ptr_receiver` false
/// and is unchanged from pre-v0.134.
#[derive(Clone)]
struct StructFn {
    params: Vec<Type>,
    ret: Type,
    is_method: bool,
    is_ptr_receiver: bool,
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
    /// Stack of enclosing loop labels (v0.147, SPEC §40.1), innermost at the
    /// back, parallel to `loop_depth`. Each entry is the loop's `label`
    /// (`None` for an unlabeled loop). A labeled `break`/`continue :name` must
    /// find `Some(name)` somewhere in this stack (else `E0121`).
    loop_labels: Vec<Option<String>>,
    /// Return type of the function/test currently being checked.
    ret_type: Type,
    /// The active type-parameter substitution while checking a generic
    /// function's instantiated body (v0.120, SPEC §17.2). Empty during the
    /// normal pass; when non-empty, `resolve_type` / `resolve_type_opt` map a
    /// bound type-parameter name to its concrete [`Type`].
    subst: HashMap<String, Type>,
    /// The active comptime **value**-parameter substitution while checking a
    /// generic function's instantiated body (v0.128, SPEC §24.2). Empty during
    /// the normal pass; when non-empty, an `ArraySize::Param(n)` resolves to the
    /// bound `i64` length and a reference to `n` folds to that value in a
    /// constant context.
    value_subst: HashMap<String, i64>,
    /// Generic function definitions (any `comptime`-typed parameter), keyed by
    /// name. Stored as full ASTs because a generic function is type-checked per
    /// concrete instantiation at its call sites rather than in the normal body
    /// pass (SPEC §17.2 / §24.2).
    generics: HashMap<String, Func>,
    /// Type-constructors (v0.129, SPEC §25): a `fn Name(comptime T: type) type`
    /// whose body returns a `struct { … }` type value, keyed by name. Stored
    /// whole and instantiated when a `const Alias = Name(C);` mentions it; never
    /// type-checked or emitted as an ordinary function.
    type_ctors: HashMap<String, Func>,
    /// Type aliases (v0.129): a top-level `const Alias = Name(C);` binds `Alias`
    /// to the monomorphised `Type::Struct(id)` produced by instantiating the
    /// type-constructor `Name` at the concrete type `C`. Consulted by
    /// `resolve_base`, so an alias is usable in type position (`var x: Alias`),
    /// as a struct-literal name (`Alias{ … }`), and for field access.
    type_aliases: HashMap<String, Type>,
    /// Generic-struct method bodies awaiting type-checking (v0.138): registered
    /// in Pass 0d but checked AFTER Pass 2, so a method body may reference
    /// top-level `const`s and free functions. Each entry is
    /// `(constructor name, instance struct id, substitution)`.
    pending_ctor_methods: Vec<(String, u32, HashMap<String, Type>)>,
    /// Named error sets (v0.139, SPEC §34.2): each declared `const Name =
    /// error{ A, B };` maps `Name` → its member names (declaration order,
    /// duplicates removed). Used to validate `Set!T` set names (E0331) and
    /// `error.X` membership against a named-set target (E0330). The members are
    /// *also* interned as global error names so each gets a stable code.
    error_sets: HashMap<String, Vec<String>>,
    /// The named error set of the current function/test's return type (v0.139):
    /// `Some(set)` when the return type was written `Set!T`, else `None` (a
    /// global `!T` or a non-error return). A `return error.X;` checks `X`'s
    /// membership against this set.
    ret_error_set: Option<String>,
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
            loop_labels: Vec::new(),
            ret_type: Type::Void,
            subst: HashMap::new(),
            value_subst: HashMap::new(),
            generics: HashMap::new(),
            type_ctors: HashMap::new(),
            type_aliases: HashMap::new(),
            pending_ctor_methods: Vec::new(),
            error_sets: HashMap::new(),
            ret_error_set: None,
        }
    }

    /// The constant environment for compile-time evaluation: the top-level
    /// consts plus any comptime **value** parameters bound in the active
    /// instantiation (each folded to an `i64`), so that a value-parameter
    /// reference evaluates to its bound value (SPEC §24.2). During the normal
    /// pass `value_subst` is empty, so this is exactly the top-level consts.
    fn const_env(&self) -> HashMap<String, ConstVal> {
        if self.value_subst.is_empty() {
            return self.consts.clone();
        }
        let mut env = self.consts.clone();
        for (name, &v) in &self.value_subst {
            env.insert(name.clone(), ConstVal::Int(v));
        }
        env
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
            Type::ErrorUnion(id) => {
                format!("!{}", self.type_name(self.structs.error_union_payload(id)))
            }
            Type::Enum(id) => self.structs.enum_get(id).name.clone(),
            Type::Union(id) => self.structs.union_get(id).name.clone(),
            Type::Array(id) => format!(
                "[{}]{}",
                self.structs.array_len(id),
                self.type_name(self.structs.array_elem(id))
            ),
            Type::Ptr(id) => format!("*{}", self.type_name(self.structs.ptr_pointee(id))),
            Type::Slice(id) => format!("[]{}", self.type_name(self.structs.slice_elem(id))),
            other => other.name().to_string(),
        }
    }

    // ---- top-level driving ------------------------------------------------

    fn check_module(&mut self, m: &Module) {
        // Pass 0 (enums): intern every enum and record its variants (SPEC §13.2).
        // Enums have no dependencies, so declaration order is irrelevant; doing
        // this first lets any signature, const or local mention an enum type
        // (resolved via `resolve_type_opt`). A variant name repeated within one
        // enum is `E0211`.
        for item in &m.items {
            if let Item::Enum(e) = item {
                let id = self.structs.intern_enum(&e.name);
                let mut variants: Vec<String> = Vec::new();
                let mut values: Vec<i64> = Vec::new();
                let mut seen: HashSet<String> = HashSet::new();
                // Resolve each variant's integer value (v0.143, SPEC §37.1): an
                // explicit `= N` sets the running counter to `N` and is used;
                // a variant without one takes the counter's current value. After
                // each variant the counter advances to `used + 1` — so the first
                // un-annotated variant is 0 and the sequence auto-increments (the
                // C rule). `values` stays parallel to `variants`.
                let mut counter: i64 = 0;
                for v in &e.variants {
                    let used = v.value.unwrap_or(counter);
                    counter = used.wrapping_add(1);
                    if !seen.insert(v.name.clone()) {
                        let msg =
                            format!("duplicate variant `{}` in enum `{}`", v.name, e.name);
                        self.error(e.span, "E0211", msg);
                        continue;
                    }
                    variants.push(v.name.clone());
                    values.push(used);
                }
                self.structs.set_enum_variants(id, variants, values);
            }
        }

        // Pass 0 (error sets, v0.139, SPEC §34.2): register every named error
        // set `const Name = error{ A, B };`. Each member is *also* interned as a
        // global error name (so `error.A` keeps a stable code, exactly as a bare
        // `error.A` literal would, §12) and recorded as belonging to the set, so
        // a `Set!T` set name (E0331) and an `error.X` membership (E0330) can be
        // checked later. A member repeated within one set is `E0331`. Error sets
        // have no dependencies, so source order is irrelevant; doing this before
        // signatures/bodies lets any later `Set!T` reference resolve.
        for item in &m.items {
            if let Item::ErrorSet(es) = item {
                let mut members: Vec<String> = Vec::new();
                let mut seen: HashSet<String> = HashSet::new();
                for member in &es.members {
                    if !seen.insert(member.clone()) {
                        let msg =
                            format!("duplicate member `{}` in error set `{}`", member, es.name);
                        self.error(es.span, "E0331", msg);
                        continue;
                    }
                    // Reuse the global error-code registration path so this
                    // member behaves identically to a bare `error.<member>`.
                    self.structs.intern_error(member);
                    members.push(member.clone());
                }
                self.error_sets.insert(es.name.clone(), members);
            }
        }

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

        // Pass 0c (tagged unions, v0.124, SPEC §20.2). Intern every union name
        // first so a variant payload may reference any union (and so
        // `resolve_base` recognises union type names from here on) — this also
        // lets later signatures, consts and locals mention a union type. Then
        // resolve each variant's payload type. A variant name repeated within
        // one union is `E0211`.
        for item in &m.items {
            if let Item::Union(u) = item {
                self.structs.intern_union(&u.name);
            }
        }
        for item in &m.items {
            if let Item::Union(u) = item {
                let id = match self.structs.union_id_of(&u.name) {
                    Some(id) => id,
                    None => continue, // unreachable: interned just above
                };
                let mut variants: Vec<(String, Type)> = Vec::new();
                let mut seen: HashSet<String> = HashSet::new();
                for v in &u.variants {
                    if !seen.insert(v.name.clone()) {
                        let msg =
                            format!("duplicate variant `{}` in union `{}`", v.name, u.name);
                        self.error(v.span, "E0211", msg);
                        continue;
                    }
                    // A variant payload type resolves to a builtin / struct /
                    // enum / union / composite (`resolve_type` emits `E0100` for
                    // an unknown name); fall back to `i64` so downstream
                    // construction / capture checks still see a usable type.
                    let pty = self.resolve_type(&v.payload).unwrap_or(Type::I64);
                    variants.push((v.name.clone(), pty));
                }
                self.structs.set_union_variants(id, variants);
            }
        }

        // Pass 0d (v0.129, SPEC §25): collect type-constructors and instantiate
        // type-alias consts (`const Alias = Name(C);`) BEFORE function signatures
        // (Pass 1), so an alias type used in a signature (`fn f(p: Alias)`)
        // resolves. (Aliases only depend on type-constructors + already-interned
        // structs/enums/unions, all available by now.)
        for item in &m.items {
            if let Item::Func(f) = item {
                if is_type_ctor(f) {
                    self.collect_type_ctor(f);
                }
            }
        }
        for item in &m.items {
            if let Item::Const(c) = item {
                if let Expr::Call { callee, args, span } = &c.value {
                    if self.type_ctors.contains_key(callee) {
                        self.instantiate_alias(&c.name, callee, args, *span);
                    }
                }
            }
        }

        // Pass 1: collect function signatures so calls can forward-reference.
        // A function with any `comptime`-typed parameter is *generic* (SPEC
        // §17): its parameter/return/body types may use type-parameter names, so
        // it is neither resolved nor body-checked in the normal pass. Instead it
        // is stored whole and type-checked per concrete instantiation discovered
        // at each call site (`check_generic_call`).
        for item in &m.items {
            if let Item::Func(f) = item {
                if matches!(
                    f.name.as_str(),
                    "print" | "expect" | "c_allocator" | "alloc" | "free"
                ) {
                    self.error(
                        f.span,
                        "E0101",
                        format!("cannot redefine builtin `{}`", f.name),
                    );
                }
                // A type-returning function `fn Name(comptime T: type) type` is a
                // *type-constructor* (v0.129, SPEC §25). It is not a value
                // function nor a generic value function: it is validated and
                // recorded here, instantiated when a `const Alias = Name(C);`
                // mentions it (Pass 2), and never checked or emitted as an
                // ordinary function. (It must precede the `is_generic` branch
                // below, since a type-constructor also has a `comptime` parameter.)
                if is_type_ctor(f) {
                    // Already collected in Pass 0d; never a value/generic fn.
                    continue;
                }
                if is_generic(f) {
                    // Each `comptime` parameter must be either a *type* parameter
                    // (annotated `type`, v0.120) or a *value* parameter of an
                    // integer type (`comptime n: usize`, v0.128). Anything else
                    // (a non-integer value annotation, or a composite wrapper) is
                    // `E0250`.
                    for p in &f.params {
                        if p.is_comptime && !is_type_kw(&p.ty) && !self.is_value_param_annotation(&p.ty)
                        {
                            let msg = format!(
                                "`comptime` parameter `{}` must be a type parameter \
                                 (annotated `type`) or a value parameter of an integer \
                                 type, found `{}`",
                                p.name, p.ty.name
                            );
                            self.error(p.span, "E0250", msg);
                        }
                    }
                    self.generics.insert(f.name.clone(), f.clone());
                } else {
                    let params = f
                        .params
                        .iter()
                        .map(|p| self.resolve_type_opt(&p.ty).unwrap_or(Type::I64))
                        .collect();
                    let ret = self.resolve_type_opt(&f.ret).unwrap_or(Type::Void);
                    self.funcs.insert(f.name.clone(), FuncSig { params, ret });
                }
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
                // Bind `Self` -> this struct while resolving the method
                // signatures (v0.136, SPEC §32.2), so a non-receiver `Self`/`*Self`
                // parameter or a `Self`/`*Self` return type resolves to the struct
                // (not the `i64`/`void` fallback). The leading `self` receiver is
                // still set explicitly below via `self_ty`.
                let prev_self = self.bind_self(id);
                let mut map: HashMap<String, StructFn> = HashMap::new();
                for f in &s.methods {
                    let is_method = f.params.first().map_or(false, |p| p.name == "self");
                    // A pointer receiver `self: *Point` / `self: *Self` (SPEC
                    // §30) gives `self` the type `Ptr(Struct)` (true in-place
                    // mutation); a value receiver is unchanged.
                    let is_ptr_receiver =
                        is_method && is_ptr_receiver_param(&f.params[0], &s.name);
                    let self_ty = if is_ptr_receiver {
                        Type::Ptr(self.structs.intern_ptr(Type::Struct(id)))
                    } else {
                        Type::Struct(id)
                    };
                    let params = f
                        .params
                        .iter()
                        .enumerate()
                        .map(|(i, p)| {
                            if i == 0 && is_method {
                                self_ty
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
                            is_ptr_receiver,
                        },
                    );
                }
                self.struct_funcs.insert(id, map);
                self.restore_self(prev_self);
            }
        }

        // Pass 2: fold top-level consts in source order.
        for item in &m.items {
            if let Item::Const(c) = item {
                // v0.129 (SPEC §25.2): a top-level `const Alias = Name(C);` whose
                // initializer is a call to a type-constructor is a **type alias**,
                // not a value const. Instantiate `Name` at `C` and bind the alias;
                // it carries no `ConstVal`, so it is *not* folded as a value const.
                // A call to a known callee that is *not* a type-constructor is an
                // invalid instantiation (`E0311`); a call to an unknown name falls
                // through to `const_eval`, which reports it as non-constant
                // (`E0130`) — preserving the pre-v0.129 behaviour.
                if let Expr::Call { callee, args, span } = &c.value {
                    if self.type_ctors.contains_key(callee) {
                        // Already instantiated as a type alias in Pass 0d; it is
                        // not a value const, so skip folding.
                        let _ = (args, span);
                        continue;
                    }
                    if self.funcs.contains_key(callee) || self.generics.contains_key(callee) {
                        let msg = format!(
                            "`{}` is not a type-constructor; it cannot be instantiated as a type alias",
                            callee
                        );
                        self.error(*span, "E0311", msg);
                        continue;
                    }
                }
                // The type annotation is optional (v0.121, SPEC §18.2). When
                // present it is resolved (`E0100` for an unknown type name) and
                // type-checked against the folded value below. When absent the
                // binding's type is *inferred* from the comptime value
                // (`Int => i64`, `Bool => bool`) — see the `unwrap_or` below.
                let declared = match &c.ty {
                    Some(te) => {
                        let d = self.resolve_type_opt(te);
                        if d.is_none() {
                            self.error(
                                te.span,
                                "E0100",
                                format!("unknown type `{}`", te.name),
                            );
                        }
                        d
                    }
                    None => None,
                };
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

        // Pass 2b (v0.138): now that free-function signatures (Pass 1) and
        // top-level consts (Pass 2) are registered, type-check the deferred
        // generic-struct method bodies, so a method may reference both. Look the
        // constructor up by name to recover its methods (its signatures were
        // already registered during Pass 0d instantiation).
        let pending = std::mem::take(&mut self.pending_ctor_methods);
        for (ctor_name, id, msubst) in &pending {
            if let Some(ctor) = self.type_ctors.get(ctor_name).cloned() {
                if let Some(methods) = type_ctor_struct_methods(&ctor) {
                    for f in methods {
                        self.check_type_ctor_method(f, *id, msubst);
                    }
                }
            }
        }

        // Pass 3: type-check function and test bodies.
        for item in &m.items {
            match item {
                // A type-constructor (v0.129) is compile-time only — validated in
                // Pass 1 and instantiated in Pass 2 — so it has no ordinary body
                // to check. A generic function's body is checked per instantiation
                // (those are discovered through calls during this pass), never
                // directly.
                Item::Func(f) if is_type_ctor(f) || is_generic(f) => {}
                Item::Func(f) => self.check_func(f),
                Item::Test(t) => self.check_test(t),
                Item::Const(_) => {}
                Item::Struct(s) => self.check_struct_methods(s),
                // Enums are fully resolved in Pass 0; they have no body to check.
                Item::Enum(_) => {}
                // Unions are fully resolved in Pass 0c; they have no body either.
                Item::Union(_) => {}
                // Named error sets (v0.139) are fully registered in the error-set
                // pre-pass; they are compile-time only and have no body to check.
                Item::ErrorSet(_) => {}
                // Imports are resolved + erased by the module flattener before
                // sema runs; a residual one means a single-file compile saw an
                // `@import` with no path to resolve.
                Item::Import(im) => self.error(
                    im.span,
                    "E0290",
                    "`@import` requires building from a file (it is resolved by the build driver)",
                ),
            }
        }
    }

    fn check_func(&mut self, f: &Func) {
        self.ret_type = self.resolve_type(&f.ret).unwrap_or(Type::Void);
        // A `Set!T` return type carries the named error set (v0.139); a global
        // `!T` (or non-error) return leaves it `None`. Used to check membership
        // of a `return error.X;`.
        self.ret_error_set = f.ret.error_set.clone();
        self.in_test = false;
        self.loop_depth = 0;
        self.loop_labels.clear();
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
        // Bind `Self` -> the enclosing struct so `Self` / `*Self` / `@This()`
        // resolve in this plain struct's method signature **and** body (v0.136,
        // SPEC §32.2): the return type, any non-receiver `Self`/`*Self`
        // parameter, `var x: Self`, `Self{ … }` literals and `Self.assoc()` calls
        // all flow through the active substitution. (The leading `self` receiver
        // is still bound explicitly below.)
        let prev_self = self.bind_self(struct_id);
        self.ret_type = self.resolve_type(&f.ret).unwrap_or(Type::Void);
        self.ret_error_set = f.ret.error_set.clone();
        self.in_test = false;
        self.loop_depth = 0;
        self.loop_labels.clear();
        self.scopes.push(HashMap::new());
        let struct_name = self.structs.get(struct_id).name.clone();
        let is_method = f.params.first().map_or(false, |p| p.name == "self");
        // A pointer receiver `self: *Point` / `self: *Self` (SPEC §30) binds
        // `self` to `Ptr(Struct)`, so `self.field` auto-derefs and mutations
        // write through. A value receiver binds the enclosing struct by value.
        let self_ty = if is_method && is_ptr_receiver_param(&f.params[0], &struct_name) {
            Type::Ptr(self.structs.intern_ptr(Type::Struct(struct_id)))
        } else {
            Type::Struct(struct_id)
        };
        for (i, p) in f.params.iter().enumerate() {
            // The receiver `self` has the enclosing struct type (or a pointer to
            // it for a pointer receiver); other parameters resolve normally
            // (emitting `E0100` for unknown types).
            let pt = if i == 0 && is_method {
                self_ty
            } else {
                self.resolve_type(&p.ty).unwrap_or(Type::I64)
            };
            // Parameters (including `self`) are immutable bindings.
            self.define(&p.name, pt, true);
        }
        self.check_block(&f.body);
        self.scopes.pop();
        self.restore_self(prev_self);
    }

    fn check_test(&mut self, t: &TestBlock) {
        // A test body behaves like a `void` function for return purposes.
        self.ret_type = Type::Void;
        self.ret_error_set = None;
        self.in_test = true;
        self.loop_depth = 0;
        self.loop_labels.clear();
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
    /// declaration mentions them (SPEC §11.1). Likewise `!T` (`error_union`)
    /// resolves to `Type::ErrorUnion(intern_error_union(payload))` (SPEC §12.1).
    ///
    /// A `[N]T` (`array_len`) resolves its element `T` by these same rules and
    /// returns `Type::Array(intern_array(elem, N))`, interning the array type
    /// (SPEC §14.1). A negative (or otherwise absurd) `N` is reported as `E0224`
    /// here; the result is still a valid (zero-length) array type so callers do
    /// not additionally flag the name as unknown.
    fn resolve_type_opt(&mut self, te: &TypeExpr) -> Option<Type> {
        // A bound type parameter (under the active substitution, SPEC §17.2)
        // takes priority over the ordinary name lookup; otherwise this is the
        // normal builtin / struct / enum resolution. During the normal pass the
        // substitution is empty, so behaviour is unchanged.
        let inner = match self.subst.get(&te.name).copied() {
            Some(t) => t,
            None => self.resolve_base(&te.name)?,
        };
        // The (small) value substitution is cloned so `wrap_type` can borrow it
        // while still taking `&mut self` for array interning. It is empty during
        // the normal pass, so this is a no-cost clone there.
        let value_subst = self.value_subst.clone();
        Some(self.wrap_type(inner, te, &value_subst))
    }

    /// Like [`resolve_type_opt`], but consults explicit type and comptime-value
    /// substitutions instead of the (caller-context) `self.subst` /
    /// `self.value_subst`. Used at a generic call site to resolve the callee's
    /// runtime-parameter and return types under the callee's freshly-built
    /// substitutions while `self`'s still hold the caller's (SPEC §17.2 / §24.2).
    fn resolve_type_opt_with(
        &mut self,
        te: &TypeExpr,
        subst: &HashMap<String, Type>,
        value_subst: &HashMap<String, i64>,
    ) -> Option<Type> {
        let inner = match subst.get(&te.name).copied() {
            Some(t) => t,
            None => self.resolve_base(&te.name)?,
        };
        Some(self.wrap_type(inner, te, value_subst))
    }

    /// Resolve a bare type *name* (no `?`/`!`/`[N]`/`*`/`[]` wrappers) to a
    /// builtin, a type alias, a registered struct, an enum, or a tagged union,
    /// without consulting any (generic) type-parameter substitution. Returns
    /// `None` for an unknown name.
    ///
    /// A **type alias** (v0.129, `const Alias = Name(C);`) resolves to its
    /// aliased `Type` (always a monomorphised `Type::Struct`), so an alias is
    /// usable wherever a type name is — `var x: Alias`, a struct-literal name,
    /// and field access all flow through here.
    fn resolve_base(&self, name: &str) -> Option<Type> {
        Type::from_name(name)
            .or_else(|| self.type_aliases.get(name).copied())
            .or_else(|| self.structs.id_of(name).map(Type::Struct))
            .or_else(|| self.structs.enum_id_of(name).map(Type::Enum))
            .or_else(|| self.structs.union_id_of(name).map(Type::Union))
    }

    /// Apply a [`TypeExpr`]'s composite wrappers around an already-resolved base
    /// type `inner`, interning the resulting composite type. `*T` and `[]T`
    /// (v0.118) take precedence and return directly (they are not combined with
    /// `?`/`!`/`[N]` in v1); then `[N]T` (v0.117), then `?T` (v0.114) / `!T`
    /// (v0.115); a bare name returns `inner` unchanged.
    fn wrap_type(
        &mut self,
        inner: Type,
        te: &TypeExpr,
        value_subst: &HashMap<String, i64>,
    ) -> Type {
        if te.pointer {
            return Type::Ptr(self.structs.intern_ptr(inner));
        }
        if te.slice {
            return Type::Slice(self.structs.intern_slice(inner));
        }
        if let Some(size) = &te.array_len {
            let len = self.resolve_array_size(size, te.span, value_subst);
            return Type::Array(self.intern_array_len(inner, len, te.span));
        }
        if te.optional {
            Type::Optional(self.structs.intern_optional(inner))
        } else if te.error_union {
            Type::ErrorUnion(self.structs.intern_error_union(inner))
        } else {
            inner
        }
    }

    /// Resolve an array size `[N]T` (SPEC §14.1 / §24.2) to its concrete length:
    /// a literal `[3]T` is its value; a comptime value-parameter form `[n]T`
    /// resolves `n` through the active value substitution (the bound `i64`). A
    /// `[n]T` whose `n` is *not* a comptime value parameter in scope (e.g. used
    /// outside any generic, or a stray name) is `E0253`; the array is then
    /// interned with length 0 so resolution still yields a usable type.
    fn resolve_array_size(
        &mut self,
        size: &ArraySize,
        span: Span,
        value_subst: &HashMap<String, i64>,
    ) -> i64 {
        match size {
            ArraySize::Lit(n) => *n,
            ArraySize::Param(name) => match value_subst.get(name) {
                Some(&v) => v,
                None => {
                    let msg = format!(
                        "array size `{}` is not a comptime value parameter in scope",
                        name
                    );
                    self.error(span, "E0253", msg);
                    0
                }
            },
        }
    }

    /// Intern an array type `[len]elem`, validating `len`. A negative length is
    /// `E0224` (SPEC §14.2); in that case the array is interned with length 0 so
    /// resolution still yields a usable array type (avoiding cascade errors).
    fn intern_array_len(&mut self, elem: Type, len: i64, span: Span) -> u32 {
        if len < 0 {
            let msg = format!("array length must be non-negative, found `{}`", len);
            self.error(span, "E0224", msg);
            return self.structs.intern_array(elem, 0);
        }
        self.structs.intern_array(elem, len as usize)
    }

    /// Bind `Self` to `Struct(struct_id)` in the active type substitution,
    /// returning the previous binding (if any) so the caller can restore it via
    /// [`restore_self`]. This lets `Self` / `*Self` / `@This()` (the parser
    /// desugars `@This()` to the type name `Self`) resolve inside a **plain**
    /// named-struct method's signature and body (v0.136, SPEC §32.2). Generic-
    /// struct methods already bind `Self` through their own `msubst`, so this is
    /// only used on the plain-struct path.
    fn bind_self(&mut self, struct_id: u32) -> Option<Type> {
        self.subst.insert("Self".to_string(), Type::Struct(struct_id))
    }

    /// Undo a [`bind_self`], restoring the previous `Self` binding (or removing
    /// it when there was none).
    fn restore_self(&mut self, prev: Option<Type>) {
        match prev {
            Some(t) => {
                self.subst.insert("Self".to_string(), t);
            }
            None => {
                self.subst.remove("Self");
            }
        }
    }

    /// Resolve a type name to a builtin or a registered struct, emitting
    /// `E0100` for an unknown name.
    fn resolve_type(&mut self, te: &TypeExpr) -> Option<Type> {
        // A `Set!T` whose *set* name is not a declared error set is `E0331`
        // (v0.139, SPEC §34.2); the set is a compile-time constraint and does
        // not change the resolved runtime type (it is still `Type::ErrorUnion`
        // over the payload, identical to the global `!T`). This is the single
        // diagnostic gateway for type names, so each written `Set!T` reports
        // once; the global `!T` (`error_set: None`) is unaffected.
        self.check_error_set_ref(te);
        match self.resolve_type_opt(te) {
            Some(t) => Some(t),
            None => {
                self.error(te.span, "E0100", format!("unknown type `{}`", te.name));
                None
            }
        }
    }

    /// Validate the *set* name of a `Set!T` type expression (v0.139, SPEC §34.2):
    /// a named error union must name a declared error set, else `E0331`. A global
    /// `!T` (`error_set: None`) and every non-error-union type are accepted
    /// unchanged. This never alters the resolved type — the set is purely a
    /// compile-time membership constraint (checked separately at error-literal
    /// sites).
    fn check_error_set_ref(&mut self, te: &TypeExpr) {
        if let (true, Some(set)) = (te.error_union, &te.error_set) {
            if !self.error_sets.contains_key(set) {
                let msg = format!("unknown error set `{}`", set);
                self.error(te.span, "E0331", msg);
            }
        }
    }

    /// Check that a directly-written `error.X` coerced to a *named* error-union
    /// target is a member of that set (v0.139, SPEC §34.2). `set` is the target
    /// type's error-set name: `None` for the global `!T` (which accepts any error
    /// name, backward compatible) means no check. Only a literal `error.X` value
    /// is checked — exactly the `return error.X;` and `var x: Set!T = error.X;`
    /// positions the SPEC names. A non-member is `E0330`. An *undeclared* set is
    /// already reported as `E0331` at the type site and so is skipped here (its
    /// member list is unknown), avoiding a duplicate diagnostic.
    fn check_error_set_membership(&mut self, value: &Expr, set: &Option<String>) {
        let set = match set {
            Some(s) => s,
            None => return,
        };
        if let Expr::ErrorLit { name, span } = value {
            if let Some(members) = self.error_sets.get(set) {
                if !members.iter().any(|m| m == name) {
                    let msg = format!("error.{} is not a member of set `{}`", name, set);
                    self.error(*span, "E0330", msg);
                }
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
        // Apply any `*T` / `[]T` / `[N]T` / `?T` / `!T` wrappers around the
        // resolved base type (the base having passed the forward/cyclic-reference
        // rules above). Identical to the wrapping in `resolve_type_opt`. A struct
        // field's array size is always a literal — comptime value parameters are
        // never in scope here — so the value substitution is empty (a `[n]T`
        // field would be `E0253`).
        Some(self.wrap_type(inner, te, &HashMap::new()))
    }

    // ---- generic structs / type-constructors (v0.129, SPEC §25) -----------

    /// Validate a type-constructor and record it (SPEC §25.2 / §31.1). A valid
    /// type-constructor takes **one or more** `comptime` *type* parameters (all
    /// must be `comptime _: type` — a value/non-type comptime parameter, e.g.
    /// `comptime n: usize`, is `E0310`) and has a body of exactly
    /// `return struct { … };` (an [`Expr::StructType`]); anything else is
    /// `E0310`. It is recorded under its name regardless, so a
    /// `const Alias = Name(C, …);` still resolves to *a* struct (avoiding
    /// cascading `E0100`s) even when the constructor itself is malformed.
    fn collect_type_ctor(&mut self, f: &Func) {
        let valid_params =
            !f.params.is_empty() && f.params.iter().all(|p| p.is_comptime && is_type_kw(&p.ty));
        if !valid_params {
            let msg = format!(
                "type-returning function `{}` must take one or more `comptime` type parameters \
                 (`comptime T: type`)",
                f.name
            );
            self.error(f.span, "E0310", msg);
        }
        if type_ctor_struct_fields(f).is_none() {
            let msg = format!(
                "type-returning function `{}` must have a body of exactly \
                 `return struct {{ … }};`",
                f.name
            );
            self.error(f.body.span, "E0310", msg);
        }
        self.type_ctors.insert(f.name.clone(), f.clone());
    }

    /// Instantiate a type-constructor for a type alias `const Alias = Name(C, …);`
    /// (SPEC §25.2 / §31.1). The call must pass **exactly as many** type
    /// arguments as the constructor has type parameters (`E0311` otherwise);
    /// each must resolve to a concrete type (`E0311` otherwise). The constructor
    /// is instantiated at those types (a monomorphised struct, memoised on the
    /// argument tuple) and the alias is bound to it.
    fn instantiate_alias(&mut self, alias_name: &str, ctor_name: &str, args: &[Expr], span: Span) {
        let ctor = match self.type_ctors.get(ctor_name) {
            Some(f) => f.clone(),
            None => return, // unreachable: the caller checked membership
        };
        // A valid type-constructor's parameters are all `comptime _: type`, so
        // the parameter count is the expected type-argument count (a malformed
        // constructor — already `E0310` — still uses its parameter count, so a
        // dependent alias degrades gracefully rather than cascading).
        let nparams = ctor.params.len();
        if args.len() != nparams {
            let msg = format!(
                "type-constructor `{}` takes {} type argument{}, found {}",
                ctor_name,
                nparams,
                if nparams == 1 { "" } else { "s" },
                args.len()
            );
            self.error(span, "E0311", msg);
            return;
        }
        // Resolve every argument to a concrete type, in parameter order. A
        // non-type argument is `E0311` (already emitted); bail without binding
        // the alias (matching the single-argument v0.129 behaviour).
        let mut concretes: Vec<Type> = Vec::with_capacity(nparams);
        for arg in args {
            match self.resolve_alias_type_arg(arg) {
                Some(t) => concretes.push(t),
                None => return, // `E0311` already emitted
            }
        }
        let id = self.instantiate_type_ctor(ctor_name, &ctor, &concretes);
        self.type_aliases.insert(alias_name.to_string(), Type::Struct(id));
        // Share the alias with the backend (which only receives the StructTable)
        // so an alias name resolves in emitted types + struct literals (v0.129).
        self.structs.add_alias(alias_name, Type::Struct(id));
    }

    /// Resolve one type-constructor argument (SPEC §25.2 / §31.1): it must be an
    /// identifier naming a concrete type — a builtin, a struct/enum/union, or
    /// another type alias (all via [`resolve_base`]). Anything else is `E0311`.
    fn resolve_alias_type_arg(&mut self, arg: &Expr) -> Option<Type> {
        match arg {
            Expr::Ident { name, span } => match self.resolve_base(name) {
                Some(t) => Some(t),
                None => {
                    let msg =
                        format!("type-constructor argument `{}` does not name a type", name);
                    self.error(*span, "E0311", msg);
                    None
                }
            },
            other => {
                self.error(
                    other.span(),
                    "E0311",
                    "a type-constructor argument must be an identifier naming a type",
                );
                None
            }
        }
    }

    /// Instantiate the type-constructor `ctor` at the concrete types `concretes`
    /// (one per type parameter, in parameter order), returning the id of the
    /// monomorphised struct (SPEC §25.2 / §31.1). The struct is named
    /// `<Ctor>__<tag1>_<tag2>…` — the [`type_mangle`](StructTable::type_mangle)
    /// of each argument joined by `_` in parameter order (so a single-parameter
    /// `Box(i32)` stays `Box__int32_t`, and `Map(i32, i64)` is
    /// `Map__int32_t_int64_t`) — and **memoised by that name** (via
    /// [`StructTable::intern`]'s de-duplication / an `id_of` guard), so the same
    /// `(constructor, argument tuple)` reuses one struct id while a different
    /// tuple (including a different *order*) yields a distinct struct. Each field
    /// (and method, §26) type is resolved under the substitution mapping every
    /// type parameter to its concrete argument.
    fn instantiate_type_ctor(&mut self, ctor_name: &str, ctor: &Func, concretes: &[Type]) -> u32 {
        let mut mangled = format!("{}__", ctor_name);
        for (i, c) in concretes.iter().enumerate() {
            if i > 0 {
                mangled.push('_');
            }
            mangled.push_str(&self.structs.type_mangle(*c));
        }
        // Memoised: a repeated `(constructor, argument tuple)` reuses the id.
        if let Some(id) = self.structs.id_of(&mangled) {
            return id;
        }
        let id = self.structs.intern(&mangled);
        // Each comptime type parameter binds to its concrete argument (parameter
        // order). A malformed constructor (no parameter / non-`struct` body)
        // zips to an empty/short substitution and a field-less struct, but is
        // still a usable type.
        let mut subst: HashMap<String, Type> = HashMap::new();
        for (p, c) in ctor.params.iter().zip(concretes.iter()) {
            subst.insert(p.name.clone(), *c);
        }
        let mut fields: Vec<(String, Type)> = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();
        if let Some(field_decls) = type_ctor_struct_fields(ctor) {
            for f in field_decls {
                if !seen.insert(f.name.clone()) {
                    let msg = format!(
                        "duplicate field `{}` in generic struct `{}`",
                        f.name, ctor_name
                    );
                    self.error(f.span, "E0162", msg);
                    continue;
                }
                // The field type resolves under `{ param -> concrete }` (and the
                // empty value substitution — a generic struct has no comptime
                // value parameters in v0.129). An unknown field type is `E0161`.
                let fty = match self.resolve_type_opt_with(&f.ty, &subst, &HashMap::new()) {
                    Some(t) => t,
                    None => {
                        let msg = format!(
                            "unknown type `{}` in generic struct `{}`",
                            f.ty.name, ctor_name
                        );
                        self.error(f.ty.span, "E0161", msg);
                        Type::I64
                    }
                };
                fields.push((f.name.clone(), fty));
            }
        }
        self.structs.set_fields(id, fields);

        // v0.130 (SPEC §26): a generic struct may also declare **methods**, which
        // use `Self` (the instantiated struct) and the type parameter(s) (§31).
        // They are monomorphised once per instance — the memoisation guard at the
        // top returns early on a repeat `(constructor, argument tuple)`, so this
        // runs at most once per struct id (no duplicate registration / re-check /
        // record).
        //
        // For each method we (1) register its signature on the struct-method
        // table (SPEC §10 / Pass-1b shape) so `x.m(args)` resolves, (2) type-check
        // its body under `{ <type params> -> concretes, Self -> Struct(id) }`, and
        // (3) record the instance so the backend emits the methods. A *fields-only*
        // generic struct (v0.129) has no methods, so it registers nothing and is
        // **not** recorded — preserving v0.129 behaviour exactly.
        if let Some(methods) = type_ctor_struct_methods(ctor) {
            if !methods.is_empty() {
                // The method substitution: every type parameter -> its concrete
                // argument, plus the contextual `Self` -> the instantiated struct.
                let mut msubst = subst.clone();
                msubst.insert("Self".to_string(), Type::Struct(id));

                // (1) Register each method's signature. A `self` receiver is the
                // instantiated struct *by value* (SPEC §10 / §26), or a pointer
                // to it for a pointer receiver `self: *Self` (SPEC §30); the
                // remaining parameter and return types resolve under `msubst`, so
                // `Self`, `*Self`, `[]T`, `?T`, … all resolve as written.
                let struct_name = self.structs.get(id).name.clone();
                let mut map: HashMap<String, StructFn> = HashMap::new();
                for f in methods {
                    let is_method = f.params.first().map_or(false, |p| p.name == "self");
                    let is_ptr_receiver =
                        is_method && is_ptr_receiver_param(&f.params[0], &struct_name);
                    let self_ty = if is_ptr_receiver {
                        Type::Ptr(self.structs.intern_ptr(Type::Struct(id)))
                    } else {
                        Type::Struct(id)
                    };
                    let mut params: Vec<Type> = Vec::with_capacity(f.params.len());
                    for (i, p) in f.params.iter().enumerate() {
                        let pt = if i == 0 && is_method {
                            self_ty
                        } else {
                            self.resolve_type_opt_with(&p.ty, &msubst, &HashMap::new())
                                .unwrap_or(Type::I64)
                        };
                        params.push(pt);
                    }
                    let ret = self
                        .resolve_type_opt_with(&f.ret, &msubst, &HashMap::new())
                        .unwrap_or(Type::Void);
                    // A duplicate method name keeps the last declaration (matching
                    // the named-struct Pass-1b policy).
                    map.insert(
                        f.name.clone(),
                        StructFn {
                            params,
                            ret,
                            is_method,
                            is_ptr_receiver,
                        },
                    );
                }
                self.struct_funcs.insert(id, map);

                // (2) Defer the method-body type-checks to AFTER Pass 2 (v0.138):
                // a body may reference top-level `const`s / free functions, which
                // are not yet registered during alias instantiation (Pass 0d).
                // The signatures registered above already let call sites resolve.
                self.pending_ctor_methods
                    .push((ctor_name.to_string(), id, msubst.clone()));

                // (3) Record the instance (with every concrete argument, in
                // parameter order) so the backend rebuilds the substitution and
                // emits its methods (SPEC §31.2).
                self.structs
                    .record_struct_instance(id, ctor_name, concretes.to_vec());
            }
        }
        id
    }

    /// Type-check one generic-struct method body (v0.130, SPEC §26.2) under the
    /// substitution `msubst` = `{ <type param> -> concrete, Self -> Struct(id) }`.
    /// Mirrors [`Checker::check_struct_func`] (the `self` receiver is the
    /// instantiated struct *by value*) but with the type substitution active, so
    /// `Self`, the type parameter and composites like `[]T` resolve in both the
    /// signature and the body. The full per-function checking context
    /// (substitution, return type, test/loop state, scope stack) is saved and
    /// restored, so this is safe to call while interning aliases.
    fn check_type_ctor_method(
        &mut self,
        f: &Func,
        struct_id: u32,
        msubst: &HashMap<String, Type>,
    ) {
        let saved_subst = std::mem::replace(&mut self.subst, msubst.clone());
        let saved_value_subst = std::mem::take(&mut self.value_subst);
        let saved_ret = self.ret_type;
        let saved_ret_error_set = self.ret_error_set.take();
        let saved_in_test = self.in_test;
        let saved_loop = self.loop_depth;
        let saved_loop_labels = std::mem::take(&mut self.loop_labels);
        let saved_scopes = std::mem::take(&mut self.scopes);

        // With `self.subst` active, the return type resolves `Self` / the type
        // parameter to concrete types.
        self.ret_type = self.resolve_type(&f.ret).unwrap_or(Type::Void);
        self.ret_error_set = f.ret.error_set.clone();
        self.in_test = false;
        self.loop_depth = 0;
        self.scopes.push(HashMap::new());
        let struct_name = self.structs.get(struct_id).name.clone();
        let is_method = f.params.first().map_or(false, |p| p.name == "self");
        // A pointer receiver `self: *Self` (SPEC §30) binds `self` to
        // `Ptr(Struct)` so `self.field` auto-derefs and mutations write through;
        // a value receiver binds the instantiated struct by value (SPEC §10/§26).
        let self_ty = if is_method && is_ptr_receiver_param(&f.params[0], &struct_name) {
            Type::Ptr(self.structs.intern_ptr(Type::Struct(struct_id)))
        } else {
            Type::Struct(struct_id)
        };
        for (i, p) in f.params.iter().enumerate() {
            // The receiver `self` is the instantiated struct by value (or a
            // pointer to it for a pointer receiver); other parameters resolve
            // under the active substitution.
            let pt = if i == 0 && is_method {
                self_ty
            } else {
                self.resolve_type(&p.ty).unwrap_or(Type::I64)
            };
            // Parameters (including `self`) are immutable bindings.
            self.define(&p.name, pt, true);
        }
        self.check_block(&f.body);
        self.scopes.pop();

        self.subst = saved_subst;
        self.value_subst = saved_value_subst;
        self.ret_type = saved_ret;
        self.ret_error_set = saved_ret_error_set;
        self.in_test = saved_in_test;
        self.loop_depth = saved_loop;
        self.loop_labels = saved_loop_labels;
        self.scopes = saved_scopes;
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
                let bind_ty = match ty {
                    // Annotated binding (unchanged from earlier versions): resolve
                    // the annotation (`E0100` for an unknown name), then check the
                    // initializer coerces to it (`E0110` on a mismatch). An
                    // initializer is a statement-level position, so a top-level
                    // `try` is allowed (SPEC §12.1); otherwise optional /
                    // error-union coercion (§11.2, §12.2) lets a `T` value, `null`,
                    // or `error.X` widen to `?T`/`!T`.
                    Some(te) => {
                        let declared = self.resolve_type(te);
                        let vt = self.check_value_with_try(value, declared);
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
                        // A `var x: Set!T = error.X;` over a *named* set requires
                        // `X` to belong to `Set` (v0.139, SPEC §34.2); a global
                        // `!T` annotation (`error_set: None`) accepts any error.
                        self.check_error_set_membership(value, &te.error_set);
                        declared.unwrap_or(Type::I64)
                    }
                    // Inferred binding (v0.121, SPEC §18.2): the binding's type is
                    // the initializer's type, checked with *no* expected type (an
                    // integer literal therefore defaults to `i64`). A value whose
                    // type needs context to be known — a bare `null`, an
                    // `error.X`, or an unqualified `.Variant` — is `E0260`.
                    None => {
                        if needs_inference_context(value) {
                            self.error(
                                value.span(),
                                "E0260",
                                "cannot infer type; add an annotation",
                            );
                            // Fall back to `i64` so dependent statements still
                            // check; the missing-annotation error is enough.
                            Type::I64
                        } else {
                            // `check_value_with_try` also yields the payload of a
                            // top-level `try` initializer. `None` here means the
                            // initializer already reported its own error; fall back
                            // to `i64` to avoid cascading diagnostics.
                            self.check_value_with_try(value, None)
                                .unwrap_or(Type::I64)
                        }
                    }
                };
                self.define(name, bind_ty, *is_const);
            }
            Stmt::Assign {
                name,
                op,
                value,
                span,
            } => match self.lookup(name) {
                Some((ty, is_const)) => {
                    if is_const {
                        // A compound assignment to a `const` is rejected exactly
                        // as a plain `=` is (SPEC §27.2): the immutability error
                        // is primary; the rhs is still checked against the place
                        // type so its own diagnostics still surface.
                        self.error(
                            *span,
                            "E0110",
                            format!("cannot assign to immutable binding `{}`", name),
                        );
                        self.check_expr(value, Some(ty));
                    } else if op.is_some() {
                        // Compound assignment `name op= value` (v0.131, §27.2):
                        // the place and rhs must be the same integer type.
                        self.check_compound_assign_arith(ty, value, *span);
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
            Stmt::FieldAssign {
                place, op, value, ..
            } => {
                if let Some(pt) = self.resolve_place(place) {
                    if op.is_some() {
                        // Compound assignment `place op= value` (v0.131, §27.2):
                        // the place is resolved once above; the place and rhs
                        // must be the same integer type.
                        self.check_compound_assign_arith(pt, value, place.span());
                    } else {
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
                    }
                } else {
                    self.check_expr(value, None);
                }
            }
            Stmt::Expr(e) => {
                // An expression statement is a statement-level position, so a
                // top-level `try` is allowed here (SPEC §12.1).
                self.check_value_with_try(e, None);
            }
            Stmt::Return { value, span } => match value {
                Some(e) => {
                    if self.ret_type == Type::Void {
                        self.error(
                            *span,
                            "E0110",
                            "cannot return a value from a `void` function",
                        );
                        // Still a statement-level position: a top-level `try`
                        // here reports E0190 (non-`!` enclosing fn), not E0191.
                        self.check_value_with_try(e, None);
                    } else {
                        let expected = self.ret_type;
                        // `return` is a statement-level position (top-level `try`
                        // allowed, SPEC §12.1); otherwise `T`/`null`/`error.X`
                        // widen to a `?T`/`!T` return type (§11.2, §12.2).
                        let vt = self.check_value_with_try(e, Some(expected));
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
                        // A `return error.X;` from a `fn … Set!T` over a *named*
                        // set requires `X` to belong to `Set` (v0.139, SPEC
                        // §34.2); a global `!T` return (`ret_error_set: None`)
                        // accepts any error.
                        let ret_set = self.ret_error_set.clone();
                        self.check_error_set_membership(e, &ret_set);
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
                cond,
                capture,
                then,
                els,
                ..
            } => match capture {
                // Optional `if` capture (SPEC §21.1): `if (opt) |v| { then } else
                // { els }`. The condition must be an optional `?T` (else `E0280`);
                // `v` binds the unwrapped `T` as an immutable local inside `then`;
                // `els` runs (with **no** binding) when the optional is null.
                Some(name) => {
                    let inner = match self.check_expr(cond, None) {
                        Some(Type::Optional(id)) => self.structs.optional_inner(id),
                        Some(other) => {
                            let msg = format!(
                                "`if` capture `|{}|` requires an optional (`?T`) condition, \
                                 found `{}`",
                                name,
                                self.type_name(other)
                            );
                            self.error(cond.span(), "E0280", msg);
                            // Fall back to `i64` so the then-block still checks
                            // (avoids a cascade of unknown-name errors on `name`).
                            Type::I64
                        }
                        // The condition itself errored; bind a fallback so the
                        // then-block still checks.
                        None => Type::I64,
                    };
                    // Bind the capture in a scope wrapping `then`; `check_block`
                    // nests its own scope inside, so the binding is visible
                    // throughout the then-block — and only there.
                    self.scopes.push(HashMap::new());
                    self.define(name, inner, true);
                    self.check_block(then);
                    self.scopes.pop();
                    // `els` is checked outside that scope: it never sees `name`.
                    if let Some(els) = els {
                        self.check_stmt(els);
                    }
                }
                // Plain `if (cond) { … }` (no capture): unchanged — the condition
                // must be `bool` (SPEC §3).
                None => {
                    self.check_condition(cond, "if");
                    self.check_block(then);
                    if let Some(els) = els {
                        self.check_stmt(els);
                    }
                }
            },
            Stmt::While {
                cond,
                cont,
                body,
                label,
                ..
            } => {
                self.check_condition(cond, "while");
                // The continue-clause statement runs in the loop's outer scope.
                if let Some(c) = cont {
                    self.check_stmt(c);
                }
                // Track this loop's label (v0.147, SPEC §40.1) alongside the
                // depth so an enclosed `break`/`continue :name` can find it.
                self.loop_depth += 1;
                self.loop_labels.push(label.clone());
                self.check_block(body);
                self.loop_labels.pop();
                self.loop_depth -= 1;
            }
            // `for (iter) |elem| { … }` / `for (iter, 0..) |elem, index| { … }`
            // (v0.133, SPEC §29.1). The iterable must be an array (`[N]T`) or a
            // slice (`[]T`); `elem` binds each element **by value** (an immutable
            // local of element type `T`), and — only for the `, 0..` index form
            // — `index` binds a `usize`. The body is checked in a new loop scope
            // holding those bindings, so `break`/`continue` are valid and the
            // bindings are visible only inside the body. The capture-count vs
            // `, 0..` agreement is enforced by the parser (it decides whether
            // `index` is `Some`), so it is not re-checked here.
            Stmt::For {
                iter,
                elem,
                index,
                body,
                label,
                ..
            } => {
                // The element type is `T` for `[]T`/`[N]T`; any other iterable is
                // an error (we still bind `elem` to a fallback so the body keeps
                // checking, avoiding a cascade of unknown-name errors on `elem`).
                let elem_ty = match self.check_expr(iter, None) {
                    Some(Type::Array(id)) => self.structs.array_elem(id),
                    Some(Type::Slice(id)) => self.structs.slice_elem(id),
                    Some(other) => {
                        let msg = format!(
                            "`for` requires an array (`[N]T`) or slice (`[]T`), found `{}`",
                            self.type_name(other)
                        );
                        self.error(iter.span(), "E0300", msg);
                        Type::I64
                    }
                    // The iterable expression already reported its own error;
                    // bind a fallback so the body still checks.
                    None => Type::I64,
                };
                // A scope wrapping the body holds the capture bindings (mirroring
                // the optional-`if` capture above); `check_block` nests its own
                // scope inside, so the captures live throughout the body — and
                // only there. `elem` (and `index`) are immutable (`is_const`),
                // matching other capture bindings: `elem` is a by-value copy.
                self.scopes.push(HashMap::new());
                self.define(elem, elem_ty, true);
                if let Some(index_name) = index {
                    self.define(index_name, Type::Usize, true);
                }
                self.loop_depth += 1;
                self.loop_labels.push(label.clone());
                self.check_block(body);
                self.loop_labels.pop();
                self.loop_depth -= 1;
                self.scopes.pop();
            }
            // `break;` / `break :label;` (v0.147, SPEC §40.1). Unlabeled: valid
            // only inside a loop (`E0120`, unchanged). Labeled: `label` must
            // name some enclosing loop's label (`E0121`).
            Stmt::Break { target, span } => {
                self.check_loop_target(target.as_deref(), *span, "break");
            }
            Stmt::Continue { target, span } => {
                self.check_loop_target(target.as_deref(), *span, "continue");
            }
            Stmt::Defer { stmt, .. } => {
                self.check_stmt(stmt);
            }
            // `errdefer stmt;` (SPEC §21.2): the deferred statement is checked
            // exactly like a `defer`'s — same scope / loop-depth treatment. It is
            // accepted in any function (it simply never fires in one that never
            // returns an error); the error-only firing is a backend concern.
            Stmt::ErrDefer { stmt, .. } => {
                self.check_stmt(stmt);
            }
            Stmt::Block(b) => {
                self.check_block(b);
            }
            Stmt::Switch {
                scrutinee,
                arms,
                default,
                span,
            } => {
                self.check_switch(scrutinee, arms, default, *span);
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

    /// Validate a `break`/`continue` target (v0.147, SPEC §40.1). `kw` is the
    /// keyword for diagnostics. An unlabeled jump (`target == None`) is valid
    /// only inside some loop (`E0120`, unchanged from v0.111). A labeled jump
    /// (`break :name`) must name some **enclosing** loop's label, i.e. `name`
    /// must appear in `loop_labels`; otherwise it is `E0121` (this also covers a
    /// labeled jump that is outside every loop, since the stack is then empty).
    fn check_loop_target(&mut self, target: Option<&str>, span: Span, kw: &str) {
        match target {
            None => {
                if self.loop_depth == 0 {
                    let msg = format!("`{}` is only valid inside a loop", kw);
                    self.error(span, "E0120", msg);
                }
            }
            Some(name) => {
                let found = self
                    .loop_labels
                    .iter()
                    .any(|l| l.as_deref() == Some(name));
                if !found {
                    let msg = format!("no enclosing loop labeled `{}`", name);
                    self.error(span, "E0121", msg);
                }
            }
        }
    }

    // ---- switch (v0.116) --------------------------------------------------

    /// Type-check a `switch` (SPEC §13.2). The scrutinee must be an enum or an
    /// integer (`E0213`); each arm's labels must be constant patterns of that
    /// type; the arms must be exhaustive (every enum variant covered, `E0210`,
    /// or an `else` for an integer, `E0214`). Each arm body and the `else` block
    /// are checked as nested scopes.
    fn check_switch(
        &mut self,
        scrutinee: &Expr,
        arms: &[SwitchArm],
        default: &Option<Block>,
        span: Span,
    ) {
        match self.check_expr(scrutinee, None) {
            // A tagged-union scrutinee is the only kind whose arms may bind a
            // payload capture (SPEC §20.2); enum/integer arms with a capture are
            // rejected (`E0272`) below.
            Some(Type::Union(uid)) => {
                // A range label `lo..hi` only matches an integer scrutinee; a
                // range on a tagged-union switch is invalid (SPEC §39.1).
                self.reject_arm_ranges(arms);
                self.check_union_switch(uid, arms, default, span)
            }
            Some(Type::Enum(eid)) => {
                self.reject_arm_captures(arms);
                // A range label is only valid for an integer scrutinee, not an
                // enum (SPEC §39.1).
                self.reject_arm_ranges(arms);
                self.check_enum_switch(eid, arms, default, span)
            }
            Some(t) if t.is_int() => {
                self.reject_arm_captures(arms);
                // Range labels (`lo..hi`) are valid here — they are checked /
                // lowered as part of the integer switch.
                self.check_int_switch(t, arms, default, span)
            }
            Some(t) => {
                let msg = format!(
                    "`switch` scrutinee must be an enum, integer or union type, found `{}`",
                    self.type_name(t)
                );
                self.error(scrutinee.span(), "E0213", msg);
                // The scrutinee is unswitchable, so labels cannot be validated,
                // but arm bodies and the `else` block are still checked so their
                // own errors surface. A range label is likewise invalid on a
                // non-integer scrutinee (SPEC §39.1).
                self.reject_arm_ranges(arms);
                self.check_switch_blocks(arms, default);
            }
            // The scrutinee itself errored; just check the arm bodies + else.
            None => self.check_switch_blocks(arms, default),
        }
    }

    /// Emit `E0272` for any arm declaring a payload capture (`|x|`) on a
    /// non-union `switch`: only a tagged-union switch binds a payload (SPEC
    /// §20.2). Enum / integer arms with a capture are otherwise checked normally
    /// (the capture name is simply not bound).
    fn reject_arm_captures(&mut self, arms: &[SwitchArm]) {
        for arm in arms {
            if arm.capture.is_some() {
                self.error(
                    arm.span,
                    "E0272",
                    "a payload capture `|x|` is only valid in a `switch` over a tagged union",
                );
            }
        }
    }

    /// Emit `E0212` for any arm carrying an inclusive integer-range label
    /// (`lo..hi`) on a `switch` whose scrutinee is **not** an integer type: a
    /// range is only a valid label for an integer `switch` (SPEC §39.1). Enum /
    /// union / otherwise non-integer scrutinees accept only their own (value)
    /// labels. (Value labels and payload captures on such arms are validated by
    /// the per-kind checkers as before; rejecting ranges here is independent.)
    fn reject_arm_ranges(&mut self, arms: &[SwitchArm]) {
        for arm in arms {
            if !arm.ranges.is_empty() {
                self.error(
                    arm.span,
                    "E0212",
                    "a range label `lo..hi` is only valid in a `switch` over an integer type",
                );
            }
        }
    }

    /// Check only the arm bodies and the `else` block of a `switch` (used when
    /// the scrutinee type is unusable, so labels are skipped).
    fn check_switch_blocks(&mut self, arms: &[SwitchArm], default: &Option<Block>) {
        for arm in arms {
            self.check_block(&arm.body);
        }
        if let Some(d) = default {
            self.check_block(d);
        }
    }

    /// Check a `switch` whose scrutinee is the enum `eid`.
    fn check_enum_switch(
        &mut self,
        eid: u32,
        arms: &[SwitchArm],
        default: &Option<Block>,
        span: Span,
    ) {
        // The set of variant indices covered so far, for exhaustiveness and to
        // detect a variant repeated across arms.
        let mut covered: HashSet<usize> = HashSet::new();
        for arm in arms {
            for label in &arm.labels {
                if let Some(idx) = self.switch_enum_label_index(eid, label) {
                    if !covered.insert(idx) {
                        let ename = self.structs.enum_get(eid).name.clone();
                        let vname = self.structs.enum_get(eid).variants[idx].clone();
                        let msg = format!("duplicate `switch` label `{}.{}`", ename, vname);
                        self.error(label.span(), "E0211", msg);
                    }
                }
            }
            self.check_block(&arm.body);
        }
        if let Some(d) = default {
            // An `else` makes the `switch` exhaustive regardless of coverage.
            self.check_block(d);
        } else {
            let total = self.structs.enum_get(eid).variants.len();
            let missing: Vec<String> = (0..total)
                .filter(|i| !covered.contains(i))
                .map(|i| self.structs.enum_get(eid).variants[i].clone())
                .collect();
            if !missing.is_empty() {
                let ename = self.structs.enum_get(eid).name.clone();
                let msg = format!(
                    "non-exhaustive `switch` on enum `{}`: missing variant(s) `{}`; \
                     cover them or add an `else` arm",
                    ename,
                    missing.join("`, `")
                );
                self.error(span, "E0210", msg);
            }
        }
    }

    /// Check a `switch` whose scrutinee is the integer type `scrut_ty`. An
    /// integer `switch` can never be proven exhaustive, so it requires an
    /// `else` (`E0214`).
    fn check_int_switch(
        &mut self,
        scrut_ty: Type,
        arms: &[SwitchArm],
        default: &Option<Block>,
        span: Span,
    ) {
        let mut covered: HashSet<i64> = HashSet::new();
        for arm in arms {
            for label in &arm.labels {
                if let Some(v) = self.switch_int_label_value(scrut_ty, label) {
                    if !covered.insert(v) {
                        self.error(
                            label.span(),
                            "E0211",
                            format!("duplicate `switch` label `{}`", v),
                        );
                    }
                }
            }
            self.check_block(&arm.body);
        }
        if let Some(d) = default {
            self.check_block(d);
        } else {
            self.error(
                span,
                "E0214",
                "non-exhaustive `switch` on an integer type: an integer `switch` requires an `else` arm",
            );
        }
    }

    /// Check a `switch` whose scrutinee is the tagged union `uid` (SPEC §20.2).
    /// Each label is a variant pattern (`E0271` if it names no variant); a
    /// variant repeated across arms is `E0211`; the arms must cover every
    /// variant or include an `else` (`E0210`). When an arm declares a payload
    /// capture, it is bound — as an immutable local of the matched variant's
    /// payload type — within that arm's body scope. (An arm without a capture is
    /// fine: the payload is simply not bound.) For a multi-label arm the capture
    /// binds the *first* matched variant's payload type.
    fn check_union_switch(
        &mut self,
        uid: u32,
        arms: &[SwitchArm],
        default: &Option<Block>,
        span: Span,
    ) {
        let mut covered: HashSet<usize> = HashSet::new();
        for arm in arms {
            // The payload type a capture in this arm binds (the first matched
            // variant's payload).
            let mut payload: Option<Type> = None;
            for label in &arm.labels {
                if let Some(idx) = self.switch_union_label_index(uid, label) {
                    if !covered.insert(idx) {
                        let uname = self.structs.union_get(uid).name.clone();
                        let vname = self.structs.union_get(uid).variants[idx].0.clone();
                        let msg = format!("duplicate `switch` label `{}.{}`", uname, vname);
                        self.error(label.span(), "E0211", msg);
                    }
                    if payload.is_none() {
                        payload = Some(self.structs.union_get(uid).variants[idx].1);
                    }
                }
            }
            // Check the arm body in a fresh scope that (when the arm captures)
            // binds the payload as an immutable local; `check_block` then nests
            // its own scope inside, so the capture is visible throughout.
            self.scopes.push(HashMap::new());
            if let Some(cap) = &arm.capture {
                let pty = payload.unwrap_or(Type::I64);
                self.define(cap, pty, true);
            }
            self.check_block(&arm.body);
            self.scopes.pop();
        }
        if let Some(d) = default {
            // An `else` makes the `switch` exhaustive regardless of coverage.
            self.check_block(d);
        } else {
            let total = self.structs.union_get(uid).variants.len();
            let missing: Vec<String> = (0..total)
                .filter(|i| !covered.contains(i))
                .map(|i| self.structs.union_get(uid).variants[i].0.clone())
                .collect();
            if !missing.is_empty() {
                let uname = self.structs.union_get(uid).name.clone();
                let msg = format!(
                    "non-exhaustive `switch` on union `{}`: missing variant(s) `{}`; \
                     cover them or add an `else` arm",
                    uname,
                    missing.join("`, `")
                );
                self.error(span, "E0210", msg);
            }
        }
    }

    /// Resolve one label of a union `switch` to the 0-based index of the variant
    /// it names, or `None` (after emitting a diagnostic). Accepts the
    /// unqualified `.V` ([`Expr::EnumLit`]) form — and, for parity with enums,
    /// the qualified `Union.V` ([`Expr::Field`]) form. A label that is not a
    /// variant of `uid` is `E0271`.
    fn switch_union_label_index(&mut self, uid: u32, label: &Expr) -> Option<usize> {
        match label {
            Expr::EnumLit { variant, span } => {
                match self.structs.union_get(uid).variant_index(variant) {
                    Some(i) => Some(i),
                    None => {
                        let uname = self.structs.union_get(uid).name.clone();
                        let msg = format!("union `{}` has no variant `{}`", uname, variant);
                        self.error(*span, "E0271", msg);
                        None
                    }
                }
            }
            Expr::Field { base, field, span } => {
                if let Expr::Ident { name, .. } = base.as_ref() {
                    if self.structs.union_id_of(name) == Some(uid) {
                        return match self.structs.union_get(uid).variant_index(field) {
                            Some(i) => Some(i),
                            None => {
                                let uname = self.structs.union_get(uid).name.clone();
                                let msg =
                                    format!("union `{}` has no variant `{}`", uname, field);
                                self.error(*span, "E0271", msg);
                                None
                            }
                        };
                    }
                }
                let uname = self.structs.union_get(uid).name.clone();
                let msg = format!(
                    "`switch` label on union `{}` must be a variant (`.V` or `{}.V`)",
                    uname, uname
                );
                self.error(*span, "E0271", msg);
                None
            }
            _ => {
                let uname = self.structs.union_get(uid).name.clone();
                let msg = format!(
                    "`switch` label on union `{}` must be a variant (`.V` or `{}.V`)",
                    uname, uname
                );
                self.error(label.span(), "E0271", msg);
                None
            }
        }
    }

    /// Resolve one label of an enum `switch` to the 0-based index of the
    /// variant it names, or `None` (after emitting a diagnostic) if it is not a
    /// valid variant pattern of enum `eid`. Accepts `.V` ([`Expr::EnumLit`]) and
    /// `Enum.V` ([`Expr::Field`] over the scrutinee enum); anything else is a
    /// type mismatch.
    fn switch_enum_label_index(&mut self, eid: u32, label: &Expr) -> Option<usize> {
        match label {
            Expr::EnumLit { variant, span } => {
                match self.structs.enum_get(eid).variant_index(variant) {
                    Some(i) => Some(i),
                    None => {
                        let ename = self.structs.enum_get(eid).name.clone();
                        let msg = format!("enum `{}` has no variant `{}`", ename, variant);
                        self.error(*span, "E0212", msg);
                        None
                    }
                }
            }
            Expr::Field { base, field, span } => {
                // A qualified `Enum.V` label must name the scrutinee's enum.
                if let Expr::Ident { name, .. } = base.as_ref() {
                    if self.structs.enum_id_of(name) == Some(eid) {
                        return match self.structs.enum_get(eid).variant_index(field) {
                            Some(i) => Some(i),
                            None => {
                                let ename = self.structs.enum_get(eid).name.clone();
                                let msg = format!("enum `{}` has no variant `{}`", ename, field);
                                self.error(*span, "E0212", msg);
                                None
                            }
                        };
                    }
                }
                // Some other field access / a different enum's literal: type it
                // (to surface its own errors) and report a mismatch.
                let lt = self.check_expr(label, None);
                let ename = self.structs.enum_get(eid).name.clone();
                let found = match lt {
                    Some(t) => self.type_name(t),
                    None => "<error>".to_string(),
                };
                let msg = format!(
                    "`switch` label of type `{}` does not match scrutinee enum `{}`",
                    found, ename
                );
                self.error(*span, "E0110", msg);
                None
            }
            _ => {
                // Not an enum-literal pattern (e.g. an integer literal).
                self.check_expr(label, Some(Type::Enum(eid)));
                let ename = self.structs.enum_get(eid).name.clone();
                let msg = format!(
                    "`switch` label on enum `{}` must be a variant (`.V` or `{}.V`)",
                    ename, ename
                );
                self.error(label.span(), "E0110", msg);
                None
            }
        }
    }

    /// Resolve one label of an integer `switch` to its constant value (for
    /// duplicate detection), or `None` (after emitting a diagnostic) if the
    /// label's type does not match the scrutinee or it is not constant.
    fn switch_int_label_value(&mut self, scrut_ty: Type, label: &Expr) -> Option<i64> {
        match self.check_expr(label, Some(scrut_ty)) {
            Some(t) if t == scrut_ty => {}
            Some(t) => {
                let msg = format!(
                    "`switch` label type mismatch: expected `{}`, found `{}`",
                    self.type_name(scrut_ty),
                    self.type_name(t)
                );
                self.error(label.span(), "E0110", msg);
                return None;
            }
            None => return None,
        }
        // The label is of the right integer type; fold it to a constant so that
        // duplicate labels across arms can be detected.
        match const_eval::eval(label, &self.consts) {
            Ok(ConstVal::Int(n)) => Some(n),
            // Unreachable: a `Bool` value would have failed the type check above.
            Ok(ConstVal::Bool(_)) => None,
            Err(d) => {
                // A non-constant integer label is itself an error.
                self.diags.push(d);
                None
            }
        }
    }

    /// Resolve `Enum.V` / `.V` to its enum value type, emitting `E0212` if `eid`
    /// has no variant named `variant`.
    fn check_enum_variant(&mut self, eid: u32, variant: &str, span: Span) -> Option<Type> {
        if self.structs.enum_get(eid).variant_index(variant).is_some() {
            Some(Type::Enum(eid))
        } else {
            let ename = self.structs.enum_get(eid).name.clone();
            let msg = format!("enum `{}` has no variant `{}`", ename, variant);
            self.error(span, "E0212", msg);
            None
        }
    }

    /// Best-effort, side-effect-free type of a *place* expression (a value
    /// identifier, field-access chain, index, or deref). Used to detect a
    /// pointer base for an auto-deref write (SPEC §30.1) **without** emitting
    /// diagnostics; returns `None` for anything it cannot resolve purely (the
    /// caller then falls back to the normal, diagnostic-emitting place path).
    /// Auto-derefs a `*Struct` base so `p.f` / `p.f.g` resolve through pointers.
    fn place_type_query(&self, e: &Expr) -> Option<Type> {
        match e {
            Expr::Ident { name, .. } => self.lookup(name).map(|(t, _)| t),
            Expr::Field { base, field, .. } => {
                let bt = self.place_type_query(base)?;
                let st = match bt {
                    Type::Ptr(pid) => self.structs.ptr_pointee(pid),
                    other => other,
                };
                match st {
                    Type::Struct(id) => self.structs.get(id).field_type(field),
                    _ => None,
                }
            }
            Expr::Index { base, .. } => {
                let bt = self.place_type_query(base)?;
                let st = match bt {
                    Type::Ptr(pid) => self.structs.ptr_pointee(pid),
                    other => other,
                };
                match st {
                    Type::Array(id) => Some(self.structs.array_elem(id)),
                    Type::Slice(id) => Some(self.structs.slice_elem(id)),
                    _ => None,
                }
            }
            Expr::Deref { expr, .. } => match self.place_type_query(expr)? {
                Type::Ptr(pid) => Some(self.structs.ptr_pointee(pid)),
                _ => None,
            },
            _ => None,
        }
    }

    /// If place expression `base` has a pointer type — so `base.field = e`
    /// writes THROUGH the pointer (SPEC §30.1), like `(*base).field = e`, and
    /// the pointer binding itself need not be mutable — return that pointer
    /// type; else `None` (the caller falls back to the normal place path, which
    /// enforces mutability for a value-struct chain). Side-effect-free.
    fn place_base_ptr_type(&self, base: &Expr) -> Option<Type> {
        match self.place_type_query(base) {
            Some(t @ Type::Ptr(_)) => Some(t),
            _ => None,
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
                // SPEC §30.1: if `base` is a pointer (to a struct), `base.field =
                // e` writes THROUGH the pointer — like `(*base).field = e` — so
                // the pointer binding itself need not be mutable (mirrors `p.* =
                // e`). Resolve the field against the pointee and skip the
                // assignable-`var` requirement. Otherwise `base` must itself be
                // an assignable place.
                let bt = match self.place_base_ptr_type(base) {
                    Some(pt) => pt,
                    None => self.resolve_place(base)?,
                };
                self.field_type_of(bt, field, *span)
            }
            // `a[i] = e`: for an array (SPEC §14.2) the base must be rooted in a
            // mutable `var` (`E0223`); for a slice (SPEC §15.2) the element is
            // always an assignable place — a slice is a mutable view, so the
            // binding's mutability is irrelevant. The result is the element type.
            Expr::Index { base, index, span } => {
                self.check_index_is_int(index);
                let (bt, mutable) = self.resolve_index_base(base)?;
                match bt {
                    Type::Array(id) => {
                        if !mutable {
                            self.error(
                                *span,
                                "E0223",
                                "cannot assign to an array element through an immutable binding \
                                 (only `var` arrays are assignable)",
                            );
                        }
                        Some(self.structs.array_elem(id))
                    }
                    Type::Slice(id) => Some(self.structs.slice_elem(id)),
                    other => {
                        let msg = format!(
                            "cannot index-assign into non-array type `{}`",
                            self.type_name(other)
                        );
                        self.error(*span, "E0223", msg);
                        None
                    }
                }
            }
            // `p.* = e` (SPEC §15.1): the deref target must be a pointer; the
            // pointee is always an assignable place (writing through a `*T` is
            // allowed regardless of the binding's mutability). A non-pointer
            // target is `E0230`.
            Expr::Deref { expr, span } => match self.check_expr(expr, None) {
                Some(Type::Ptr(id)) => Some(self.structs.ptr_pointee(id)),
                Some(other) => {
                    let msg = format!(
                        "`.*` requires a pointer (`*T`) operand, found `{}`",
                        self.type_name(other)
                    );
                    self.error(*span, "E0230", msg);
                    None
                }
                None => None,
            },
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

    /// Resolve the base of an index-assignment place to its `(type, mutable)`,
    /// where `mutable` is whether its root binding is an assignable `var` (not a
    /// `const`/parameter). Emits structural diagnostics (unknown name `E0100`,
    /// bad field `E0165`/`E0166`, indexing a non-array base `E0223`) but leaves
    /// the mutability verdict to the caller, which reports it as `E0223` for an
    /// index-assignment (distinct from the field-assignment `E0167`).
    fn resolve_index_base(&mut self, base: &Expr) -> Option<(Type, bool)> {
        match base {
            Expr::Ident { name, span } => match self.lookup(name) {
                Some((ty, is_const)) => Some((ty, !is_const)),
                None => {
                    self.error(*span, "E0100", format!("unknown name `{}`", name));
                    None
                }
            },
            Expr::Field { base: inner, field, span } => {
                let (bt, mutable) = self.resolve_index_base(inner)?;
                let ft = self.field_type_of(bt, field, *span)?;
                // SPEC §30.1: reaching a field THROUGH a `*Struct` base writes
                // through the pointer, so its storage is mutable regardless of
                // the pointer binding's own (im)mutability (e.g. `self.arr[i] =
                // e` in a pointer-receiver method).
                Some((ft, mutable || matches!(bt, Type::Ptr(_))))
            }
            // A nested index base (`m[i][j] = e`); v0.117 has no array-of-array,
            // so this generally lands on a non-array element (reported here).
            Expr::Index { base: inner, index, span } => {
                self.check_index_is_int(index);
                let (bt, mutable) = self.resolve_index_base(inner)?;
                match bt {
                    Type::Array(id) => Some((self.structs.array_elem(id), mutable)),
                    Type::Slice(id) => Some((self.structs.slice_elem(id), mutable)),
                    other => {
                        let msg = format!(
                            "cannot index-assign into non-array type `{}`",
                            self.type_name(other)
                        );
                        self.error(*span, "E0223", msg);
                        None
                    }
                }
            }
            _ => {
                self.error(
                    base.span(),
                    "E0223",
                    "index-assignment base must be a `var` array or a field/element of one",
                );
                self.check_expr(base, None);
                None
            }
        }
    }

    /// Resolve `<base type>.field`, emitting `E0165` if `base` is not a struct
    /// or `E0166` if it has no such field. Returns the field's type.
    fn field_type_of(&mut self, base: Type, field: &str, span: Span) -> Option<Type> {
        // SPEC §30.1: a `*Struct` value auto-derefs for field access — `p.field`
        // resolves against the pointed-to struct (`(*p).field`). This applies to
        // ANY `*Struct` value, not only a method's `self`. A pointer to a
        // non-struct is left as-is so its field access still reports `E0165`.
        let base = match base {
            Type::Ptr(pid) => match self.structs.ptr_pointee(pid) {
                s @ Type::Struct(_) => s,
                _ => base,
            },
            _ => base,
        };
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

    /// Resolve the type of an lvalue place for `&place` (SPEC §15.1). A place is
    /// a value identifier, a field chain, an index (into an array or slice), or
    /// a deref. Unlike [`resolve_place`], address-of does **not** require
    /// mutability — a pointer to an immutable binding is allowed — so a `const`
    /// / parameter root is accepted. Anything that is not a place is `E0231`.
    fn resolve_lvalue_type(&mut self, place: &Expr) -> Option<Type> {
        match place {
            Expr::Ident { name, span } => match self.lookup(name) {
                Some((ty, _)) => Some(ty),
                None => {
                    self.error(*span, "E0100", format!("unknown name `{}`", name));
                    None
                }
            },
            Expr::Field { base, field, span } => {
                let bt = self.resolve_lvalue_type(base)?;
                self.field_type_of(bt, field, *span)
            }
            Expr::Index { base, index, span } => {
                self.check_index_is_int(index);
                let bt = self.resolve_lvalue_type(base)?;
                match bt {
                    Type::Array(id) => Some(self.structs.array_elem(id)),
                    Type::Slice(id) => Some(self.structs.slice_elem(id)),
                    other => {
                        let msg = format!(
                            "cannot index into non-array type `{}`",
                            self.type_name(other)
                        );
                        self.error(*span, "E0220", msg);
                        None
                    }
                }
            }
            Expr::Deref { expr, span } => match self.check_expr(expr, None) {
                Some(Type::Ptr(id)) => Some(self.structs.ptr_pointee(id)),
                Some(other) => {
                    let msg = format!(
                        "`.*` requires a pointer (`*T`) operand, found `{}`",
                        self.type_name(other)
                    );
                    self.error(*span, "E0230", msg);
                    None
                }
                None => None,
            },
            _ => {
                self.error(
                    place.span(),
                    "E0231",
                    "`&` requires an lvalue (a variable, a field, an array/slice element, \
                     or a pointer dereference)",
                );
                self.check_expr(place, None);
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
            // A floating-point literal `3.14` is always `f64` (v0.144, SPEC §38).
            // Unlike integer literals it is not polymorphic — `f64` is the only
            // float type, and there is no implicit int↔float mixing.
            Expr::Float { .. } => Some(Type::F64),
            Expr::Bool { .. } => Some(Type::Bool),
            // A string literal `"…"` is a value of type `[]u8` — a slice over
            // static bytes (SPEC §23.1). It reuses the slice machinery, so the
            // interned `[]u8` slice type carries every slice operation already:
            // `s.len` (`usize`), `s[i]` (`u8`), `s[lo..hi]` (`[]u8`).
            Expr::StrLit { .. } => Some(Type::Slice(self.structs.intern_slice(Type::U8))),
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
                // top-level consts (plus any comptime value parameters bound in
                // the active instantiation, SPEC §24.2). Its type follows the
                // folded value (with integer-literal polymorphism applied to int
                // results).
                let env = self.const_env();
                match const_eval::eval(inner, &env) {
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
            // An anonymous `struct { … }` type value (v0.129) is *only* valid as
            // the body of a type-returning function, which is validated in Pass 1
            // and never body-checked here. Reaching `check_expr` means it appeared
            // in an ordinary value position — `E0310`.
            Expr::StructType { span, .. } => {
                self.error(
                    *span,
                    "E0310",
                    "a `struct { … }` type value may only be the body of a type-returning \
                     function (`fn Name(comptime T: type) type`)",
                );
                None
            }
            Expr::Field { base, field, span } => {
                // `Enum.Variant` — a qualified enum literal (SPEC §13.1). It is
                // recognised when the base is an identifier naming an enum type
                // that is not shadowed by a value in scope (mirroring the
                // associated-call rule in `check_method_call`), and is handled
                // *before* struct field access so `Color.Red` is an enum value,
                // not field access on a value named `Color`.
                if let Expr::Ident { name, .. } = base.as_ref() {
                    if self.lookup(name).is_none() {
                        if let Some(eid) = self.structs.enum_id_of(name) {
                            return self.check_enum_variant(eid, field, *span);
                        }
                    }
                }
                let bt = self.check_expr(base, None)?;
                // `.len` on an array is its compile-time-constant length and on a
                // slice its runtime length — both `usize` (SPEC §14.1, §15.2).
                // Any other field on an array/slice falls through to
                // `field_type_of`, which reports it as `E0165`.
                if field == "len" && matches!(bt, Type::Array(_) | Type::Slice(_)) {
                    return Some(Type::Usize);
                }
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
            // `error.Name` registers `Name` in the implicit global error set and
            // coerces to any expected `!T` (SPEC §12.1). With no error-union
            // expectation at this position it is `E0193`.
            Expr::ErrorLit { name, span } => {
                self.structs.intern_error(name);
                match expected {
                    Some(Type::ErrorUnion(id)) => Some(Type::ErrorUnion(id)),
                    _ => {
                        let msg = format!(
                            "error value `error.{}` has no expected error-union type here; \
                             use it where an `!T` is expected",
                            name
                        );
                        self.error(*span, "E0193", msg);
                        None
                    }
                }
            }
            // A `try` reaching `check_expr` is not at a statement-level position
            // (those are routed through `check_value_with_try`), so it is
            // `E0191` (SPEC §12.1). The operand is still checked to surface its
            // own errors.
            Expr::Try { expr: inner, span } => {
                self.error(
                    *span,
                    "E0191",
                    "`try` is only allowed as the whole value of a `var`/`const` initializer, \
                     a `return`, or an expression statement",
                );
                self.check_expr(inner, None);
                None
            }
            // `.Variant` — an unqualified enum literal whose enum type comes from
            // the expected type at this position (SPEC §13.1). With no expected
            // enum type it is `E0215`.
            Expr::EnumLit { variant, span } => match expected {
                Some(Type::Enum(id)) => self.check_enum_variant(id, variant, *span),
                _ => {
                    let msg = format!(
                        "enum literal `.{}` has no expected enum type here; \
                         use it where an enum is expected or write `Enum.{}`",
                        variant, variant
                    );
                    self.error(*span, "E0215", msg);
                    None
                }
            },
            // `expr catch default` / `expr catch |e| default`: `expr` must be `!T`
            // (else `E0192`); `default` is a `T`; the result is `T` (SPEC §12.1).
            // With a capture, `e` binds the error code (an immutable `i32`) only
            // inside `default` (SPEC §36.1); the non-capturing form is unchanged.
            Expr::Catch {
                expr: inner,
                capture,
                default,
                span,
            } => {
                let inner_expected = self.as_error_union_expectation(expected);
                match self.check_expr(inner, inner_expected) {
                    Some(Type::ErrorUnion(id)) => {
                        let payload = self.structs.error_union_payload(id);
                        if let Some(dt) = self.check_catch_default(capture, default, Some(payload)) {
                            if dt != payload {
                                let msg = format!(
                                    "`catch` default type mismatch: expected `{}`, found `{}`",
                                    self.type_name(payload),
                                    self.type_name(dt)
                                );
                                self.error(default.span(), "E0110", msg);
                            }
                        }
                        Some(payload)
                    }
                    Some(other) => {
                        let msg = format!(
                            "`catch` requires an error-union (`!T`) left operand, found `{}`",
                            self.type_name(other)
                        );
                        self.error(*span, "E0192", msg);
                        // Still check the default to surface its own errors (with
                        // the capture bound, so a handler that uses `e` does not
                        // also cascade an unknown-name error).
                        self.check_catch_default(capture, default, None);
                        None
                    }
                    None => {
                        self.check_catch_default(capture, default, None);
                        None
                    }
                }
            }
            // An array literal `[N]T{ e0, … }` (SPEC §14.2): `elem` resolves to
            // the array type `Type::Array(id)`; the literal must hold exactly
            // `N` elements (`E0221`), each coercing to the element type
            // (`E0110`). The result is `Type::Array(id)`.
            Expr::ArrayLit { elem, elems, span } => {
                match self.resolve_type(elem) {
                    Some(Type::Array(id)) => {
                        let elem_ty = self.structs.array_elem(id);
                        let len = self.structs.array_len(id);
                        if elems.len() != len {
                            let msg = format!(
                                "array literal has {} element(s), but type `{}` expects {}",
                                elems.len(),
                                self.type_name(Type::Array(id)),
                                len
                            );
                            self.error(*span, "E0221", msg);
                        }
                        for e in elems {
                            if let Some(et) = self.check_coerce(e, elem_ty) {
                                if et != elem_ty {
                                    let msg = format!(
                                        "array element type mismatch: expected `{}`, found `{}`",
                                        self.type_name(elem_ty),
                                        self.type_name(et)
                                    );
                                    self.error(e.span(), "E0110", msg);
                                }
                            }
                        }
                        Some(Type::Array(id))
                    }
                    // `elem` did not resolve to an array (its element type is
                    // unknown — already reported by `resolve_type`). Still check
                    // the elements so their own errors surface.
                    _ => {
                        for e in elems {
                            self.check_expr(e, None);
                        }
                        None
                    }
                }
            }
            // Indexing `base[index]` (read): `base` must be an array (SPEC
            // §14.2) or a slice (SPEC §15.2, `E0220` otherwise) and `index` an
            // integer; the result is the element type.
            Expr::Index { base, index, span } => {
                let bt = self.check_expr(base, None);
                self.check_index_is_int(index);
                match bt {
                    Some(Type::Array(id)) => Some(self.structs.array_elem(id)),
                    Some(Type::Slice(id)) => Some(self.structs.slice_elem(id)),
                    Some(other) => {
                        let msg = format!(
                            "cannot index into non-array, non-slice type `{}`",
                            self.type_name(other)
                        );
                        self.error(*span, "E0220", msg);
                        None
                    }
                    None => None,
                }
            }
            // `&place` (SPEC §15.1): `place` must be an lvalue (`E0231`); the
            // result is a pointer to its type.
            Expr::AddrOf { place, .. } => {
                let pt = self.resolve_lvalue_type(place)?;
                Some(Type::Ptr(self.structs.intern_ptr(pt)))
            }
            // `expr.*` (SPEC §15.1): `expr` must be a pointer (`E0230`); the
            // result is its pointee.
            Expr::Deref { expr: inner, span } => match self.check_expr(inner, None)? {
                Type::Ptr(id) => Some(self.structs.ptr_pointee(id)),
                other => {
                    let msg = format!(
                        "`.*` requires a pointer (`*T`) operand, found `{}`",
                        self.type_name(other)
                    );
                    self.error(*span, "E0230", msg);
                    None
                }
            },
            // Slicing `base[lo..hi]` (SPEC §15.2): `base` is an array — which
            // must be an addressable place, since the slice borrows its storage
            // — or a slice; `lo`/`hi` are integers; the result is `[]T`. Any
            // other base is `E0232`.
            Expr::SliceExpr { base, lo, hi, span } => {
                let bt = self.check_expr(base, None);
                self.check_slice_bound(lo);
                self.check_slice_bound(hi);
                match bt {
                    Some(Type::Array(id)) => {
                        if !is_addressable_place(base) {
                            self.error(
                                *span,
                                "E0232",
                                "can only slice an addressable array (a variable, or a \
                                 field/element of one)",
                            );
                        }
                        let elem = self.structs.array_elem(id);
                        Some(Type::Slice(self.structs.intern_slice(elem)))
                    }
                    Some(Type::Slice(id)) => {
                        let elem = self.structs.slice_elem(id);
                        Some(Type::Slice(self.structs.intern_slice(elem)))
                    }
                    Some(other) => {
                        let msg = format!(
                            "cannot slice non-array, non-slice type `{}`",
                            self.type_name(other)
                        );
                        self.error(*span, "E0232", msg);
                        None
                    }
                    None => None,
                }
            }
            // `unreachable` (v0.141, SPEC §35.1) — a diverging expression: it
            // never returns, so in a value position it adopts the *expected*
            // type (e.g. `else => unreachable`, `var x: i32 = unreachable`, the
            // `x orelse unreachable` form). With no expectation (a bare
            // statement) it is `void`. It is never a type error; the trap (exit
            // 101) is emitted by the backend.
            Expr::Unreachable { .. } => Some(expected.unwrap_or(Type::Void)),
            // A comptime reflection builtin `@name(T)` in expression position
            // (v0.136, SPEC §32.1). The single argument names a *type*, resolved
            // exactly like `alloc`'s type argument (`resolve_type_arg`, which is
            // substitution-aware so `@sizeOf(T)` / `@typeName(T)` work inside a
            // generic body). `@sizeOf(T)` yields `usize`; `@typeName(T)` yields a
            // `[]u8` string (the §23 slice). `@panic(msg)` (v0.141) is a diverging
            // runtime-safety builtin (handled below). An unknown `@name`, or a
            // wrong argument count, is `E0320`.
            Expr::Builtin { name, args, span } => match name.as_str() {
                "sizeOf" | "typeName" => {
                    if args.len() != 1 {
                        let msg = format!(
                            "`@{}` takes exactly 1 type argument, found {}",
                            name,
                            args.len()
                        );
                        self.error(*span, "E0320", msg);
                        return None;
                    }
                    // Validate (and intern) the type argument; the result type of
                    // each builtin does not otherwise depend on which type it is.
                    let _ = self.resolve_type_arg(&args[0]);
                    if name == "sizeOf" {
                        Some(Type::Usize)
                    } else {
                        Some(Type::Slice(self.structs.intern_slice(Type::U8)))
                    }
                }
                "as" => {
                    // `@as(T, e)` casts a numeric value `e` to numeric type `T`
                    // (SPEC §33, extended for `f64` in v0.144). Both the target and
                    // the value may now be any numeric type — an integer or `f64` —
                    // so `@as(f64, n)` (int→float) and `@as(i32, x)` (float→int)
                    // both type-check; a non-numeric target/value is `E0321`.
                    if args.len() != 2 {
                        self.error(
                            *span,
                            "E0320",
                            format!("`@as` takes a type and a value, found {} arguments", args.len()),
                        );
                        return None;
                    }
                    let target = self.resolve_type_arg(&args[0]);
                    if let Some(t) = target {
                        if !t.is_numeric() {
                            self.error(
                                args[0].span(),
                                "E0321",
                                format!("`@as` target must be a numeric type (an integer or `f64`), found `{}`", self.type_name(t)),
                            );
                        }
                    }
                    if let Some(et) = self.check_expr(&args[1], None) {
                        if !et.is_numeric() {
                            self.error(
                                args[1].span(),
                                "E0321",
                                format!("`@as` value must be a number (an integer or `f64`), found `{}`", self.type_name(et)),
                            );
                        }
                    }
                    target
                }
                "panic" => {
                    // `@panic(msg)` (v0.141, SPEC §35) — a diverging runtime-safety
                    // builtin: write `msg` (a `[]u8`, the §23 string) to stderr and
                    // `exit(101)` (the panic convention). Exactly one argument, a
                    // `[]u8`; a wrong argument count is `E0320` (the `@`-builtin
                    // arity code). Because it diverges (never returns), the result
                    // type ADOPTS the *expected* type — so `@panic(…)` type-checks
                    // anywhere a value is expected (`x orelse @panic(…)`,
                    // `else => @panic(…)`, `var x: i32 = @panic(…)`); with no
                    // expectation (a bare statement) it is `void`.
                    if args.len() != 1 {
                        self.error(
                            *span,
                            "E0320",
                            format!(
                                "`@panic` takes exactly 1 argument (a `[]u8` message), found {}",
                                args.len()
                            ),
                        );
                        return None;
                    }
                    // Check the message against the expected `[]u8` (so e.g. a
                    // string literal types as `[]u8`); any non-`[]u8` argument is a
                    // type error (`E0110`). The result still adopts the expected
                    // type — the divergence is independent of the argument.
                    let u8_slice = Type::Slice(self.structs.intern_slice(Type::U8));
                    if let Some(at) = self.check_expr(&args[0], Some(u8_slice)) {
                        let is_u8_slice =
                            matches!(at, Type::Slice(id) if self.structs.slice_elem(id) == Type::U8);
                        if !is_u8_slice {
                            self.error(
                                args[0].span(),
                                "E0110",
                                format!(
                                    "`@panic` message must be a `[]u8`, found `{}`",
                                    self.type_name(at)
                                ),
                            );
                        }
                    }
                    Some(expected.unwrap_or(Type::Void))
                }
                "intFromEnum" => {
                    // `@intFromEnum(e)` (v0.143, SPEC §37) — the integer value of
                    // an enum value `e`. Exactly one argument, which must itself be
                    // an enum value (`Type::Enum`); the result is `i64`. A wrong
                    // argument count is `E0320`; a non-enum argument is `E0321`.
                    if args.len() != 1 {
                        self.error(
                            *span,
                            "E0320",
                            format!(
                                "`@intFromEnum` takes exactly 1 argument (an enum value), found {}",
                                args.len()
                            ),
                        );
                        return None;
                    }
                    if let Some(at) = self.check_expr(&args[0], None) {
                        if !matches!(at, Type::Enum(_)) {
                            self.error(
                                args[0].span(),
                                "E0321",
                                format!(
                                    "`@intFromEnum` requires an enum value, found `{}`",
                                    self.type_name(at)
                                ),
                            );
                        }
                    }
                    Some(Type::I64)
                }
                "enumFromInt" => {
                    // `@enumFromInt(E, n)` (v0.143, SPEC §37) — the value of enum
                    // type `E` whose integer value is `n` (no range check in
                    // v0.143). Two arguments: the first *names* an enum type `E`
                    // (resolved like `alloc`'s type argument — substitution-aware),
                    // the second is an integer; the result type is `E`. A wrong
                    // argument count is `E0320`; a first argument that does not name
                    // an enum type, or a non-integer value, is `E0321`.
                    if args.len() != 2 {
                        self.error(
                            *span,
                            "E0320",
                            format!(
                                "`@enumFromInt` takes an enum type and an integer, found {} arguments",
                                args.len()
                            ),
                        );
                        return None;
                    }
                    let result = match self.resolve_type_arg(&args[0]) {
                        Some(Type::Enum(id)) => Some(Type::Enum(id)),
                        Some(other_ty) => {
                            self.error(
                                args[0].span(),
                                "E0321",
                                format!(
                                    "`@enumFromInt`'s first argument must name an enum type, found `{}`",
                                    self.type_name(other_ty)
                                ),
                            );
                            None
                        }
                        // `resolve_type_arg` already reported that it names no type.
                        None => None,
                    };
                    if let Some(vt) = self.check_expr(&args[1], None) {
                        if !vt.is_int() {
                            self.error(
                                args[1].span(),
                                "E0321",
                                format!(
                                    "`@enumFromInt`'s value must be an integer, found `{}`",
                                    self.type_name(vt)
                                ),
                            );
                        }
                    }
                    result
                }
                "readFile" => {
                    // `@readFile(a, path)` (v0.148, SPEC §41) — read the whole file
                    // named by `path` (a `[]u8`) into a fresh `[]u8` allocated on the
                    // `Allocator` `a`. Exactly two arguments: an `Allocator` and a
                    // `[]u8` path; the result is a `[]u8`. A wrong argument count is
                    // `E0320`; a non-`Allocator` first argument is `E0321`, a
                    // non-`[]u8` path is `E0110`. On any open/read error the runtime
                    // helper yields an empty slice (there is no `![]u8` to express the
                    // error — the optional/error-union named-type-only limit, §11/§12).
                    let u8_slice = Type::Slice(self.structs.intern_slice(Type::U8));
                    if args.len() != 2 {
                        self.error(
                            *span,
                            "E0320",
                            format!(
                                "`@readFile` takes exactly 2 arguments (an `Allocator` and a `[]u8` path), found {}",
                                args.len()
                            ),
                        );
                        // Still validate every argument expression.
                        for a in args {
                            self.check_expr(a, None);
                        }
                        return None;
                    }
                    // arg0 must be an `Allocator`.
                    if let Some(at) = self.check_expr(&args[0], Some(Type::Allocator)) {
                        if at != Type::Allocator {
                            self.error(
                                args[0].span(),
                                "E0321",
                                format!(
                                    "`@readFile`'s first argument must be an `Allocator`, found `{}`",
                                    self.type_name(at)
                                ),
                            );
                        }
                    }
                    // arg1 must be a `[]u8` path.
                    if let Some(pt) = self.check_expr(&args[1], Some(u8_slice)) {
                        let is_u8_slice = matches!(
                            pt,
                            Type::Slice(id) if self.structs.slice_elem(id) == Type::U8
                        );
                        if !is_u8_slice {
                            self.error(
                                args[1].span(),
                                "E0110",
                                format!(
                                    "`@readFile`'s path must be a `[]u8`, found `{}`",
                                    self.type_name(pt)
                                ),
                            );
                        }
                    }
                    Some(u8_slice)
                }
                "readLine" => {
                    // `@readLine(a)` (v0.148, SPEC §41) — read one line from stdin
                    // (without the trailing newline) into a fresh `[]u8` allocated on
                    // the `Allocator` `a`. Exactly one argument, an `Allocator`; the
                    // result is a `[]u8`. A wrong argument count is `E0320`; a
                    // non-`Allocator` argument is `E0321`. An empty line / EOF yields a
                    // zero-length slice.
                    let u8_slice = Type::Slice(self.structs.intern_slice(Type::U8));
                    if args.len() != 1 {
                        self.error(
                            *span,
                            "E0320",
                            format!(
                                "`@readLine` takes exactly 1 argument (an `Allocator`), found {}",
                                args.len()
                            ),
                        );
                        for a in args {
                            self.check_expr(a, None);
                        }
                        return None;
                    }
                    if let Some(at) = self.check_expr(&args[0], Some(Type::Allocator)) {
                        if at != Type::Allocator {
                            self.error(
                                args[0].span(),
                                "E0321",
                                format!(
                                    "`@readLine`'s argument must be an `Allocator`, found `{}`",
                                    self.type_name(at)
                                ),
                            );
                        }
                    }
                    Some(u8_slice)
                }
                other => {
                    let msg = format!(
                        "unknown `@`-builtin `@{}` (expected `@sizeOf`, `@typeName`, `@as`, `@panic`, `@intFromEnum`, `@enumFromInt`, `@readFile`, or `@readLine`)",
                        other
                    );
                    self.error(*span, "E0320", msg);
                    None
                }
            },
        }
    }

    /// Type-check an array index expression and verify it is an integer type
    /// (reusing `E0110`). Any non-integer index is reported; a flexible integer
    /// literal index defaults to `i64`, which is accepted.
    fn check_index_is_int(&mut self, index: &Expr) {
        if let Some(it) = self.check_expr(index, None) {
            if !it.is_int() {
                let msg = format!("array index must be an integer, found `{}`", self.type_name(it));
                self.error(index.span(), "E0110", msg);
            }
        }
    }

    /// Type-check a slice-bound expression (`lo` / `hi` in `a[lo..hi]`) and
    /// verify it is an integer type (reusing `E0110`, SPEC §15.2).
    fn check_slice_bound(&mut self, bound: &Expr) {
        if let Some(bt) = self.check_expr(bound, None) {
            if !bt.is_int() {
                let msg = format!("slice bound must be an integer, found `{}`", self.type_name(bt));
                self.error(bound.span(), "E0110", msg);
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

    /// The error-union analogue of [`as_optional_expectation`]: when a `catch`'s
    /// *result* is expected to be `T`, its left operand should be `!T`. An
    /// expected payload `T` becomes `!T` (interning it if necessary), an
    /// already-`!T` expectation is kept, and no expectation stays `None`.
    fn as_error_union_expectation(&mut self, expected: Option<Type>) -> Option<Type> {
        match expected {
            Some(t @ Type::ErrorUnion(_)) => Some(t),
            Some(other) => Some(Type::ErrorUnion(self.structs.intern_error_union(other))),
            None => None,
        }
    }

    /// Type-check a `catch` handler `default` expecting type `expected`. For the
    /// capturing form `expr catch |name| default` (SPEC §36.1) the capture
    /// `name` is bound as an immutable `i32` (the error code) in a fresh scope
    /// wrapping `default`, so it is visible only inside the handler — mirroring
    /// the optional-`if` and union-`switch` payload captures. For the
    /// non-capturing form (`capture == None`) this is just [`check_expr`], so
    /// the existing behaviour is unchanged. Returns the handler's type.
    fn check_catch_default(
        &mut self,
        capture: &Option<String>,
        default: &Expr,
        expected: Option<Type>,
    ) -> Option<Type> {
        match capture {
            Some(name) => {
                self.scopes.push(HashMap::new());
                self.define(name, Type::I32, true);
                let dt = self.check_expr(default, expected);
                self.scopes.pop();
                dt
            }
            None => self.check_expr(default, expected),
        }
    }

    /// Type-check `expr` against an expected type, applying optional coercion
    /// (SPEC §11.2) and error-union coercion (SPEC §12.2). When `expected` is
    /// `?T`, this accepts:
    /// - a `null` literal (which adopts `?T`),
    /// - a value whose type is the inner `T` (which widens to `?T`), or
    /// - a value already of type `?T`.
    /// When `expected` is `!T`, this accepts:
    /// - an `error.X` literal (which adopts `!T`),
    /// - a value whose type is the payload `T` (which widens to `!T`), or
    /// - a value already of type `!T`.
    /// In each accepting case the expected composite type is returned so the
    /// caller's equality check passes. Any other type is returned unchanged for
    /// the caller to report as `E0110`. For a plain `expected`, this is just
    /// [`check_expr`] with that expectation.
    fn check_coerce(&mut self, expr: &Expr, expected: Type) -> Option<Type> {
        match expected {
            Type::Optional(id) => {
                // `null` adopts the optional type directly.
                if matches!(expr, Expr::Null { .. }) {
                    return self.check_expr(expr, Some(expected));
                }
                // Otherwise check against the inner `T` so that integer literals
                // and nested constructs adopt it, then accept either `T`
                // (coerces) or an already-`?T` value.
                let inner = self.structs.optional_inner(id);
                let vt = self.check_expr(expr, Some(inner))?;
                if vt == inner || vt == expected {
                    Some(expected)
                } else {
                    Some(vt)
                }
            }
            Type::ErrorUnion(id) => {
                // `error.X` adopts the error-union type directly.
                if matches!(expr, Expr::ErrorLit { .. }) {
                    return self.check_expr(expr, Some(expected));
                }
                // Otherwise check against the payload `T` so integer literals
                // adopt it, then accept either `T` (coerces) or an already-`!T`
                // value.
                let payload = self.structs.error_union_payload(id);
                let vt = self.check_expr(expr, Some(payload))?;
                if vt == payload || vt == expected {
                    Some(expected)
                } else {
                    Some(vt)
                }
            }
            _ => self.check_expr(expr, Some(expected)),
        }
    }

    /// Check the value expression of a statement-level position — a `var`/`const`
    /// initializer, a `return`, or an expression statement — where a *top-level*
    /// `try` is permitted (SPEC §12.1). If `value` is a `try`, it is handled by
    /// [`check_try`] (yielding the operand's payload, then coerced to `declared`
    /// if given); otherwise it is checked normally, so a `try` nested anywhere
    /// inside is reported as `E0191` by [`check_expr`].
    fn check_value_with_try(&mut self, value: &Expr, declared: Option<Type>) -> Option<Type> {
        if let Expr::Try { expr: inner, span } = value {
            let payload = self.check_try(inner, *span)?;
            Some(match declared {
                Some(dt) => self.coerce_type(payload, dt),
                None => payload,
            })
        } else {
            match declared {
                Some(dt) => self.check_coerce(value, dt),
                None => self.check_expr(value, None),
            }
        }
    }

    /// Type-check `try inner` at a statement-level position (SPEC §12.1). The
    /// enclosing function must return some `!U` (`E0190`) and `inner` must be an
    /// error union `!T` (`E0190`); the result is the payload `T`. (Propagation
    /// of the error is the backend's job — see SPEC §12.3.)
    fn check_try(&mut self, inner: &Expr, span: Span) -> Option<Type> {
        if !matches!(self.ret_type, Type::ErrorUnion(_)) {
            let msg = format!(
                "`try` requires the enclosing function to return an error union (`!T`), found `{}`",
                self.type_name(self.ret_type)
            );
            self.error(span, "E0190", msg);
        }
        match self.check_expr(inner, None)? {
            Type::ErrorUnion(id) => Some(self.structs.error_union_payload(id)),
            other => {
                let msg = format!(
                    "`try` requires an error-union (`!T`) operand, found `{}`",
                    self.type_name(other)
                );
                self.error(span, "E0190", msg);
                None
            }
        }
    }

    /// The type a value of type `from` takes at a position expecting `to`,
    /// applying the implicit widenings `T -> ?T` (§11.2) and `T -> !T` (§12.2).
    /// Used for a `try` result (a payload `T`) flowing into a `?T`/`!T` target.
    /// Returns `to` when the widening applies, otherwise `from` unchanged.
    fn coerce_type(&self, from: Type, to: Type) -> Type {
        match to {
            Type::Optional(id) if self.structs.optional_inner(id) == from => to,
            Type::ErrorUnion(id) if self.structs.error_union_payload(id) == from => to,
            _ => from,
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
        // by a value in scope → associated / static call. A struct name, a type
        // alias (v0.129, `const Alias = List(C);`), or `Self` / a struct-bound
        // type parameter (v0.130, the active generic-struct instantiation) may all
        // front such a call.
        if let Expr::Ident { name, .. } = receiver {
            if self.lookup(name).is_none() {
                let static_id = self
                    .structs
                    .id_of(name)
                    .or_else(|| match self.type_aliases.get(name) {
                        Some(Type::Struct(id)) => Some(*id),
                        _ => None,
                    })
                    .or_else(|| match self.subst.get(name) {
                        Some(Type::Struct(id)) => Some(*id),
                        _ => None,
                    });
                if let Some(id) = static_id {
                    return self.check_static_call(id, name, method, args, span);
                }
            }
        }
        // Case (a): evaluate the receiver as a value; it must be a struct or a
        // pointer-to-struct (SPEC §30.1: a `*Struct` receiver auto-derefs, so a
        // value-receiver method takes `*obj` by value and a pointer-receiver
        // method passes the pointer straight through).
        let recv_ty = self.check_expr(receiver, None)?;
        let (id, recv_is_ptr) = match recv_ty {
            Type::Struct(id) => (id, false),
            Type::Ptr(pid) => match self.structs.ptr_pointee(pid) {
                Type::Struct(id) => (id, true),
                _ => {
                    let msg = format!(
                        "type `{}` has no method `{}` (method calls require a struct receiver)",
                        self.type_name(recv_ty),
                        method
                    );
                    self.error(span, "E0170", msg);
                    for a in args {
                        self.check_expr(a, None);
                    }
                    return None;
                }
            },
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
        self.check_value_method_call(id, method, args, span, receiver, recv_is_ptr)
    }

    /// Resolve `value.method(args)` — a method call on a struct value (case a).
    /// `receiver` is the receiver expression and `recv_is_ptr` whether it was
    /// already a `*Struct` (auto-deref'd). For a **pointer-receiver** method
    /// (SPEC §30.2) a *value* receiver auto-refs `&obj`, so `obj` must be an
    /// addressable lvalue; a receiver that is already a pointer is passed through.
    fn check_value_method_call(
        &mut self,
        id: u32,
        method: &str,
        args: &[Expr],
        span: Span,
        receiver: &Expr,
        recv_is_ptr: bool,
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
        // SPEC §30.2: a pointer-receiver method passes `&obj`, so a *value*
        // receiver must be an addressable lvalue (a variable, field, or element
        // — the same lvalue set as `&`; a temporary is rejected, reusing the
        // address-of error `E0231`). A receiver that is already a pointer is
        // passed straight through (no addressability requirement); a
        // value-receiver method takes the receiver by value, unchanged.
        if sf.is_ptr_receiver && !recv_is_ptr && !is_addressable_place(receiver) {
            let sname = self.structs.get(id).name.clone();
            let msg = format!(
                "method `{}` of `{}` has a pointer receiver (`self: *{}`); its receiver must be \
                 an addressable lvalue (a variable, field, or element), not a temporary",
                method, sname, sname
            );
            self.error(receiver.span(), "E0231", msg);
            // Continue checking the arguments so their own diagnostics surface.
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
        // A `Name{ … }` literal whose name is a tagged union is *union
        // construction* (SPEC §20.2), not a struct literal — it reuses
        // `Expr::StructLit` but means "build this variant".
        if let Some(uid) = self.structs.union_id_of(name) {
            return self.check_union_lit(name, inits, span, uid);
        }
        // A type alias (v0.129) names a monomorphised struct: a literal
        // `Alias{ … }` builds that struct. v0.130: while checking a generic-struct
        // method, `Self` (and a type parameter bound to a struct) resolve through
        // the active type substitution, so `Self{ … }` builds the instantiated
        // struct. Aliases never name a union, so this follows the union check and
        // falls back to the ordinary struct lookup.
        let alias_id = match self.type_aliases.get(name) {
            Some(Type::Struct(id)) => Some(*id),
            _ => None,
        }
        .or_else(|| match self.subst.get(name) {
            Some(Type::Struct(id)) => Some(*id),
            _ => None,
        });
        let id = match alias_id.or_else(|| self.structs.id_of(name)) {
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

    /// Type-check a tagged-union construction `Name{ .v = e }` (SPEC §20.2).
    /// Exactly **one** initializer is required (`E0270`); it must name a variant
    /// of the union (`E0271`); its value must coerce to that variant's payload
    /// type (`E0110`). The result type is always `Type::Union(uid)` — even on a
    /// recoverable error — so a typed target avoids cascading diagnostics.
    fn check_union_lit(
        &mut self,
        name: &str,
        inits: &[FieldInit],
        span: Span,
        uid: u32,
    ) -> Option<Type> {
        if inits.len() != 1 {
            let msg = format!(
                "union `{}` is constructed with exactly one variant initializer, found {}",
                name,
                inits.len()
            );
            self.error(span, "E0270", msg);
            // Still check each initializer's value to surface its own errors,
            // typing it against the named variant's payload when one matches.
            for fi in inits {
                match self.structs.union_get(uid).payload_type(&fi.name) {
                    Some(p) => {
                        self.check_coerce(&fi.value, p);
                    }
                    None => {
                        self.check_expr(&fi.value, None);
                    }
                }
            }
            return Some(Type::Union(uid));
        }
        let fi = &inits[0];
        // The single initializer must name a variant of the union (`E0271`).
        let payload = match self.structs.union_get(uid).payload_type(&fi.name) {
            Some(p) => p,
            None => {
                let msg = format!("union `{}` has no variant `{}`", name, fi.name);
                self.error(fi.span, "E0271", msg);
                self.check_expr(&fi.value, None);
                return Some(Type::Union(uid));
            }
        };
        // The value must coerce to the variant's payload type (`E0110`).
        if let Some(vt) = self.check_coerce(&fi.value, payload) {
            if vt != payload {
                let msg = format!(
                    "union `{}` variant `{}` payload type mismatch: expected `{}`, found `{}`",
                    name,
                    fi.name,
                    self.type_name(payload),
                    self.type_name(vt)
                );
                self.error(fi.value.span(), "E0110", msg);
            }
        }
        Some(Type::Union(uid))
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
            // Bitwise complement (v0.132, SPEC §28.2): the operand must be an
            // integer and the result is that integer type. Mirrors `UnOp::Neg`,
            // but accepts any integer (signed or unsigned). An integer-literal
            // operand adopts `expected` when it is an integer type.
            UnOp::BitNot => {
                let t = self.check_expr(inner, expected.filter(|t| t.is_int()))?;
                if t.is_int() {
                    Some(t)
                } else {
                    let msg = format!(
                        "unary `~` requires an integer, found `{}`",
                        self.type_name(t)
                    );
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
                // Integer-literal polymorphism is anchored only on an *integer*
                // expectation; `f64` literals are concrete (not flexible), so the
                // helper resolves an `f64 + f64` to `(F64, F64)` on its own (the
                // operands anchor each other). `%` stays integer-only (no float
                // modulo in v0.144, SPEC §38).
                let (lt, rt) = self.check_int_operands(lhs, rhs, expected.filter(|t| t.is_int()));
                let lt = lt?;
                let rt = rt?;
                // `+ - * /` accept two `f64`s as well as two same-type integers;
                // `%` is integer-only.
                let float_ok = !matches!(op, BinOp::Rem);
                let operand_ok = |t: Type| t.is_int() || (float_ok && t.is_float());
                if !operand_ok(lt) {
                    let msg = if float_ok {
                        format!(
                            "arithmetic operand must be a number (an integer or `f64`), found `{}`",
                            self.type_name(lt)
                        )
                    } else {
                        format!(
                            "arithmetic operand must be an integer, found `{}`",
                            self.type_name(lt)
                        )
                    };
                    self.error(lhs.span(), "E0110", msg);
                    return None;
                }
                if !operand_ok(rt) {
                    let msg = if float_ok {
                        format!(
                            "arithmetic operand must be a number (an integer or `f64`), found `{}`",
                            self.type_name(rt)
                        )
                    } else {
                        format!(
                            "arithmetic operand must be an integer, found `{}`",
                            self.type_name(rt)
                        )
                    };
                    self.error(rhs.span(), "E0110", msg);
                    return None;
                }
                if lt != rt {
                    // A mix of `f64` and an integer is never implicitly converted:
                    // point the programmer at `@as` (SPEC §38).
                    let msg = if lt.is_float() != rt.is_float() {
                        format!(
                            "no implicit conversion between integer and float; \
                             cast with `@as`, found `{}` and `{}`",
                            self.type_name(lt),
                            self.type_name(rt)
                        )
                    } else {
                        format!(
                            "arithmetic operands must have the same type, found `{}` and `{}`",
                            self.type_name(lt),
                            self.type_name(rt)
                        )
                    };
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
                // An `Allocator` is an opaque interface value (SPEC §16): it
                // supports assignment / parameters / return but never comparison.
                if matches!(lt, Type::Allocator) || matches!(rt, Type::Allocator) {
                    self.error(
                        span,
                        "E0110",
                        "`Allocator` values do not support comparison",
                    );
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
            // Bitwise & shift (v0.132, SPEC §28.2): both operands must be the
            // **same integer type** and the result is that integer type
            // (`is_bool_result` is false for these). A shift's right operand is
            // also an integer; the result is the left operand's type. This reuses
            // the same operand rule as the arithmetic operators (the result is
            // never `bool`), so an integer-literal operand adopts the other
            // operand's concrete type. A non-integer operand / type mismatch is
            // the usual binop type error (`E0110`).
            BinOp::BitAnd | BinOp::BitOr | BinOp::BitXor | BinOp::Shl | BinOp::Shr => {
                let (lt, rt) = self.check_int_operands(lhs, rhs, expected.filter(|t| t.is_int()));
                let lt = lt?;
                let rt = rt?;
                if !lt.is_int() {
                    let msg = format!(
                        "`{}` requires integer operands, found `{}`",
                        op.c_op(),
                        self.type_name(lt)
                    );
                    self.error(lhs.span(), "E0110", msg);
                    return None;
                }
                if !rt.is_int() {
                    let msg = format!(
                        "`{}` requires integer operands, found `{}`",
                        op.c_op(),
                        self.type_name(rt)
                    );
                    self.error(rhs.span(), "E0110", msg);
                    return None;
                }
                if lt != rt {
                    let msg = format!(
                        "`{}` operands must have the same type, found `{}` and `{}`",
                        op.c_op(),
                        self.type_name(lt),
                        self.type_name(rt)
                    );
                    self.error(span, "E0110", msg);
                    return None;
                }
                Some(lt)
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

    /// Type-check the arithmetic of a compound assignment `place op= rhs`
    /// (v0.131, SPEC §27.2). The place type (`place_ty`) is already resolved by
    /// the caller; this enforces the **same rule as the binary arithmetic
    /// operators**: both the place and `rhs` must be the *same integer type*.
    /// The `rhs` is checked against `place_ty` (when integer) so a flexible
    /// integer literal adopts it, matching the binary-op integer-literal
    /// polymorphism. Reports the general type error `E0110` on a non-integer
    /// place, a non-integer `rhs`, or a mismatch; `span` locates the statement.
    fn check_compound_assign_arith(&mut self, place_ty: Type, rhs: &Expr, span: Span) {
        // Evaluate the rhs first (surfacing any of its own diagnostics), with
        // the place type as the expected type only when it is an integer.
        let rt = self.check_expr(rhs, Some(place_ty).filter(|t| t.is_int()));
        // The place must be an integer — arithmetic operands are integers.
        if !place_ty.is_int() {
            let msg = format!(
                "compound assignment requires an integer place, found `{}`",
                self.type_name(place_ty)
            );
            self.error(span, "E0110", msg);
            return;
        }
        let rt = match rt {
            Some(t) => t,
            None => return,
        };
        if !rt.is_int() {
            let msg = format!(
                "compound assignment requires an integer operand, found `{}`",
                self.type_name(rt)
            );
            self.error(rhs.span(), "E0110", msg);
            return;
        }
        if rt != place_ty {
            let msg = format!(
                "compound assignment operands must have the same type, found `{}` and `{}`",
                self.type_name(place_ty),
                self.type_name(rt)
            );
            self.error(span, "E0110", msg);
        }
    }

    /// Type-check the first argument of `alloc` / `free`, which must be an
    /// `Allocator` (SPEC §16.1). A mismatch is the general type error `E0110`.
    fn check_allocator_arg(&mut self, arg: &Expr, callee: &str) {
        if let Some(t) = self.check_expr(arg, Some(Type::Allocator)) {
            if t != Type::Allocator {
                let msg = format!(
                    "`{}` requires an `Allocator` as its first argument, found `{}`",
                    callee,
                    self.type_name(t)
                );
                self.error(arg.span(), "E0110", msg);
            }
        }
    }

    /// Resolve `alloc`'s second argument, which must be an identifier naming a
    /// type — a builtin (via [`Type::from_name`]), a struct, or an enum (SPEC
    /// §16.1). It is *not* type-checked as a value. A non-identifier argument or
    /// an identifier that names no type is `E0241`.
    fn resolve_type_arg(&mut self, arg: &Expr) -> Option<Type> {
        match arg {
            Expr::Ident { name, span } => {
                // A type-parameter name bound by the active substitution (a
                // generic function v0.120, or a generic-struct method v0.130)
                // resolves to its concrete type — so `alloc(a, T, n)` works
                // inside a generic body.
                let resolved = self
                    .subst
                    .get(name)
                    .copied()
                    .or_else(|| Type::from_name(name))
                    .or_else(|| self.structs.id_of(name).map(Type::Struct))
                    .or_else(|| self.structs.enum_id_of(name).map(Type::Enum));
                match resolved {
                    Some(t) => Some(t),
                    None => {
                        let msg = format!(
                            "`alloc`'s second argument `{}` does not name a type",
                            name
                        );
                        self.error(*span, "E0241", msg);
                        None
                    }
                }
            }
            other => {
                self.error(
                    other.span(),
                    "E0241",
                    "`alloc`'s second argument must be a type name",
                );
                None
            }
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
                    // `print` accepts an integer, an `f64` (v0.144, SPEC §38), or a
                    // string — a `[]u8` slice (SPEC §23.1). Any other type is
                    // rejected.
                    let is_string = match t {
                        Type::Slice(id) => self.structs.slice_elem(id) == Type::U8,
                        _ => false,
                    };
                    if !t.is_int() && !t.is_float() && !is_string {
                        let msg = format!(
                            "`print` requires an integer, `f64`, or string (`[]u8`) argument, found `{}`",
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
            // `c_allocator() -> Allocator` (SPEC §16): the malloc/free-backed
            // allocator. Takes no arguments; its result is always `Allocator`.
            "c_allocator" => {
                if !args.is_empty() {
                    self.error(
                        span,
                        "E0110",
                        format!(
                            "`c_allocator` takes no arguments, found {}",
                            args.len()
                        ),
                    );
                    for a in args {
                        self.check_expr(a, None);
                    }
                }
                Some(Type::Allocator)
            }
            // `alloc(a: Allocator, T, n: usize) -> []T` (SPEC §16). The second
            // argument is a *type name* (an identifier), resolved here — it is
            // never type-checked as a value. The result is the interned `[]T`.
            "alloc" => {
                if args.len() != 3 {
                    self.error(
                        span,
                        "E0110",
                        format!(
                            "`alloc` takes exactly 3 arguments (an `Allocator`, a type, and a count), found {}",
                            args.len()
                        ),
                    );
                    // Recover: check the allocator + any trailing value args, but
                    // never the slot that would hold the type name (index 1), to
                    // avoid a spurious "unknown name" for a type identifier.
                    if let Some(a) = args.first() {
                        self.check_allocator_arg(a, "alloc");
                    }
                    for a in args.iter().skip(2) {
                        self.check_expr(a, None);
                    }
                    return None;
                }
                // arg0: the allocator.
                self.check_allocator_arg(&args[0], "alloc");
                // arg2: the element count — any integer (a bare literal adopts
                // `usize`).
                if let Some(nt) = self.check_expr(&args[2], Some(Type::Usize)) {
                    if !nt.is_int() {
                        let msg = format!(
                            "`alloc` requires an integer count, found `{}`",
                            self.type_name(nt)
                        );
                        self.error(args[2].span(), "E0110", msg);
                    }
                }
                // arg1: the element *type* — an identifier naming a builtin,
                // struct or enum. Resolved without type-checking it as a value.
                match self.resolve_type_arg(&args[1]) {
                    Some(elem) => Some(Type::Slice(self.structs.intern_slice(elem))),
                    None => None,
                }
            }
            // `free(a: Allocator, s: []T) -> void` (SPEC §16): the second
            // argument must be a slice (`E0242`).
            "free" => {
                if args.len() != 2 {
                    self.error(
                        span,
                        "E0110",
                        format!(
                            "`free` takes exactly 2 arguments (an `Allocator` and a slice), found {}",
                            args.len()
                        ),
                    );
                    for a in args {
                        self.check_expr(a, None);
                    }
                    return Some(Type::Void);
                }
                self.check_allocator_arg(&args[0], "free");
                match self.check_expr(&args[1], None) {
                    Some(Type::Slice(_)) | None => {}
                    Some(other) => {
                        let msg = format!(
                            "`free` requires a slice (`[]T`) as its second argument, found `{}`",
                            self.type_name(other)
                        );
                        self.error(args[1].span(), "E0242", msg);
                    }
                }
                Some(Type::Void)
            }
            _ => {
                // A generic function (any `comptime` type parameter) resolves
                // through monomorphisation, never the ordinary signature path
                // (SPEC §17.2). Its first arguments are type arguments, not
                // values, so this must precede the `funcs` lookup.
                if let Some(gen) = self.generics.get(callee).cloned() {
                    return self.check_generic_call(&gen, args, span);
                }
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

    // ---- comptime generics (v0.120) + value parameters (v0.128) ------------

    /// Type-check a call `g(C1, …, Ck, a1, …)` to the generic function `gen`
    /// (SPEC §17.2 / §24.2). The leading arguments — one per `comptime`
    /// parameter — bind it: a *type* parameter to a `Type` (the argument names a
    /// concrete type), a *value* parameter to an `i64` (the argument
    /// const-evaluates to an integer). They build a type substitution and a value
    /// substitution under which the remaining (runtime) arguments are checked
    /// against the substituted parameter types (so `[n]T` becomes `[<value>]T`)
    /// and which yield the substituted return type. A newly-seen instantiation is
    /// recorded (keyed on the ordered comptime arguments) and its body
    /// type-checked under the substitutions, which may transitively discover
    /// further instantiations.
    fn check_generic_call(&mut self, gen: &Func, args: &[Expr], span: Span) -> Option<Type> {
        let comptime_params: Vec<&Param> = gen.params.iter().filter(|p| p.is_comptime).collect();
        let runtime_params: Vec<&Param> = gen.params.iter().filter(|p| !p.is_comptime).collect();
        let k = comptime_params.len();

        // Too few arguments to bind every type parameter (E0252). The provided
        // arguments are all (intended) type names, so they are not value-checked.
        if args.len() < k {
            let msg = format!(
                "generic function `{}` needs {} type argument(s), found {}",
                gen.name,
                k,
                args.len()
            );
            self.error(span, "E0252", msg);
            return None;
        }

        // Resolve the leading comptime arguments and build the substitutions. A
        // *type* parameter binds to a `Type` from an identifier naming a concrete
        // type (E0251); a *value* parameter binds to an `i64` obtained by
        // const-evaluating the argument (a non-constant value argument is E0253).
        // The ordered `comptime_args` form the instantiation key.
        let mut comptime_args: Vec<ComptimeArg> = Vec::with_capacity(k);
        let mut subst: HashMap<String, Type> = HashMap::new();
        let mut value_subst: HashMap<String, i64> = HashMap::new();
        let mut subst_ok = true;
        for (i, cp) in comptime_params.iter().enumerate() {
            if is_type_kw(&cp.ty) {
                match self.resolve_type_arg_generic(&args[i]) {
                    Some(t) => {
                        comptime_args.push(ComptimeArg::Type(t));
                        subst.insert(cp.name.clone(), t);
                    }
                    None => subst_ok = false,
                }
            } else {
                match self.eval_comptime_value_arg(&args[i]) {
                    Some(v) => {
                        comptime_args.push(ComptimeArg::Value(v));
                        value_subst.insert(cp.name.clone(), v);
                    }
                    None => subst_ok = false,
                }
            }
        }
        let runtime_args = &args[k..];
        if !subst_ok {
            // A comptime argument failed (E0251/E0253 already emitted); still
            // surface any errors in the runtime arguments, then bail to avoid
            // cascading mismatches against an incomplete substitution.
            for a in runtime_args {
                self.check_expr(a, None);
            }
            return None;
        }

        // The runtime-parameter types and return type, resolved under both the
        // type and value substitutions (so `[n]T` becomes the bound length).
        let mut param_tys: Vec<Type> = Vec::with_capacity(runtime_params.len());
        for p in &runtime_params {
            param_tys.push(
                self.resolve_type_opt_with(&p.ty, &subst, &value_subst)
                    .unwrap_or(Type::I64),
            );
        }
        let ret_ty = self
            .resolve_type_opt_with(&gen.ret, &subst, &value_subst)
            .unwrap_or(Type::Void);

        if runtime_args.len() != param_tys.len() {
            let msg = format!(
                "`{}` takes {} value argument(s) after {} type argument(s), found {}",
                gen.name,
                param_tys.len(),
                k,
                runtime_args.len()
            );
            self.error(span, "E0110", msg);
            for a in runtime_args {
                self.check_expr(a, None);
            }
            return Some(ret_ty);
        }

        // Check each runtime argument against its substituted parameter type,
        // applying the usual optional / error-union coercions (§11.2, §12.2).
        for (a, &pt) in runtime_args.iter().zip(param_tys.iter()) {
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

        // Record the instantiation; if newly seen, type-check the body under the
        // substitution (which may discover further instantiations through nested
        // generic calls). The dedup in `intern_instantiation` bounds recursion
        // for (mutually) recursive generics with identical type arguments.
        if self
            .structs
            .intern_instantiation(&gen.name, comptime_args)
        {
            self.check_instance_body(gen, subst, value_subst);
        }
        Some(ret_ty)
    }

    /// Resolve a generic call's comptime **value** argument (SPEC §24.2): it must
    /// const-evaluate to an integer over the in-scope constants (the top-level
    /// consts plus any comptime value parameters already bound, via
    /// [`const_env`]). A non-constant argument, or one that folds to a bool, is
    /// `E0253`.
    fn eval_comptime_value_arg(&mut self, arg: &Expr) -> Option<i64> {
        let env = self.const_env();
        match const_eval::eval(arg, &env) {
            Ok(ConstVal::Int(n)) => Some(n),
            Ok(ConstVal::Bool(_)) => {
                self.error(
                    arg.span(),
                    "E0253",
                    "comptime value argument must be an integer, found a `bool`",
                );
                None
            }
            Err(d) => {
                let msg = format!(
                    "comptime value argument is not a compile-time constant: {}",
                    d.message
                );
                self.error(arg.span(), "E0253", msg);
                None
            }
        }
    }

    /// Resolve a generic call's type argument (SPEC §17.2): it must be an
    /// identifier naming a concrete type — a bound type parameter (via the active
    /// substitution, for a generic calling another generic), a builtin, a
    /// struct, or an enum. Anything else is `E0251`.
    fn resolve_type_arg_generic(&mut self, arg: &Expr) -> Option<Type> {
        match arg {
            Expr::Ident { name, span } => {
                let resolved = self.subst.get(name).copied().or_else(|| self.resolve_base(name));
                match resolved {
                    Some(t) => Some(t),
                    None => {
                        let msg = format!("type argument `{}` does not name a type", name);
                        self.error(*span, "E0251", msg);
                        None
                    }
                }
            }
            other => {
                self.error(
                    other.span(),
                    "E0251",
                    "a generic call's type argument must be an identifier naming a type",
                );
                None
            }
        }
    }

    /// Whether `te` is a valid annotation for a comptime **value** parameter
    /// (SPEC §24.1): a bare integer type name (`usize`, `i32`, …) with no
    /// composite wrapper (`?`/`!`/`[N]`/`*`/`[]`). A bound type-parameter name is
    /// never an integer, so this consults only the base resolution.
    fn is_value_param_annotation(&self, te: &TypeExpr) -> bool {
        !te.optional
            && !te.error_union
            && te.array_len.is_none()
            && !te.pointer
            && !te.slice
            && self.resolve_base(&te.name).map_or(false, |t| t.is_int())
    }

    /// Type-check one monomorphised instantiation of a generic function: its
    /// body, under the substitution `subst` (SPEC §17.2). Saves and restores the
    /// whole per-function checking context (substitution, return type, test /
    /// loop state, scope stack) so this may be called re-entrantly from the
    /// middle of checking another body (a generic call nested inside a body).
    fn check_instance_body(
        &mut self,
        f: &Func,
        subst: HashMap<String, Type>,
        value_subst: HashMap<String, i64>,
    ) {
        let saved_subst = std::mem::replace(&mut self.subst, subst);
        let saved_value_subst = std::mem::replace(&mut self.value_subst, value_subst);
        let saved_ret = self.ret_type;
        let saved_ret_error_set = self.ret_error_set.take();
        let saved_in_test = self.in_test;
        let saved_loop = self.loop_depth;
        let saved_loop_labels = std::mem::take(&mut self.loop_labels);
        let saved_scopes = std::mem::take(&mut self.scopes);

        // With `self.subst` / `self.value_subst` active, the return type and
        // runtime-parameter types resolve their type-parameter uses to concrete
        // types and any `[n]T` to its bound length.
        self.ret_type = self.resolve_type(&f.ret).unwrap_or(Type::Void);
        self.ret_error_set = f.ret.error_set.clone();
        self.in_test = false;
        self.loop_depth = 0;
        self.scopes.push(HashMap::new());
        for p in &f.params {
            if p.is_comptime {
                // A comptime *type* parameter is not a runtime value. A comptime
                // *value* parameter is an immutable constant of its declared
                // integer type, usable in the body (SPEC §24.2).
                if is_type_kw(&p.ty) {
                    continue;
                }
                let pt = self.resolve_type(&p.ty).unwrap_or(Type::I64);
                self.define(&p.name, pt, true);
                continue;
            }
            let pt = self.resolve_type(&p.ty).unwrap_or(Type::I64);
            self.define(&p.name, pt, true);
        }
        self.check_block(&f.body);
        self.scopes.pop();

        self.subst = saved_subst;
        self.value_subst = saved_value_subst;
        self.ret_type = saved_ret;
        self.ret_error_set = saved_ret_error_set;
        self.in_test = saved_in_test;
        self.loop_depth = saved_loop;
        self.loop_labels = saved_loop_labels;
        self.scopes = saved_scopes;
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

/// Whether `e` denotes an addressable place — a variable, a field chain, an
/// array/slice index, or a pointer dereference (SPEC §15.1/§15.2). Slicing an
/// array requires this, because the slice borrows the array's storage.
fn is_addressable_place(e: &Expr) -> bool {
    matches!(
        e,
        Expr::Ident { .. } | Expr::Field { .. } | Expr::Index { .. } | Expr::Deref { .. }
    )
}

/// Whether `f` is a generic function — one with at least one `comptime` type
/// parameter (SPEC §17.1). Generic functions are monomorphised per concrete
/// instantiation, not checked or emitted directly.
fn is_generic(f: &Func) -> bool {
    f.params.iter().any(|p| p.is_comptime)
}

/// Whether a method's leading parameter `p` is a **pointer receiver** (SPEC §30)
/// for the enclosing struct named `struct_name`: a `self` whose annotated type is
/// a bare pointer (`*T`, no `?`/`!`/`[N]`/`[]` wrapper) to the enclosing struct
/// (`*Point`) or to `Self` (`*Self`). Such a method takes `self` as `Ptr(Struct)`
/// and mutates the receiver in place. A value receiver (`self: Point`/`self: Self`,
/// the pre-v0.134 form) returns false and is unchanged. The caller must already
/// have established that `p` is the `self` parameter (`p.name == "self"`).
fn is_ptr_receiver_param(p: &Param, struct_name: &str) -> bool {
    p.ty.pointer
        && !p.ty.optional
        && !p.ty.error_union
        && p.ty.array_len.is_none()
        && !p.ty.slice
        && (p.ty.name == struct_name || p.ty.name == "Self")
}

/// Whether a [`TypeExpr`] is exactly the bare type keyword `type` — the only
/// valid annotation for a `comptime` type parameter (SPEC §17.2). Any wrapper
/// (`?`/`!`/`[N]`/`*`/`[]`) or other name makes it not a plain `type`.
fn is_type_kw(te: &TypeExpr) -> bool {
    te.name == "type"
        && !te.optional
        && !te.error_union
        && te.array_len.is_none()
        && !te.pointer
        && !te.slice
}

/// Whether `f` is a **type-constructor** — a type-returning function
/// `fn Name(comptime T: type) type` (v0.129, SPEC §25). Recognised solely by its
/// return type being the bare `type` keyword; its parameter list and body are
/// validated separately ([`Checker::collect_type_ctor`]). A type-constructor is
/// compile-time only: neither checked as an ordinary function nor emitted.
fn is_type_ctor(f: &Func) -> bool {
    is_type_kw(&f.ret)
}

/// Extract the field list of a valid type-constructor's body (SPEC §25.2): its
/// body must be exactly `return struct { … };` — a single [`Stmt::Return`] of an
/// [`Expr::StructType`]. Returns `None` for any other body shape (reported as
/// `E0310`).
fn type_ctor_struct_fields(f: &Func) -> Option<&[FieldDecl]> {
    if f.body.stmts.len() != 1 {
        return None;
    }
    match &f.body.stmts[0] {
        Stmt::Return {
            value: Some(Expr::StructType { fields, .. }),
            ..
        } => Some(fields),
        _ => None,
    }
}

/// Extract the method list of a valid type-constructor's body (v0.130, SPEC
/// §26.2): like [`type_ctor_struct_fields`], but yields the
/// [`Expr::StructType`]'s `methods`. Returns `None` for any other body shape and
/// an empty slice for a fields-only generic struct (v0.129) — which therefore
/// registers no methods and is not recorded as an instance.
fn type_ctor_struct_methods(f: &Func) -> Option<&[Func]> {
    if f.body.stmts.len() != 1 {
        return None;
    }
    match &f.body.stmts[0] {
        Stmt::Return {
            value: Some(Expr::StructType { methods, .. }),
            ..
        } => Some(methods),
        _ => None,
    }
}

/// Whether `e` is a value whose type cannot be inferred without a contextual
/// expectation (SPEC §18.2), so an *un-annotated* `var`/`const` binding of it is
/// `E0260` ("cannot infer type; add an annotation"). These are exactly the
/// literals whose [`check_expr`] type comes solely from the expected type at
/// their position: a bare `null` ([`Expr::Null`]), an `error.X`
/// ([`Expr::ErrorLit`]), and an unqualified `.Variant` ([`Expr::EnumLit`]).
///
/// An array literal `[N]T{ … }` is **not** included: it always carries its
/// element type `T` (and length `N`), so its type is inferable even when empty.
fn needs_inference_context(e: &Expr) -> bool {
    matches!(
        e,
        Expr::Null { .. } | Expr::ErrorLit { .. } | Expr::EnumLit { .. }
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{
        ConstDecl, EnumDecl, EnumVariant, ErrorSetDecl, FieldDecl, FieldInit, Func, Param,
        StructDecl, TestBlock, UnionDecl, UnionVariant,
    };

    fn sp() -> Span {
        Span::DUMMY
    }
    fn te(name: &str) -> TypeExpr {
        TypeExpr {
            name: name.into(),
            optional: false,
            error_union: false,
            error_set: None,
            array_len: None,
            pointer: false,
            slice: false,
            span: sp(),
        }
    }
    /// An optional type expression `?name`.
    fn te_opt(name: &str) -> TypeExpr {
        TypeExpr {
            name: name.into(),
            optional: true,
            error_union: false,
            error_set: None,
            array_len: None,
            pointer: false,
            slice: false,
            span: sp(),
        }
    }
    /// An error-union type expression `!name` (the global error set, v0.115).
    fn te_err(name: &str) -> TypeExpr {
        TypeExpr {
            name: name.into(),
            optional: false,
            error_union: true,
            error_set: None,
            array_len: None,
            pointer: false,
            slice: false,
            span: sp(),
        }
    }
    /// A *named* error-union type expression `set!name` (v0.139): the error union
    /// over the named error set `set` with payload type `name`.
    fn te_err_set(set: &str, name: &str) -> TypeExpr {
        TypeExpr {
            name: name.into(),
            optional: false,
            error_union: true,
            error_set: Some(set.into()),
            array_len: None,
            pointer: false,
            slice: false,
            span: sp(),
        }
    }
    /// A fixed-size array type expression `[len]elem` with a literal length
    /// (v0.117).
    fn te_arr(elem: &str, len: i64) -> TypeExpr {
        TypeExpr {
            name: elem.into(),
            optional: false,
            error_union: false,
            error_set: None,
            array_len: Some(ArraySize::Lit(len)),
            pointer: false,
            slice: false,
            span: sp(),
        }
    }
    /// An array type expression `[name]elem` whose length is the comptime
    /// value-parameter `name` (v0.128).
    fn te_arr_param(elem: &str, name: &str) -> TypeExpr {
        TypeExpr {
            name: elem.into(),
            optional: false,
            error_union: false,
            error_set: None,
            array_len: Some(ArraySize::Param(name.into())),
            pointer: false,
            slice: false,
            span: sp(),
        }
    }
    /// A pointer type expression `*name` (v0.118).
    fn te_ptr(name: &str) -> TypeExpr {
        TypeExpr {
            name: name.into(),
            optional: false,
            error_union: false,
            error_set: None,
            array_len: None,
            pointer: true,
            slice: false,
            span: sp(),
        }
    }
    /// A slice type expression `[]name` (v0.118).
    fn te_slice(name: &str) -> TypeExpr {
        TypeExpr {
            name: name.into(),
            optional: false,
            error_union: false,
            error_set: None,
            array_len: None,
            pointer: false,
            slice: true,
            span: sp(),
        }
    }
    /// `&place` — address-of (v0.118).
    fn addr_of(place: Expr) -> Expr {
        Expr::AddrOf {
            place: Box::new(place),
            span: sp(),
        }
    }
    /// `expr.*` — pointer dereference (v0.118).
    fn deref(expr: Expr) -> Expr {
        Expr::Deref {
            expr: Box::new(expr),
            span: sp(),
        }
    }
    /// `base[lo..hi]` — slice expression (v0.118).
    fn slice_expr(base: Expr, lo: Expr, hi: Expr) -> Expr {
        Expr::SliceExpr {
            base: Box::new(base),
            lo: Box::new(lo),
            hi: Box::new(hi),
            span: sp(),
        }
    }
    /// `var name: *elem = value;`
    fn let_var_ptr(name: &str, elem: &str, value: Expr) -> Stmt {
        Stmt::Let {
            is_const: false,
            name: name.into(),
            ty: Some(te_ptr(elem)),
            value,
            span: sp(),
        }
    }
    /// `var name: []elem = value;`
    fn let_var_slice(name: &str, elem: &str, value: Expr) -> Stmt {
        Stmt::Let {
            is_const: false,
            name: name.into(),
            ty: Some(te_slice(elem)),
            value,
            span: sp(),
        }
    }
    /// An array literal `[len]elem{ elems... }`.
    fn array_lit(elem: &str, len: i64, elems: Vec<Expr>) -> Expr {
        Expr::ArrayLit {
            elem: te_arr(elem, len),
            elems,
            span: sp(),
        }
    }
    /// An index expression `base[idx]`.
    fn index(base: Expr, idx: Expr) -> Expr {
        Expr::Index {
            base: Box::new(base),
            index: Box::new(idx),
            span: sp(),
        }
    }
    /// A parameter of array type: `name: [len]elem`.
    fn param_arr(name: &str, elem: &str, len: i64) -> Param {
        Param {
            name: name.into(),
            ty: te_arr(elem, len),
            is_comptime: false,
            span: sp(),
        }
    }
    /// A parameter of array type whose length is a comptime value parameter:
    /// `name: [size]elem` (v0.128).
    fn param_arr_param(name: &str, elem: &str, size: &str) -> Param {
        Param {
            name: name.into(),
            ty: te_arr_param(elem, size),
            is_comptime: false,
            span: sp(),
        }
    }
    /// A parameter of slice type: `name: []elem`.
    fn param_slice(name: &str, elem: &str) -> Param {
        Param {
            name: name.into(),
            ty: te_slice(elem),
            is_comptime: false,
            span: sp(),
        }
    }
    /// `var name: [len]elem = value;`
    fn let_var_arr(name: &str, elem: &str, len: i64, value: Expr) -> Stmt {
        Stmt::Let {
            is_const: false,
            name: name.into(),
            ty: Some(te_arr(elem, len)),
            value,
            span: sp(),
        }
    }
    fn error_lit(name: &str) -> Expr {
        Expr::ErrorLit {
            name: name.into(),
            span: sp(),
        }
    }
    fn try_expr(e: Expr) -> Expr {
        Expr::Try {
            expr: Box::new(e),
            span: sp(),
        }
    }
    fn catch_expr(e: Expr, default: Expr) -> Expr {
        Expr::Catch {
            expr: Box::new(e),
            capture: None,
            default: Box::new(default),
            span: sp(),
        }
    }
    /// `expr catch |name| default` — the capturing handler form (v0.142, §36).
    fn catch_capture_expr(e: Expr, name: &str, default: Expr) -> Expr {
        Expr::Catch {
            expr: Box::new(e),
            capture: Some(name.into()),
            default: Box::new(default),
            span: sp(),
        }
    }
    /// A function with an arbitrary [`TypeExpr`] return type (e.g. `!i32`).
    fn func_te(name: &str, params: Vec<Param>, ret: TypeExpr, body: Vec<Stmt>) -> Item {
        Item::Func(Func {
            is_pub: false,
            name: name.into(),
            params,
            ret,
            body: block(body),
            span: sp(),
        })
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
            is_comptime: false,
            span: sp(),
        }
    }
    /// `var name: ?inner = value;`
    fn let_var_opt(name: &str, inner: &str, value: Expr) -> Stmt {
        Stmt::Let {
            is_const: false,
            name: name.into(),
            ty: Some(te_opt(inner)),
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
    /// `if (cond) |cap| { then } [else { els }]` — an optional-capture `if`
    /// (v0.125, SPEC §21.1). `els` (when `Some`) becomes an `else { … }` block.
    fn if_capture(cond: Expr, cap: &str, then: Vec<Stmt>, els: Option<Vec<Stmt>>) -> Stmt {
        Stmt::If {
            cond,
            capture: Some(cap.into()),
            then: block(then),
            els: els.map(|s| Box::new(Stmt::Block(block(s)))),
            span: sp(),
        }
    }
    /// `errdefer stmt;` (v0.125, SPEC §21.2).
    fn errdefer_stmt(stmt: Stmt) -> Stmt {
        Stmt::ErrDefer {
            stmt: Box::new(stmt),
            span: sp(),
        }
    }
    fn int(v: i64) -> Expr {
        Expr::Int { value: v, span: sp() }
    }
    fn boolean(v: bool) -> Expr {
        Expr::Bool { value: v, span: sp() }
    }
    /// A string literal `"…"` (v0.127); its type is `[]u8`.
    fn str_lit(s: &str) -> Expr {
        Expr::StrLit {
            value: s.into(),
            span: sp(),
        }
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
    fn unary(op: UnOp, e: Expr) -> Expr {
        Expr::Unary {
            op,
            expr: Box::new(e),
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
            is_comptime: false,
            span: sp(),
        }
    }
    /// A pointer-typed parameter `name: *pointee` — used for pointer receivers
    /// (`self: *Point` / `self: *Self`) and `*Struct` value parameters (v0.134).
    fn param_ptr(name: &str, pointee: &str) -> Param {
        Param {
            name: name.into(),
            ty: te_ptr(pointee),
            is_comptime: false,
            span: sp(),
        }
    }
    /// A `comptime IDENT: type` type parameter (v0.120).
    fn param_comptime(name: &str) -> Param {
        Param {
            name: name.into(),
            ty: te("type"),
            is_comptime: true,
            span: sp(),
        }
    }
    /// A `comptime IDENT: <int type>` comptime **value** parameter (v0.128).
    fn param_comptime_val(name: &str, ty: &str) -> Param {
        Param {
            name: name.into(),
            ty: te(ty),
            is_comptime: true,
            span: sp(),
        }
    }
    /// A `comptime IDENT: ty` parameter with an annotation that is neither
    /// `type` nor an integer type — used to exercise the `E0250` diagnostic.
    fn param_comptime_bad(name: &str, ty: &str) -> Param {
        Param {
            name: name.into(),
            ty: te(ty),
            is_comptime: true,
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
            ty: Some(te(ty)),
            value,
            span: sp(),
        })
    }
    /// A top-level `const NAME = value;` with **no** annotation (v0.121); the
    /// type is inferred from the comptime value.
    fn const_item_infer(name: &str, value: Expr) -> Item {
        Item::Const(ConstDecl {
            is_pub: false,
            name: name.into(),
            ty: None,
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
            op: None,
            value,
            span: sp(),
        }
    }
    /// A compound field/index assignment `place op= value;` (v0.131).
    fn field_assign_op(place: Expr, op: BinOp, value: Expr) -> Stmt {
        Stmt::FieldAssign {
            place,
            op: Some(op),
            value,
            span: sp(),
        }
    }
    /// `for (iter) |elem| { body }` (no index) or, when `index` is `Some`,
    /// `for (iter, 0..) |elem, index| { body }` (v0.133, SPEC §29).
    fn for_stmt(iter: Expr, elem: &str, index: Option<&str>, body: Vec<Stmt>) -> Stmt {
        Stmt::For {
            iter,
            elem: elem.into(),
            index: index.map(|s| s.into()),
            body: block(body),
            label: None,
            span: sp(),
        }
    }
    fn let_var(name: &str, ty: &str, value: Expr) -> Stmt {
        Stmt::Let {
            is_const: false,
            name: name.into(),
            ty: Some(te(ty)),
            value,
            span: sp(),
        }
    }
    fn let_const(name: &str, ty: &str, value: Expr) -> Stmt {
        Stmt::Let {
            is_const: true,
            name: name.into(),
            ty: Some(te(ty)),
            value,
            span: sp(),
        }
    }
    /// `var name = value;` — an *inferred* local (no annotation, v0.121).
    fn let_var_infer(name: &str, value: Expr) -> Stmt {
        Stmt::Let {
            is_const: false,
            name: name.into(),
            ty: None,
            value,
            span: sp(),
        }
    }
    /// `const name = value;` — an *inferred* local `const` (no annotation, v0.121).
    fn let_const_infer(name: &str, value: Expr) -> Stmt {
        Stmt::Let {
            is_const: true,
            name: name.into(),
            ty: None,
            value,
            span: sp(),
        }
    }
    fn assign(name: &str, value: Expr) -> Stmt {
        Stmt::Assign {
            name: name.into(),
            op: None,
            value,
            span: sp(),
        }
    }
    /// A compound name assignment `name op= value;` (v0.131).
    fn assign_op(name: &str, op: BinOp, value: Expr) -> Stmt {
        Stmt::Assign {
            name: name.into(),
            op: Some(op),
            value,
            span: sp(),
        }
    }
    fn ret(value: Option<Expr>) -> Stmt {
        Stmt::Return { value, span: sp() }
    }
    /// A `struct { … }` type value (v0.129) whose field types are plain names.
    fn struct_type_expr(fields: Vec<(&str, &str)>) -> Expr {
        Expr::StructType {
            methods: vec![],
            fields: field_decls(fields),
            span: sp(),
        }
    }
    /// A `struct { … }` type value whose field types are explicit [`TypeExpr`]s
    /// (so composite forms like `?T` can be exercised).
    fn struct_type_expr_te(fields: Vec<(&str, TypeExpr)>) -> Expr {
        Expr::StructType {
            methods: vec![],
            fields: fields
                .into_iter()
                .map(|(n, ty)| FieldDecl {
                    name: n.into(),
                    ty,
                    span: sp(),
                })
                .collect(),
            span: sp(),
        }
    }
    /// A type-constructor `fn Name(comptime P: type) type { return struct { … }; }`
    /// (v0.129) whose struct fields have plain-name types.
    fn type_ctor(name: &str, param: &str, fields: Vec<(&str, &str)>) -> Item {
        Item::Func(raw_func(
            name,
            vec![param_comptime(param)],
            "type",
            vec![ret(Some(struct_type_expr(fields)))],
        ))
    }
    /// A `struct { … }` type value with explicit field [`TypeExpr`]s **and**
    /// methods (v0.130).
    fn struct_type_expr_m(fields: Vec<(&str, TypeExpr)>, methods: Vec<Func>) -> Expr {
        Expr::StructType {
            methods,
            fields: fields
                .into_iter()
                .map(|(n, ty)| FieldDecl {
                    name: n.into(),
                    ty,
                    span: sp(),
                })
                .collect(),
            span: sp(),
        }
    }
    /// A type-constructor whose generic struct has methods (v0.130):
    /// `fn Name(comptime P: type) type { return struct { fields…; methods… }; }`.
    fn type_ctor_m(
        name: &str,
        param: &str,
        fields: Vec<(&str, TypeExpr)>,
        methods: Vec<Func>,
    ) -> Item {
        Item::Func(raw_func(
            name,
            vec![param_comptime(param)],
            "type",
            vec![ret(Some(struct_type_expr_m(fields, methods)))],
        ))
    }

    fn codes(items: Vec<Item>) -> Vec<&'static str> {
        let m = Module { items };
        match check(&m) {
            Ok(_) => vec![],
            Err(ds) => ds.iter().map(|d| d.code).collect(),
        }
    }

    /// Check a module that is expected to pass, returning the built
    /// [`StructTable`] (so tests can inspect e.g. recorded instantiations).
    fn check_ok(items: Vec<Item>) -> StructTable {
        let m = Module { items };
        match check(&m) {
            Ok(table) => table,
            Err(ds) => panic!(
                "expected program to type-check, got diagnostics: {:?}",
                ds.iter().map(|d| d.code).collect::<Vec<_>>()
            ),
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
            vec![Stmt::Break { target: None, span: sp() }],
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

    // ---- bitwise & shift operators (v0.132, SPEC §28) ---------------------

    #[test]
    fn bitwise_and_shift_ops_yield_int_type() {
        // fn f(a: i32, b: i32) i32 {
        //   var w: i32 = a & b; var x: i32 = a | b; var y: i32 = a ^ b;
        //   var z: i32 = a << b; return a >> b;
        // }
        // Each `let_var` asserts the operator result coerces to i32, and the
        // final `return` asserts `a >> b` is i32 — so every op yields i32.
        let items = vec![func(
            "f",
            vec![param("a", "i32"), param("b", "i32")],
            "i32",
            vec![
                let_var("w", "i32", bin(BinOp::BitAnd, ident("a"), ident("b"))),
                let_var("x", "i32", bin(BinOp::BitOr, ident("a"), ident("b"))),
                let_var("y", "i32", bin(BinOp::BitXor, ident("a"), ident("b"))),
                let_var("z", "i32", bin(BinOp::Shl, ident("a"), ident("b"))),
                ret(Some(bin(BinOp::Shr, ident("a"), ident("b")))),
            ],
        )];
        assert_eq!(codes(items), Vec::<&str>::new());
    }

    #[test]
    fn bitnot_yields_int_type() {
        // fn f(a: i32) i32 { return ~a; }
        let items = vec![func(
            "f",
            vec![param("a", "i32")],
            "i32",
            vec![ret(Some(unary(UnOp::BitNot, ident("a"))))],
        )];
        assert_eq!(codes(items), Vec::<&str>::new());
    }

    #[test]
    fn bitwise_ops_on_unsigned_ok() {
        // Bitwise ops (and `~`, unlike `-`) accept unsigned integers.
        // fn f(a: u8, b: u8) u8 { var x: u8 = a & b; return ~a; }
        let items = vec![func(
            "f",
            vec![param("a", "u8"), param("b", "u8")],
            "u8",
            vec![
                let_var("x", "u8", bin(BinOp::BitAnd, ident("a"), ident("b"))),
                ret(Some(unary(UnOp::BitNot, ident("a")))),
            ],
        )];
        assert_eq!(codes(items), Vec::<&str>::new());
    }

    #[test]
    fn bitwise_literal_operand_adopts_type() {
        // A flexible integer literal adopts the other operand's type:
        // fn f(a: i32) i32 { return a & 255; }
        let items = vec![func(
            "f",
            vec![param("a", "i32")],
            "i32",
            vec![ret(Some(bin(BinOp::BitAnd, ident("a"), int(255))))],
        )];
        assert_eq!(codes(items), Vec::<&str>::new());
    }

    #[test]
    fn bitwise_on_bool_is_e0110() {
        // fn f(a: bool, b: bool) void { var r: i32 = a & b; }
        let items = vec![func(
            "f",
            vec![param("a", "bool"), param("b", "bool")],
            "void",
            vec![let_var("r", "i32", bin(BinOp::BitAnd, ident("a"), ident("b")))],
        )];
        assert!(codes(items).contains(&"E0110"));
    }

    #[test]
    fn bitnot_on_bool_is_e0110() {
        // fn f() i32 { return ~true; }
        let items = vec![func(
            "f",
            vec![],
            "i32",
            vec![ret(Some(unary(UnOp::BitNot, boolean(true))))],
        )];
        assert!(codes(items).contains(&"E0110"));
    }

    #[test]
    fn bitwise_mixed_int_types_is_e0110() {
        // fn f(a: i32, b: i64) i32 { return a & b; } — same-type rule fails.
        let items = vec![func(
            "f",
            vec![param("a", "i32"), param("b", "i64")],
            "i32",
            vec![ret(Some(bin(BinOp::BitAnd, ident("a"), ident("b"))))],
        )];
        assert!(codes(items).contains(&"E0110"));
    }

    #[test]
    fn comparison_still_yields_bool() {
        // A comparison is unaffected by the bitwise additions: still `bool`.
        // fn f(a: i32, b: i32) bool { return a < b; }
        let items = vec![func(
            "f",
            vec![param("a", "i32"), param("b", "i32")],
            "bool",
            vec![ret(Some(bin(BinOp::Lt, ident("a"), ident("b"))))],
        )];
        assert_eq!(codes(items), Vec::<&str>::new());
    }

    // ---- compound assignment (v0.131, SPEC §27) ---------------------------

    #[test]
    fn compound_assign_var_ok() {
        // fn main() void { var x: i32 = 0; x += 5; x -= 1; x *= 2; x /= 2; x %= 3; }
        let items = vec![func(
            "main",
            vec![],
            "void",
            vec![
                let_var("x", "i32", int(0)),
                assign_op("x", BinOp::Add, int(5)),
                assign_op("x", BinOp::Sub, int(1)),
                assign_op("x", BinOp::Mul, int(2)),
                assign_op("x", BinOp::Div, int(2)),
                assign_op("x", BinOp::Rem, int(3)),
            ],
        )];
        assert_eq!(codes(items), Vec::<&str>::new());
    }

    #[test]
    fn compound_assign_var_matches_place_int_type() {
        // fn main() void { var x: u8 = 0; x += 7; }   — literal adopts u8.
        let items = vec![func(
            "main",
            vec![],
            "void",
            vec![let_var("x", "u8", int(0)), assign_op("x", BinOp::Add, int(7))],
        )];
        assert_eq!(codes(items), Vec::<&str>::new());
    }

    #[test]
    fn compound_assign_bool_rhs_is_e0110() {
        // fn main() void { var x: i32 = 0; x += true; }
        let items = vec![func(
            "main",
            vec![],
            "void",
            vec![
                let_var("x", "i32", int(0)),
                assign_op("x", BinOp::Add, boolean(true)),
            ],
        )];
        assert!(codes(items).contains(&"E0110"));
    }

    #[test]
    fn compound_assign_mismatched_int_types_is_e0110() {
        // fn main() void { var x: i32 = 0; var y: i64 = 0; x += y; }
        let items = vec![func(
            "main",
            vec![],
            "void",
            vec![
                let_var("x", "i32", int(0)),
                let_var("y", "i64", int(0)),
                assign_op("x", BinOp::Add, ident("y")),
            ],
        )];
        assert!(codes(items).contains(&"E0110"));
    }

    #[test]
    fn compound_assign_to_const_is_e0110() {
        // fn main() void { const c: i32 = 5; c += 1; }  — like a plain `=` to a const.
        let items = vec![func(
            "main",
            vec![],
            "void",
            vec![let_const("c", "i32", int(5)), assign_op("c", BinOp::Add, int(1))],
        )];
        assert!(codes(items).contains(&"E0110"));
    }

    #[test]
    fn compound_assign_on_bool_place_is_e0110() {
        // fn main() void { var b: bool = false; b += 1; }  — non-integer place.
        let items = vec![func(
            "main",
            vec![],
            "void",
            vec![
                let_var("b", "bool", boolean(false)),
                assign_op("b", BinOp::Add, int(1)),
            ],
        )];
        assert!(codes(items).contains(&"E0110"));
    }

    #[test]
    fn compound_assign_on_struct_place_is_e0110() {
        // const Point = struct { x: i32, y: i32 };
        // fn main() void { var p: Point = Point{ .x = 0, .y = 0 }; p += 1; }
        let items = vec![
            struct_item("Point", vec![("x", "i32"), ("y", "i32")]),
            func(
                "main",
                vec![],
                "void",
                vec![
                    let_var(
                        "p",
                        "Point",
                        struct_lit("Point", vec![("x", int(0)), ("y", int(0))]),
                    ),
                    assign_op("p", BinOp::Add, int(1)),
                ],
            ),
        ];
        assert!(codes(items).contains(&"E0110"));
    }

    #[test]
    fn compound_assign_struct_field_ok() {
        // const Point = struct { x: i32, y: i32 };
        // fn main() void { var p: Point = Point{ .x = 0, .y = 0 }; p.x += 5; }
        let items = vec![
            struct_item("Point", vec![("x", "i32"), ("y", "i32")]),
            func(
                "main",
                vec![],
                "void",
                vec![
                    let_var(
                        "p",
                        "Point",
                        struct_lit("Point", vec![("x", int(0)), ("y", int(0))]),
                    ),
                    field_assign_op(field(ident("p"), "x"), BinOp::Add, int(5)),
                ],
            ),
        ];
        assert_eq!(codes(items), Vec::<&str>::new());
    }

    #[test]
    fn compound_assign_struct_field_bool_rhs_is_e0110() {
        // p.x += true;  — rhs is not an integer.
        let items = vec![
            struct_item("Point", vec![("x", "i32"), ("y", "i32")]),
            func(
                "main",
                vec![],
                "void",
                vec![
                    let_var(
                        "p",
                        "Point",
                        struct_lit("Point", vec![("x", int(0)), ("y", int(0))]),
                    ),
                    field_assign_op(field(ident("p"), "x"), BinOp::Add, boolean(true)),
                ],
            ),
        ];
        assert!(codes(items).contains(&"E0110"));
    }

    #[test]
    fn compound_assign_slice_index_ok() {
        // fn f(a: []i32, i: usize) void { a[i] += 1; }  — slice elem is assignable,
        // and the index `i` is read once (a backend concern; sema accepts it).
        let items = vec![func(
            "f",
            vec![param_slice("a", "i32"), param("i", "usize")],
            "void",
            vec![field_assign_op(
                index(ident("a"), ident("i")),
                BinOp::Add,
                int(1),
            )],
        )];
        assert_eq!(codes(items), Vec::<&str>::new());
    }

    #[test]
    fn compound_assign_array_index_ok() {
        // fn main() void { var a: [3]i32 = [3]i32{1,2,3}; a[0] += 4; }
        let items = vec![func(
            "main",
            vec![],
            "void",
            vec![
                let_var_arr("a", "i32", 3, array_lit("i32", 3, vec![int(1), int(2), int(3)])),
                field_assign_op(index(ident("a"), int(0)), BinOp::Add, int(4)),
            ],
        )];
        assert_eq!(codes(items), Vec::<&str>::new());
    }

    #[test]
    fn compound_assign_index_into_immutable_array_is_e0223() {
        // fn f(a: [3]i32) void { a[0] += 1; }  — array param is immutable.
        let items = vec![func(
            "f",
            vec![param_arr("a", "i32", 3)],
            "void",
            vec![field_assign_op(index(ident("a"), int(0)), BinOp::Add, int(1))],
        )];
        assert!(codes(items).contains(&"E0223"));
    }

    #[test]
    fn compound_assign_unknown_name_is_e0100() {
        // fn main() void { z += 1; }
        let items = vec![func(
            "main",
            vec![],
            "void",
            vec![assign_op("z", BinOp::Add, int(1))],
        )];
        assert!(codes(items).contains(&"E0100"));
    }

    #[test]
    fn plain_assign_still_ok_after_compound_support() {
        // fn main() void { var x: i32 = 0; x = 5; }  — plain `=` is unchanged.
        let items = vec![func(
            "main",
            vec![],
            "void",
            vec![let_var("x", "i32", int(0)), assign("x", int(5))],
        )];
        assert_eq!(codes(items), Vec::<&str>::new());
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
            vec![Stmt::Continue { target: None, span: sp() }],
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
                body: block(vec![Stmt::Break { target: None, span: sp() }]),
                label: None,
                span: sp(),
            }],
        )];
        assert_eq!(codes(items), Vec::<&str>::new());
    }

    // ---- labeled break / continue (v0.147, SPEC §40) ----------------------

    /// `name: while (true) { body }` for the labeled-loop tests.
    fn labeled_while(label: &str, body: Vec<Stmt>) -> Stmt {
        Stmt::While {
            cond: boolean(true),
            cont: None,
            body: block(body),
            label: Some(label.into()),
            span: sp(),
        }
    }

    /// `while (true) { body }` (unlabeled).
    fn plain_while(body: Vec<Stmt>) -> Stmt {
        Stmt::While {
            cond: boolean(true),
            cont: None,
            body: block(body),
            label: None,
            span: sp(),
        }
    }

    #[test]
    fn break_outer_label_from_nested_while_is_ok() {
        // fn main() void { outer: while (true) { while (true) { break :outer; } } }
        let inner = plain_while(vec![Stmt::Break {
            target: Some("outer".into()),
            span: sp(),
        }]);
        let items = vec![func(
            "main",
            vec![],
            "void",
            vec![labeled_while("outer", vec![inner])],
        )];
        assert_eq!(codes(items), Vec::<&str>::new());
    }

    #[test]
    fn continue_outer_label_from_nested_while_is_ok() {
        // fn main() void { outer: while (true) { while (true) { continue :outer; } } }
        let inner = plain_while(vec![Stmt::Continue {
            target: Some("outer".into()),
            span: sp(),
        }]);
        let items = vec![func(
            "main",
            vec![],
            "void",
            vec![labeled_while("outer", vec![inner])],
        )];
        assert_eq!(codes(items), Vec::<&str>::new());
    }

    #[test]
    fn break_unknown_label_is_e0121() {
        // fn main() void { while (true) { break :nope; } }
        let items = vec![func(
            "main",
            vec![],
            "void",
            vec![plain_while(vec![Stmt::Break {
                target: Some("nope".into()),
                span: sp(),
            }])],
        )];
        assert!(codes(items).contains(&"E0121"));
    }

    #[test]
    fn continue_unknown_label_is_e0121() {
        // fn main() void { while (true) { continue :nope; } }
        let items = vec![func(
            "main",
            vec![],
            "void",
            vec![plain_while(vec![Stmt::Continue {
                target: Some("nope".into()),
                span: sp(),
            }])],
        )];
        assert!(codes(items).contains(&"E0121"));
    }

    #[test]
    fn labeled_break_outside_any_loop_is_e0121() {
        // fn main() void { break :outer; }  — no loop at all.
        let items = vec![func(
            "main",
            vec![],
            "void",
            vec![Stmt::Break {
                target: Some("outer".into()),
                span: sp(),
            }],
        )];
        // A labeled jump with no enclosing labeled loop is E0121, not E0120.
        assert!(codes(items.clone()).contains(&"E0121"));
        assert!(!codes(items).contains(&"E0120"));
    }

    #[test]
    fn labeled_loop_body_with_unlabeled_break_typechecks() {
        // fn main() void { outer: while (true) { break; } }  — an unlabeled
        // break inside a *labeled* loop still targets the innermost loop and is
        // accepted with no diagnostics.
        let items = vec![func(
            "main",
            vec![],
            "void",
            vec![labeled_while(
                "outer",
                vec![Stmt::Break { target: None, span: sp() }],
            )],
        )];
        assert_eq!(codes(items), Vec::<&str>::new());
    }

    #[test]
    fn label_only_visible_inside_its_loop() {
        // fn main() void {
        //   outer: while (true) {}     // label scope ends with the loop
        //   while (true) { break :outer; }   // `outer` not in scope here
        // }
        let items = vec![func(
            "main",
            vec![],
            "void",
            vec![
                labeled_while("outer", vec![]),
                plain_while(vec![Stmt::Break {
                    target: Some("outer".into()),
                    span: sp(),
                }]),
            ],
        )];
        assert!(codes(items).contains(&"E0121"));
    }

    // ---- for loops over arrays & slices (v0.133, SPEC §29) ----------------

    /// Build a `var a: [3]i32 = [3]i32{0,0,0};` binding for the for-loop tests.
    fn let_arr3() -> Stmt {
        let_var_arr(
            "a",
            "i32",
            3,
            array_lit("i32", 3, vec![int(0), int(0), int(0)]),
        )
    }

    #[test]
    fn for_over_array_binds_elem_to_element_type() {
        // fn main() void {
        //   var a: [3]i32 = [3]i32{0,0,0};
        //   for (a) |x| { var y: i32 = x; }   // `x` is i32, so `y: i32 = x` ok
        // }
        let items = vec![func(
            "main",
            vec![],
            "void",
            vec![
                let_arr3(),
                for_stmt(
                    ident("a"),
                    "x",
                    None,
                    vec![let_var("y", "i32", ident("x"))],
                ),
            ],
        )];
        assert_eq!(codes(items), Vec::<&str>::new());
    }

    #[test]
    fn for_index_form_binds_index_to_usize() {
        // fn main() void {
        //   var a: [3]i32 = [3]i32{0,0,0};
        //   for (a, 0..) |x, i| { var j: usize = i; var y: i32 = x; }
        // }
        let items = vec![func(
            "main",
            vec![],
            "void",
            vec![
                let_arr3(),
                for_stmt(
                    ident("a"),
                    "x",
                    Some("i"),
                    vec![
                        let_var("j", "usize", ident("i")),
                        let_var("y", "i32", ident("x")),
                    ],
                ),
            ],
        )];
        assert_eq!(codes(items), Vec::<&str>::new());
    }

    #[test]
    fn for_over_slice_binds_elem_to_element_type() {
        // fn f(xs: []i32) void { for (xs) |x| { var y: i32 = x; } }
        let items = vec![func(
            "f",
            vec![param_slice("xs", "i32")],
            "void",
            vec![for_stmt(
                ident("xs"),
                "x",
                None,
                vec![let_var("y", "i32", ident("x"))],
            )],
        )];
        assert_eq!(codes(items), Vec::<&str>::new());
    }

    #[test]
    fn for_over_non_iterable_is_e0300() {
        // fn f(n: i32) void { for (n) |x| {} }   // `n` is neither array nor slice
        let items = vec![func(
            "f",
            vec![param("n", "i32")],
            "void",
            vec![for_stmt(ident("n"), "x", None, vec![])],
        )];
        assert!(codes(items).contains(&"E0300"));
    }

    #[test]
    fn break_and_continue_inside_for_are_ok() {
        // fn main() void {
        //   var a: [3]i32 = [3]i32{0,0,0};
        //   for (a) |x| { break; continue; }
        // }
        let items = vec![func(
            "main",
            vec![],
            "void",
            vec![
                let_arr3(),
                for_stmt(
                    ident("a"),
                    "x",
                    None,
                    vec![
                        Stmt::Break { target: None, span: sp() },
                        Stmt::Continue { target: None, span: sp() },
                    ],
                ),
            ],
        )];
        assert_eq!(codes(items), Vec::<&str>::new());
    }

    #[test]
    fn for_elem_out_of_scope_after_loop_is_e0100() {
        // fn main() void {
        //   var a: [3]i32 = [3]i32{0,0,0};
        //   for (a) |x| {}
        //   var y: i32 = x;   // `x` is out of scope here
        // }
        let items = vec![func(
            "main",
            vec![],
            "void",
            vec![
                let_arr3(),
                for_stmt(ident("a"), "x", None, vec![]),
                let_var("y", "i32", ident("x")),
            ],
        )];
        assert!(codes(items).contains(&"E0100"));
    }

    #[test]
    fn for_elem_is_immutable_binding() {
        // fn main() void {
        //   var a: [3]i32 = [3]i32{0,0,0};
        //   for (a) |x| { x = 5; }   // `x` is an immutable copy
        // }
        let items = vec![func(
            "main",
            vec![],
            "void",
            vec![
                let_arr3(),
                for_stmt(ident("a"), "x", None, vec![assign("x", int(5))]),
            ],
        )];
        assert!(codes(items).contains(&"E0110"));
    }

    #[test]
    fn for_index_is_out_of_scope_after_loop() {
        // The index capture, like `elem`, lives only inside the body.
        // fn main() void {
        //   var a: [3]i32 = [3]i32{0,0,0};
        //   for (a, 0..) |x, i| {}
        //   var j: usize = i;   // `i` is out of scope here
        // }
        let items = vec![func(
            "main",
            vec![],
            "void",
            vec![
                let_arr3(),
                for_stmt(ident("a"), "x", Some("i"), vec![]),
                let_var("j", "usize", ident("i")),
            ],
        )];
        assert!(codes(items).contains(&"E0100"));
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

    // ---- error-union tests (v0.115) --------------------------------------

    #[test]
    fn error_union_return_value_and_errorlit_coerce_ok() {
        // fn f() !i32 { return 3; }      // T coerces to !T on return
        // fn g() !i32 { return error.Oops; }   // error.X coerces to !T
        let items = vec![
            func_te("f", vec![], te_err("i32"), vec![ret(Some(int(3)))]),
            func_te("g", vec![], te_err("i32"), vec![ret(Some(error_lit("Oops")))]),
        ];
        assert_eq!(codes(items), Vec::<&str>::new());
    }

    // ---- named error sets (v0.139, SPEC §34) ------------------------------

    /// A named error set `const Name = error{ members… };`.
    fn error_set_item(name: &str, members: Vec<&str>) -> Item {
        Item::ErrorSet(ErrorSetDecl {
            is_pub: false,
            name: name.into(),
            members: members.into_iter().map(|s| s.to_string()).collect(),
            span: sp(),
        })
    }
    /// `var name: set!payload = value;` — a local annotated with a *named* error
    /// union (v0.139).
    fn let_var_err_set(name: &str, set: &str, payload: &str, value: Expr) -> Stmt {
        Stmt::Let {
            is_const: false,
            name: name.into(),
            ty: Some(te_err_set(set, payload)),
            value,
            span: sp(),
        }
    }

    #[test]
    fn named_error_set_return_member_ok() {
        // const E = error{ A, B };
        // fn f() E!i32 { return error.A; }   // A is a member of E
        let items = vec![
            error_set_item("E", vec!["A", "B"]),
            func_te("f", vec![], te_err_set("E", "i32"), vec![ret(Some(error_lit("A")))]),
        ];
        assert_eq!(codes(items), Vec::<&str>::new());
    }

    #[test]
    fn named_error_set_return_non_member_is_e0330() {
        // const E = error{ A, B };
        // fn g() E!i32 { return error.C; }   // C is NOT a member of E
        let items = vec![
            error_set_item("E", vec!["A", "B"]),
            func_te("g", vec![], te_err_set("E", "i32"), vec![ret(Some(error_lit("C")))]),
        ];
        assert!(codes(items).contains(&"E0330"));
    }

    #[test]
    fn global_error_union_accepts_any_error_name_ok() {
        // const E = error{ A, B };   (declared but irrelevant to a global `!T`)
        // fn h() !i32 { return error.Whatever; }   // global `!T` accepts any
        let items = vec![
            error_set_item("E", vec!["A", "B"]),
            func_te("h", vec![], te_err("i32"), vec![ret(Some(error_lit("Whatever")))]),
        ];
        assert_eq!(codes(items), Vec::<&str>::new());
    }

    #[test]
    fn unknown_error_set_name_is_e0331() {
        // fn k() Unknown!i32 { return 0; }   // `Unknown` is not a declared set
        let items = vec![func_te(
            "k",
            vec![],
            te_err_set("Unknown", "i32"),
            vec![ret(Some(int(0)))],
        )];
        assert!(codes(items).contains(&"E0331"));
    }

    #[test]
    fn duplicate_error_set_member_is_e0331() {
        // const Dups = error{ A, A };   // duplicate member
        let items = vec![error_set_item("Dups", vec!["A", "A"])];
        assert!(codes(items).contains(&"E0331"));
    }

    #[test]
    fn named_error_set_let_initializer_member_ok_and_non_member_is_e0330() {
        // const E = error{ A, B };
        // fn ok()  void { var x: E!i32 = error.A; }   // member → ok
        // fn bad() void { var y: E!i32 = error.C; }   // non-member → E0330
        let ok = func(
            "ok",
            vec![],
            "void",
            vec![let_var_err_set("x", "E", "i32", error_lit("A"))],
        );
        let bad = func(
            "bad",
            vec![],
            "void",
            vec![let_var_err_set("y", "E", "i32", error_lit("C"))],
        );
        // The member case alone passes.
        assert_eq!(
            codes(vec![error_set_item("E", vec!["A", "B"]), ok]),
            Vec::<&str>::new()
        );
        // The non-member case reports E0330.
        assert!(codes(vec![error_set_item("E", vec!["A", "B"]), bad]).contains(&"E0330"));
    }

    #[test]
    fn named_error_set_payload_coerces_and_try_catch_unchanged_ok() {
        // const E = error{ A, B };
        // fn f() E!i32 { return 3; }                         // payload T → Set!T
        // fn g() E!i32 { var x: i32 = try f(); return x; }   // try on a named set
        // fn main() void { var v: i32 = f() catch 0; print(v); }  // catch unchanged
        let items = vec![
            error_set_item("E", vec!["A", "B"]),
            func_te("f", vec![], te_err_set("E", "i32"), vec![ret(Some(int(3)))]),
            func_te(
                "g",
                vec![],
                te_err_set("E", "i32"),
                vec![
                    let_var("x", "i32", try_expr(call("f", vec![]))),
                    ret(Some(ident("x"))),
                ],
            ),
            func(
                "main",
                vec![],
                "void",
                vec![
                    let_var("v", "i32", catch_expr(call("f", vec![]), int(0))),
                    Stmt::Expr(call("print", vec![ident("v")])),
                ],
            ),
        ];
        assert_eq!(codes(items), Vec::<&str>::new());
    }

    #[test]
    fn named_set_resolves_to_same_error_union_type_as_global_and_members_registered() {
        // const E = error{ A, B };
        // fn f() E!i32 { return error.A; }   // named-set error union over i32
        // fn h() !i32  { return 0; }          // global error union over i32
        // The set is purely a sema constraint, so `E!i32` interns the SAME
        // `Type::ErrorUnion(i32)` as `!i32` — exactly one payload entry. Every
        // set member (even an unused `B`) is registered as a global error name.
        let table = check_ok(vec![
            error_set_item("E", vec!["A", "B"]),
            func_te("f", vec![], te_err_set("E", "i32"), vec![ret(Some(error_lit("A")))]),
            func_te("h", vec![], te_err("i32"), vec![ret(Some(int(0)))]),
        ]);
        let payloads: Vec<Type> = table.error_unions().map(|(_, t)| t).collect();
        assert_eq!(payloads, vec![Type::I32], "named + global `!i32` share one error union");
        // Both members get stable global codes (B is never written as error.B).
        assert!(table.error_code("A").is_some(), "set member A must be registered");
        assert!(table.error_code("B").is_some(), "set member B must be registered");
    }

    #[test]
    fn catch_yields_payload_type_ok() {
        // fn f() !i32 { return 1; }
        // fn main() void { var v: i32 = f() catch 0; print(v); }
        let items = vec![
            func_te("f", vec![], te_err("i32"), vec![ret(Some(int(1)))]),
            func(
                "main",
                vec![],
                "void",
                vec![
                    let_var("v", "i32", catch_expr(call("f", vec![]), int(0))),
                    Stmt::Expr(call("print", vec![ident("v")])),
                ],
            ),
        ];
        assert_eq!(codes(items), Vec::<&str>::new());
    }

    #[test]
    fn try_in_let_initializer_ok() {
        // fn f() !i32 { return 1; }
        // fn g() !i32 { var x: i32 = try f(); return x; }
        let items = vec![
            func_te("f", vec![], te_err("i32"), vec![ret(Some(int(1)))]),
            func_te(
                "g",
                vec![],
                te_err("i32"),
                vec![
                    let_var("x", "i32", try_expr(call("f", vec![]))),
                    ret(Some(ident("x"))),
                ],
            ),
        ];
        assert_eq!(codes(items), Vec::<&str>::new());
    }

    #[test]
    fn try_as_expression_statement_ok() {
        // fn f() !i32 { return 1; }
        // fn g() !i32 { try f(); return 0; }
        let items = vec![
            func_te("f", vec![], te_err("i32"), vec![ret(Some(int(1)))]),
            func_te(
                "g",
                vec![],
                te_err("i32"),
                vec![Stmt::Expr(try_expr(call("f", vec![]))), ret(Some(int(0)))],
            ),
        ];
        assert_eq!(codes(items), Vec::<&str>::new());
    }

    #[test]
    fn return_try_coerces_payload_to_error_union_ok() {
        // fn f() !i32 { return 1; }
        // fn g() !i32 { return try f(); }   // try yields i32, coerces to !i32
        let items = vec![
            func_te("f", vec![], te_err("i32"), vec![ret(Some(int(1)))]),
            func_te(
                "g",
                vec![],
                te_err("i32"),
                vec![ret(Some(try_expr(call("f", vec![]))))],
            ),
        ];
        assert_eq!(codes(items), Vec::<&str>::new());
    }

    #[test]
    fn try_outside_error_union_fn_is_e0190() {
        // fn f() !i32 { return 1; }
        // fn main() void { var x: i32 = try f(); }   // enclosing returns void
        let items = vec![
            func_te("f", vec![], te_err("i32"), vec![ret(Some(int(1)))]),
            func(
                "main",
                vec![],
                "void",
                vec![let_var("x", "i32", try_expr(call("f", vec![])))],
            ),
        ];
        assert!(codes(items).contains(&"E0190"));
    }

    #[test]
    fn try_on_non_error_union_operand_is_e0190() {
        // fn g() !i32 { var x: i32 = try 5; return x; }   // 5 is not an !T
        let items = vec![func_te(
            "g",
            vec![],
            te_err("i32"),
            vec![
                let_var("x", "i32", try_expr(int(5))),
                ret(Some(ident("x"))),
            ],
        )];
        assert!(codes(items).contains(&"E0190"));
    }

    #[test]
    fn try_in_compound_expr_is_e0191() {
        // fn f() !i32 { return 1; }
        // fn g() !i32 { var x: i32 = (try f()) + 1; return x; }
        let items = vec![
            func_te("f", vec![], te_err("i32"), vec![ret(Some(int(1)))]),
            func_te(
                "g",
                vec![],
                te_err("i32"),
                vec![
                    let_var(
                        "x",
                        "i32",
                        bin(BinOp::Add, try_expr(call("f", vec![])), int(1)),
                    ),
                    ret(Some(ident("x"))),
                ],
            ),
        ];
        assert!(codes(items).contains(&"E0191"));
    }

    #[test]
    fn catch_on_non_error_union_is_e0192() {
        // fn f(x: i32) void { var v: i32 = x catch 0; }   // x is i32, not !T
        let items = vec![func(
            "f",
            vec![param("x", "i32")],
            "void",
            vec![let_var("v", "i32", catch_expr(ident("x"), int(0)))],
        )];
        assert!(codes(items).contains(&"E0192"));
    }

    #[test]
    fn error_lit_without_context_is_e0193() {
        // fn f() void { var x: i32 = error.Oops; }   // i32 is not an !T
        let items = vec![func(
            "f",
            vec![],
            "void",
            vec![let_var("x", "i32", error_lit("Oops"))],
        )];
        assert!(codes(items).contains(&"E0193"));
    }

    #[test]
    fn catch_default_type_mismatch_is_e0110() {
        // fn f() !i32 { return 1; }
        // fn main() void { var v: i32 = f() catch true; }   // default is bool
        let items = vec![
            func_te("f", vec![], te_err("i32"), vec![ret(Some(int(1)))]),
            func(
                "main",
                vec![],
                "void",
                vec![let_var("v", "i32", catch_expr(call("f", vec![]), boolean(true)))],
            ),
        ];
        assert!(codes(items).contains(&"E0110"));
    }

    // ---- capturing `catch |e|` (v0.142, SPEC §36) -------------------------

    #[test]
    fn catch_capture_yields_payload_and_binds_error_code_as_i32() {
        // fn f() !i32 { return 1; }
        // fn main() void { var x: i32 = f() catch |e| e; print(x); }
        // The result is the payload `i32`; the handler's `e` is the error code
        // (`i32`), so returning `e` as the default type-checks (else E0110).
        let items = vec![
            func_te("f", vec![], te_err("i32"), vec![ret(Some(int(1)))]),
            func(
                "main",
                vec![],
                "void",
                vec![
                    let_var("x", "i32", catch_capture_expr(call("f", vec![]), "e", ident("e"))),
                    Stmt::Expr(call("print", vec![ident("x")])),
                ],
            ),
        ];
        assert_eq!(codes(items), Vec::<&str>::new());
    }

    #[test]
    fn catch_capture_handler_can_use_error_code_in_expression() {
        // fn f() !i32 { return 1; }
        // fn main() void { var x: i32 = f() catch |e| (0 - e); }
        // `e` (an i32) participates in arithmetic; the i32 result matches the
        // i32 payload.
        let items = vec![
            func_te("f", vec![], te_err("i32"), vec![ret(Some(int(1)))]),
            func(
                "main",
                vec![],
                "void",
                vec![let_var(
                    "x",
                    "i32",
                    catch_capture_expr(call("f", vec![]), "e", bin(BinOp::Sub, int(0), ident("e"))),
                )],
            ),
        ];
        assert_eq!(codes(items), Vec::<&str>::new());
    }

    #[test]
    fn catch_capture_does_not_leak_after_handler() {
        // fn f() !i32 { return 1; }
        // fn main() void { var x: i32 = f() catch |e| e; var y: i32 = e; }
        // The capture binds only inside the handler; using `e` afterwards is an
        // unknown name (E0100).
        let items = vec![
            func_te("f", vec![], te_err("i32"), vec![ret(Some(int(1)))]),
            func(
                "main",
                vec![],
                "void",
                vec![
                    let_var("x", "i32", catch_capture_expr(call("f", vec![]), "e", ident("e"))),
                    let_var("y", "i32", ident("e")),
                ],
            ),
        ];
        assert!(codes(items).contains(&"E0100"));
    }

    #[test]
    fn catch_capture_on_non_error_union_is_e0192() {
        // fn f(x: i32) void { var v: i32 = x catch |e| e; }   // x is i32, not !T
        let items = vec![func(
            "f",
            vec![param("x", "i32")],
            "void",
            vec![let_var("v", "i32", catch_capture_expr(ident("x"), "e", ident("e")))],
        )];
        assert!(codes(items).contains(&"E0192"));
    }

    #[test]
    fn catch_capture_default_type_mismatch_is_e0110() {
        // fn f() !i32 { return 1; }
        // fn main() void { var v: i32 = f() catch |e| true; }   // default is bool
        let items = vec![
            func_te("f", vec![], te_err("i32"), vec![ret(Some(int(1)))]),
            func(
                "main",
                vec![],
                "void",
                vec![let_var(
                    "v",
                    "i32",
                    catch_capture_expr(call("f", vec![]), "e", boolean(true)),
                )],
            ),
        ];
        assert!(codes(items).contains(&"E0110"));
    }

    #[test]
    fn error_union_interned_in_table_and_error_registered() {
        // fn f() !i32 { return error.Oops; }
        // The table holds one error union over i32 and registers `Oops` (code 1).
        let items = vec![func_te(
            "f",
            vec![],
            te_err("i32"),
            vec![ret(Some(error_lit("Oops")))],
        )];
        let m = Module { items };
        let table = check(&m).expect("error-union program should type-check");
        let erus: Vec<Type> = table.error_unions().map(|(_, t)| t).collect();
        assert_eq!(erus, vec![Type::I32]);
        assert_eq!(table.error_code("Oops"), Some(1));
    }

    #[test]
    fn error_union_struct_field_ok_and_interned() {
        // const Box = struct { val: !i32 };
        // fn mk() Box { return Box{ .val = 9 }; }          // T coerces in field
        // fn err() Box { return Box{ .val = error.Oops }; } // error.X in field
        let box_struct = Item::Struct(StructDecl {
            is_pub: false,
            name: "Box".into(),
            fields: vec![FieldDecl {
                name: "val".into(),
                ty: te_err("i32"),
                span: sp(),
            }],
            methods: Vec::new(),
            span: sp(),
        });
        let items = vec![
            box_struct,
            func_te("mk", vec![], te("Box"), vec![ret(Some(struct_lit("Box", vec![("val", int(9))])))]),
            func_te(
                "err",
                vec![],
                te("Box"),
                vec![ret(Some(struct_lit("Box", vec![("val", error_lit("Oops"))])))],
            ),
        ];
        let m = Module { items };
        let table = check(&m).expect("error-union-field program should type-check");
        let erus: Vec<Type> = table.error_unions().map(|(_, t)| t).collect();
        assert_eq!(erus, vec![Type::I32]);
        let id = table.id_of("Box").unwrap();
        assert_eq!(
            table.get(id).fields,
            vec![("val".to_string(), Type::ErrorUnion(0))]
        );
    }

    // ---- enum + switch tests (v0.116) ------------------------------------

    fn enum_item(name: &str, variants: Vec<&str>) -> Item {
        Item::Enum(EnumDecl {
            is_pub: false,
            name: name.into(),
            variants: variants
                .into_iter()
                .map(|v| EnumVariant {
                    name: v.into(),
                    value: None,
                    span: sp(),
                })
                .collect(),
            span: sp(),
        })
    }
    /// An enum item whose variants carry explicit values (`A = 1`), for the
    /// v0.143 explicit-value tests. Each `(name, Some(n))` is `name = n`; a
    /// `(name, None)` auto-increments.
    fn enum_item_valued(name: &str, variants: Vec<(&str, Option<i64>)>) -> Item {
        Item::Enum(EnumDecl {
            is_pub: false,
            name: name.into(),
            variants: variants
                .into_iter()
                .map(|(n, v)| EnumVariant {
                    name: n.into(),
                    value: v,
                    span: sp(),
                })
                .collect(),
            span: sp(),
        })
    }
    /// An unqualified enum literal `.variant`.
    fn enum_lit(variant: &str) -> Expr {
        Expr::EnumLit {
            variant: variant.into(),
            span: sp(),
        }
    }
    fn switch_arm(labels: Vec<Expr>, body: Vec<Stmt>) -> SwitchArm {
        SwitchArm {
            labels,
            ranges: vec![],
            capture: None,
            body: block(body),
            span: sp(),
        }
    }
    /// A `switch` arm that binds a payload capture `|cap|` (v0.124).
    fn switch_arm_cap(labels: Vec<Expr>, cap: &str, body: Vec<Stmt>) -> SwitchArm {
        SwitchArm {
            labels,
            ranges: vec![],
            capture: Some(cap.into()),
            body: block(body),
            span: sp(),
        }
    }
    /// A `switch` arm carrying inclusive integer-range labels `lo..hi` (v0.146),
    /// optionally alongside value `labels` (the arm matches any label OR range).
    fn switch_arm_ranges(
        labels: Vec<Expr>,
        ranges: Vec<(i64, i64)>,
        body: Vec<Stmt>,
    ) -> SwitchArm {
        SwitchArm {
            labels,
            ranges,
            capture: None,
            body: block(body),
            span: sp(),
        }
    }
    fn switch_stmt(scrutinee: Expr, arms: Vec<SwitchArm>, default: Option<Vec<Stmt>>) -> Stmt {
        Stmt::Switch {
            scrutinee,
            arms,
            default: default.map(block),
            span: sp(),
        }
    }
    /// The canonical three-variant `Color` enum used by the switch tests.
    fn color_enum() -> Item {
        enum_item("Color", vec!["Red", "Green", "Blue"])
    }

    #[test]
    fn enum_value_typing_ok_and_interned() {
        // const Color = enum { Red, Green, Blue };
        // fn qualified() Color { return Color.Red; }   // `Enum.V`
        // fn unqualified() Color { return .Green; }     // `.V`
        let items = vec![
            color_enum(),
            func(
                "qualified",
                vec![],
                "Color",
                vec![ret(Some(field(ident("Color"), "Red")))],
            ),
            func(
                "unqualified",
                vec![],
                "Color",
                vec![ret(Some(enum_lit("Green")))],
            ),
        ];
        let m = Module { items };
        let table = check(&m).expect("enum program should type-check");
        let id = table.enum_id_of("Color").expect("Color should be registered");
        assert_eq!(table.enum_get(id).variants, vec!["Red", "Green", "Blue"]);
        assert_eq!(table.enum_get(id).variant_index("Blue"), Some(2));
    }

    #[test]
    fn switch_exhaustive_over_enum_ok() {
        // fn classify(c: Color) void {
        //     switch (c) { .Red => { print(1); }, .Green => { print(2); }, .Blue => { print(3); } }
        // }
        let items = vec![
            color_enum(),
            func(
                "classify",
                vec![param("c", "Color")],
                "void",
                vec![switch_stmt(
                    ident("c"),
                    vec![
                        switch_arm(vec![enum_lit("Red")], vec![Stmt::Expr(call("print", vec![int(1)]))]),
                        switch_arm(vec![enum_lit("Green")], vec![Stmt::Expr(call("print", vec![int(2)]))]),
                        switch_arm(vec![enum_lit("Blue")], vec![Stmt::Expr(call("print", vec![int(3)]))]),
                    ],
                    None,
                )],
            ),
        ];
        assert_eq!(codes(items), Vec::<&str>::new());
    }

    #[test]
    fn switch_qualified_labels_and_multi_label_arm_ok() {
        // Mixing `Color.V` labels and a multi-label arm still covers every variant.
        // switch (c) { Color.Red, Color.Green => {}, Color.Blue => {} }
        let items = vec![
            color_enum(),
            func(
                "classify",
                vec![param("c", "Color")],
                "void",
                vec![switch_stmt(
                    ident("c"),
                    vec![
                        switch_arm(
                            vec![field(ident("Color"), "Red"), field(ident("Color"), "Green")],
                            vec![],
                        ),
                        switch_arm(vec![field(ident("Color"), "Blue")], vec![]),
                    ],
                    None,
                )],
            ),
        ];
        assert_eq!(codes(items), Vec::<&str>::new());
    }

    #[test]
    fn switch_missing_variant_is_e0210() {
        // switch (c) { .Red => {}, .Green => {} }   // missing Blue, no else
        let items = vec![
            color_enum(),
            func(
                "classify",
                vec![param("c", "Color")],
                "void",
                vec![switch_stmt(
                    ident("c"),
                    vec![
                        switch_arm(vec![enum_lit("Red")], vec![]),
                        switch_arm(vec![enum_lit("Green")], vec![]),
                    ],
                    None,
                )],
            ),
        ];
        assert!(codes(items).contains(&"E0210"));
    }

    #[test]
    fn switch_else_covers_missing_variant_ok() {
        // switch (c) { .Red => {}, else => {} }   // else makes it exhaustive
        let items = vec![
            color_enum(),
            func(
                "classify",
                vec![param("c", "Color")],
                "void",
                vec![switch_stmt(
                    ident("c"),
                    vec![switch_arm(vec![enum_lit("Red")], vec![])],
                    Some(vec![]),
                )],
            ),
        ];
        assert_eq!(codes(items), Vec::<&str>::new());
    }

    #[test]
    fn switch_duplicate_label_is_e0211() {
        // switch (c) { .Red => {}, .Red => {}, .Green => {}, .Blue => {} }
        let items = vec![
            color_enum(),
            func(
                "classify",
                vec![param("c", "Color")],
                "void",
                vec![switch_stmt(
                    ident("c"),
                    vec![
                        switch_arm(vec![enum_lit("Red")], vec![]),
                        switch_arm(vec![enum_lit("Red")], vec![]),
                        switch_arm(vec![enum_lit("Green")], vec![]),
                        switch_arm(vec![enum_lit("Blue")], vec![]),
                    ],
                    None,
                )],
            ),
        ];
        assert!(codes(items).contains(&"E0211"));
    }

    #[test]
    fn duplicate_enum_variant_decl_is_e0211() {
        // const Color = enum { Red, Red };
        let items = vec![enum_item("Color", vec!["Red", "Red"])];
        assert!(codes(items).contains(&"E0211"));
    }

    #[test]
    fn unknown_enum_variant_value_is_e0212() {
        // fn f() Color { return Color.Purple; }   // Purple is not a variant
        let items = vec![
            color_enum(),
            func(
                "f",
                vec![],
                "Color",
                vec![ret(Some(field(ident("Color"), "Purple")))],
            ),
        ];
        assert!(codes(items).contains(&"E0212"));
    }

    #[test]
    fn unknown_enum_variant_label_is_e0212() {
        // switch (c) { .Red => {}, .Green => {}, .Blue => {}, .Purple => {} }
        let items = vec![
            color_enum(),
            func(
                "classify",
                vec![param("c", "Color")],
                "void",
                vec![switch_stmt(
                    ident("c"),
                    vec![
                        switch_arm(vec![enum_lit("Red")], vec![]),
                        switch_arm(vec![enum_lit("Green")], vec![]),
                        switch_arm(vec![enum_lit("Blue")], vec![]),
                        switch_arm(vec![enum_lit("Purple")], vec![]),
                    ],
                    None,
                )],
            ),
        ];
        assert!(codes(items).contains(&"E0212"));
    }

    #[test]
    fn int_switch_without_else_is_e0214() {
        // fn f(x: i32) void { switch (x) { 0 => {}, 1 => {} } }   // no else
        let items = vec![func(
            "f",
            vec![param("x", "i32")],
            "void",
            vec![switch_stmt(
                ident("x"),
                vec![
                    switch_arm(vec![int(0)], vec![]),
                    switch_arm(vec![int(1)], vec![]),
                ],
                None,
            )],
        )];
        assert!(codes(items).contains(&"E0214"));
    }

    #[test]
    fn int_switch_with_else_ok() {
        // fn f(x: i32) void { switch (x) { 0 => { print(0); }, else => { print(9); } } }
        let items = vec![func(
            "f",
            vec![param("x", "i32")],
            "void",
            vec![switch_stmt(
                ident("x"),
                vec![switch_arm(vec![int(0)], vec![Stmt::Expr(call("print", vec![int(0)]))])],
                Some(vec![Stmt::Expr(call("print", vec![int(9)]))]),
            )],
        )];
        assert_eq!(codes(items), Vec::<&str>::new());
    }

    #[test]
    fn int_switch_duplicate_label_is_e0211() {
        // fn f(x: i32) void { switch (x) { 0 => {}, 0 => {}, else => {} } }
        let items = vec![func(
            "f",
            vec![param("x", "i32")],
            "void",
            vec![switch_stmt(
                ident("x"),
                vec![
                    switch_arm(vec![int(0)], vec![]),
                    switch_arm(vec![int(0)], vec![]),
                ],
                Some(vec![]),
            )],
        )];
        assert!(codes(items).contains(&"E0211"));
    }

    // ---- switch range labels (v0.146) ------------------------------------

    #[test]
    fn int_switch_with_range_label_and_else_ok() {
        // fn f(x: i32) void { switch (x) { 1..5 => { print(1); }, else => {} } }
        let items = vec![func(
            "f",
            vec![param("x", "i32")],
            "void",
            vec![switch_stmt(
                ident("x"),
                vec![switch_arm_ranges(
                    vec![],
                    vec![(1, 5)],
                    vec![Stmt::Expr(call("print", vec![int(1)]))],
                )],
                Some(vec![]),
            )],
        )];
        assert_eq!(codes(items), Vec::<&str>::new());
    }

    #[test]
    fn int_switch_value_plus_range_mixed_arm_ok() {
        // fn f(x: i32) void { switch (x) { 0, 10..20 => {}, else => {} } }
        let items = vec![func(
            "f",
            vec![param("x", "i32")],
            "void",
            vec![switch_stmt(
                ident("x"),
                vec![switch_arm_ranges(vec![int(0)], vec![(10, 20)], vec![])],
                Some(vec![]),
            )],
        )];
        assert_eq!(codes(items), Vec::<&str>::new());
    }

    #[test]
    fn int_switch_backwards_range_matches_nothing_ok() {
        // fn f(x: i32) void { switch (x) { 5..1 => {}, else => {} } }
        // A backwards range matches nothing (SPEC §39.1) — not an error.
        let items = vec![func(
            "f",
            vec![param("x", "i32")],
            "void",
            vec![switch_stmt(
                ident("x"),
                vec![switch_arm_ranges(vec![], vec![(5, 1)], vec![])],
                Some(vec![]),
            )],
        )];
        assert_eq!(codes(items), Vec::<&str>::new());
    }

    #[test]
    fn range_label_on_enum_switch_is_e0212() {
        // switch (c) { 0..2 => {}, else => {} }   // a range on an enum scrutinee
        let items = vec![
            color_enum(),
            func(
                "classify",
                vec![param("c", "Color")],
                "void",
                vec![switch_stmt(
                    ident("c"),
                    vec![switch_arm_ranges(vec![], vec![(0, 2)], vec![])],
                    Some(vec![]),
                )],
            ),
        ];
        assert!(codes(items).contains(&"E0212"));
    }

    #[test]
    fn range_label_on_union_switch_is_e0212() {
        // switch (n) { 0..1 => {}, else => {} }   // a range on a union scrutinee
        let items = vec![
            num_union(),
            func(
                "consume",
                vec![param("n", "Num")],
                "void",
                vec![switch_stmt(
                    ident("n"),
                    vec![switch_arm_ranges(vec![], vec![(0, 1)], vec![])],
                    Some(vec![]),
                )],
            ),
        ];
        assert!(codes(items).contains(&"E0212"));
    }

    #[test]
    fn switch_on_bool_is_e0213() {
        // fn f(b: bool) void { switch (b) { else => {} } }   // bool is not switchable
        let items = vec![func(
            "f",
            vec![param("b", "bool")],
            "void",
            vec![switch_stmt(ident("b"), vec![], Some(vec![]))],
        )];
        assert!(codes(items).contains(&"E0213"));
    }

    #[test]
    fn switch_on_struct_is_e0213() {
        // const Point = struct { x: i32 };
        // fn f(p: Point) void { switch (p) { else => {} } }
        let items = vec![
            struct_item("Point", vec![("x", "i32")]),
            func(
                "f",
                vec![param("p", "Point")],
                "void",
                vec![switch_stmt(ident("p"), vec![], Some(vec![]))],
            ),
        ];
        assert!(codes(items).contains(&"E0213"));
    }

    #[test]
    fn bare_enum_literal_without_context_is_e0215() {
        // fn f() void { var x: i32 = .Red; }   // i32 is not an enum
        let items = vec![
            color_enum(),
            func(
                "f",
                vec![],
                "void",
                vec![let_var("x", "i32", enum_lit("Red"))],
            ),
        ];
        assert!(codes(items).contains(&"E0215"));
    }

    // ---- fixed-size array tests (v0.117) ---------------------------------

    #[test]
    fn array_literal_param_return_and_len_ok_and_interned() {
        // fn make() [3]i32 { return [3]i32{ 1, 2, 3 }; }
        // fn first(a: [3]i32) i32 { return a[0]; }
        // fn main() void { var a: [3]i32 = make(); print(first(a)); print(a.len); }
        let items = vec![
            func_te(
                "make",
                vec![],
                te_arr("i32", 3),
                vec![ret(Some(array_lit("i32", 3, vec![int(1), int(2), int(3)])))],
            ),
            func(
                "first",
                vec![param_arr("a", "i32", 3)],
                "i32",
                vec![ret(Some(index(ident("a"), int(0))))],
            ),
            func(
                "main",
                vec![],
                "void",
                vec![
                    let_var_arr("a", "i32", 3, call("make", vec![])),
                    Stmt::Expr(call("print", vec![call("first", vec![ident("a")])])),
                    Stmt::Expr(call("print", vec![field(ident("a"), "len")])),
                ],
            ),
        ];
        let m = Module { items };
        let table = check(&m).expect("array program should type-check");
        // The `[3]i32` type was interned exactly once (deduplicated across the
        // return type, the parameter, the local and the literal).
        let arrs: Vec<(Type, usize)> = table.arrays().map(|(_, e, l)| (e, l)).collect();
        assert_eq!(arrs, vec![(Type::I32, 3)]);
    }

    #[test]
    fn array_literal_count_mismatch_is_e0221() {
        // fn f() [3]i32 { return [3]i32{ 1, 2 }; }   // 2 elements, expected 3
        let items = vec![func_te(
            "f",
            vec![],
            te_arr("i32", 3),
            vec![ret(Some(array_lit("i32", 3, vec![int(1), int(2)])))],
        )];
        assert!(codes(items).contains(&"E0221"));
    }

    #[test]
    fn array_element_type_mismatch_is_e0110() {
        // fn f() [2]i32 { return [2]i32{ 1, true }; }   // second element is bool
        let items = vec![func_te(
            "f",
            vec![],
            te_arr("i32", 2),
            vec![ret(Some(array_lit("i32", 2, vec![int(1), boolean(true)])))],
        )];
        assert!(codes(items).contains(&"E0110"));
    }

    #[test]
    fn index_yields_element_type_mismatch_is_e0110() {
        // fn f(a: [4]i32) bool { return a[0]; }   // a[0] is i32, declared bool
        let items = vec![func(
            "f",
            vec![param_arr("a", "i32", 4)],
            "bool",
            vec![ret(Some(index(ident("a"), int(0))))],
        )];
        assert!(codes(items).contains(&"E0110"));
    }

    #[test]
    fn index_on_non_array_is_e0220() {
        // fn f(x: i32) i32 { return x[0]; }   // x is not an array
        let items = vec![func(
            "f",
            vec![param("x", "i32")],
            "i32",
            vec![ret(Some(index(ident("x"), int(0))))],
        )];
        assert!(codes(items).contains(&"E0220"));
    }

    #[test]
    fn array_len_is_usize_ok() {
        // fn f(a: [3]i32) usize { return a.len; }   // a.len is a usize constant
        let items = vec![func(
            "f",
            vec![param_arr("a", "i32", 3)],
            "usize",
            vec![ret(Some(field(ident("a"), "len")))],
        )];
        assert_eq!(codes(items), Vec::<&str>::new());
    }

    #[test]
    fn index_assign_ok() {
        // fn main() void { var a: [3]i32 = [3]i32{ 0, 0, 0 }; a[1] = 7; }
        let items = vec![func(
            "main",
            vec![],
            "void",
            vec![
                let_var_arr("a", "i32", 3, array_lit("i32", 3, vec![int(0), int(0), int(0)])),
                field_assign(index(ident("a"), int(1)), int(7)),
            ],
        )];
        assert_eq!(codes(items), Vec::<&str>::new());
    }

    #[test]
    fn index_assign_into_immutable_param_is_e0223() {
        // fn f(a: [3]i32) void { a[0] = 5; }   // a is an immutable parameter
        let items = vec![func(
            "f",
            vec![param_arr("a", "i32", 3)],
            "void",
            vec![field_assign(index(ident("a"), int(0)), int(5))],
        )];
        assert!(codes(items).contains(&"E0223"));
    }

    #[test]
    fn index_assign_into_non_array_is_e0223() {
        // fn main() void { var x: i32 = 0; x[0] = 5; }   // x is not an array
        let items = vec![func(
            "main",
            vec![],
            "void",
            vec![
                let_var("x", "i32", int(0)),
                field_assign(index(ident("x"), int(0)), int(5)),
            ],
        )];
        assert!(codes(items).contains(&"E0223"));
    }

    #[test]
    fn index_assign_value_type_mismatch_is_e0110() {
        // fn main() void { var a: [2]i32 = [2]i32{ 0, 0 }; a[0] = true; }
        let items = vec![func(
            "main",
            vec![],
            "void",
            vec![
                let_var_arr("a", "i32", 2, array_lit("i32", 2, vec![int(0), int(0)])),
                field_assign(index(ident("a"), int(0)), boolean(true)),
            ],
        )];
        assert!(codes(items).contains(&"E0110"));
    }

    #[test]
    fn negative_array_length_is_e0224() {
        // fn f(a: [-1]i32) void {}   // a negative array length
        let items = vec![func(
            "f",
            vec![param_arr("a", "i32", -1)],
            "void",
            vec![],
        )];
        assert!(codes(items).contains(&"E0224"));
    }

    #[test]
    fn array_of_struct_element_ok() {
        // const Point = struct { x: i32 };
        // fn f(a: [2]Point) i32 { return a[0].x; }
        let items = vec![
            struct_item("Point", vec![("x", "i32")]),
            func(
                "f",
                vec![param_arr("a", "Point", 2)],
                "i32",
                vec![ret(Some(field(index(ident("a"), int(0)), "x")))],
            ),
        ];
        assert_eq!(codes(items), Vec::<&str>::new());
    }

    #[test]
    fn array_struct_field_resolves_to_array_type() {
        // const Row = struct { cells: [3]i32 };
        // The field type resolves to a `[3]i32` array, interned in the table.
        let row = Item::Struct(StructDecl {
            is_pub: false,
            name: "Row".into(),
            fields: vec![FieldDecl {
                name: "cells".into(),
                ty: te_arr("i32", 3),
                span: sp(),
            }],
            methods: Vec::new(),
            span: sp(),
        });
        let m = Module { items: vec![row] };
        let table = check(&m).expect("array-field struct should type-check");
        let id = table.id_of("Row").unwrap();
        let (fname, fty) = table.get(id).fields[0].clone();
        assert_eq!(fname, "cells");
        match fty {
            Type::Array(aid) => {
                assert_eq!(table.array_elem(aid), Type::I32);
                assert_eq!(table.array_len(aid), 3);
            }
            other => panic!("expected an array field type, found {:?}", other),
        }
    }

    // ---- pointer & slice tests (v0.118) ----------------------------------

    /// Build a fresh array `var a: [3]i32 = [3]i32{1,2,3};`.
    fn arr3_decl(name: &str) -> Stmt {
        let_var_arr(
            name,
            "i32",
            3,
            array_lit("i32", 3, vec![int(1), int(2), int(3)]),
        )
    }

    #[test]
    fn addr_of_var_yields_pointer() {
        // fn main() void { var x: i32 = 5; var p: *i32 = &x; }
        let items = vec![func(
            "main",
            vec![],
            "void",
            vec![
                let_var("x", "i32", int(5)),
                let_var_ptr("p", "i32", addr_of(ident("x"))),
            ],
        )];
        assert_eq!(codes(items), Vec::<&str>::new());
    }

    #[test]
    fn addr_of_wrong_pointee_is_e0110() {
        // var x: i32 = 5; var p: *bool = &x;  → *i32 is not *bool
        let items = vec![func(
            "main",
            vec![],
            "void",
            vec![
                let_var("x", "i32", int(5)),
                let_var_ptr("p", "bool", addr_of(ident("x"))),
            ],
        )];
        assert!(codes(items).contains(&"E0110"));
    }

    #[test]
    fn addr_of_const_or_param_is_ok() {
        // `&` does not require mutability — addressing a parameter is allowed.
        // fn f(x: i32) i32 { var p: *i32 = &x; return p.*; }
        let items = vec![func(
            "f",
            vec![param("x", "i32")],
            "i32",
            vec![
                let_var_ptr("p", "i32", addr_of(ident("x"))),
                ret(Some(deref(ident("p")))),
            ],
        )];
        assert_eq!(codes(items), Vec::<&str>::new());
    }

    #[test]
    fn addr_of_field_yields_pointer() {
        // const Point = struct { x: i32 };
        // fn main() void { var pt: Point = Point{.x=1}; var p: *i32 = &pt.x; }
        let items = vec![
            struct_item("Point", vec![("x", "i32")]),
            func(
                "main",
                vec![],
                "void",
                vec![
                    let_var("pt", "Point", struct_lit("Point", vec![("x", int(1))])),
                    let_var_ptr("p", "i32", addr_of(field(ident("pt"), "x"))),
                ],
            ),
        ];
        assert_eq!(codes(items), Vec::<&str>::new());
    }

    #[test]
    fn addr_of_array_element_yields_pointer() {
        // fn main() void { var a: [3]i32 = ...; var p: *i32 = &a[0]; }
        let items = vec![func(
            "main",
            vec![],
            "void",
            vec![
                arr3_decl("a"),
                let_var_ptr("p", "i32", addr_of(index(ident("a"), int(0)))),
            ],
        )];
        assert_eq!(codes(items), Vec::<&str>::new());
    }

    #[test]
    fn addr_of_non_lvalue_is_e0231() {
        // var p: *i32 = &(1 + 1);  → a value, not an lvalue
        let items = vec![func(
            "main",
            vec![],
            "void",
            vec![let_var_ptr(
                "p",
                "i32",
                addr_of(bin(BinOp::Add, int(1), int(1))),
            )],
        )];
        assert!(codes(items).contains(&"E0231"));
    }

    #[test]
    fn deref_yields_pointee() {
        // var x: i32 = 5; var p: *i32 = &x; var y: i32 = p.*;
        let items = vec![func(
            "main",
            vec![],
            "void",
            vec![
                let_var("x", "i32", int(5)),
                let_var_ptr("p", "i32", addr_of(ident("x"))),
                let_var("y", "i32", deref(ident("p"))),
            ],
        )];
        assert_eq!(codes(items), Vec::<&str>::new());
    }

    #[test]
    fn deref_non_pointer_is_e0230() {
        // var x: i32 = 5; var y: i32 = x.*;
        let items = vec![func(
            "main",
            vec![],
            "void",
            vec![
                let_var("x", "i32", int(5)),
                let_var("y", "i32", deref(ident("x"))),
            ],
        )];
        assert!(codes(items).contains(&"E0230"));
    }

    #[test]
    fn deref_assign_through_pointer() {
        // var x: i32 = 5; var p: *i32 = &x; p.* = 10;
        let items = vec![func(
            "main",
            vec![],
            "void",
            vec![
                let_var("x", "i32", int(5)),
                let_var_ptr("p", "i32", addr_of(ident("x"))),
                field_assign(deref(ident("p")), int(10)),
            ],
        )];
        assert_eq!(codes(items), Vec::<&str>::new());
    }

    #[test]
    fn deref_assign_type_mismatch_is_e0110() {
        // var x: i32 = 5; var p: *i32 = &x; p.* = true;
        let items = vec![func(
            "main",
            vec![],
            "void",
            vec![
                let_var("x", "i32", int(5)),
                let_var_ptr("p", "i32", addr_of(ident("x"))),
                field_assign(deref(ident("p")), boolean(true)),
            ],
        )];
        assert!(codes(items).contains(&"E0110"));
    }

    #[test]
    fn deref_assign_through_non_pointer_is_e0230() {
        // var x: i32 = 5; x.* = 1;
        let items = vec![func(
            "main",
            vec![],
            "void",
            vec![
                let_var("x", "i32", int(5)),
                field_assign(deref(ident("x")), int(1)),
            ],
        )];
        assert!(codes(items).contains(&"E0230"));
    }

    #[test]
    fn pointer_param_and_return() {
        // fn get(p: *i32) i32 { return p.*; }
        // fn main() void { var x: i32 = 5; var y: i32 = get(&x); }
        let items = vec![
            Item::Func(Func {
                is_pub: false,
                name: "get".into(),
                params: vec![Param {
                    name: "p".into(),
                    ty: te_ptr("i32"),
                    is_comptime: false,
                    span: sp(),
                }],
                ret: te("i32"),
                body: block(vec![ret(Some(deref(ident("p"))))]),
                span: sp(),
            }),
            func(
                "main",
                vec![],
                "void",
                vec![
                    let_var("x", "i32", int(5)),
                    let_var("y", "i32", call("get", vec![addr_of(ident("x"))])),
                ],
            ),
        ];
        assert_eq!(codes(items), Vec::<&str>::new());
    }

    #[test]
    fn slice_of_array_yields_slice() {
        // var a: [3]i32 = ...; var s: []i32 = a[0..2];
        let items = vec![func(
            "main",
            vec![],
            "void",
            vec![
                arr3_decl("a"),
                let_var_slice("s", "i32", slice_expr(ident("a"), int(0), int(2))),
            ],
        )];
        assert_eq!(codes(items), Vec::<&str>::new());
    }

    #[test]
    fn slice_of_non_array_is_e0232() {
        // var x: i32 = 5; var s: []i32 = x[0..1];
        let items = vec![func(
            "main",
            vec![],
            "void",
            vec![
                let_var("x", "i32", int(5)),
                let_var_slice("s", "i32", slice_expr(ident("x"), int(0), int(1))),
            ],
        )];
        assert!(codes(items).contains(&"E0232"));
    }

    #[test]
    fn slice_of_array_literal_is_e0232() {
        // Slicing a non-addressable array (a literal) is rejected.
        // var s: []i32 = [3]i32{1,2,3}[0..2];
        let items = vec![func(
            "main",
            vec![],
            "void",
            vec![let_var_slice(
                "s",
                "i32",
                slice_expr(
                    array_lit("i32", 3, vec![int(1), int(2), int(3)]),
                    int(0),
                    int(2),
                ),
            )],
        )];
        assert!(codes(items).contains(&"E0232"));
    }

    #[test]
    fn slice_of_slice_yields_slice() {
        // var a: [3]i32 = ...; var s: []i32 = a[0..3]; var s2: []i32 = s[0..2];
        let items = vec![func(
            "main",
            vec![],
            "void",
            vec![
                arr3_decl("a"),
                let_var_slice("s", "i32", slice_expr(ident("a"), int(0), int(3))),
                let_var_slice("s2", "i32", slice_expr(ident("s"), int(0), int(2))),
            ],
        )];
        assert_eq!(codes(items), Vec::<&str>::new());
    }

    #[test]
    fn slice_bound_must_be_integer_e0110() {
        // var a: [3]i32 = ...; var s: []i32 = a[true..2];
        let items = vec![func(
            "main",
            vec![],
            "void",
            vec![
                arr3_decl("a"),
                let_var_slice("s", "i32", slice_expr(ident("a"), boolean(true), int(2))),
            ],
        )];
        assert!(codes(items).contains(&"E0110"));
    }

    #[test]
    fn slice_index_yields_element_type() {
        // var a: [3]i32 = ...; var s: []i32 = a[0..3]; var v: i32 = s[0];
        let items = vec![func(
            "main",
            vec![],
            "void",
            vec![
                arr3_decl("a"),
                let_var_slice("s", "i32", slice_expr(ident("a"), int(0), int(3))),
                let_var("v", "i32", index(ident("s"), int(0))),
            ],
        )];
        assert_eq!(codes(items), Vec::<&str>::new());
    }

    #[test]
    fn slice_index_wrong_element_type_is_e0110() {
        // var s: []i32 = ...; var b: bool = s[0];
        let items = vec![func(
            "main",
            vec![],
            "void",
            vec![
                arr3_decl("a"),
                let_var_slice("s", "i32", slice_expr(ident("a"), int(0), int(3))),
                let_var("b", "bool", index(ident("s"), int(0))),
            ],
        )];
        assert!(codes(items).contains(&"E0110"));
    }

    #[test]
    fn slice_len_is_usize() {
        // var s: []i32 = ...; var n: usize = s.len;
        let items = vec![func(
            "main",
            vec![],
            "void",
            vec![
                arr3_decl("a"),
                let_var_slice("s", "i32", slice_expr(ident("a"), int(0), int(3))),
                let_var("n", "usize", field(ident("s"), "len")),
            ],
        )];
        assert_eq!(codes(items), Vec::<&str>::new());
    }

    #[test]
    fn slice_index_assign() {
        // var a: [3]i32 = ...; var s: []i32 = a[0..3]; s[0] = 9;
        let items = vec![func(
            "main",
            vec![],
            "void",
            vec![
                arr3_decl("a"),
                let_var_slice("s", "i32", slice_expr(ident("a"), int(0), int(3))),
                field_assign(index(ident("s"), int(0)), int(9)),
            ],
        )];
        assert_eq!(codes(items), Vec::<&str>::new());
    }

    #[test]
    fn slice_index_assign_wrong_type_is_e0110() {
        // var s: []i32 = ...; s[0] = true;
        let items = vec![func(
            "main",
            vec![],
            "void",
            vec![
                arr3_decl("a"),
                let_var_slice("s", "i32", slice_expr(ident("a"), int(0), int(3))),
                field_assign(index(ident("s"), int(0)), boolean(true)),
            ],
        )];
        assert!(codes(items).contains(&"E0110"));
    }

    #[test]
    fn slice_param_and_len() {
        // fn count(s: []i32) usize { return s.len; }
        // fn main() void { var a: [2]i32 = [2]i32{1,2}; var n: usize = count(a[0..2]); }
        let items = vec![
            Item::Func(Func {
                is_pub: false,
                name: "count".into(),
                params: vec![Param {
                    name: "s".into(),
                    ty: te_slice("i32"),
                    is_comptime: false,
                    span: sp(),
                }],
                ret: te("usize"),
                body: block(vec![ret(Some(field(ident("s"), "len")))]),
                span: sp(),
            }),
            func(
                "main",
                vec![],
                "void",
                vec![
                    let_var_arr("a", "i32", 2, array_lit("i32", 2, vec![int(1), int(2)])),
                    let_var(
                        "n",
                        "usize",
                        call("count", vec![slice_expr(ident("a"), int(0), int(2))]),
                    ),
                ],
            ),
        ];
        assert_eq!(codes(items), Vec::<&str>::new());
    }

    #[test]
    fn pointer_struct_field_resolves_to_ptr_type() {
        // const Point = struct { x: i32 };
        // const Holder = struct { p: *Point };  → field `p` resolves to `*Point`.
        let point = struct_item("Point", vec![("x", "i32")]);
        let holder = Item::Struct(StructDecl {
            is_pub: false,
            name: "Holder".into(),
            fields: vec![FieldDecl {
                name: "p".into(),
                ty: te_ptr("Point"),
                span: sp(),
            }],
            methods: Vec::new(),
            span: sp(),
        });
        let m = Module {
            items: vec![point, holder],
        };
        let table = check(&m).expect("pointer-field struct should type-check");
        let id = table.id_of("Holder").unwrap();
        let (fname, fty) = table.get(id).fields[0].clone();
        assert_eq!(fname, "p");
        match fty {
            Type::Ptr(pid) => {
                let pointee = table.ptr_pointee(pid);
                let sid = table.id_of("Point").unwrap();
                assert_eq!(pointee, Type::Struct(sid));
            }
            other => panic!("expected a pointer field type, found {:?}", other),
        }
    }

    #[test]
    fn slice_struct_field_resolves_to_slice_type() {
        // const Row = struct { cells: []i32 };  → field `cells` resolves to `[]i32`.
        let row = Item::Struct(StructDecl {
            is_pub: false,
            name: "Row".into(),
            fields: vec![FieldDecl {
                name: "cells".into(),
                ty: te_slice("i32"),
                span: sp(),
            }],
            methods: Vec::new(),
            span: sp(),
        });
        let m = Module { items: vec![row] };
        let table = check(&m).expect("slice-field struct should type-check");
        let id = table.id_of("Row").unwrap();
        let (fname, fty) = table.get(id).fields[0].clone();
        assert_eq!(fname, "cells");
        match fty {
            Type::Slice(sid) => assert_eq!(table.slice_elem(sid), Type::I32),
            other => panic!("expected a slice field type, found {:?}", other),
        }
    }

    // ---- Allocator + heap tests (v0.119) ---------------------------------

    #[test]
    fn c_allocator_yields_allocator_and_alloc_free_ok() {
        // fn main() void {
        //   var a: Allocator = c_allocator();
        //   var s: []i32 = alloc(a, i32, 4);
        //   free(a, s);
        // }
        let items = vec![func(
            "main",
            vec![],
            "void",
            vec![
                let_var("a", "Allocator", call("c_allocator", vec![])),
                let_var_slice(
                    "s",
                    "i32",
                    call("alloc", vec![ident("a"), ident("i32"), int(4)]),
                ),
                Stmt::Expr(call("free", vec![ident("a"), ident("s")])),
            ],
        )];
        assert_eq!(codes(items), Vec::<&str>::new());
    }

    #[test]
    fn alloc_yields_slice_of_elem_type() {
        // fn f(a: Allocator) []i32 { return alloc(a, i32, 4); }
        let items = vec![func_te(
            "f",
            vec![param("a", "Allocator")],
            te_slice("i32"),
            vec![ret(Some(call(
                "alloc",
                vec![ident("a"), ident("i32"), int(4)],
            )))],
        )];
        let m = Module { items };
        let table = check(&m).expect("alloc program should type-check");
        // The result is `[]i32`: the interned slice's element is `i32`.
        let elems: Vec<Type> = table.slices().map(|(_, e)| e).collect();
        assert!(elems.contains(&Type::I32), "slices = {:?}", elems);
    }

    #[test]
    fn alloc_of_struct_type_ok() {
        // const Point = struct { x: i32, y: i32 };
        // fn f(a: Allocator) []Point { return alloc(a, Point, 2); }
        let point = struct_item("Point", vec![("x", "i32"), ("y", "i32")]);
        let f = func_te(
            "f",
            vec![param("a", "Allocator")],
            te_slice("Point"),
            vec![ret(Some(call(
                "alloc",
                vec![ident("a"), ident("Point"), int(2)],
            )))],
        );
        let m = Module {
            items: vec![point, f],
        };
        let table = check(&m).expect("alloc-of-struct should type-check");
        let pid = table.id_of("Point").unwrap();
        let elems: Vec<Type> = table.slices().map(|(_, e)| e).collect();
        assert!(
            elems.contains(&Type::Struct(pid)),
            "expected a `[]Point` slice, slices = {:?}",
            elems
        );
    }

    #[test]
    fn alloc_with_non_type_second_arg_is_e0241() {
        // fn f(a: Allocator) []i32 { return alloc(a, nope, 4); }  // `nope` is no type
        let items = vec![func_te(
            "f",
            vec![param("a", "Allocator")],
            te_slice("i32"),
            vec![ret(Some(call(
                "alloc",
                vec![ident("a"), ident("nope"), int(4)],
            )))],
        )];
        let cs = codes(items);
        assert!(cs.contains(&"E0241"), "codes = {:?}", cs);
        // The type-name slot is never type-checked as a value, so there is no
        // spurious "unknown name" diagnostic.
        assert!(!cs.contains(&"E0100"), "codes = {:?}", cs);
    }

    #[test]
    fn alloc_with_non_ident_second_arg_is_e0241() {
        // fn f(a: Allocator) []i32 { return alloc(a, 5, 4); }  // `5` is not a type
        let items = vec![func_te(
            "f",
            vec![param("a", "Allocator")],
            te_slice("i32"),
            vec![ret(Some(call("alloc", vec![ident("a"), int(5), int(4)])))],
        )];
        assert!(codes(items).contains(&"E0241"));
    }

    #[test]
    fn free_of_non_slice_is_e0242() {
        // fn f(a: Allocator, x: i32) void { free(a, x); }   // x is not a slice
        let items = vec![func(
            "f",
            vec![param("a", "Allocator"), param("x", "i32")],
            "void",
            vec![Stmt::Expr(call("free", vec![ident("a"), ident("x")]))],
        )];
        assert!(codes(items).contains(&"E0242"));
    }

    #[test]
    fn alloc_with_non_allocator_first_arg_is_e0110() {
        // fn f(x: i32) []i32 { return alloc(x, i32, 4); }   // x is not an Allocator
        let items = vec![func_te(
            "f",
            vec![param("x", "i32")],
            te_slice("i32"),
            vec![ret(Some(call(
                "alloc",
                vec![ident("x"), ident("i32"), int(4)],
            )))],
        )];
        assert!(codes(items).contains(&"E0110"));
    }

    #[test]
    fn redefining_allocator_builtins_is_e0101() {
        for name in ["c_allocator", "alloc", "free"] {
            let items = vec![func(name, vec![], "void", vec![])];
            assert!(
                codes(items).contains(&"E0101"),
                "redefining `{}` should be E0101",
                name
            );
        }
    }

    #[test]
    fn allocator_param_and_return_ok() {
        // fn dup(a: Allocator) Allocator { return a; }  — assign/param/return ok.
        let items = vec![func(
            "dup",
            vec![param("a", "Allocator")],
            "Allocator",
            vec![ret(Some(ident("a")))],
        )];
        assert_eq!(codes(items), Vec::<&str>::new());
    }

    #[test]
    fn allocator_arithmetic_is_type_error() {
        // fn f(a: Allocator) Allocator { return a + a; }  — not arithmetic-able.
        let items = vec![func(
            "f",
            vec![param("a", "Allocator")],
            "Allocator",
            vec![ret(Some(bin(BinOp::Add, ident("a"), ident("a"))))],
        )];
        assert!(codes(items).contains(&"E0110"));
    }

    #[test]
    fn allocator_comparison_is_type_error() {
        // fn f(a: Allocator) bool { return a == a; }  — not comparable.
        let items = vec![func(
            "f",
            vec![param("a", "Allocator")],
            "bool",
            vec![ret(Some(bin(BinOp::Eq, ident("a"), ident("a"))))],
        )];
        assert!(codes(items).contains(&"E0110"));
    }

    #[test]
    fn allocator_index_is_e0220() {
        // fn f(a: Allocator) i32 { return a[0]; }  — not indexable.
        let items = vec![func(
            "f",
            vec![param("a", "Allocator")],
            "i32",
            vec![ret(Some(index(ident("a"), int(0))))],
        )];
        assert!(codes(items).contains(&"E0220"));
    }

    #[test]
    fn allocator_field_access_is_e0165() {
        // fn f(a: Allocator) i32 { return a.x; }  — no fields.
        let items = vec![func(
            "f",
            vec![param("a", "Allocator")],
            "i32",
            vec![ret(Some(field(ident("a"), "x")))],
        )];
        assert!(codes(items).contains(&"E0165"));
    }

    #[test]
    fn c_allocator_with_args_is_e0110() {
        // fn f() void { var x: i32 = c_allocator(5); }  — takes no arguments.
        let items = vec![func(
            "f",
            vec![],
            "void",
            vec![let_var("a", "Allocator", call("c_allocator", vec![int(5)]))],
        )];
        assert!(codes(items).contains(&"E0110"));
    }

    // ---- comptime generics (v0.120) ---------------------------------------

    /// `fn max(comptime T: type, a: T, b: T) T { if (a > b) { return a; } return b; }`
    fn generic_max() -> Item {
        func(
            "max",
            vec![param_comptime("T"), param("a", "T"), param("b", "T")],
            "T",
            vec![
                Stmt::If {
                    cond: bin(BinOp::Gt, ident("a"), ident("b")),
                    capture: None,
                    then: block(vec![ret(Some(ident("a")))]),
                    els: None,
                    span: sp(),
                },
                ret(Some(ident("b"))),
            ],
        )
    }

    #[test]
    fn generic_two_instantiations_recorded() {
        // Calling `max` at i32 and i64 records exactly two instantiations, and
        // its body type-checks under each substitution.
        let items = vec![
            generic_max(),
            func(
                "main",
                vec![],
                "void",
                vec![
                    let_var("x", "i32", call("max", vec![ident("i32"), int(1), int(2)])),
                    let_var("y", "i64", call("max", vec![ident("i64"), int(3), int(4)])),
                    Stmt::Expr(call("print", vec![ident("x")])),
                ],
            ),
        ];
        let table = check_ok(items);
        let insts = table.instantiations();
        assert_eq!(insts.len(), 2, "expected two instantiations: {:?}", insts);
        assert!(insts
            .iter()
            .any(|i| i.fn_name == "max" && i.args == vec![ComptimeArg::Type(Type::I32)]));
        assert!(insts
            .iter()
            .any(|i| i.fn_name == "max" && i.args == vec![ComptimeArg::Type(Type::I64)]));
    }

    #[test]
    fn generic_returning_type_param_ok() {
        // fn id(comptime T: type, a: T) T { return a; }
        // fn main() void { var x: i32 = id(i32, 5); print(x); }
        let items = vec![
            func(
                "id",
                vec![param_comptime("T"), param("a", "T")],
                "T",
                vec![ret(Some(ident("a")))],
            ),
            func(
                "main",
                vec![],
                "void",
                vec![
                    let_var("x", "i32", call("id", vec![ident("i32"), int(5)])),
                    Stmt::Expr(call("print", vec![ident("x")])),
                ],
            ),
        ];
        let table = check_ok(items);
        assert_eq!(table.instantiations().len(), 1);
        assert!(table
            .instantiations()
            .iter()
            .any(|i| i.fn_name == "id" && i.args == vec![ComptimeArg::Type(Type::I32)]));
    }

    #[test]
    fn generic_local_of_type_param_resolves_under_subst() {
        // fn dup(comptime T: type, a: T) T { var b: T = a; return b; }
        // The local `b: T` resolves to the concrete type via the substitution.
        let items = vec![
            func(
                "dup",
                vec![param_comptime("T"), param("a", "T")],
                "T",
                vec![
                    let_var("b", "T", ident("a")),
                    ret(Some(ident("b"))),
                ],
            ),
            func(
                "main",
                vec![],
                "void",
                vec![let_var("x", "i32", call("dup", vec![ident("i32"), int(7)]))],
            ),
        ];
        let table = check_ok(items);
        assert_eq!(table.instantiations().len(), 1);
    }

    #[test]
    fn generic_body_type_checked_under_subst_is_e0110() {
        // fn bad(comptime T: type, a: T) T { return true; }  — instantiated at
        // i32, the body's `return true` mismatches the substituted return `i32`.
        let items = vec![
            func(
                "bad",
                vec![param_comptime("T"), param("a", "T")],
                "T",
                vec![ret(Some(boolean(true)))],
            ),
            func(
                "main",
                vec![],
                "void",
                vec![let_var("x", "i32", call("bad", vec![ident("i32"), int(5)]))],
            ),
        ];
        assert!(codes(items).contains(&"E0110"));
    }

    #[test]
    fn uninstantiated_generic_body_is_not_checked() {
        // A generic function that is never called is never body-checked, so the
        // unknown name in its body does not surface (no normal-pass check).
        let items = vec![
            func(
                "neverused",
                vec![param_comptime("T"), param("a", "T")],
                "T",
                vec![ret(Some(ident("undefined_name")))],
            ),
            func("main", vec![], "void", vec![]),
        ];
        assert_eq!(codes(items), Vec::<&str>::new());
    }

    #[test]
    fn generic_calling_generic_records_transitive_instantiation() {
        // fn inner(comptime T: type, a: T) T { return a; }
        // fn outer(comptime T: type, a: T) T { return inner(T, a); }
        // fn main() void { var x: i32 = outer(i32, 5); print(x); }
        let items = vec![
            func(
                "inner",
                vec![param_comptime("T"), param("a", "T")],
                "T",
                vec![ret(Some(ident("a")))],
            ),
            func(
                "outer",
                vec![param_comptime("T"), param("a", "T")],
                "T",
                vec![ret(Some(call("inner", vec![ident("T"), ident("a")])))],
            ),
            func(
                "main",
                vec![],
                "void",
                vec![
                    let_var("x", "i32", call("outer", vec![ident("i32"), int(5)])),
                    Stmt::Expr(call("print", vec![ident("x")])),
                ],
            ),
        ];
        let table = check_ok(items);
        let insts = table.instantiations();
        assert_eq!(insts.len(), 2, "expected outer + inner: {:?}", insts);
        assert!(insts
            .iter()
            .any(|i| i.fn_name == "outer" && i.args == vec![ComptimeArg::Type(Type::I32)]));
        assert!(insts
            .iter()
            .any(|i| i.fn_name == "inner" && i.args == vec![ComptimeArg::Type(Type::I32)]));
    }

    #[test]
    fn recursive_generic_terminates() {
        // A self-recursive generic at the same type argument records a single
        // instantiation (the dedup in `intern_instantiation` bounds recursion).
        // fn rec(comptime T: type, a: T) T { return rec(T, a); }
        let items = vec![
            func(
                "rec",
                vec![param_comptime("T"), param("a", "T")],
                "T",
                vec![ret(Some(call("rec", vec![ident("T"), ident("a")])))],
            ),
            func(
                "main",
                vec![],
                "void",
                vec![let_var("x", "i32", call("rec", vec![ident("i32"), int(1)]))],
            ),
        ];
        let table = check_ok(items);
        assert_eq!(table.instantiations().len(), 1);
    }

    #[test]
    fn non_type_argument_is_e0251() {
        // fn id(comptime T: type, a: T) T { return a; }
        // fn main() void { var x: i32 = id(5, 3); }  — `5` is not a type name.
        let items = vec![
            func(
                "id",
                vec![param_comptime("T"), param("a", "T")],
                "T",
                vec![ret(Some(ident("a")))],
            ),
            func(
                "main",
                vec![],
                "void",
                vec![let_var("x", "i32", call("id", vec![int(5), int(3)]))],
            ),
        ];
        assert!(codes(items).contains(&"E0251"));
    }

    #[test]
    fn missing_type_argument_is_e0252() {
        // fn id(comptime T: type, a: T) T { return a; }
        // fn main() void { var x: i32 = id(); }  — no type argument supplied.
        let items = vec![
            func(
                "id",
                vec![param_comptime("T"), param("a", "T")],
                "T",
                vec![ret(Some(ident("a")))],
            ),
            func(
                "main",
                vec![],
                "void",
                vec![let_var("x", "i32", call("id", vec![]))],
            ),
        ];
        assert!(codes(items).contains(&"E0252"));
    }

    #[test]
    fn comptime_non_type_non_int_annotation_is_e0250() {
        // fn f(comptime x: bool) void { }  — a comptime param must be `type` or
        // an integer-typed value parameter; `bool` is neither (v0.128).
        let items = vec![func(
            "f",
            vec![param_comptime_bad("x", "bool")],
            "void",
            vec![],
        )];
        assert!(codes(items).contains(&"E0250"));
    }

    #[test]
    fn generic_with_struct_type_argument_ok() {
        // const Point = struct { x: i32, y: i32 };
        // fn id(comptime T: type, a: T) T { return a; }
        // fn main() void { var p: Point = Point{ .x = 1, .y = 2 }; var q: Point = id(Point, p); }
        let items = vec![
            struct_item("Point", vec![("x", "i32"), ("y", "i32")]),
            func(
                "id",
                vec![param_comptime("T"), param("a", "T")],
                "T",
                vec![ret(Some(ident("a")))],
            ),
            func(
                "main",
                vec![],
                "void",
                vec![
                    let_var(
                        "p",
                        "Point",
                        struct_lit("Point", vec![("x", int(1)), ("y", int(2))]),
                    ),
                    let_var("q", "Point", call("id", vec![ident("Point"), ident("p")])),
                ],
            ),
        ];
        let table = check_ok(items);
        let pid = table.id_of("Point").expect("Point interned");
        assert!(table
            .instantiations()
            .iter()
            .any(|i| i.fn_name == "id" && i.args == vec![ComptimeArg::Type(Type::Struct(pid))]));
    }

    #[test]
    fn generic_argument_type_mismatch_is_e0110() {
        // fn id(comptime T: type, a: T) T { return a; }
        // fn main() void { var x: i32 = id(i32, true); }  — `true` is not i32.
        let items = vec![
            func(
                "id",
                vec![param_comptime("T"), param("a", "T")],
                "T",
                vec![ret(Some(ident("a")))],
            ),
            func(
                "main",
                vec![],
                "void",
                vec![let_var("x", "i32", call("id", vec![ident("i32"), boolean(true)]))],
            ),
        ];
        assert!(codes(items).contains(&"E0110"));
    }

    // ---- comptime value parameters (v0.128, SPEC §24) ---------------------

    #[test]
    fn comptime_value_param_array_size_instantiates_at_n() {
        // fn zeros(comptime n: usize) [n]i32 { var a: [n]i32 = [n]i32{0,0,0}; return a; }
        // fn main() void { var a: [3]i32 = zeros(3); print(a.len); }
        // Instantiated at n=3, the return type resolves to `[3]i32` — verified by
        // looking up the interned array length for the instantiated size.
        let mut zeros = raw_func(
            "zeros",
            vec![param_comptime_val("n", "usize")],
            "i32",
            vec![
                Stmt::Let {
                    is_const: false,
                    name: "a".into(),
                    ty: Some(te_arr_param("i32", "n")),
                    value: Expr::ArrayLit {
                        elem: te_arr_param("i32", "n"),
                        elems: vec![int(0), int(0), int(0)],
                        span: sp(),
                    },
                    span: sp(),
                },
                ret(Some(ident("a"))),
            ],
        );
        zeros.ret = te_arr_param("i32", "n");
        let items = vec![
            Item::Func(zeros),
            func(
                "main",
                vec![],
                "void",
                vec![
                    Stmt::Let {
                        is_const: false,
                        name: "a".into(),
                        ty: Some(te_arr("i32", 3)),
                        value: call("zeros", vec![int(3)]),
                        span: sp(),
                    },
                    Stmt::Expr(call("print", vec![field(ident("a"), "len")])),
                ],
            ),
        ];
        let table = check_ok(items);
        // The instantiation records the value argument 3.
        let insts = table.instantiations();
        assert_eq!(insts.len(), 1, "expected one instantiation: {:?}", insts);
        assert!(insts
            .iter()
            .any(|i| i.fn_name == "zeros" && i.args == vec![ComptimeArg::Value(3)]));
        // A `[3]i32` array type is interned with length 3.
        let aid = table
            .arrays()
            .find(|&(_, elem, len)| elem == Type::I32 && len == 3)
            .map(|(id, _, _)| id)
            .expect("a [3]i32 array should be interned");
        assert_eq!(table.array_len(aid), 3);
        assert_eq!(table.array_elem(aid), Type::I32);
    }

    #[test]
    fn comptime_value_param_used_as_constant_in_body() {
        // fn f(comptime n: usize) usize { return n; }
        // fn main() void { var x: usize = f(7); print(x); }
        // The value parameter `n` is an immutable constant of its declared type
        // (usize) inside the body.
        let mut zeros = raw_func(
            "f",
            vec![param_comptime_val("n", "usize")],
            "usize",
            vec![ret(Some(ident("n")))],
        );
        zeros.ret = te("usize");
        let items = vec![
            Item::Func(zeros),
            func(
                "main",
                vec![],
                "void",
                vec![
                    let_var("x", "usize", call("f", vec![int(7)])),
                    Stmt::Expr(call("print", vec![ident("x")])),
                ],
            ),
        ];
        let table = check_ok(items);
        assert!(table
            .instantiations()
            .iter()
            .any(|i| i.fn_name == "f" && i.args == vec![ComptimeArg::Value(7)]));
    }

    #[test]
    fn comptime_value_two_distinct_values_record_two_instantiations() {
        // fn f(comptime n: usize) usize { return n; }
        // fn main() void { var a = f(2); var b = f(5); }  — two instantiations.
        let mut f = raw_func(
            "f",
            vec![param_comptime_val("n", "usize")],
            "usize",
            vec![ret(Some(ident("n")))],
        );
        f.ret = te("usize");
        let items = vec![
            Item::Func(f),
            func(
                "main",
                vec![],
                "void",
                vec![
                    let_var("a", "usize", call("f", vec![int(2)])),
                    let_var("b", "usize", call("f", vec![int(5)])),
                ],
            ),
        ];
        let table = check_ok(items);
        let insts = table.instantiations();
        assert_eq!(insts.len(), 2, "expected two instantiations: {:?}", insts);
        assert!(insts
            .iter()
            .any(|i| i.fn_name == "f" && i.args == vec![ComptimeArg::Value(2)]));
        assert!(insts
            .iter()
            .any(|i| i.fn_name == "f" && i.args == vec![ComptimeArg::Value(5)]));
    }

    #[test]
    fn comptime_value_const_arg_folds() {
        // const N: usize = 4;
        // fn f(comptime n: usize) usize { return n; }
        // fn main() void { var x: usize = f(N); }  — N folds to 4.
        let mut f = raw_func(
            "f",
            vec![param_comptime_val("n", "usize")],
            "usize",
            vec![ret(Some(ident("n")))],
        );
        f.ret = te("usize");
        let items = vec![
            const_item("N", "usize", int(4)),
            Item::Func(f),
            func(
                "main",
                vec![],
                "void",
                vec![let_var("x", "usize", call("f", vec![ident("N")]))],
            ),
        ];
        let table = check_ok(items);
        assert!(table
            .instantiations()
            .iter()
            .any(|i| i.fn_name == "f" && i.args == vec![ComptimeArg::Value(4)]));
    }

    #[test]
    fn comptime_value_non_constant_argument_is_e0253() {
        // fn f(comptime n: usize) usize { return n; }
        // fn g() usize { return 1; }
        // fn main() void { var x: usize = f(g()); }  — `g()` is not constant.
        let mut f = raw_func(
            "f",
            vec![param_comptime_val("n", "usize")],
            "usize",
            vec![ret(Some(ident("n")))],
        );
        f.ret = te("usize");
        let items = vec![
            Item::Func(f),
            func("g", vec![], "usize", vec![ret(Some(int(1)))]),
            func(
                "main",
                vec![],
                "void",
                vec![let_var("x", "usize", call("f", vec![call("g", vec![])]))],
            ),
        ];
        assert!(codes(items).contains(&"E0253"));
    }

    #[test]
    fn array_size_param_outside_generic_is_e0253() {
        // fn f(a: [n]i32) i32 { return a[0]; }  — `n` is not a comptime value
        // parameter in scope, so the `[n]i32` array size is unbound.
        let items = vec![func(
            "f",
            vec![param_arr_param("a", "i32", "n")],
            "i32",
            vec![ret(Some(index(ident("a"), int(0))))],
        )];
        assert!(codes(items).contains(&"E0253"));
    }

    #[test]
    fn literal_array_size_still_works() {
        // fn f(a: [3]i32) i32 { return a[0]; }  — the v0.117 literal form is
        // unchanged by the v0.128 generalisation.
        let items = vec![func(
            "f",
            vec![param_arr("a", "i32", 3)],
            "i32",
            vec![ret(Some(index(ident("a"), int(0))))],
        )];
        assert_eq!(codes(items), Vec::<&str>::new());
    }

    #[test]
    fn comptime_value_int_param_is_valid_not_e0250() {
        // fn f(comptime n: usize) usize { return n; }  — a value parameter of an
        // integer type is valid (it must NOT be flagged E0250).
        let mut f = raw_func(
            "f",
            vec![param_comptime_val("n", "usize")],
            "usize",
            vec![ret(Some(ident("n")))],
        );
        f.ret = te("usize");
        let items = vec![
            Item::Func(f),
            func(
                "main",
                vec![],
                "void",
                vec![let_var("x", "usize", call("f", vec![int(1)]))],
            ),
        ];
        assert!(!codes(items).contains(&"E0250"));
    }

    // ---- v0.121: type inference for `var`/`const` (SPEC §18) ---------------

    #[test]
    fn inferred_var_int_defaults_to_i64_and_is_usable() {
        // fn main() void { var x = 5; print(x); }  — `x` infers `i64`.
        let items = vec![func(
            "main",
            vec![],
            "void",
            vec![
                let_var_infer("x", int(5)),
                Stmt::Expr(call("print", vec![ident("x")])),
            ],
        )];
        assert!(codes(items).is_empty());
    }

    #[test]
    fn inferred_var_int_is_concretely_i64() {
        // A bare integer literal with no annotation infers `i64` (§18.2), so
        // re-binding it to an `i64` is fine but to an `i32` is a mismatch.
        // fn main() void { var x = 5; var y: i64 = x; var z: i32 = x; }
        let items = vec![func(
            "main",
            vec![],
            "void",
            vec![
                let_var_infer("x", int(5)),
                let_var("y", "i64", ident("x")),
                let_var("z", "i32", ident("x")),
            ],
        )];
        // Only the `i32` re-binding mismatches (`i64` -> `i32`).
        assert_eq!(codes(items), vec!["E0110"]);
    }

    #[test]
    fn annotated_var_still_respects_annotation() {
        // `var x: i32 = 5;` still binds `i32` (annotation path unchanged), so a
        // later `var y: i64 = x;` is a mismatch.
        // fn main() void { var x: i32 = 5; var y: i64 = x; }
        let items = vec![func(
            "main",
            vec![],
            "void",
            vec![
                let_var("x", "i32", int(5)),
                let_var("y", "i64", ident("x")),
            ],
        )];
        assert_eq!(codes(items), vec!["E0110"]);
    }

    #[test]
    fn inferred_binding_used_in_arithmetic() {
        // fn main() void { var x = 5; var y = x + 1; var z: i64 = y; print(z); }
        // `x` and `y` both infer `i64`, so the arithmetic and the `i64`
        // re-binding all type-check.
        let items = vec![func(
            "main",
            vec![],
            "void",
            vec![
                let_var_infer("x", int(5)),
                let_var_infer("y", bin(BinOp::Add, ident("x"), int(1))),
                let_var("z", "i64", ident("y")),
                Stmt::Expr(call("print", vec![ident("z")])),
            ],
        )];
        assert!(codes(items).is_empty());
    }

    #[test]
    fn inferred_bool_binding() {
        // fn main() void { var b = true; var c: bool = b; var d: i64 = b; }
        // `b` infers `bool`: the `bool` re-binding is fine, the `i64` one is not.
        let items = vec![func(
            "main",
            vec![],
            "void",
            vec![
                let_var_infer("b", boolean(true)),
                let_var("c", "bool", ident("b")),
                let_var("d", "i64", ident("b")),
            ],
        )];
        assert_eq!(codes(items), vec!["E0110"]);
    }

    #[test]
    fn inferred_local_const_is_immutable() {
        // An inferred local `const` keeps its `is_const` flag, so assigning to
        // it is rejected (E0110, "cannot assign to immutable binding").
        // fn main() void { const x = 5; x = 6; }
        let items = vec![func(
            "main",
            vec![],
            "void",
            vec![let_const_infer("x", int(5)), assign("x", int(6))],
        )];
        assert_eq!(codes(items), vec!["E0110"]);
    }

    #[test]
    fn inferred_from_call_result() {
        // fn id(a: i32) i32 { return a; }
        // fn main() void { var x = id(0); var y: i32 = x; var z: i64 = x; }
        // `x` infers the call's return type `i32`.
        let items = vec![
            func(
                "id",
                vec![param("a", "i32")],
                "i32",
                vec![ret(Some(ident("a")))],
            ),
            func(
                "main",
                vec![],
                "void",
                vec![
                    let_var_infer("x", call("id", vec![int(0)])),
                    let_var("y", "i32", ident("x")),
                    let_var("z", "i64", ident("x")),
                ],
            ),
        ];
        assert_eq!(codes(items), vec!["E0110"]);
    }

    #[test]
    fn inferred_from_struct_literal() {
        // const Point = struct { x: i32, y: i32 };
        // fn main() void { var p = Point{ .x = 1, .y = 2 }; var q: Point = p; print(p.x); }
        // `p` infers `Point`, so the `Point` re-binding and field access type-check.
        let items = vec![
            struct_item("Point", vec![("x", "i32"), ("y", "i32")]),
            func(
                "main",
                vec![],
                "void",
                vec![
                    let_var_infer("p", struct_lit("Point", vec![("x", int(1)), ("y", int(2))])),
                    let_var("q", "Point", ident("p")),
                    Stmt::Expr(call("print", vec![field(ident("p"), "x")])),
                ],
            ),
        ];
        assert!(codes(items).is_empty());
    }

    #[test]
    fn inferred_top_level_const_int() {
        // const Z = 7;
        // fn main() void { var x: i64 = Z; print(x); }
        // `Z` infers `i64` from its comptime value (§18.2).
        let items = vec![
            const_item_infer("Z", int(7)),
            func(
                "main",
                vec![],
                "void",
                vec![
                    let_var("x", "i64", ident("Z")),
                    Stmt::Expr(call("print", vec![ident("x")])),
                ],
            ),
        ];
        assert!(codes(items).is_empty());
    }

    #[test]
    fn inferred_top_level_const_int_is_i64_not_i32() {
        // const Z = 7;  fn main() void { var x: i32 = Z; }
        // The inferred const is `i64`, so re-binding it to `i32` is a mismatch.
        let items = vec![
            const_item_infer("Z", int(7)),
            func(
                "main",
                vec![],
                "void",
                vec![let_var("x", "i32", ident("Z"))],
            ),
        ];
        assert_eq!(codes(items), vec!["E0110"]);
    }

    #[test]
    fn inferred_top_level_const_bool() {
        // const FLAG = true;  fn main() void { var b: bool = FLAG; }
        let items = vec![
            const_item_infer("FLAG", boolean(true)),
            func(
                "main",
                vec![],
                "void",
                vec![let_var("b", "bool", ident("FLAG"))],
            ),
        ];
        assert!(codes(items).is_empty());
    }

    #[test]
    fn inferred_top_level_const_usable_in_other_const() {
        // const A = 2;  const B = A + 3;  fn main() void { var x: i64 = B; }
        // An inferred top-level const folds and is referable by later consts.
        let items = vec![
            const_item_infer("A", int(2)),
            const_item_infer("B", bin(BinOp::Add, ident("A"), int(3))),
            func(
                "main",
                vec![],
                "void",
                vec![let_var("x", "i64", ident("B"))],
            ),
        ];
        assert!(codes(items).is_empty());
    }

    #[test]
    fn inferred_var_null_is_e0260() {
        // fn main() void { var x = null; }  — `null` has no inferable type (§18.2).
        let items = vec![func(
            "main",
            vec![],
            "void",
            vec![let_var_infer("x", null_lit())],
        )];
        // Exactly E0260 — not also E0180 (the no-context-null error is suppressed
        // in favour of the missing-annotation message).
        assert_eq!(codes(items), vec!["E0260"]);
    }

    #[test]
    fn inferred_var_error_lit_is_e0260() {
        // fn main() void { var x = error.Boom; }
        let items = vec![func(
            "main",
            vec![],
            "void",
            vec![let_var_infer("x", error_lit("Boom"))],
        )];
        assert_eq!(codes(items), vec!["E0260"]);
    }

    #[test]
    fn inferred_var_unqualified_enum_lit_is_e0260() {
        // const Color = enum { Red, Green };
        // fn main() void { var x = .Red; }  — `.Red` needs an enum context.
        let items = vec![
            Item::Enum(EnumDecl {
                is_pub: false,
                name: "Color".into(),
                variants: vec![
                    EnumVariant { name: "Red".into(), value: None, span: sp() },
                    EnumVariant { name: "Green".into(), value: None, span: sp() },
                ],
                span: sp(),
            }),
            func(
                "main",
                vec![],
                "void",
                vec![let_var_infer("x", Expr::EnumLit {
                    variant: "Red".into(),
                    span: sp(),
                })],
            ),
        ];
        assert_eq!(codes(items), vec!["E0260"]);
    }

    #[test]
    fn inferred_const_null_is_e0260() {
        // An inferred local `const` of `null` is also E0260.
        // fn main() void { const x = null; }
        let items = vec![func(
            "main",
            vec![],
            "void",
            vec![let_const_infer("x", null_lit())],
        )];
        assert_eq!(codes(items), vec!["E0260"]);
    }

    #[test]
    fn inferred_var_from_qualified_enum_lit() {
        // A *qualified* enum literal carries its enum type, so it is inferable.
        // const Color = enum { Red, Green };
        // fn main() void { var c = Color.Red; var d: Color = c; }
        let items = vec![
            Item::Enum(EnumDecl {
                is_pub: false,
                name: "Color".into(),
                variants: vec![
                    EnumVariant { name: "Red".into(), value: None, span: sp() },
                    EnumVariant { name: "Green".into(), value: None, span: sp() },
                ],
                span: sp(),
            }),
            func(
                "main",
                vec![],
                "void",
                vec![
                    let_var_infer("c", field(ident("Color"), "Red")),
                    let_var("d", "Color", ident("c")),
                ],
            ),
        ];
        assert!(codes(items).is_empty());
    }

    #[test]
    fn inferred_var_does_not_mask_real_error() {
        // An inferred binding off an unknown name reports just that error
        // (E0100), not also E0260 — the value's error is already surfaced.
        // fn main() void { var x = bogus; }
        let items = vec![func(
            "main",
            vec![],
            "void",
            vec![let_var_infer("x", ident("bogus"))],
        )];
        assert_eq!(codes(items), vec!["E0100"]);
    }

    // ---- tagged unions `union(enum)` + capture (v0.124) ------------------

    /// A tagged-union item `const name = union(enum) { v: T, ... };` (v0.124).
    fn union_item(name: &str, variants: Vec<(&str, &str)>) -> Item {
        Item::Union(UnionDecl {
            is_pub: false,
            name: name.into(),
            variants: variants
                .into_iter()
                .map(|(n, t)| UnionVariant {
                    name: n.into(),
                    payload: te(t),
                    span: sp(),
                })
                .collect(),
            span: sp(),
        })
    }
    /// The canonical `Num = union(enum) { i: i32, b: bool }` of the union tests.
    fn num_union() -> Item {
        union_item("Num", vec![("i", "i32"), ("b", "bool")])
    }

    #[test]
    fn union_construction_typed_and_interned() {
        // const Num = union(enum) { i: i32, b: bool };
        // fn make() Num { return Num{ .i = 5 }; }
        let items = vec![
            num_union(),
            func(
                "make",
                vec![],
                "Num",
                vec![ret(Some(struct_lit("Num", vec![("i", int(5))])))],
            ),
        ];
        let m = Module { items };
        let table = check(&m).expect("union program should type-check");
        let id = table.union_id_of("Num").expect("Num should be registered");
        assert_eq!(table.union_get(id).variant_index("i"), Some(0));
        assert_eq!(table.union_get(id).variant_index("b"), Some(1));
        assert_eq!(table.union_get(id).payload_type("i"), Some(Type::I32));
        assert_eq!(table.union_get(id).payload_type("b"), Some(Type::Bool));
    }

    #[test]
    fn union_construction_wrong_field_count_is_e0270() {
        // Num{ .i = 1, .b = true } — two initializers, not exactly one.
        let items = vec![
            num_union(),
            func(
                "make",
                vec![],
                "Num",
                vec![ret(Some(struct_lit(
                    "Num",
                    vec![("i", int(1)), ("b", boolean(true))],
                )))],
            ),
        ];
        assert!(codes(items).contains(&"E0270"));
    }

    #[test]
    fn union_construction_zero_fields_is_e0270() {
        // Num{} — zero initializers, not exactly one.
        let items = vec![
            num_union(),
            func(
                "make",
                vec![],
                "Num",
                vec![ret(Some(struct_lit("Num", vec![])))],
            ),
        ];
        assert!(codes(items).contains(&"E0270"));
    }

    #[test]
    fn union_construction_unknown_variant_is_e0271() {
        // Num{ .z = 1 } — `z` is not a variant of `Num`.
        let items = vec![
            num_union(),
            func(
                "make",
                vec![],
                "Num",
                vec![ret(Some(struct_lit("Num", vec![("z", int(1))])))],
            ),
        ];
        assert!(codes(items).contains(&"E0271"));
    }

    #[test]
    fn union_construction_payload_mismatch_is_e0110() {
        // Num{ .i = true } — variant `i` carries `i32`, value is `bool`.
        let items = vec![
            num_union(),
            func(
                "make",
                vec![],
                "Num",
                vec![ret(Some(struct_lit("Num", vec![("i", boolean(true))])))],
            ),
        ];
        assert!(codes(items).contains(&"E0110"));
    }

    #[test]
    fn union_switch_exhaustive_with_capture_binds_payload() {
        // fn f(n: Num) void {
        //     switch (n) {
        //         .i => |x| { var w: i32 = x; },
        //         .b => |y| { var z: bool = y; },
        //     }
        // }
        // Binding the captures to their declared payload types proves each
        // capture is typed as the matched variant's payload (else E0110).
        let items = vec![
            num_union(),
            func(
                "f",
                vec![param("n", "Num")],
                "void",
                vec![switch_stmt(
                    ident("n"),
                    vec![
                        switch_arm_cap(
                            vec![enum_lit("i")],
                            "x",
                            vec![let_var("w", "i32", ident("x"))],
                        ),
                        switch_arm_cap(
                            vec![enum_lit("b")],
                            "y",
                            vec![let_var("z", "bool", ident("y"))],
                        ),
                    ],
                    None,
                )],
            ),
        ];
        assert_eq!(codes(items), Vec::<&str>::new());
    }

    #[test]
    fn union_switch_capture_type_is_the_variant_payload() {
        // Using the `i32` payload `x` where a `bool` is expected is `E0110` —
        // confirming the capture is the variant payload type, not something else.
        // switch (n) { .i => |x| { var w: bool = x; }, .b => {} }
        let items = vec![
            num_union(),
            func(
                "f",
                vec![param("n", "Num")],
                "void",
                vec![switch_stmt(
                    ident("n"),
                    vec![
                        switch_arm_cap(
                            vec![enum_lit("i")],
                            "x",
                            vec![let_var("w", "bool", ident("x"))],
                        ),
                        switch_arm(vec![enum_lit("b")], vec![]),
                    ],
                    None,
                )],
            ),
        ];
        assert!(codes(items).contains(&"E0110"));
    }

    #[test]
    fn union_switch_arm_without_capture_ok() {
        // A union switch arm need not capture; the payload is simply not bound.
        // switch (n) { .i => {}, .b => {} }
        let items = vec![
            num_union(),
            func(
                "f",
                vec![param("n", "Num")],
                "void",
                vec![switch_stmt(
                    ident("n"),
                    vec![
                        switch_arm(vec![enum_lit("i")], vec![]),
                        switch_arm(vec![enum_lit("b")], vec![]),
                    ],
                    None,
                )],
            ),
        ];
        assert_eq!(codes(items), Vec::<&str>::new());
    }

    #[test]
    fn union_switch_with_else_is_exhaustive() {
        // switch (n) { .i => |x| { var w: i32 = x; }, else => {} }
        let items = vec![
            num_union(),
            func(
                "f",
                vec![param("n", "Num")],
                "void",
                vec![switch_stmt(
                    ident("n"),
                    vec![switch_arm_cap(
                        vec![enum_lit("i")],
                        "x",
                        vec![let_var("w", "i32", ident("x"))],
                    )],
                    Some(vec![]),
                )],
            ),
        ];
        assert_eq!(codes(items), Vec::<&str>::new());
    }

    #[test]
    fn union_switch_missing_variant_is_e0210() {
        // switch (n) { .i => |x| {} } — missing `.b`, no `else`.
        let items = vec![
            num_union(),
            func(
                "f",
                vec![param("n", "Num")],
                "void",
                vec![switch_stmt(
                    ident("n"),
                    vec![switch_arm_cap(vec![enum_lit("i")], "x", vec![])],
                    None,
                )],
            ),
        ];
        assert!(codes(items).contains(&"E0210"));
    }

    #[test]
    fn union_switch_unknown_variant_label_is_e0271() {
        // switch (n) { .i => {}, .z => {}, else => {} } — `.z` is no variant.
        let items = vec![
            num_union(),
            func(
                "f",
                vec![param("n", "Num")],
                "void",
                vec![switch_stmt(
                    ident("n"),
                    vec![
                        switch_arm(vec![enum_lit("i")], vec![]),
                        switch_arm(vec![enum_lit("z")], vec![]),
                    ],
                    Some(vec![]),
                )],
            ),
        ];
        assert!(codes(items).contains(&"E0271"));
    }

    #[test]
    fn capture_on_enum_switch_is_e0272() {
        // A payload capture on an *enum* switch is invalid (E0272).
        // switch (c) { .Red => |x| {}, .Green => {}, .Blue => {} }
        let items = vec![
            color_enum(),
            func(
                "classify",
                vec![param("c", "Color")],
                "void",
                vec![switch_stmt(
                    ident("c"),
                    vec![
                        switch_arm_cap(vec![enum_lit("Red")], "x", vec![]),
                        switch_arm(vec![enum_lit("Green")], vec![]),
                        switch_arm(vec![enum_lit("Blue")], vec![]),
                    ],
                    None,
                )],
            ),
        ];
        assert!(codes(items).contains(&"E0272"));
    }

    #[test]
    fn capture_on_int_switch_is_e0272() {
        // A payload capture on an *integer* switch is invalid (E0272).
        // switch (n) { 0 => |x| {}, else => {} }
        let items = vec![func(
            "f",
            vec![param("n", "i32")],
            "void",
            vec![switch_stmt(
                ident("n"),
                vec![switch_arm_cap(vec![int(0)], "x", vec![])],
                Some(vec![]),
            )],
        )];
        assert!(codes(items).contains(&"E0272"));
    }

    #[test]
    fn union_variant_with_struct_payload_ok() {
        // const Point = struct { x: i32, y: i32 };
        // const Shape = union(enum) { p: Point, n: i32 };
        // fn make() Shape { return Shape{ .p = Point{ .x = 1, .y = 2 } }; }
        let items = vec![
            struct_item("Point", vec![("x", "i32"), ("y", "i32")]),
            union_item("Shape", vec![("p", "Point"), ("n", "i32")]),
            func(
                "make",
                vec![],
                "Shape",
                vec![ret(Some(struct_lit(
                    "Shape",
                    vec![(
                        "p",
                        struct_lit("Point", vec![("x", int(1)), ("y", int(2))]),
                    )],
                )))],
            ),
        ];
        let m = Module { items };
        let table = check(&m).expect("union-with-struct-payload should type-check");
        let pid = table.id_of("Point").unwrap();
        let sid = table.union_id_of("Shape").unwrap();
        assert_eq!(
            table.union_get(sid).payload_type("p"),
            Some(Type::Struct(pid))
        );
    }

    #[test]
    fn duplicate_union_variant_is_e0211() {
        // const Bad = union(enum) { i: i32, i: bool };
        let items = vec![union_item("Bad", vec![("i", "i32"), ("i", "bool")])];
        assert!(codes(items).contains(&"E0211"));
    }

    #[test]
    fn union_variant_unknown_payload_type_is_e0100() {
        // const U = union(enum) { x: Nope };  — `Nope` is not a type.
        let items = vec![union_item("U", vec![("x", "Nope")])];
        assert!(codes(items).contains(&"E0100"));
    }

    // ---- optional `if` capture + `errdefer` (v0.125, SPEC §21) ------------

    #[test]
    fn if_capture_binds_unwrapped_value_as_inner_type() {
        // fn f(o: ?i32) void { if (o) |v| { var x: i32 = v + 1; print(x); } }
        // `v` is the unwrapped `i32`, so `v + 1` is `i32` and assigns to `x: i32`.
        let items = vec![func(
            "f",
            vec![param_opt("o", "i32")],
            "void",
            vec![if_capture(
                ident("o"),
                "v",
                vec![
                    let_var("x", "i32", bin(BinOp::Add, ident("v"), int(1))),
                    Stmt::Expr(call("print", vec![ident("x")])),
                ],
                None,
            )],
        )];
        assert_eq!(codes(items), Vec::<&str>::new());
    }

    #[test]
    fn if_capture_value_is_returnable_as_inner_type() {
        // fn f(o: ?i32) i32 { if (o) |v| { return v; } return 0; }
        // The captured `v` (an `i32`) matches the `i32` return type.
        let items = vec![func(
            "f",
            vec![param_opt("o", "i32")],
            "i32",
            vec![
                if_capture(ident("o"), "v", vec![ret(Some(ident("v")))], None),
                ret(Some(int(0))),
            ],
        )];
        assert_eq!(codes(items), Vec::<&str>::new());
    }

    #[test]
    fn if_capture_on_non_optional_is_e0280() {
        // fn f(b: bool) void { if (b) |v| { print(0); } }
        // A capture requires an optional condition; `bool` is rejected.
        let items = vec![func(
            "f",
            vec![param("b", "bool")],
            "void",
            vec![if_capture(
                ident("b"),
                "v",
                vec![Stmt::Expr(call("print", vec![int(0)]))],
                None,
            )],
        )];
        assert_eq!(codes(items), vec!["E0280"]);
    }

    #[test]
    fn if_capture_else_branch_has_no_binding() {
        // fn f(o: ?i32) i32 {
        //     if (o) |v| { return v; } else { return v; }  // `v` unknown in else
        //     return 0;
        // }
        // The capture binds only in `then`; using it in `else` is an unknown name.
        let items = vec![func(
            "f",
            vec![param_opt("o", "i32")],
            "i32",
            vec![
                if_capture(
                    ident("o"),
                    "v",
                    vec![ret(Some(ident("v")))],
                    Some(vec![ret(Some(ident("v")))]),
                ),
                ret(Some(int(0))),
            ],
        )];
        assert!(codes(items).contains(&"E0100"));
    }

    #[test]
    fn if_capture_does_not_leak_into_following_statements() {
        // fn f(o: ?i32) void { if (o) |v| { print(v); } print(v); }  // 2nd `v` unknown
        let items = vec![func(
            "f",
            vec![param_opt("o", "i32")],
            "void",
            vec![
                if_capture(
                    ident("o"),
                    "v",
                    vec![Stmt::Expr(call("print", vec![ident("v")]))],
                    None,
                ),
                Stmt::Expr(call("print", vec![ident("v")])),
            ],
        )];
        assert!(codes(items).contains(&"E0100"));
    }

    #[test]
    fn errdefer_stmt_type_checks_in_any_function() {
        // fn f() void { errdefer print(0); }
        // `errdefer` is accepted in any function and its inner stmt is checked.
        let items = vec![func(
            "f",
            vec![],
            "void",
            vec![errdefer_stmt(Stmt::Expr(call("print", vec![int(0)])))],
        )];
        assert_eq!(codes(items), Vec::<&str>::new());
    }

    #[test]
    fn errdefer_bad_inner_stmt_reports_its_error() {
        // fn f() void { errdefer print(missing); }  // `missing` unknown => E0100
        let items = vec![func(
            "f",
            vec![],
            "void",
            vec![errdefer_stmt(Stmt::Expr(call("print", vec![ident("missing")])))],
        )];
        assert!(codes(items).contains(&"E0100"));
    }

    // ---- strings (`[]u8` literals, v0.127, SPEC §23) ----------------------

    #[test]
    fn str_lit_is_slice_of_u8() {
        // A string literal `"hi"` type-checks to the interned `[]u8` slice type.
        let mut cx = Checker::new();
        let t = cx.check_expr(&str_lit("hi"), None);
        let expected = Type::Slice(cx.structs.intern_slice(Type::U8));
        assert_eq!(t, Some(expected));
        // And `type_name` renders it as `[]u8` via the existing slice naming.
        assert_eq!(cx.type_name(expected), "[]u8");
    }

    #[test]
    fn inferred_str_var_is_slice_of_u8() {
        // fn main() void { var s = "hi"; var t: []u8 = s; }
        // The inferred `s` must be `[]u8`, so assigning it to a `[]u8` binding
        // type-checks (and only an `[]u8` would).
        let items = vec![func(
            "main",
            vec![],
            "void",
            vec![
                let_var_infer("s", str_lit("hi")),
                let_var_slice("t", "u8", ident("s")),
            ],
        )];
        assert_eq!(codes(items), Vec::<&str>::new());
    }

    #[test]
    fn inferred_str_var_is_not_slice_of_i32() {
        // fn main() void { var s = "hi"; var t: []i32 = s; }  // []u8 != []i32
        let items = vec![func(
            "main",
            vec![],
            "void",
            vec![
                let_var_infer("s", str_lit("hi")),
                let_var_slice("t", "i32", ident("s")),
            ],
        )];
        assert!(codes(items).contains(&"E0110"));
    }

    #[test]
    fn print_of_string_literal_ok() {
        // fn main() void { print("hi"); }
        let items = vec![func(
            "main",
            vec![],
            "void",
            vec![Stmt::Expr(call("print", vec![str_lit("hi")]))],
        )];
        assert_eq!(codes(items), Vec::<&str>::new());
    }

    #[test]
    fn print_of_string_var_ok() {
        // fn main() void { var s = "hi"; print(s); }
        let items = vec![func(
            "main",
            vec![],
            "void",
            vec![
                let_var_infer("s", str_lit("hi")),
                Stmt::Expr(call("print", vec![ident("s")])),
            ],
        )];
        assert_eq!(codes(items), Vec::<&str>::new());
    }

    #[test]
    fn print_of_u8_slice_param_ok() {
        // fn f(s: []u8) void { print(s); }
        let items = vec![func(
            "f",
            vec![param_slice("s", "u8")],
            "void",
            vec![Stmt::Expr(call("print", vec![ident("s")]))],
        )];
        assert_eq!(codes(items), Vec::<&str>::new());
    }

    #[test]
    fn string_len_is_usize_and_index_is_u8() {
        // fn main() void { var s = "hi"; var n: usize = s.len; var b: u8 = s[0]; }
        let items = vec![func(
            "main",
            vec![],
            "void",
            vec![
                let_var_infer("s", str_lit("hi")),
                let_var("n", "usize", field(ident("s"), "len")),
                let_var("b", "u8", index(ident("s"), int(0))),
            ],
        )];
        assert_eq!(codes(items), Vec::<&str>::new());
    }

    #[test]
    fn string_slice_expr_is_slice_of_u8() {
        // fn main() void { var s = "hello"; var t: []u8 = s[1..3]; }
        let items = vec![func(
            "main",
            vec![],
            "void",
            vec![
                let_var_infer("s", str_lit("hello")),
                let_var_slice("t", "u8", slice_expr(ident("s"), int(1), int(3))),
            ],
        )];
        assert_eq!(codes(items), Vec::<&str>::new());
    }

    #[test]
    fn print_of_bool_still_errors() {
        // fn main() void { print(true); }
        let items = vec![func(
            "main",
            vec![],
            "void",
            vec![Stmt::Expr(call("print", vec![boolean(true)]))],
        )];
        assert!(codes(items).contains(&"E0110"));
    }

    #[test]
    fn print_of_struct_still_errors() {
        // const P = struct { x: i32 }; fn main() void { print(P{ .x = 1 }); }
        let items = vec![
            struct_item("P", vec![("x", "i32")]),
            func(
                "main",
                vec![],
                "void",
                vec![Stmt::Expr(call(
                    "print",
                    vec![struct_lit("P", vec![("x", int(1))])],
                ))],
            ),
        ];
        assert!(codes(items).contains(&"E0110"));
    }

    #[test]
    fn print_of_non_u8_slice_errors() {
        // fn f(s: []i32) void { print(s); }  // []i32 is not a valid print arg
        let items = vec![func(
            "f",
            vec![param_slice("s", "i32")],
            "void",
            vec![Stmt::Expr(call("print", vec![ident("s")]))],
        )];
        assert!(codes(items).contains(&"E0110"));
    }

    // ---- generic structs / type-constructors (v0.129, SPEC §25) -----------

    #[test]
    fn generic_struct_alias_resolves_and_field_typechecks() {
        // fn Box(comptime T: type) type { return struct { v: T }; }
        // const IB = Box(i32);
        // fn main() void { var b: IB = IB{ .v = 5 }; print(b.v); }
        let items = vec![
            type_ctor("Box", "T", vec![("v", "T")]),
            const_item_infer("IB", call("Box", vec![ident("i32")])),
            func(
                "main",
                vec![],
                "void",
                vec![
                    let_var("b", "IB", struct_lit("IB", vec![("v", int(5))])),
                    Stmt::Expr(call("print", vec![field(ident("b"), "v")])),
                ],
            ),
        ];
        let table = check_ok(items);
        // The alias interned a monomorphised struct `Box__int32_t` with `v: i32`.
        let id = table
            .id_of("Box__int32_t")
            .expect("monomorphised struct interned");
        assert_eq!(table.get(id).fields, vec![("v".to_string(), Type::I32)]);
    }

    #[test]
    fn two_aliases_of_same_instantiation_share_struct_id() {
        // const A = Box(i32); const B = Box(i32);  → A and B are the *same* struct,
        // so `var b: B = A{ .v = 1 };` type-checks (struct equality is by id).
        let items = vec![
            type_ctor("Box", "T", vec![("v", "T")]),
            const_item_infer("A", call("Box", vec![ident("i32")])),
            const_item_infer("B", call("Box", vec![ident("i32")])),
            func(
                "main",
                vec![],
                "void",
                vec![
                    let_var("b", "B", struct_lit("A", vec![("v", int(1))])),
                    Stmt::Expr(call("print", vec![field(ident("b"), "v")])),
                ],
            ),
        ];
        let table = check_ok(items);
        assert!(table.id_of("Box__int32_t").is_some());
    }

    #[test]
    fn distinct_concrete_types_make_distinct_structs() {
        // const BI32 = Box(i32); const BI64 = Box(i64);  → two different structs.
        let items = vec![
            type_ctor("Box", "T", vec![("v", "T")]),
            const_item_infer("BI32", call("Box", vec![ident("i32")])),
            const_item_infer("BI64", call("Box", vec![ident("i64")])),
        ];
        let table = check_ok(items);
        let a = table.id_of("Box__int32_t").unwrap();
        let b = table.id_of("Box__int64_t").unwrap();
        assert_ne!(a, b);
        assert_eq!(table.get(a).fields, vec![("v".to_string(), Type::I32)]);
        assert_eq!(table.get(b).fields, vec![("v".to_string(), Type::I64)]);
    }

    #[test]
    fn assigning_across_distinct_instantiations_is_e0110() {
        // var x: Box(i32) = Box(i64){ .v = 1 };  → a struct-id mismatch.
        let items = vec![
            type_ctor("Box", "T", vec![("v", "T")]),
            const_item_infer("BI32", call("Box", vec![ident("i32")])),
            const_item_infer("BI64", call("Box", vec![ident("i64")])),
            func(
                "main",
                vec![],
                "void",
                vec![let_var(
                    "x",
                    "BI32",
                    struct_lit("BI64", vec![("v", int(1))]),
                )],
            ),
        ];
        assert!(codes(items).contains(&"E0110"));
    }

    #[test]
    fn instantiating_non_type_constructor_is_e0311() {
        // fn id(x: i32) i32 { return x; }   const Bad = id(i32);
        let items = vec![
            func(
                "id",
                vec![param("x", "i32")],
                "i32",
                vec![ret(Some(ident("x")))],
            ),
            const_item_infer("Bad", call("id", vec![ident("i32")])),
        ];
        assert!(codes(items).contains(&"E0311"));
    }

    #[test]
    fn type_constructor_arg_not_a_type_is_e0311() {
        // const Bad = Box(5);  → the argument is not a type.
        let items = vec![
            type_ctor("Box", "T", vec![("v", "T")]),
            const_item_infer("Bad", call("Box", vec![int(5)])),
        ];
        assert!(codes(items).contains(&"E0311"));
    }

    #[test]
    fn type_constructor_arg_unknown_name_is_e0311() {
        // const Bad = Box(Nope);  → `Nope` names no type.
        let items = vec![
            type_ctor("Box", "T", vec![("v", "T")]),
            const_item_infer("Bad", call("Box", vec![ident("Nope")])),
        ];
        assert!(codes(items).contains(&"E0311"));
    }

    #[test]
    fn type_ctor_body_not_return_struct_is_e0310() {
        // fn Box(comptime T: type) type { return 5; }  → body is not a struct type.
        let items = vec![Item::Func(raw_func(
            "Box",
            vec![param_comptime("T")],
            "type",
            vec![ret(Some(int(5)))],
        ))];
        assert!(codes(items).contains(&"E0310"));
    }

    #[test]
    fn type_ctor_with_non_comptime_param_is_e0310() {
        // fn Box(x: i32) type { return struct { v: i32 }; }  → not a type parameter.
        let items = vec![Item::Func(raw_func(
            "Box",
            vec![param("x", "i32")],
            "type",
            vec![ret(Some(struct_type_expr(vec![("v", "i32")])))],
        ))];
        assert!(codes(items).contains(&"E0310"));
    }

    #[test]
    fn struct_type_value_in_ordinary_position_is_e0310() {
        // fn main() void { var x = struct { a: i32 }; }  → a struct type is not a value.
        let items = vec![func(
            "main",
            vec![],
            "void",
            vec![let_var_infer("x", struct_type_expr(vec![("a", "i32")]))],
        )];
        assert!(codes(items).contains(&"E0310"));
    }

    #[test]
    fn field_access_through_alias_unknown_field_is_e0166() {
        // const IB = Box(i32); ... print(b.nope);  → no such field.
        let items = vec![
            type_ctor("Box", "T", vec![("v", "T")]),
            const_item_infer("IB", call("Box", vec![ident("i32")])),
            func(
                "main",
                vec![],
                "void",
                vec![
                    let_var("b", "IB", struct_lit("IB", vec![("v", int(1))])),
                    Stmt::Expr(call("print", vec![field(ident("b"), "nope")])),
                ],
            ),
        ];
        assert!(codes(items).contains(&"E0166"));
    }

    #[test]
    fn alias_struct_lit_field_type_mismatch_is_e0110() {
        // var b: IB = IB{ .v = true };  → `v` is `i32`, not `bool`.
        let items = vec![
            type_ctor("Box", "T", vec![("v", "T")]),
            const_item_infer("IB", call("Box", vec![ident("i32")])),
            func(
                "main",
                vec![],
                "void",
                vec![let_var(
                    "b",
                    "IB",
                    struct_lit("IB", vec![("v", boolean(true))]),
                )],
            ),
        ];
        assert!(codes(items).contains(&"E0110"));
    }

    #[test]
    fn using_type_alias_as_a_value_is_e0100() {
        // const IB = Box(i32); fn main() void { print(IB); }  → IB is a type, not a value.
        let items = vec![
            type_ctor("Box", "T", vec![("v", "T")]),
            const_item_infer("IB", call("Box", vec![ident("i32")])),
            func(
                "main",
                vec![],
                "void",
                vec![Stmt::Expr(call("print", vec![ident("IB")]))],
            ),
        ];
        assert!(codes(items).contains(&"E0100"));
    }

    #[test]
    fn generic_struct_with_optional_field_resolves() {
        // fn Maybe(comptime T: type) type { return struct { val: ?T, present: bool }; }
        // const MI = Maybe(i32);
        let items = vec![
            Item::Func(raw_func(
                "Maybe",
                vec![param_comptime("T")],
                "type",
                vec![ret(Some(struct_type_expr_te(vec![
                    ("val", te_opt("T")),
                    ("present", te("bool")),
                ])))],
            )),
            const_item_infer("MI", call("Maybe", vec![ident("i32")])),
        ];
        let table = check_ok(items);
        let id = table.id_of("Maybe__int32_t").unwrap();
        let fields = &table.get(id).fields;
        assert_eq!(fields.len(), 2);
        assert_eq!(fields[0].0, "val");
        match fields[0].1 {
            Type::Optional(oid) => assert_eq!(table.optional_inner(oid), Type::I32),
            other => panic!("expected `?i32`, got {:?}", other),
        }
        assert_eq!(fields[1], ("present".to_string(), Type::Bool));
    }

    #[test]
    fn alias_of_alias_as_type_argument_resolves() {
        // const IB = Box(i32);   const BB = Box(IB);  → nests a prior alias.
        let items = vec![
            type_ctor("Box", "T", vec![("v", "T")]),
            const_item_infer("IB", call("Box", vec![ident("i32")])),
            const_item_infer("BB", call("Box", vec![ident("IB")])),
        ];
        let table = check_ok(items);
        let inner = table.id_of("Box__int32_t").unwrap();
        let outer = table.id_of("Box__struct_Box__int32_t").unwrap();
        assert_eq!(
            table.get(outer).fields,
            vec![("v".to_string(), Type::Struct(inner))]
        );
    }

    #[test]
    fn non_type_constructor_const_with_unknown_callee_stays_e0130() {
        // const X: i32 = bar();  (bar undefined)  → still the non-constant path.
        let items = vec![const_item("X", "i32", call("bar", vec![]))];
        let cs = codes(items);
        assert!(cs.contains(&"E0130"));
        assert!(!cs.contains(&"E0311"));
    }

    // ---- generic-struct methods + `ArrayList(T)` (v0.130, SPEC §26) -------

    #[test]
    fn generic_struct_method_call_typechecks_and_records_instance() {
        // fn List(comptime T: type) type {
        //   return struct {
        //     items: []T,
        //     fn get(self: Self, i: usize) T { return self.items[i]; }
        //   };
        // }
        // const IL = List(i32);
        // fn first(l: IL) i32 { return l.get(0); }
        let get = raw_func(
            "get",
            vec![param("self", "Self"), param("i", "usize")],
            "T",
            vec![ret(Some(index(field(ident("self"), "items"), ident("i"))))],
        );
        let items = vec![
            type_ctor_m("List", "T", vec![("items", te_slice("T"))], vec![get]),
            const_item_infer("IL", call("List", vec![ident("i32")])),
            func(
                "first",
                vec![param("l", "IL")],
                "i32",
                vec![ret(Some(method_call(ident("l"), "get", vec![int(0)])))],
            ),
        ];
        let table = check_ok(items);
        // The monomorphised struct exists and its instance was recorded so the
        // backend emits the methods.
        let id = table.id_of("List__int32_t").expect("instance struct interned");
        assert!(table.struct_instances().iter().any(
            |i| i.struct_id == id && i.ctor == "List" && i.args == vec![Type::I32]
        ));
    }

    #[test]
    fn generic_struct_method_returning_self_typechecks() {
        // fn List(comptime T: type) type {
        //   return struct {
        //     items: []T, len: usize,
        //     fn with(self: Self, x: T) Self {
        //       return Self{ .items = self.items, .len = self.len };
        //     }
        //   };
        // }
        // const IL = List(i32);
        let with = raw_func(
            "with",
            vec![param("self", "Self"), param("x", "T")],
            "Self",
            vec![ret(Some(struct_lit(
                "Self",
                vec![
                    ("items", field(ident("self"), "items")),
                    ("len", field(ident("self"), "len")),
                ],
            )))],
        );
        let items = vec![
            type_ctor_m(
                "List",
                "T",
                vec![("items", te_slice("T")), ("len", te("usize"))],
                vec![with],
            ),
            const_item_infer("IL", call("List", vec![ident("i32")])),
        ];
        let table = check_ok(items);
        let id = table.id_of("List__int32_t").expect("instance struct interned");
        assert!(table.struct_instances().iter().any(|i| i.struct_id == id));
    }

    #[test]
    fn fields_only_generic_struct_records_no_instance() {
        // v0.129 preserved: a generic struct with NO methods records no instance.
        let items = vec![
            type_ctor("Box", "T", vec![("v", "T")]),
            const_item_infer("IB", call("Box", vec![ident("i32")])),
        ];
        let table = check_ok(items);
        let id = table.id_of("Box__int32_t").unwrap();
        assert!(table.struct_instances().iter().all(|i| i.struct_id != id));
        assert!(table.struct_instances().is_empty());
    }

    #[test]
    fn generic_struct_method_body_type_error_is_caught() {
        // fn get(self: Self) T returns a bool from a `T`(==i32) body → E0110.
        let get = raw_func(
            "get",
            vec![param("self", "Self")],
            "T",
            vec![ret(Some(boolean(true)))],
        );
        let items = vec![
            type_ctor_m("List", "T", vec![("items", te_slice("T"))], vec![get]),
            const_item_infer("IL", call("List", vec![ident("i32")])),
        ];
        assert!(codes(items).contains(&"E0110"));
    }

    #[test]
    fn generic_struct_method_arity_mismatch_is_e0171() {
        // `get` takes one extra arg; calling `l.get()` with none → E0171.
        let get = raw_func(
            "get",
            vec![param("self", "Self"), param("i", "usize")],
            "T",
            vec![ret(Some(index(field(ident("self"), "items"), ident("i"))))],
        );
        let items = vec![
            type_ctor_m("List", "T", vec![("items", te_slice("T"))], vec![get]),
            const_item_infer("IL", call("List", vec![ident("i32")])),
            func(
                "first",
                vec![param("l", "IL")],
                "i32",
                vec![ret(Some(method_call(ident("l"), "get", vec![])))],
            ),
        ];
        assert!(codes(items).contains(&"E0171"));
    }

    #[test]
    fn generic_struct_associated_fn_via_alias_typechecks() {
        // fn List(comptime T: type) type {
        //   return struct { len: usize, fn empty() usize { return 0; } };
        // }
        // const IL = List(i32);
        // fn n() usize { return IL.empty(); }   ← static call through the alias.
        let empty = raw_func("empty", vec![], "usize", vec![ret(Some(int(0)))]);
        let items = vec![
            type_ctor_m("List", "T", vec![("len", te("usize"))], vec![empty]),
            const_item_infer("IL", call("List", vec![ident("i32")])),
            func(
                "n",
                vec![],
                "usize",
                vec![ret(Some(method_call(ident("IL"), "empty", vec![])))],
            ),
        ];
        assert!(codes(items).is_empty());
    }

    #[test]
    fn generic_struct_method_at_two_types_distinct_returns() {
        // The same constructor instantiated at i32 and i64 yields two structs,
        // each with its own `get` returning the respective concrete type.
        let mk_get = || {
            raw_func(
                "get",
                vec![param("self", "Self"), param("i", "usize")],
                "T",
                vec![ret(Some(index(field(ident("self"), "items"), ident("i"))))],
            )
        };
        let items = vec![
            type_ctor_m("List", "T", vec![("items", te_slice("T"))], vec![mk_get()]),
            const_item_infer("LI", call("List", vec![ident("i32")])),
            const_item_infer("LL", call("List", vec![ident("i64")])),
            // `l.get(0)` must be `i32` here (assigning to an `i32`).
            func(
                "fi",
                vec![param("l", "LI")],
                "void",
                vec![let_var("x", "i32", method_call(ident("l"), "get", vec![int(0)]))],
            ),
            // `l.get(0)` must be `i64` here.
            func(
                "fl",
                vec![param("l", "LL")],
                "void",
                vec![let_var("x", "i64", method_call(ident("l"), "get", vec![int(0)]))],
            ),
        ];
        let table = check_ok(items);
        assert!(table.id_of("List__int32_t").is_some());
        assert!(table.id_of("List__int64_t").is_some());
        // Two distinct instances recorded.
        assert_eq!(table.struct_instances().len(), 2);
    }

    // ---- multiple type parameters for type-constructors (v0.135, §31) -----

    /// A type-constructor with two `comptime` type parameters
    /// `fn Name(comptime A: type, comptime B: type) type { return struct { … }; }`
    /// whose struct fields/methods are given as explicit [`TypeExpr`]s.
    fn type_ctor2(
        name: &str,
        a: &str,
        b: &str,
        fields: Vec<(&str, TypeExpr)>,
        methods: Vec<Func>,
    ) -> Item {
        Item::Func(raw_func(
            name,
            vec![param_comptime(a), param_comptime(b)],
            "type",
            vec![ret(Some(struct_type_expr_m(fields, methods)))],
        ))
    }

    #[test]
    fn type_ctor_two_type_params_fields_and_method_typecheck() {
        // fn Pair(comptime A: type, comptime B: type) type {
        //   return struct {
        //     a: A, b: B,
        //     fn make(self: Self, a: A, b: B) Self { return Self{ .a=a, .b=b }; }
        //   };
        // }
        // const IB = Pair(i32, i64);
        // fn use_pair(p: IB) void { var pa: i32 = p.a; var pb: i64 = p.b; }
        let make = raw_func(
            "make",
            vec![param("self", "Self"), param("a", "A"), param("b", "B")],
            "Self",
            vec![ret(Some(struct_lit(
                "Self",
                vec![("a", ident("a")), ("b", ident("b"))],
            )))],
        );
        let items = vec![
            type_ctor2(
                "Pair",
                "A",
                "B",
                vec![("a", te("A")), ("b", te("B"))],
                vec![make],
            ),
            const_item_infer("IB", call("Pair", vec![ident("i32"), ident("i64")])),
            func(
                "use_pair",
                vec![param("p", "IB")],
                "void",
                vec![
                    // p.a is `i32`, p.b is `i64` — the two type parameters resolve
                    // independently through the substitution.
                    let_var("pa", "i32", field(ident("p"), "a")),
                    let_var("pb", "i64", field(ident("p"), "b")),
                ],
            ),
        ];
        let table = check_ok(items);
        let id = table
            .id_of("Pair__int32_t_int64_t")
            .expect("monomorphised struct interned");
        assert_eq!(
            table.get(id).fields,
            vec![("a".to_string(), Type::I32), ("b".to_string(), Type::I64)]
        );
        // The instance is recorded with both concrete args, in parameter order.
        assert!(table.struct_instances().iter().any(
            |i| i.struct_id == id && i.ctor == "Pair" && i.args == vec![Type::I32, Type::I64]
        ));
    }

    #[test]
    fn type_ctor_field_type_mismatch_under_two_params_is_e0110() {
        // const IB = Pair(i32, i64); var p: IB = IB{ .a = true, .b = 9 };
        //   → field `a` is `i32`, not `bool`.
        let items = vec![
            type_ctor2(
                "Pair",
                "A",
                "B",
                vec![("a", te("A")), ("b", te("B"))],
                vec![],
            ),
            const_item_infer("IB", call("Pair", vec![ident("i32"), ident("i64")])),
            func(
                "main",
                vec![],
                "void",
                vec![let_var(
                    "p",
                    "IB",
                    struct_lit("IB", vec![("a", boolean(true)), ("b", int(9))]),
                )],
            ),
        ];
        assert!(codes(items).contains(&"E0110"));
    }

    #[test]
    fn type_ctor_wrong_arg_count_is_e0311() {
        // const Bad = Pair(i32);  → Pair takes two type arguments.
        let items = vec![
            type_ctor2(
                "Pair",
                "A",
                "B",
                vec![("a", te("A")), ("b", te("B"))],
                vec![],
            ),
            const_item_infer("Bad", call("Pair", vec![ident("i32")])),
        ];
        assert!(codes(items).contains(&"E0311"));
    }

    #[test]
    fn type_ctor_too_many_args_is_e0311() {
        // const Bad = Pair(i32, i64, bool);  → too many type arguments.
        let items = vec![
            type_ctor2(
                "Pair",
                "A",
                "B",
                vec![("a", te("A")), ("b", te("B"))],
                vec![],
            ),
            const_item_infer(
                "Bad",
                call("Pair", vec![ident("i32"), ident("i64"), ident("bool")]),
            ),
        ];
        assert!(codes(items).contains(&"E0311"));
    }

    #[test]
    fn type_ctor_arg_order_makes_distinct_structs() {
        // Pair(i32, i64) and Pair(i64, i32) are distinct structs (order matters in
        // both the mangled name and the field types).
        let pair = type_ctor2(
            "Pair",
            "A",
            "B",
            vec![("a", te("A")), ("b", te("B"))],
            vec![],
        );
        let items = vec![
            pair,
            const_item_infer("AB", call("Pair", vec![ident("i32"), ident("i64")])),
            const_item_infer("BA", call("Pair", vec![ident("i64"), ident("i32")])),
        ];
        let table = check_ok(items);
        let ab = table.id_of("Pair__int32_t_int64_t").unwrap();
        let ba = table.id_of("Pair__int64_t_int32_t").unwrap();
        assert_ne!(ab, ba);
        assert_eq!(
            table.get(ab).fields,
            vec![("a".to_string(), Type::I32), ("b".to_string(), Type::I64)]
        );
        assert_eq!(
            table.get(ba).fields,
            vec![("a".to_string(), Type::I64), ("b".to_string(), Type::I32)]
        );
    }

    #[test]
    fn single_type_param_ctor_name_preserved() {
        // The multi-param refactor must keep a single-parameter `Box(i32)`
        // interning *exactly* `Box__int32_t` (the v0.129/§25 name) and recording
        // a one-element `args` vector.
        let get = raw_func(
            "get",
            vec![param("self", "Self")],
            "T",
            vec![ret(Some(field(ident("self"), "v")))],
        );
        let items = vec![
            type_ctor_m("Box", "T", vec![("v", te("T"))], vec![get]),
            const_item_infer("IB", call("Box", vec![ident("i32")])),
        ];
        let table = check_ok(items);
        let id = table
            .id_of("Box__int32_t")
            .expect("single-param name preserved");
        assert_eq!(table.get(id).fields, vec![("v".to_string(), Type::I32)]);
        assert!(table
            .struct_instances()
            .iter()
            .any(|i| i.struct_id == id && i.ctor == "Box" && i.args == vec![Type::I32]));
    }

    #[test]
    fn type_ctor_with_value_comptime_param_is_e0310() {
        // fn Bad(comptime n: usize) type { return struct { v: i32 }; }
        //   → a comptime *value* parameter is not allowed in a type-constructor.
        let items = vec![Item::Func(raw_func(
            "Bad",
            vec![param_comptime_val("n", "usize")],
            "type",
            vec![ret(Some(struct_type_expr(vec![("v", "i32")])))],
        ))];
        assert!(codes(items).contains(&"E0310"));
    }

    #[test]
    fn type_ctor_mixed_type_and_value_params_is_e0310() {
        // fn Bad(comptime T: type, comptime n: usize) type { return struct { v: T }; }
        //   → every type-constructor parameter must be `comptime _: type`.
        let items = vec![Item::Func(raw_func(
            "Bad",
            vec![param_comptime("T"), param_comptime_val("n", "usize")],
            "type",
            vec![ret(Some(struct_type_expr(vec![("v", "T")])))],
        ))];
        assert!(codes(items).contains(&"E0310"));
    }

    // ---- pointer-receiver methods (true mutation) (v0.134, SPEC §30) ------

    /// A `Point` struct mixing pointer-receiver methods, a value-receiver method,
    /// and an associated function:
    /// ```text
    /// const Point = struct {
    ///     x: i32,
    ///     fn inc(self: *Point) void { self.x += 1; }              // compound, through *self
    ///     fn add(self: *Point, by: i32) void { self.x = self.x + by; } // read+write through *self
    ///     fn get(self: Point) i32 { return self.x; }              // value receiver (unchanged)
    ///     fn make() Point { return Point{ .x = 0 }; }             // associated (no self)
    /// };
    /// ```
    fn point_ptr_struct() -> Item {
        let inc = raw_func(
            "inc",
            vec![param_ptr("self", "Point")],
            "void",
            vec![field_assign_op(field(ident("self"), "x"), BinOp::Add, int(1))],
        );
        let add = raw_func(
            "add",
            vec![param_ptr("self", "Point"), param("by", "i32")],
            "void",
            vec![field_assign(
                field(ident("self"), "x"),
                bin(BinOp::Add, field(ident("self"), "x"), ident("by")),
            )],
        );
        let get = raw_func(
            "get",
            vec![param("self", "Point")],
            "i32",
            vec![ret(Some(field(ident("self"), "x")))],
        );
        let make = raw_func(
            "make",
            vec![],
            "Point",
            vec![ret(Some(struct_lit("Point", vec![("x", int(0))])))],
        );
        struct_item_m("Point", vec![("x", "i32")], vec![inc, add, get, make])
    }

    #[test]
    fn pointer_receiver_method_bodies_typecheck() {
        // The pointer-receiver bodies (`self.x += 1`, `self.x = self.x + by`)
        // read and write the field through `self: *Point` — the struct on its
        // own must type-check.
        assert_eq!(codes(vec![point_ptr_struct()]), Vec::<&str>::new());
    }

    #[test]
    fn pointer_receiver_call_on_var_and_assoc_call_typecheck() {
        // fn main() void {
        //     var p: Point = Point.make();   // associated call — unchanged
        //     p.inc();                       // pointer-receiver call on a var (auto-ref &p)
        //     p.add(5);                       // pointer-receiver call with an arg
        // }
        let items = vec![
            point_ptr_struct(),
            func(
                "main",
                vec![],
                "void",
                vec![
                    let_var("p", "Point", method_call(ident("Point"), "make", vec![])),
                    Stmt::Expr(method_call(ident("p"), "inc", vec![])),
                    Stmt::Expr(method_call(ident("p"), "add", vec![int(5)])),
                ],
            ),
        ];
        assert_eq!(codes(items), Vec::<&str>::new());
    }

    #[test]
    fn pointer_receiver_call_on_field_lvalue_ok() {
        // A field is an addressable lvalue, so a pointer-receiver call on it is
        // allowed (auto-ref `&w.p`).
        // const Wrap = struct { p: Point };
        // fn f(w: Wrap) void { w.p.inc(); }
        let items = vec![
            point_ptr_struct(),
            struct_item("Wrap", vec![("p", "Point")]),
            func(
                "f",
                vec![param("w", "Wrap")],
                "void",
                vec![Stmt::Expr(method_call(field(ident("w"), "p"), "inc", vec![]))],
            ),
        ];
        assert_eq!(codes(items), Vec::<&str>::new());
    }

    #[test]
    fn pointer_receiver_call_on_temporary_is_e0231() {
        // fn main() void { Point.make().inc(); }   — receiver is a temporary, so
        // the auto-ref `&<temp>` is rejected exactly like `&<temp>` (E0231).
        let items = vec![
            point_ptr_struct(),
            func(
                "main",
                vec![],
                "void",
                vec![Stmt::Expr(method_call(
                    method_call(ident("Point"), "make", vec![]),
                    "inc",
                    vec![],
                ))],
            ),
        ];
        assert!(codes(items).contains(&"E0231"));
    }

    #[test]
    fn value_receiver_method_on_temporary_still_ok() {
        // A value-receiver call on a temporary is unchanged (no auto-ref): it
        // passes the temporary by value.
        // fn main() void { var r: i32 = Point.make().get(); print(r); }
        let items = vec![
            point_ptr_struct(),
            func(
                "main",
                vec![],
                "void",
                vec![
                    let_var(
                        "r",
                        "i32",
                        method_call(method_call(ident("Point"), "make", vec![]), "get", vec![]),
                    ),
                    Stmt::Expr(call("print", vec![ident("r")])),
                ],
            ),
        ];
        assert_eq!(codes(items), Vec::<&str>::new());
    }

    #[test]
    fn pointer_param_field_read_and_write_typecheck() {
        // A `*Struct` param (not `self`) auto-derefs for field access too
        // (general, SPEC §30.1): read + write through the pointer.
        // const Point = struct { x: i32 };
        // fn set(q: *Point, v: i32) i32 { q.x = v; return q.x; }
        let items = vec![
            struct_item("Point", vec![("x", "i32")]),
            Item::Func(raw_func(
                "set",
                vec![param_ptr("q", "Point"), param("v", "i32")],
                "i32",
                vec![
                    field_assign(field(ident("q"), "x"), ident("v")),
                    ret(Some(field(ident("q"), "x"))),
                ],
            )),
        ];
        assert_eq!(codes(items), Vec::<&str>::new());
    }

    #[test]
    fn pointer_param_compound_field_assign_typechecks() {
        // `q.x += 1` through a `*Point` parameter (write through the pointer).
        let items = vec![
            struct_item("Point", vec![("x", "i32")]),
            Item::Func(raw_func(
                "bump",
                vec![param_ptr("q", "Point")],
                "void",
                vec![field_assign_op(field(ident("q"), "x"), BinOp::Add, int(1))],
            )),
        ];
        assert_eq!(codes(items), Vec::<&str>::new());
    }

    #[test]
    fn pointer_field_assign_does_not_require_mutable_binding() {
        // Even a `const`/immutable `*Point` binding may write through the pointer:
        // the *pointer* binding's mutability is irrelevant (mirrors `p.* = e`).
        // fn f(q: *Point) void { q.x = 9; }  — q is an (immutable) parameter.
        let items = vec![
            struct_item("Point", vec![("x", "i32")]),
            Item::Func(raw_func(
                "f",
                vec![param_ptr("q", "Point")],
                "void",
                vec![field_assign(field(ident("q"), "x"), int(9))],
            )),
        ];
        // No E0167 (immutable-binding) — writing THROUGH the pointer is allowed.
        assert_eq!(codes(items), Vec::<&str>::new());
    }

    #[test]
    fn value_struct_field_assign_still_requires_mutable_var() {
        // Regression guard: a *value* struct parameter is still immutable, so
        // `p.x = 5` on a by-value `Point` param stays `E0167` (unchanged).
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
    fn pointer_param_indexed_field_assign_through_pointer_ok() {
        // `self.arr[i] = e` through a `*Holder` writes through the pointer, so the
        // array element is mutable even though the pointer binding is immutable.
        // const Holder = struct { arr: [3]i32 };
        // fn put(h: *Holder, i: usize, v: i32) void { h.arr[i] = v; }
        let items = vec![
            Item::Struct(StructDecl {
                is_pub: false,
                name: "Holder".into(),
                fields: vec![FieldDecl {
                    name: "arr".into(),
                    ty: te_arr("i32", 3),
                    span: sp(),
                }],
                methods: Vec::new(),
                span: sp(),
            }),
            Item::Func(raw_func(
                "put",
                vec![param_ptr("h", "Holder"), param("i", "usize"), param("v", "i32")],
                "void",
                vec![field_assign(
                    index(field(ident("h"), "arr"), ident("i")),
                    ident("v"),
                )],
            )),
        ];
        assert_eq!(codes(items), Vec::<&str>::new());
    }

    #[test]
    fn pointer_receiver_call_through_pointer_value_ok() {
        // A receiver that is already a `*Point` is passed straight through (no
        // addressability requirement); the call still resolves.
        // fn run(q: *Point) void { q.inc(); }
        let items = vec![
            point_ptr_struct(),
            Item::Func(raw_func(
                "run",
                vec![param_ptr("q", "Point")],
                "void",
                vec![Stmt::Expr(method_call(ident("q"), "inc", vec![]))],
            )),
        ];
        assert_eq!(codes(items), Vec::<&str>::new());
    }

    #[test]
    fn value_receiver_method_on_pointer_receiver_auto_derefs() {
        // A `*Point` receiver calling a *value*-receiver method auto-derefs
        // (passes `*q` by value), SPEC §30.1.
        // fn val(q: *Point) i32 { return q.get(); }
        let items = vec![
            point_ptr_struct(),
            Item::Func(raw_func(
                "val",
                vec![param_ptr("q", "Point")],
                "i32",
                vec![ret(Some(method_call(ident("q"), "get", vec![])))],
            )),
        ];
        assert_eq!(codes(items), Vec::<&str>::new());
    }

    #[test]
    fn explicit_pointer_self_static_call_requires_pointer_arg() {
        // `Point.inc(p)` (static form) binds the explicit `self: *Point`, so the
        // argument must be a `*Point`; passing a value `Point` is `E0110`,
        // passing `&p` type-checks.
        // fn ok(p: *Point) void { Point.inc(p); }
        // fn bad(p: Point) void { Point.inc(p); }   -> E0110
        let ok_items = vec![
            point_ptr_struct(),
            Item::Func(raw_func(
                "ok",
                vec![param_ptr("p", "Point")],
                "void",
                vec![Stmt::Expr(method_call(ident("Point"), "inc", vec![ident("p")]))],
            )),
        ];
        assert_eq!(codes(ok_items), Vec::<&str>::new());

        let bad_items = vec![
            point_ptr_struct(),
            func(
                "bad",
                vec![param("p", "Point")],
                "void",
                vec![Stmt::Expr(method_call(ident("Point"), "inc", vec![ident("p")]))],
            ),
        ];
        assert!(codes(bad_items).contains(&"E0110"));
    }

    #[test]
    fn generic_struct_pointer_receiver_method_typechecks() {
        // fn Counter(comptime T: type) type {
        //   return struct {
        //     n: T,
        //     fn set(self: *Self, v: T) void { self.n = v; }   // write through *Self
        //   };
        // }
        // const IC = Counter(i32);
        // fn run() void { var c: IC = IC{ .n = 0 }; c.set(5); }
        let set = raw_func(
            "set",
            vec![param_ptr("self", "Self"), param("v", "T")],
            "void",
            vec![field_assign(field(ident("self"), "n"), ident("v"))],
        );
        let items = vec![
            type_ctor_m("Counter", "T", vec![("n", te("T"))], vec![set]),
            const_item_infer("IC", call("Counter", vec![ident("i32")])),
            func(
                "run",
                vec![],
                "void",
                vec![
                    let_var("c", "IC", struct_lit("IC", vec![("n", int(0))])),
                    Stmt::Expr(method_call(ident("c"), "set", vec![int(5)])),
                ],
            ),
        ];
        let table = check_ok(items);
        // The instance was recorded so the backend emits the pointer-receiver
        // method.
        let id = table.id_of("Counter__int32_t").expect("instance struct interned");
        assert!(table
            .struct_instances()
            .iter()
            .any(|i| i.struct_id == id && i.ctor == "Counter" && i.args == vec![Type::I32]));
    }

    #[test]
    fn generic_struct_pointer_receiver_call_on_temp_is_e0231() {
        // A pointer-receiver generic-struct method called on a temporary is
        // E0231 (the auto-ref `&<temp>` is rejected). Here the receiver is an
        // associated call's result.
        // fn Counter(comptime T: type) type {
        //   return struct {
        //     n: T,
        //     fn set(self: *Self, v: T) void { self.n = v; }
        //     fn make() Self { return Self{ .n = 0 }; }
        //   };
        // }
        // const IC = Counter(i32);
        // fn run() void { IC.make().set(5); }
        let set = raw_func(
            "set",
            vec![param_ptr("self", "Self"), param("v", "T")],
            "void",
            vec![field_assign(field(ident("self"), "n"), ident("v"))],
        );
        let make = raw_func(
            "make",
            vec![],
            "Self",
            vec![ret(Some(struct_lit("Self", vec![("n", int(0))])))],
        );
        let items = vec![
            type_ctor_m("Counter", "T", vec![("n", te("T"))], vec![set, make]),
            const_item_infer("IC", call("Counter", vec![ident("i32")])),
            func(
                "run",
                vec![],
                "void",
                vec![Stmt::Expr(method_call(
                    method_call(ident("IC"), "make", vec![]),
                    "set",
                    vec![int(5)],
                ))],
            ),
        ];
        assert!(codes(items).contains(&"E0231"));
    }

    // ---- comptime reflection builtins (v0.136, SPEC §32) ------------------

    /// A `@name(args)` comptime builtin call in expression position (v0.136).
    fn builtin(name: &str, args: Vec<Expr>) -> Expr {
        Expr::Builtin {
            name: name.into(),
            args,
            span: sp(),
        }
    }

    #[test]
    fn sizeof_builtin_is_usize() {
        // `@sizeOf(i32)` type-checks to `usize`.
        let mut cx = Checker::new();
        let t = cx.check_expr(&builtin("sizeOf", vec![ident("i32")]), None);
        assert_eq!(t, Some(Type::Usize));
    }

    #[test]
    fn typename_builtin_is_slice_of_u8() {
        // `@typeName(i32)` type-checks to the interned `[]u8` slice type.
        let mut cx = Checker::new();
        let t = cx.check_expr(&builtin("typeName", vec![ident("i32")]), None);
        let expected = Type::Slice(cx.structs.intern_slice(Type::U8));
        assert_eq!(t, Some(expected));
        assert_eq!(cx.type_name(expected), "[]u8");
    }

    #[test]
    fn inferred_sizeof_var_is_usize() {
        // fn main() void { var n = @sizeOf(i32); var m: usize = n; }
        // The inferred `n` must be `usize`, so assigning it to a `usize` binding
        // type-checks.
        let items = vec![func(
            "main",
            vec![],
            "void",
            vec![
                let_var_infer("n", builtin("sizeOf", vec![ident("i32")])),
                let_var("m", "usize", ident("n")),
            ],
        )];
        assert_eq!(codes(items), Vec::<&str>::new());
    }

    #[test]
    fn typename_assignable_to_slice_of_u8() {
        // fn main() void { var s: []u8 = @typeName(i32); }
        let items = vec![func(
            "main",
            vec![],
            "void",
            vec![let_var_slice("s", "u8", builtin("typeName", vec![ident("i32")]))],
        )];
        assert_eq!(codes(items), Vec::<&str>::new());
    }

    #[test]
    fn sizeof_of_struct_type_ok() {
        // const Point = struct { x: i32, y: i32 };
        // fn main() void { var n: usize = @sizeOf(Point); }
        let items = vec![
            struct_item("Point", vec![("x", "i32"), ("y", "i32")]),
            func(
                "main",
                vec![],
                "void",
                vec![let_var("n", "usize", builtin("sizeOf", vec![ident("Point")]))],
            ),
        ];
        assert_eq!(codes(items), Vec::<&str>::new());
    }

    #[test]
    fn sizeof_of_type_parameter_in_generic_body() {
        // fn sz(comptime T: type) usize { return @sizeOf(T); }
        // fn main() void { var n: usize = sz(i32); }
        // `@sizeOf(T)` resolves `T` through the active type substitution at the
        // instantiation, so the generic body type-checks.
        let sz = Item::Func(raw_func(
            "sz",
            vec![param_comptime("T")],
            "usize",
            vec![ret(Some(builtin("sizeOf", vec![ident("T")])))],
        ));
        let items = vec![
            sz,
            func(
                "main",
                vec![],
                "void",
                vec![let_var("n", "usize", call("sz", vec![ident("i32")]))],
            ),
        ];
        assert_eq!(codes(items), Vec::<&str>::new());
    }

    #[test]
    fn unknown_builtin_is_e0320() {
        // fn main() void { var n = @foo(i32); }
        let items = vec![func(
            "main",
            vec![],
            "void",
            vec![let_var_infer("n", builtin("foo", vec![ident("i32")]))],
        )];
        assert!(codes(items).contains(&"E0320"));
    }

    #[test]
    fn builtin_too_many_args_is_e0320() {
        // fn main() void { var n = @sizeOf(i32, i32); }
        let items = vec![func(
            "main",
            vec![],
            "void",
            vec![let_var_infer(
                "n",
                builtin("sizeOf", vec![ident("i32"), ident("i32")]),
            )],
        )];
        assert!(codes(items).contains(&"E0320"));
    }

    #[test]
    fn builtin_no_args_is_e0320() {
        // fn main() void { var n = @typeName(); }
        let items = vec![func(
            "main",
            vec![],
            "void",
            vec![let_var_infer("n", builtin("typeName", vec![]))],
        )];
        assert!(codes(items).contains(&"E0320"));
    }

    #[test]
    fn builtin_in_const_is_e0130() {
        // const C: usize = @sizeOf(i32);  // not a constant expression
        let items = vec![const_item("C", "usize", builtin("sizeOf", vec![ident("i32")]))];
        assert!(codes(items).contains(&"E0130"));
    }

    // ---- `@panic` and `unreachable` (v0.141, SPEC §35.1) -------------------

    /// `unreachable` (v0.141).
    fn unreachable_expr() -> Expr {
        Expr::Unreachable { span: sp() }
    }

    #[test]
    fn unreachable_adopts_expected_type() {
        // In a value position `unreachable` adopts the expected type; as a bare
        // statement (no expectation) it is `void`. It is never a type error.
        let mut cx = Checker::new();
        assert_eq!(
            cx.check_expr(&unreachable_expr(), Some(Type::I32)),
            Some(Type::I32)
        );
        assert_eq!(cx.check_expr(&unreachable_expr(), None), Some(Type::Void));
    }

    #[test]
    fn unreachable_annotated_var_ok() {
        // fn main() void { var x: i32 = unreachable; }
        // The diverging `unreachable` adopts the `i32` annotation, so the
        // initializer type-checks.
        let items = vec![func(
            "main",
            vec![],
            "void",
            vec![let_var("x", "i32", unreachable_expr())],
        )];
        assert_eq!(codes(items), Vec::<&str>::new());
    }

    #[test]
    fn unreachable_statement_ok() {
        // fn main() void { unreachable; }   // a bare statement is `void`
        let items = vec![func(
            "main",
            vec![],
            "void",
            vec![Stmt::Expr(unreachable_expr())],
        )];
        assert_eq!(codes(items), Vec::<&str>::new());
    }

    #[test]
    fn panic_adopts_expected_type() {
        // fn main() void { var s: i32 = @panic("x"); }
        // `@panic` diverges, so it adopts the `i32` annotation (works in any
        // value position).
        let items = vec![func(
            "main",
            vec![],
            "void",
            vec![let_var("s", "i32", builtin("panic", vec![str_lit("x")]))],
        )];
        assert_eq!(codes(items), Vec::<&str>::new());
    }

    #[test]
    fn panic_statement_ok() {
        // fn main() void { @panic("x"); }   // a bare statement is `void`
        let items = vec![func(
            "main",
            vec![],
            "void",
            vec![Stmt::Expr(builtin("panic", vec![str_lit("x")]))],
        )];
        assert_eq!(codes(items), Vec::<&str>::new());
    }

    #[test]
    fn panic_non_slice_arg_is_error() {
        // fn main() void { @panic(5); }   // the message must be a `[]u8`
        let items = vec![func(
            "main",
            vec![],
            "void",
            vec![Stmt::Expr(builtin("panic", vec![int(5)]))],
        )];
        assert!(codes(items).contains(&"E0110"));
    }

    #[test]
    fn panic_no_args_is_e0320() {
        // fn main() void { @panic(); }   // wrong arity (the `@`-builtin code)
        let items = vec![func(
            "main",
            vec![],
            "void",
            vec![Stmt::Expr(builtin("panic", vec![]))],
        )];
        assert!(codes(items).contains(&"E0320"));
    }

    #[test]
    fn panic_too_many_args_is_e0320() {
        // fn main() void { @panic("a", "b"); }   // wrong arity
        let items = vec![func(
            "main",
            vec![],
            "void",
            vec![Stmt::Expr(builtin("panic", vec![str_lit("a"), str_lit("b")]))],
        )];
        assert!(codes(items).contains(&"E0320"));
    }

    // ---- enum explicit values + conversions (v0.143, SPEC §37) ------------

    #[test]
    fn enum_explicit_values_recorded() {
        // const E = enum { A = 1, B, C = 10 };  => values [1, 2, 10]
        // (an explicit value sets the counter; the gap auto-increments).
        let table = check_ok(vec![enum_item_valued(
            "E",
            vec![("A", Some(1)), ("B", None), ("C", Some(10))],
        )]);
        let id = table.enum_id_of("E").expect("E should be registered");
        assert_eq!(table.enum_get(id).values, vec![1, 2, 10]);
        // The convenience accessor agrees.
        assert_eq!(table.enum_get(id).variant_value("C"), Some(10));
    }

    #[test]
    fn enum_no_explicit_values_auto_increment() {
        // const E = enum { A, B, C };  => values [0, 1, 2] (unchanged behaviour).
        let table = check_ok(vec![enum_item("E", vec!["A", "B", "C"])]);
        let id = table.enum_id_of("E").expect("E should be registered");
        assert_eq!(table.enum_get(id).values, vec![0, 1, 2]);
    }

    #[test]
    fn enum_explicit_value_can_lower_counter() {
        // const E = enum { A = 5, B, C = 0, D };  => values [5, 6, 0, 1].
        let table = check_ok(vec![enum_item_valued(
            "E",
            vec![("A", Some(5)), ("B", None), ("C", Some(0)), ("D", None)],
        )]);
        let id = table.enum_id_of("E").expect("E should be registered");
        assert_eq!(table.enum_get(id).values, vec![5, 6, 0, 1]);
    }

    #[test]
    fn int_from_enum_is_i64() {
        // const Color = enum { Red, Green, Blue };
        // fn main() void { var x: i64 = @intFromEnum(Color.Red); }
        let items = vec![
            enum_item("Color", vec!["Red", "Green", "Blue"]),
            func(
                "main",
                vec![],
                "void",
                vec![let_var(
                    "x",
                    "i64",
                    builtin("intFromEnum", vec![field(ident("Color"), "Red")]),
                )],
            ),
        ];
        assert_eq!(codes(items), Vec::<&str>::new());
    }

    #[test]
    fn enum_from_int_is_the_enum_type() {
        // const Color = enum { Red, Green, Blue };
        // fn main() void { var c: Color = @enumFromInt(Color, 2); }
        let items = vec![
            enum_item("Color", vec!["Red", "Green", "Blue"]),
            func(
                "main",
                vec![],
                "void",
                vec![let_var(
                    "c",
                    "Color",
                    builtin("enumFromInt", vec![ident("Color"), int(2)]),
                )],
            ),
        ];
        assert_eq!(codes(items), Vec::<&str>::new());
    }

    #[test]
    fn int_from_enum_of_non_enum_is_error() {
        // fn main() void { var x: i64 = @intFromEnum(5); }  — 5 is not an enum.
        let items = vec![func(
            "main",
            vec![],
            "void",
            vec![let_var("x", "i64", builtin("intFromEnum", vec![int(5)]))],
        )];
        assert!(codes(items).contains(&"E0321"));
    }

    #[test]
    fn enum_from_int_non_enum_type_first_arg_is_error() {
        // fn main() void { var c = @enumFromInt(i32, 2); }  — i32 is not an enum.
        let items = vec![func(
            "main",
            vec![],
            "void",
            vec![let_var_infer(
                "c",
                builtin("enumFromInt", vec![ident("i32"), int(2)]),
            )],
        )];
        assert!(codes(items).contains(&"E0321"));
    }

    #[test]
    fn int_from_enum_wrong_arity_is_e0320() {
        // fn main() void { var x: i64 = @intFromEnum(); }  — wrong arity.
        let items = vec![func(
            "main",
            vec![],
            "void",
            vec![let_var("x", "i64", builtin("intFromEnum", vec![]))],
        )];
        assert!(codes(items).contains(&"E0320"));
    }

    // ---- stdin / file I/O builtins (v0.148, SPEC §41) ---------------------

    #[test]
    fn read_file_is_slice_of_u8() {
        // fn f(a: Allocator) void { var s: []u8 = @readFile(a, "x.txt"); }
        // The result types as `[]u8`, so the slice binding type-checks cleanly.
        let items = vec![func(
            "f",
            vec![param("a", "Allocator")],
            "void",
            vec![let_var_slice(
                "s",
                "u8",
                builtin("readFile", vec![ident("a"), str_lit("x.txt")]),
            )],
        )];
        assert_eq!(codes(items), Vec::<&str>::new());
    }

    #[test]
    fn read_line_is_slice_of_u8() {
        // fn f(a: Allocator) void { var s: []u8 = @readLine(a); }
        let items = vec![func(
            "f",
            vec![param("a", "Allocator")],
            "void",
            vec![let_var_slice(
                "s",
                "u8",
                builtin("readLine", vec![ident("a")]),
            )],
        )];
        assert_eq!(codes(items), Vec::<&str>::new());
    }

    #[test]
    fn read_file_non_allocator_first_arg_is_error() {
        // fn main() void { var s: []u8 = @readFile(5, "x.txt"); }  — 5 is no Allocator.
        let items = vec![func(
            "main",
            vec![],
            "void",
            vec![let_var_slice(
                "s",
                "u8",
                builtin("readFile", vec![int(5), str_lit("x.txt")]),
            )],
        )];
        assert!(codes(items).contains(&"E0321"));
    }

    #[test]
    fn read_file_non_slice_path_is_error() {
        // fn f(a: Allocator) void { var s: []u8 = @readFile(a, 5); }  — 5 is no []u8.
        let items = vec![func(
            "f",
            vec![param("a", "Allocator")],
            "void",
            vec![let_var_slice(
                "s",
                "u8",
                builtin("readFile", vec![ident("a"), int(5)]),
            )],
        )];
        assert!(codes(items).contains(&"E0110"));
    }

    #[test]
    fn read_line_wrong_arity_is_e0320() {
        // fn main() void { var s = @readLine(); }  — wrong arity.
        let items = vec![func(
            "main",
            vec![],
            "void",
            vec![let_var_infer("s", builtin("readLine", vec![]))],
        )];
        assert!(codes(items).contains(&"E0320"));
    }

    #[test]
    fn read_file_wrong_arity_is_e0320() {
        // fn f(a: Allocator) void { var s = @readFile(a); }  — wrong arity.
        let items = vec![func(
            "f",
            vec![param("a", "Allocator")],
            "void",
            vec![let_var_infer("s", builtin("readFile", vec![ident("a")]))],
        )];
        assert!(codes(items).contains(&"E0320"));
    }

    #[test]
    fn unreachable_in_switch_else_arm_ok() {
        // const Color = enum { Red, Green, Blue };
        // fn classify(c: Color) void {
        //     switch (c) { .Red => { print(1); } else => { unreachable; } }
        // }
        // The `else` arm (a void block) accepts the diverging `unreachable`, and
        // its presence makes the switch exhaustive.
        let items = vec![
            color_enum(),
            func(
                "classify",
                vec![param("c", "Color")],
                "void",
                vec![switch_stmt(
                    ident("c"),
                    vec![switch_arm(
                        vec![enum_lit("Red")],
                        vec![Stmt::Expr(call("print", vec![int(1)]))],
                    )],
                    Some(vec![Stmt::Expr(unreachable_expr())]),
                )],
            ),
        ];
        assert_eq!(codes(items), Vec::<&str>::new());
    }

    #[test]
    fn panic_in_switch_else_arm_ok() {
        // const Color = enum { Red, Green, Blue };
        // fn classify(c: Color) void {
        //     switch (c) { .Red => { print(1); } else => { @panic("bad color"); } }
        // }
        let items = vec![
            color_enum(),
            func(
                "classify",
                vec![param("c", "Color")],
                "void",
                vec![switch_stmt(
                    ident("c"),
                    vec![switch_arm(
                        vec![enum_lit("Red")],
                        vec![Stmt::Expr(call("print", vec![int(1)]))],
                    )],
                    Some(vec![Stmt::Expr(builtin("panic", vec![str_lit("bad color")]))]),
                )],
            ),
        ];
        assert_eq!(codes(items), Vec::<&str>::new());
    }

    // ---- `@This()` / `Self` in plain struct methods (v0.136, SPEC §32.2) ---

    #[test]
    fn plain_struct_ptr_self_receiver_ok() {
        // const P = struct { x: i32, fn at(self: *Self) i32 { return self.x; } };
        // (`@This()` desugars to `Self`.) The `*Self` receiver auto-derefs.
        let at = raw_func(
            "at",
            vec![param_ptr("self", "Self")],
            "i32",
            vec![ret(Some(field(ident("self"), "x")))],
        );
        let items = vec![struct_item_m("P", vec![("x", "i32")], vec![at])];
        assert_eq!(codes(items), Vec::<&str>::new());
    }

    #[test]
    fn plain_struct_self_return_and_literal_ok() {
        // const Q = struct { x: i32, fn with(self: Self, v: i32) Self {
        //   return Self{ .x = v };
        // } };
        // A `Self` return type and a `Self{ … }` literal resolve to the struct.
        let with = raw_func(
            "with",
            vec![param("self", "Self"), param("v", "i32")],
            "Self",
            vec![ret(Some(struct_lit("Self", vec![("x", ident("v"))])))],
        );
        let items = vec![struct_item_m("Q", vec![("x", "i32")], vec![with])];
        assert_eq!(codes(items), Vec::<&str>::new());
    }

    #[test]
    fn plain_struct_self_value_parameter_ok() {
        // const R = struct { x: i32, fn eq(self: Self, other: Self) bool {
        //   return self.x == other.x;
        // } };
        // A non-receiver `Self` parameter resolves to the struct (Pass-1b binds
        // `Self`), so the field access on `other` type-checks.
        let eq = raw_func(
            "eq",
            vec![param("self", "Self"), param("other", "Self")],
            "bool",
            vec![ret(Some(bin(
                BinOp::Eq,
                field(ident("self"), "x"),
                field(ident("other"), "x"),
            )))],
        );
        let items = vec![struct_item_m("R", vec![("x", "i32")], vec![eq])];
        assert_eq!(codes(items), Vec::<&str>::new());
    }

    #[test]
    fn plain_struct_self_call_resolves_through_other_param() {
        // Calling `a.eq(b)` where both `self` and `other` are `Self`: the
        // registered signature's `other` parameter is `Struct(R)`, so passing an
        // `R` value type-checks. (Guards the Pass-1b `Self` binding.)
        let eq = raw_func(
            "eq",
            vec![param("self", "Self"), param("other", "Self")],
            "bool",
            vec![ret(Some(bin(
                BinOp::Eq,
                field(ident("self"), "x"),
                field(ident("other"), "x"),
            )))],
        );
        let items = vec![
            struct_item_m("R", vec![("x", "i32")], vec![eq]),
            func(
                "main",
                vec![],
                "void",
                vec![
                    let_var("a", "R", struct_lit("R", vec![("x", int(1))])),
                    let_var("b", "R", struct_lit("R", vec![("x", int(2))])),
                    Stmt::Expr(method_call(ident("a"), "eq", vec![ident("b")])),
                ],
            ),
        ];
        assert_eq!(codes(items), Vec::<&str>::new());
    }

    // ---- floating point `f64` (v0.144, SPEC §38) --------------------------

    /// A floating-point literal `3.14` of type `f64`.
    fn float(v: f64) -> Expr {
        Expr::Float { value: v, span: sp() }
    }

    #[test]
    fn float_literal_is_f64() {
        let mut cx = Checker::new();
        assert_eq!(cx.check_expr(&float(3.14), None), Some(Type::F64));
        // A contextual integer expectation does NOT coerce a float literal.
        assert_eq!(cx.check_expr(&float(3.14), Some(Type::I32)), Some(Type::F64));
    }

    #[test]
    fn float_var_decl_ok() {
        // fn main() void { var x: f64 = 3.14; }
        let items = vec![func(
            "main",
            vec![],
            "void",
            vec![let_var("x", "f64", float(3.14))],
        )];
        assert_eq!(codes(items), Vec::<&str>::new());
    }

    #[test]
    fn float_arithmetic_yields_f64() {
        // fn f(a: f64, b: f64) f64 { return a + b; }   (also - * / via vars)
        let items = vec![func(
            "f",
            vec![param("a", "f64"), param("b", "f64")],
            "f64",
            vec![
                let_var("s", "f64", bin(BinOp::Add, ident("a"), ident("b"))),
                let_var("d", "f64", bin(BinOp::Sub, ident("a"), ident("b"))),
                let_var("m", "f64", bin(BinOp::Mul, ident("a"), ident("b"))),
                ret(Some(bin(BinOp::Div, ident("a"), ident("b")))),
            ],
        )];
        assert_eq!(codes(items), Vec::<&str>::new());
    }

    #[test]
    fn float_comparison_yields_bool() {
        // fn lt(a: f64, b: f64) bool { return a < b; }   (and == via a let)
        let items = vec![func(
            "lt",
            vec![param("a", "f64"), param("b", "f64")],
            "bool",
            vec![
                let_var("e", "bool", bin(BinOp::Eq, ident("a"), ident("b"))),
                ret(Some(bin(BinOp::Lt, ident("a"), ident("b")))),
            ],
        )];
        assert_eq!(codes(items), Vec::<&str>::new());
    }

    #[test]
    fn float_plus_int_is_e0110() {
        // fn f(a: f64, b: i32) f64 { return a + b; }   — no implicit int↔float.
        let items = vec![func(
            "f",
            vec![param("a", "f64"), param("b", "i32")],
            "f64",
            vec![ret(Some(bin(BinOp::Add, ident("a"), ident("b"))))],
        )];
        assert!(codes(items).contains(&"E0110"));
    }

    #[test]
    fn float_modulo_is_rejected() {
        // fn f(a: f64, b: f64) f64 { return a % b; }   — `%` stays integer-only.
        let items = vec![func(
            "f",
            vec![param("a", "f64"), param("b", "f64")],
            "f64",
            vec![ret(Some(bin(BinOp::Rem, ident("a"), ident("b"))))],
        )];
        assert!(codes(items).contains(&"E0110"));
    }

    #[test]
    fn as_int_to_float_is_f64() {
        // fn main() void { var x: f64 = @as(f64, 5); }
        let items = vec![func(
            "main",
            vec![],
            "void",
            vec![let_var(
                "x",
                "f64",
                builtin("as", vec![ident("f64"), int(5)]),
            )],
        )];
        assert_eq!(codes(items), Vec::<&str>::new());
    }

    #[test]
    fn as_float_to_int_is_int() {
        // fn main() void { var x: i32 = @as(i32, 3.5); }
        let items = vec![func(
            "main",
            vec![],
            "void",
            vec![let_var(
                "x",
                "i32",
                builtin("as", vec![ident("i32"), float(3.5)]),
            )],
        )];
        assert_eq!(codes(items), Vec::<&str>::new());
    }

    #[test]
    fn print_accepts_f64() {
        // fn f(a: f64) void { print(a); }
        let items = vec![func(
            "f",
            vec![param("a", "f64")],
            "void",
            vec![Stmt::Expr(call("print", vec![ident("a")]))],
        )];
        assert_eq!(codes(items), Vec::<&str>::new());
    }

    #[test]
    fn integer_arithmetic_and_compare_unchanged() {
        // fn add(a: i32, b: i32) i32 { return a + b; }
        // fn cmp(a: i32, b: i32) bool { return a < b; }
        // Integer arithmetic still anchors literal polymorphism: `1 + 2` is i64.
        let items = vec![
            func(
                "add",
                vec![param("a", "i32"), param("b", "i32")],
                "i32",
                vec![ret(Some(bin(BinOp::Add, ident("a"), ident("b"))))],
            ),
            func(
                "cmp",
                vec![param("a", "i32"), param("b", "i32")],
                "bool",
                vec![ret(Some(bin(BinOp::Lt, ident("a"), ident("b"))))],
            ),
            func(
                "lit",
                vec![],
                "i64",
                vec![ret(Some(bin(BinOp::Add, int(1), int(2))))],
            ),
        ];
        assert_eq!(codes(items), Vec::<&str>::new());
    }

    #[test]
    fn integer_plus_bool_still_errors() {
        // fn f(a: i32, b: bool) i32 { return a + b; }  — non-numeric operand.
        let items = vec![func(
            "f",
            vec![param("a", "i32"), param("b", "bool")],
            "i32",
            vec![ret(Some(bin(BinOp::Add, ident("a"), ident("b"))))],
        )];
        assert!(codes(items).contains(&"E0110"));
    }

    #[test]
    fn float_const_is_e0130() {
        // const P = 3.14;   — floats are runtime-only in v0.144.
        let items = vec![const_item_infer("P", float(3.14))];
        assert!(codes(items).contains(&"E0130"));
    }

    #[test]
    fn float_const_with_annotation_is_e0130() {
        // const P: f64 = 3.14;   — still rejected (const_eval cannot fold a float).
        let items = vec![const_item("P", "f64", float(3.14))];
        assert!(codes(items).contains(&"E0130"));
    }
}
