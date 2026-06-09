//! C backend: a validated AST → portable, deterministic C11 source text.
//!
//! `defer` statements are lowered here: each scope tracks its deferred
//! statements and flushes them in LIFO (reverse registration) order at every
//! exit edge — fall-through off the end of a block, `return`, `break` and
//! `continue` (and, in test mode, a failed `expect`). Identical input always
//! produces byte-identical output.

use std::collections::HashMap;

use crate::ast::{
    ArraySize, BinOp, Block, Expr, Func, Item, Module, Param, Stmt, SwitchArm, TestBlock, TypeExpr,
    UnOp,
};
use crate::const_eval::ConstVal;
use crate::types::{ComptimeArg, Instantiation, StructTable, Type};

/// Id-space base for emit-local pointer types (v0.118). The [`StructTable`]
/// interns slices (and exposes a `slices()` iterator) but pointers carry no
/// typedef and there is no `pointers()` iterator to map a `*T` source type back
/// to its `Type::Ptr(id)`. So emission maintains its own small pointee registry
/// ([`Emitter::local_ptr_pointees`]) for the `*T` types written in signatures /
/// locals; ids into it are offset by this base so they never collide with the
/// table's own (small) pointer ids. Pointer ids that come *out of* the table
/// (e.g. a struct field of type `*T`) stay below the base and are resolved
/// against the table; emit-local ids are resolved against the registry. See
/// [`Emitter::ptr_pointee_any`].
const PTR_LOCAL_BASE: u32 = 0x4000_0000;

/// What kind of program to emit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EmitMode {
    /// A normal program: emit a C `main` that calls the user's `main`.
    Program,
    /// A test harness: emit a C `main` that runs every `test` block and
    /// reports pass/fail counts; the process exit code is the failure count.
    Test,
}

/// Lower a validated `module` to C11 source text for `mode`. `structs` is the
/// table produced by semantic analysis; its declaration order drives the C
/// `typedef` emission and resolves every `Type::Struct(id)` to its C name.
pub fn emit(module: &Module, structs: &crate::types::StructTable, mode: EmitMode) -> String {
    let mut em = Emitter::new(mode, structs);
    em.collect_signatures(module);
    // Whether the `kd_panic` runtime helper is needed (SPEC §35.2): emitted only
    // when an `@panic(..)` lowering will appear, so a string-using program that
    // never panics is unaffected.
    em.uses_panic = module_uses_panic(module);
    em.emit_prelude();
    em.emit_type_defs();
    em.emit_consts(module);
    em.emit_forward_decls(module);
    em.emit_func_defs(module);
    match mode {
        EmitMode::Program => em.emit_program_main(module),
        EmitMode::Test => em.emit_test_harness(module),
    }
    em.out
}

// -- v0.141 `@panic` usage scan (SPEC §35.2) -------------------------------
//
// Whether the module contains any `@panic(..)` builtin — in a function body, a
// struct method, a test block, a `const` initializer, or a (monomorphised)
// generic-struct method. Drives whether the `kd_panic` runtime helper is
// emitted. Over-counting is harmless (an unused `_Noreturn` helper), but
// under-counting would leave a `kd_panic(..)` call referencing an undeclared
// function, so the walk is exhaustive over the current AST.

fn module_uses_panic(module: &Module) -> bool {
    module.items.iter().any(item_uses_panic)
}

fn item_uses_panic(item: &Item) -> bool {
    match item {
        Item::Func(f) => block_uses_panic(&f.body),
        Item::Const(c) => expr_uses_panic(&c.value),
        Item::Test(t) => block_uses_panic(&t.body),
        Item::Struct(s) => s.methods.iter().any(|m| block_uses_panic(&m.body)),
        Item::Enum(_) | Item::Union(_) | Item::Import(_) | Item::ErrorSet(_) => false,
    }
}

fn block_uses_panic(b: &Block) -> bool {
    b.stmts.iter().any(stmt_uses_panic)
}

fn stmt_uses_panic(s: &Stmt) -> bool {
    match s {
        Stmt::Let { value, .. } => expr_uses_panic(value),
        Stmt::Assign { value, .. } => expr_uses_panic(value),
        Stmt::FieldAssign { place, value, .. } => {
            expr_uses_panic(place) || expr_uses_panic(value)
        }
        Stmt::Expr(e) => expr_uses_panic(e),
        Stmt::Return { value, .. } => value.as_ref().is_some_and(expr_uses_panic),
        Stmt::If {
            cond, then, els, ..
        } => {
            expr_uses_panic(cond)
                || block_uses_panic(then)
                || els.as_deref().is_some_and(stmt_uses_panic)
        }
        Stmt::While {
            cond, cont, body, ..
        } => {
            expr_uses_panic(cond)
                || cont.as_deref().is_some_and(stmt_uses_panic)
                || block_uses_panic(body)
        }
        Stmt::For { iter, body, .. } => expr_uses_panic(iter) || block_uses_panic(body),
        Stmt::Break(_) | Stmt::Continue(_) => false,
        Stmt::Defer { stmt, .. } | Stmt::ErrDefer { stmt, .. } => stmt_uses_panic(stmt),
        Stmt::Block(b) => block_uses_panic(b),
        Stmt::Switch {
            scrutinee,
            arms,
            default,
            ..
        } => {
            expr_uses_panic(scrutinee)
                || arms.iter().any(|a| {
                    a.labels.iter().any(expr_uses_panic) || block_uses_panic(&a.body)
                })
                || default.as_ref().is_some_and(block_uses_panic)
        }
    }
}

fn expr_uses_panic(e: &Expr) -> bool {
    match e {
        Expr::Builtin { name, args, .. } => name == "panic" || args.iter().any(expr_uses_panic),
        Expr::Unary { expr, .. } => expr_uses_panic(expr),
        Expr::Binary { lhs, rhs, .. } => expr_uses_panic(lhs) || expr_uses_panic(rhs),
        Expr::Call { args, .. } => args.iter().any(expr_uses_panic),
        Expr::Comptime { expr, .. } => expr_uses_panic(expr),
        Expr::StructLit { fields, .. } => fields.iter().any(|f| expr_uses_panic(&f.value)),
        Expr::Field { base, .. } => expr_uses_panic(base),
        Expr::MethodCall { receiver, args, .. } => {
            expr_uses_panic(receiver) || args.iter().any(expr_uses_panic)
        }
        Expr::Orelse { lhs, rhs, .. } => expr_uses_panic(lhs) || expr_uses_panic(rhs),
        Expr::Unwrap { expr, .. } => expr_uses_panic(expr),
        Expr::ArrayLit { elems, .. } => elems.iter().any(expr_uses_panic),
        Expr::Index { base, index, .. } => expr_uses_panic(base) || expr_uses_panic(index),
        Expr::AddrOf { place, .. } => expr_uses_panic(place),
        Expr::Deref { expr, .. } => expr_uses_panic(expr),
        Expr::SliceExpr { base, lo, hi, .. } => {
            expr_uses_panic(base) || expr_uses_panic(lo) || expr_uses_panic(hi)
        }
        Expr::Try { expr, .. } => expr_uses_panic(expr),
        Expr::Catch { expr, default, .. } => expr_uses_panic(expr) || expr_uses_panic(default),
        Expr::StructType { methods, .. } => methods.iter().any(|m| block_uses_panic(&m.body)),
        Expr::Int { .. }
        | Expr::Bool { .. }
        | Expr::Ident { .. }
        | Expr::StrLit { .. }
        | Expr::Null { .. }
        | Expr::ErrorLit { .. }
        | Expr::EnumLit { .. }
        | Expr::Unreachable { .. } => false,
    }
}

/// A lexical scope active during emission. Each one accumulates the `defer`
/// statement bodies registered within it (in registration order) and notes
/// whether it is the body of a `while` loop (so `break`/`continue` know where
/// to stop flushing). A loop-body scope also carries the loop's optional
/// continue-expression.
struct Scope {
    /// Deferred statements in registration order, each tagged `is_errdefer`.
    /// `defer`s run on every scope exit; `errdefer`s run only on error-return
    /// edges (SPEC §21.2).
    defers: Vec<(bool, Stmt)>,
    is_loop_body: bool,
    cont: Option<Stmt>,
    /// A raw C continue-clause emitted verbatim at every loop edge (fall-through
    /// off the body end and before each `continue`), beside any `cont`
    /// (SPEC §29.2). A `for` loop uses this for its index increment
    /// `__kd_fi{N} += 1;`, which is not expressible as an AST `Stmt` (the index
    /// temp is not a `kd_`-prefixed source binding). `None` for `while`/plain
    /// scopes, so their lowering is unchanged.
    cont_raw: Option<String>,
    /// Types of locals/params introduced in this scope, keyed by source name.
    /// Used both to resolve a method call's receiver to the struct whose
    /// function is invoked (`kd_<Struct>_<method>`) and to decide optional
    /// coercion (see [`Emitter::type_of_expr`]). Scoped, so a shadowing inner
    /// binding masks an outer one and is forgotten when the scope pops.
    var_types: HashMap<String, Type>,
}

impl Scope {
    fn plain() -> Scope {
        Scope {
            defers: Vec::new(),
            is_loop_body: false,
            cont: None,
            cont_raw: None,
            var_types: HashMap::new(),
        }
    }

    /// The outermost scope of a function or test body. Structurally identical
    /// to a plain block scope; named for clarity at the call sites.
    fn function() -> Scope {
        Scope::plain()
    }

    fn loop_body(cont: Option<Stmt>) -> Scope {
        Scope {
            defers: Vec::new(),
            is_loop_body: true,
            cont,
            cont_raw: None,
            var_types: HashMap::new(),
        }
    }
}

struct Emitter<'a> {
    mode: EmitMode,
    out: String,
    indent: usize,
    /// Active scopes, innermost last. Reset at the start of every function /
    /// test body, so index 0 is always the current function scope.
    scopes: Vec<Scope>,
    /// Return type of the function currently being emitted (drives the
    /// `__kd_ret` temporary on a deferred return).
    current_ret: Type,
    /// Folded top-level constants, in source order, used to evaluate
    /// `comptime` expressions during emission.
    consts: HashMap<String, ConstVal>,
    /// The struct table from sema: resolves struct C names and field types.
    structs: &'a StructTable,
    /// Resolved return type of every struct function, keyed by
    /// `(struct_name, method_name)`. Lets a chained method call resolve the
    /// struct of its receiver when that receiver is itself a method call.
    method_ret: HashMap<(String, String), Type>,
    /// Resolved return type of every top-level `fn`, keyed by name. Lets a
    /// method call whose receiver is a free-function call resolve the struct.
    fn_ret: HashMap<String, Type>,
    /// Resolved parameter types of every top-level `fn`, keyed by name. Drives
    /// optional coercion of call arguments.
    fn_params: HashMap<String, Vec<Type>>,
    /// Resolved parameter types of every struct function (including any leading
    /// `self`), keyed by `(struct_name, method_name)`. Drives optional coercion
    /// of method/associated-call arguments.
    method_params: HashMap<(String, String), Vec<Type>>,
    /// Monotonic counter for the `__kd_tryN` temporaries that lower `try`
    /// expressions. Reset at the start of every function/test body so the
    /// numbering stays small and deterministic; names never collide because
    /// distinct functions are distinct C blocks.
    try_counter: usize,
    /// Monotonic counter for the `__kd_idxN` temporaries that lower a
    /// bounds-checked array index-assignment (SPEC §14.3). Reset per
    /// function/test body, exactly like `try_counter`.
    idx_counter: usize,
    /// Monotonic counter for the `__kd_ifN` temporaries of an optional-`if`
    /// capture (SPEC §21.1). Reset per function/test body.
    if_counter: usize,
    /// Monotonic counter for the `__kd_strN` temporaries that hoist the slice of
    /// a `print(s)` where `s: []u8` (a string), so the slice expression is only
    /// evaluated once before `fwrite` (SPEC §23.2). Reset per function/test body.
    str_counter: usize,
    /// Monotonic counter for the `__kd_for{N}` iterable temporary and `__kd_fi{N}`
    /// walking index of a `for` loop (SPEC §29.2). Reset per function/test body,
    /// exactly like the other temp counters.
    for_counter: usize,
    /// Pointee types of the `*T` pointer types written in this module's
    /// signatures / locals, in first-seen order (SPEC §15.1). Pointers have no
    /// typedef and the table exposes no `pointers()` iterator, so emit keeps
    /// this registry to map a `*T` source type back to a `Type::Ptr` it can
    /// resolve (ids are offset by [`PTR_LOCAL_BASE`]). Populated in
    /// [`Emitter::collect_signatures`] before any type is resolved.
    local_ptr_pointees: Vec<Type>,
    /// The active type-parameter substitution while emitting a generic
    /// function's instantiation (v0.120, SPEC §17.3). Maps a comptime
    /// type-parameter name (e.g. `"T"`) to the concrete [`Type`] it stands for
    /// in the current instance. Empty everywhere else, so `resolve_ty` / `cty`
    /// behave exactly as before for non-generic code.
    subst: HashMap<String, Type>,
    /// The active comptime **value**-parameter substitution while emitting a
    /// generic instance (v0.128, SPEC §24.3). Maps a comptime value-parameter
    /// name (e.g. `"n"`) to the concrete `i64` it stands for in the current
    /// instance — used to resolve an `ArraySize::Param(n)` (so `[n]i32` becomes
    /// `[5]i32`) and to emit a body reference to `n` as the bound literal. Empty
    /// everywhere else, so non-generic code is unaffected.
    value_subst: HashMap<String, i64>,
    /// Every generic top-level function (one with ≥1 `comptime` type
    /// parameter), keyed by name. A generic function is never emitted under its
    /// plain name — only one specialised C function per recorded instantiation
    /// is emitted (see [`Emitter::emit_instance_defs`]). Stored as clones so a
    /// `Call` to a generic can be lowered to its instance's C name anywhere.
    generics: HashMap<String, Func>,
    /// v0.141: whether the module contains any `@panic(..)` (SPEC §35). Drives
    /// whether the `kd_panic` runtime helper is emitted in `emit_type_defs` — it
    /// must accompany every `kd_panic(..)` lowering and be absent otherwise, so a
    /// string-using program that never panics keeps its `fwrite`-free output.
    /// Computed once in [`emit`] before any type is emitted.
    uses_panic: bool,
}

impl<'a> Emitter<'a> {
    fn new(mode: EmitMode, structs: &'a StructTable) -> Emitter<'a> {
        Emitter {
            mode,
            out: String::new(),
            indent: 0,
            scopes: Vec::new(),
            current_ret: Type::Void,
            consts: HashMap::new(),
            structs,
            method_ret: HashMap::new(),
            fn_ret: HashMap::new(),
            fn_params: HashMap::new(),
            method_params: HashMap::new(),
            try_counter: 0,
            idx_counter: 0,
            if_counter: 0,
            str_counter: 0,
            for_counter: 0,
            local_ptr_pointees: Vec::new(),
            subst: HashMap::new(),
            value_subst: HashMap::new(),
            generics: HashMap::new(),
            uses_panic: false,
        }
    }

    /// True if `f` is a generic function: it has at least one `comptime`
    /// parameter — a type parameter (`comptime T: type`, v0.120) or a value
    /// parameter (`comptime n: usize`, v0.128). Such a function is checked +
    /// emitted only per instantiation, never under its plain name.
    fn is_generic(f: &Func) -> bool {
        f.params.iter().any(|p| p.is_comptime)
    }

    /// True if `f` is a **type-constructor** (v0.129, SPEC §25): a function
    /// whose return type is the bare `type` keyword
    /// (`fn Name(comptime T: type) type { return struct {…}; }`). Like a generic
    /// function it is compile-time only and is **never emitted** to C — sema has
    /// already turned each `const Alias = Name(C);` instantiation into an
    /// ordinary monomorphised struct typedef. A conforming type-constructor also
    /// has a `comptime` parameter (so [`Emitter::is_generic`] already returns
    /// `true` for it), but checking the return type directly is robust even for a
    /// parameter-less `fn F() type`, which carries no C return type. The
    /// `type` spelling is never decorated with `?`/`!`/`[N]`/`*`/`[]`.
    fn is_type_ctor(f: &Func) -> bool {
        f.ret.name == "type"
            && !f.ret.optional
            && !f.ret.error_union
            && f.ret.array_len.is_none()
            && !f.ret.pointer
            && !f.ret.slice
    }

    /// True if `c` is a v0.129 **type-alias** const — `const Alias = Name(C);`
    /// whose initializer calls a type-constructor function (SPEC §25.3). Such a
    /// const named a monomorphised struct, not a runtime value, so emit must
    /// **not** emit it as a C `const`. Detected purely from the module's own
    /// functions by the callee name (a validated `const` with a `Call`
    /// initializer can only be a type-constructor instantiation — ordinary value
    /// consts must be compile-time constant, and a `Call` is not, §3).
    fn is_type_alias_const(module: &Module, c: &crate::ast::ConstDecl) -> bool {
        if let Expr::Call { callee, .. } = &c.value {
            module
                .items
                .iter()
                .any(|it| matches!(it, Item::Func(f) if &f.name == callee && Self::is_type_ctor(f)))
        } else {
            false
        }
    }

    /// True if `p` is a comptime **value** parameter (`comptime n: usize`,
    /// v0.128): a comptime parameter whose annotation is not the `type` kind.
    /// (A `comptime T: type` parameter is a type parameter, v0.120.)
    fn is_value_param(p: &Param) -> bool {
        p.is_comptime && p.ty.name != "type"
    }

    /// Set the active substitutions to map `f`'s comptime parameters (in order)
    /// to `args` — the substitution active while emitting one instance (SPEC
    /// §24.3). A `type` parameter binds a [`Type`] into [`Emitter::subst`]; a
    /// value parameter binds an `i64` into [`Emitter::value_subst`].
    fn set_subst_for(&mut self, f: &Func, args: &[ComptimeArg]) {
        self.subst.clear();
        self.value_subst.clear();
        for (p, a) in f.params.iter().filter(|p| p.is_comptime).zip(args.iter()) {
            match a {
                ComptimeArg::Type(t) => {
                    self.subst.insert(p.name.clone(), *t);
                }
                ComptimeArg::Value(v) => {
                    self.value_subst.insert(p.name.clone(), *v);
                }
            }
        }
    }

    /// Clear both the type and value substitutions after emitting an instance,
    /// so subsequent non-generic emission is unaffected.
    fn clear_subst(&mut self) {
        self.subst.clear();
        self.value_subst.clear();
    }

    /// The type-constructor `Func` (cloned) that produced generic-struct
    /// instance `inst`, if present in this module (v0.130, SPEC §26.3). It is
    /// the `fn Name(comptime T: type) type` whose `return struct {…};` body
    /// carries the methods to monomorphise for this instance.
    fn instance_ctor(module: &Module, inst: &crate::types::StructInstance) -> Option<Func> {
        module.items.iter().find_map(|it| match it {
            Item::Func(f) if f.name == inst.ctor && Self::is_type_ctor(f) => Some(f.clone()),
            _ => None,
        })
    }

    /// The methods (cloned) declared inside a type-constructor's `struct {…}`
    /// body (v0.130, SPEC §26.1). A conforming constructor body is `return
    /// struct {…};`; the `Expr::StructType`'s `methods` are returned. An empty
    /// vector for the v0.129 fields-only shape (or any non-canonical body —
    /// impossible for validated input), so a fields-only generic struct keeps
    /// behaving exactly as v0.129.
    fn ctor_methods(ctor: &Func) -> Vec<Func> {
        for s in &ctor.body.stmts {
            if let Stmt::Return {
                value: Some(Expr::StructType { methods, .. }),
                ..
            } = s
            {
                return methods.clone();
            }
        }
        Vec::new()
    }

    /// Set the active substitution for emitting a generic-struct instance's
    /// methods (v0.130/v0.135, SPEC §26.3/§31.2): each of the constructor's
    /// comptime type parameters (in declaration order) → the corresponding
    /// concrete argument from `args`, plus the contextual `Self` → the
    /// instantiated struct `Struct(struct_id)`. Mirrors
    /// [`Emitter::set_subst_for`] but adds the `Self` binding. A type-constructor
    /// has only comptime *type* parameters (never value ones), so each is bound
    /// into [`Emitter::subst`]; v0.135 allows **more than one** of them, zipped
    /// positionally with `args` (v0.129/v0.130's single-parameter case is the
    /// length-1 zip and behaves identically).
    fn set_instance_subst(&mut self, ctor: &Func, args: &[Type], struct_id: u32) {
        self.subst.clear();
        self.value_subst.clear();
        for (p, a) in ctor.params.iter().filter(|p| p.is_comptime).zip(args.iter()) {
            self.subst.insert(p.name.clone(), *a);
        }
        self.subst.insert("Self".to_string(), Type::Struct(struct_id));
    }

    /// Pre-pass: record the resolved return type of every top-level function and
    /// every struct function. These let [`Emitter::struct_of_expr`] follow a
    /// receiver chain that passes through a call to find the struct whose
    /// function a method call invokes. Pure bookkeeping — emits nothing.
    fn collect_signatures(&mut self, module: &Module) {
        // Register every generic top-level function first (SPEC §17). The
        // pointer pre-pass and the signature pass below both recognise and skip
        // generic functions — their signatures only make sense under a concrete
        // substitution, handled per instantiation.
        for item in &module.items {
            if let Item::Func(f) = item {
                if Self::is_generic(f) {
                    self.generics.insert(f.name.clone(), f.clone());
                }
            }
        }
        // Pre-pass: register every `*T` written in a signature or local so
        // `resolve_ty` can map those pointer types to a `Type::Ptr` id. Must run
        // before any `resolve_ty` call below (which already resolves return /
        // parameter types into the signature tables).
        self.collect_ptr_types(module);
        // Register the `*T` pointee types that appear *inside* each generic
        // instantiation, resolved under that instance's substitution — so a
        // `*T` used in a generic body (e.g. `*i32` for the `i32` instance) maps
        // to a real pointee in [`Emitter::local_ptr_pointees`] (SPEC §17.3).
        let structs = self.structs;
        let insts = structs.instantiations().to_vec();
        for inst in &insts {
            if let Some(f) = self.generics.get(&inst.fn_name).cloned() {
                self.set_subst_for(&f, &inst.args);
                self.note_func_ptrs(&f);
                self.clear_subst();
            }
        }
        // Register the `*T` / `*Self` pointee types used inside each
        // generic-struct instance's methods, under that instance's substitution
        // (v0.130, SPEC §26.3) — so a pointer receiver (`self: *Self`) or local
        // resolves to a real pointee, mirroring the generic-function pre-pass
        // above. By-value methods register no pointers, so this is a no-op for
        // the common case.
        let sinsts = structs.struct_instances().to_vec();
        for inst in &sinsts {
            if let Some(ctor) = Self::instance_ctor(module, inst) {
                self.set_instance_subst(&ctor, &inst.args, inst.struct_id);
                for m in Self::ctor_methods(&ctor) {
                    self.note_func_ptrs(&m);
                }
                self.clear_subst();
            }
        }
        for item in &module.items {
            match item {
                Item::Func(f) => {
                    // A generic function has no resolvable signature without a
                    // substitution; it is handled per instantiation, not here.
                    if Self::is_generic(f) {
                        continue;
                    }
                    let ret = self.resolve_ty(&f.ret);
                    self.fn_ret.insert(f.name.clone(), ret);
                    let ptys: Vec<Type> = f.params.iter().map(|p| self.resolve_ty(&p.ty)).collect();
                    self.fn_params.insert(f.name.clone(), ptys);
                }
                Item::Struct(s) => {
                    // Bind `Self` so a `*Self` / `@This()` receiver resolves to a
                    // pointer to this struct (v0.136, §32.2).
                    let sid = self.structs.id_of(&s.name);
                    if let Some(id) = sid {
                        self.subst.insert("Self".to_string(), Type::Struct(id));
                    }
                    for m in &s.methods {
                        let ret = self.resolve_ty(&m.ret);
                        self.method_ret.insert((s.name.clone(), m.name.clone()), ret);
                        let ptys: Vec<Type> =
                            m.params.iter().map(|p| self.resolve_ty(&p.ty)).collect();
                        self.method_params
                            .insert((s.name.clone(), m.name.clone()), ptys);
                    }
                    if sid.is_some() {
                        self.subst.remove("Self");
                    }
                }
                _ => {}
            }
        }
        // Register the return + parameter types of every monomorphised
        // generic-struct instance method (v0.130, SPEC §26.3), resolved under
        // the instance substitution `{ type-param → arg, Self → Struct(id) }`,
        // keyed by the instantiated struct's source name. These let a method
        // call on an instance value resolve its return type (`type_of_expr` /
        // `struct_of_expr`) and coerce its arguments, exactly as for an
        // ordinary struct method (SPEC §10).
        for inst in &sinsts {
            let ctor = match Self::instance_ctor(module, inst) {
                Some(c) => c,
                None => continue,
            };
            let sname = self.structs.get(inst.struct_id).name.clone();
            self.set_instance_subst(&ctor, &inst.args, inst.struct_id);
            for m in Self::ctor_methods(&ctor) {
                let ret = self.resolve_ty(&m.ret);
                self.method_ret.insert((sname.clone(), m.name.clone()), ret);
                let ptys: Vec<Type> =
                    m.params.iter().map(|p| self.resolve_ty(&p.ty)).collect();
                self.method_params
                    .insert((sname.clone(), m.name.clone()), ptys);
            }
            self.clear_subst();
        }
    }

    /// Walk every `TypeExpr` written in a signature or local declaration and
    /// register the pointee of each `*T` into [`Emitter::local_ptr_pointees`]
    /// (deduplicated, first-seen order). This gives `resolve_ty` a stable id to
    /// hand back for pointer types, which the table cannot supply (pointers have
    /// no typedef and no `pointers()` iterator). Struct **field** pointer types
    /// are excluded on purpose: those are stored resolved in the table already
    /// (with the table's own pointer ids) and are resolved against it.
    fn collect_ptr_types(&mut self, module: &Module) {
        for item in &module.items {
            match item {
                // Generic functions are scanned per instantiation (under a
                // substitution) in `collect_signatures`, not here.
                Item::Func(f) => {
                    if !Self::is_generic(f) {
                        self.note_func_ptrs(f);
                    }
                }
                Item::Struct(s) => {
                    // Bind `Self` so a `*Self` / `@This()` receiver registers a
                    // pointer to THIS struct as a pointee (v0.136, §32.2), so
                    // `self.field` auto-derefs in the lowering.
                    let sid = self.structs.id_of(&s.name);
                    if let Some(id) = sid {
                        self.subst.insert("Self".to_string(), Type::Struct(id));
                    }
                    for m in &s.methods {
                        self.note_func_ptrs(m);
                    }
                    if sid.is_some() {
                        self.subst.remove("Self");
                    }
                }
                Item::Const(c) => {
                    // A const's type annotation is optional (v0.121); an inferred
                    // const carries no `*T` source type to register here.
                    if let Some(t) = &c.ty {
                        self.note_ptr_ty(t);
                    }
                }
                Item::Test(t) => self.note_block_ptrs(&t.body),
                Item::Enum(_) => {}
                // A union's variant payload types are stored resolved in the
                // table (a `*T` payload carries a table pointer id, resolved via
                // `ptr_pointee_any`); none are registered in the emit-local
                // pointer registry, exactly like struct field types.
                Item::Union(_) => {}
                // Imports are erased by the module flattener before emit.
                Item::Import(_) => {}
                // A named error set (v0.139, §34) is a compile-time-only sema
                // constraint with no runtime representation — it declares no
                // types and registers no pointers.
                Item::ErrorSet(_) => {}
            }
        }
    }

    fn note_func_ptrs(&mut self, f: &Func) {
        self.note_ptr_ty(&f.ret);
        for p in &f.params {
            self.note_ptr_ty(&p.ty);
        }
        self.note_block_ptrs(&f.body);
    }

    fn note_block_ptrs(&mut self, b: &Block) {
        for s in &b.stmts {
            self.note_stmt_ptrs(s);
        }
    }

    fn note_stmt_ptrs(&mut self, s: &Stmt) {
        match s {
            Stmt::Let { ty, .. } => {
                // The annotation is optional (v0.121); an inferred binding has no
                // `*T` source type to register here.
                if let Some(t) = ty {
                    self.note_ptr_ty(t);
                }
            }
            Stmt::If { then, els, .. } => {
                self.note_block_ptrs(then);
                if let Some(e) = els {
                    self.note_stmt_ptrs(e);
                }
            }
            Stmt::While { body, .. } => self.note_block_ptrs(body),
            // A `for` body may contain `*T` source types to register, exactly
            // like a `while` body (SPEC §29).
            Stmt::For { body, .. } => self.note_block_ptrs(body),
            Stmt::Block(b) => self.note_block_ptrs(b),
            Stmt::Defer { stmt, .. } => self.note_stmt_ptrs(stmt),
            Stmt::ErrDefer { stmt, .. } => self.note_stmt_ptrs(stmt),
            Stmt::Switch { arms, default, .. } => {
                for a in arms {
                    self.note_block_ptrs(&a.body);
                }
                if let Some(d) = default {
                    self.note_block_ptrs(d);
                }
            }
            _ => {}
        }
    }

    /// Register `t`'s pointee if `t` is a `*T` (the pointee is `t.name` resolved
    /// as a base type; v0.118 does not combine `*` with `?`/`!`/`[N]`).
    fn note_ptr_ty(&mut self, t: &TypeExpr) {
        if t.pointer {
            let pointee = self.base_type(&t.name);
            if !self.local_ptr_pointees.contains(&pointee) {
                self.local_ptr_pointees.push(pointee);
            }
        }
    }

    // -- low-level output ---------------------------------------------------

    /// Emit one indented line (terminated by a newline).
    fn line(&mut self, s: &str) {
        for _ in 0..self.indent {
            self.out.push_str("    ");
        }
        self.out.push_str(s);
        self.out.push('\n');
    }

    fn blank(&mut self) {
        self.out.push('\n');
    }

    // -- prelude / sections -------------------------------------------------

    fn emit_prelude(&mut self) {
        self.out.push_str("#include <stdint.h>\n");
        self.out.push_str("#include <stdbool.h>\n");
        self.out.push_str("#include <stdio.h>\n");
        // `<stdlib.h>` provides `exit` (the `?T` unwrap panic helper) and, since
        // v0.114, `malloc`/`free` (the allocator helpers + `free` builtin).
        self.out.push_str("#include <stdlib.h>\n");
        // The `Allocator` interface value (SPEC §16). v0.119 ships a single,
        // empty malloc/free-backed allocator (`c_allocator()`); the `int _unused`
        // field exists only so the struct is non-empty and stays valid C.
        self.out
            .push_str("typedef struct { int _unused; } kd_allocator;\n");
        self.out
            .push_str("static void kd_print(long long v) { printf(\"%lld\\n\", v); }\n");
        // v0.141 runtime-safety traps (SPEC §35.2). `kd_unreachable` has no type
        // dependency, so it lives here in the prelude and is emitted
        // unconditionally — a never-called `_Noreturn` function is harmless (no
        // diagnostic for an unused non-`static` definition). Its sibling
        // `kd_panic` takes the message as a `[]u8` (`kd_slice_uint8_t`) and so is
        // emitted *after* that slice typedef, at the tail of `emit_type_defs`.
        self.out.push_str(
            "_Noreturn void kd_unreachable(void) { fputs(\"reached unreachable code\\n\", stderr); exit(101); }\n",
        );
        self.blank();
    }

    /// Emit one C `typedef struct { ... } kd_struct_<Name>;` per struct, in
    /// declaration (id) order — exactly the table's iteration order, so a
    /// field of a previously-declared struct type is always already in scope.
    /// An empty struct gets a `char _unused;` member so it stays valid C.
    /// Emit every aggregate/composite C typedef (structs and optionals) in
    /// **dependency order**: a definition is emitted only after the definitions
    /// of every type it embeds by value. A struct embeds its struct/optional
    /// field types; an optional `?T` embeds `T` when `T` is a struct/optional.
    /// (Recursive value embedding is impossible without pointers, so the
    /// dependency graph is acyclic in v0.114.)
    fn emit_type_defs(&mut self) {
        use std::collections::HashSet;
        let structs = self.structs;
        if structs.is_empty()
            && structs.optionals().next().is_none()
            && structs.error_unions().next().is_none()
            && structs.enums().next().is_none()
            && structs.unions().next().is_none()
            && structs.arrays().next().is_none()
            && structs.slices().next().is_none()
        {
            return;
        }

        // A definition node: a struct, an interned optional, an interned error
        // union, a plain enum, an array, or a slice. An enum has no by-value
        // dependencies, so it is always a leaf of the dependency graph.
        #[derive(Clone, Copy, PartialEq, Eq, Hash)]
        enum Node {
            Struct(u32),
            Opt(u32),
            ErrU(u32),
            Enum(u32),
            Union(u32),
            Array(u32),
            Slice(u32),
        }
        fn dep_of(t: Type, structs: &crate::types::StructTable) -> Option<Node> {
            match t {
                Type::Struct(s) => Some(Node::Struct(s)),
                Type::Optional(o) => Some(Node::Opt(o)),
                Type::ErrorUnion(e) => Some(Node::ErrU(e)),
                Type::Enum(e) => Some(Node::Enum(e)),
                Type::Union(u) => Some(Node::Union(u)),
                Type::Array(a) => Some(Node::Array(a)),
                Type::Slice(s) => Some(Node::Slice(s)),
                // A pointer needs no typedef of its own, but the type it points
                // to must still be declared first — the C `T*` spelling names
                // that typedef — so a `*T` field's dependency is `T`'s. (A
                // self/mutually-recursive pointer is broken by the `seen` set;
                // genuinely recursive types are out of scope for v0.118.)
                Type::Ptr(p) => dep_of(structs.ptr_pointee(p), structs),
                _ => None,
            }
        }
        fn visit(
            n: Node,
            structs: &crate::types::StructTable,
            seen: &mut HashSet<Node>,
            order: &mut Vec<Node>,
        ) {
            if !seen.insert(n) {
                return;
            }
            match n {
                Node::Struct(s) => {
                    for (_, fty) in &structs.get(s).fields {
                        if let Some(d) = dep_of(*fty, structs) {
                            visit(d, structs, seen, order);
                        }
                    }
                }
                Node::Opt(o) => {
                    if let Some(d) = dep_of(structs.optional_inner(o), structs) {
                        visit(d, structs, seen, order);
                    }
                }
                Node::ErrU(e) => {
                    if let Some(d) = dep_of(structs.error_union_payload(e), structs) {
                        visit(d, structs, seen, order);
                    }
                }
                // A plain enum embeds nothing by value: it is a graph leaf.
                Node::Enum(_) => {}
                // A tagged union embeds each variant's payload type by value
                // (inside its `data` union), so every payload must be declared
                // first — exactly like a struct's fields.
                Node::Union(u) => {
                    for (_, pty) in &structs.union_get(u).variants {
                        if let Some(d) = dep_of(*pty, structs) {
                            visit(d, structs, seen, order);
                        }
                    }
                }
                // An array `[N]T` embeds its element type `T` by value.
                Node::Array(a) => {
                    if let Some(d) = dep_of(structs.array_elem(a), structs) {
                        visit(d, structs, seen, order);
                    }
                }
                // A slice `[]T` embeds (the C name of) its element type `T`.
                Node::Slice(s) => {
                    if let Some(d) = dep_of(structs.slice_elem(s), structs) {
                        visit(d, structs, seen, order);
                    }
                }
            }
            order.push(n);
        }

        let mut seen = HashSet::new();
        let mut order = Vec::new();
        // Enums first: they have no dependencies, and a struct/optional/error
        // union that embeds one will already have pulled it in by the time it
        // is visited (so it is never emitted twice).
        for (id, _) in structs.enums() {
            visit(Node::Enum(id), structs, &mut seen, &mut order);
        }
        for (id, _) in structs.iter() {
            visit(Node::Struct(id), structs, &mut seen, &mut order);
        }
        for (id, _) in structs.optionals() {
            visit(Node::Opt(id), structs, &mut seen, &mut order);
        }
        for (id, _) in structs.error_unions() {
            visit(Node::ErrU(id), structs, &mut seen, &mut order);
        }
        for (id, _) in structs.unions() {
            visit(Node::Union(id), structs, &mut seen, &mut order);
        }
        for (id, _, _) in structs.arrays() {
            visit(Node::Array(id), structs, &mut seen, &mut order);
        }
        for (id, _) in structs.slices() {
            visit(Node::Slice(id), structs, &mut seen, &mut order);
        }

        for n in order {
            match n {
                Node::Struct(id) => self.emit_one_struct(id),
                Node::Opt(id) => self.emit_one_optional(id),
                Node::ErrU(id) => self.emit_one_error_union(id),
                Node::Enum(id) => self.emit_one_enum(id),
                Node::Union(id) => self.emit_one_union(id),
                Node::Array(id) => self.emit_one_array(id),
                Node::Slice(id) => self.emit_one_slice(id),
            }
        }
        // v0.141 (SPEC §35.2): the `@panic` runtime trap, emitted here — at the
        // tail of the type-def section — because it takes the panic message as a
        // `[]u8` slice (`kd_slice_uint8_t`) and so must follow that typedef. It is
        // emitted only when the module actually uses `@panic` (a `[]u8` slice may
        // exist for plain strings without any panic, and its `fwrite` must not
        // leak into such programs); the `[]u8` guard is then always satisfied,
        // since `@panic`'s message argument is a `[]u8`. (`kd_unreachable`, which
        // needs no typedef, lives in the prelude.)
        if self.uses_panic && self.structs.slices().any(|(_, e)| e == Type::U8) {
            self.line(
                "_Noreturn void kd_panic(kd_slice_uint8_t m) { fwrite(m.ptr, 1, m.len, stderr); fputc(0x0a, stderr); exit(101); }",
            );
        }
        self.blank();
    }

    /// Emit one `kd_enum_<Name>` typedef. Each variant becomes a C enumerator
    /// `kd_enum_<Name>_<Variant>` with its explicit 0-based index, so the C
    /// value matches the variant's declaration order regardless of how the
    /// typedef is later reordered. An enum carries no by-value dependencies.
    fn emit_one_enum(&mut self, id: u32) {
        let structs = self.structs;
        let info = structs.enum_get(id);
        let cname = structs.enum_c_name(id);
        let body = if info.variants.is_empty() {
            // A variant-less enum is degenerate (sema rejects it), but an empty
            // C `enum {}` is invalid; give it one placeholder enumerator so the
            // emitted source always compiles.
            format!("{}__empty = 0", cname)
        } else {
            info.variants
                .iter()
                .enumerate()
                .map(|(i, v)| format!("{} = {}", structs.enum_variant_c_name(id, v), i))
                .collect::<Vec<_>>()
                .join(", ")
        };
        self.line(&format!("typedef enum {{ {} }} {};", body, cname));
    }

    /// Emit one tagged-union typedef (SPEC §20.3): a struct carrying an
    /// `int32_t tag` (the active variant's 0-based index) and an anonymous C
    /// `union` of every variant payload, keyed by `data.kd_<variant>`:
    /// ```c
    /// typedef struct { int32_t tag; union { <T1 cty> kd_<v1>; ... } data; } kd_union_<Name>;
    /// ```
    /// A union depends on every payload type (pulled in first by the dependency
    /// walk). Sema requires at least one variant; a degenerate empty union would
    /// be invalid C (`union { }`), so a placeholder member keeps the output
    /// compilable in that impossible case.
    fn emit_one_union(&mut self, id: u32) {
        let structs = self.structs;
        let info = structs.union_get(id);
        let cname = structs.union_c_name(id);
        let body = if info.variants.is_empty() {
            "char _unused;".to_string()
        } else {
            info.variants
                .iter()
                .map(|(vname, pty)| format!("{} kd_{};", self.cty_of(*pty), vname))
                .collect::<Vec<_>>()
                .join(" ")
        };
        self.line(&format!(
            "typedef struct {{ int32_t tag; union {{ {} }} data; }} {};",
            body, cname
        ));
    }

    fn emit_one_struct(&mut self, id: u32) {
        let structs = self.structs;
        let info = structs.get(id);
        let body = if info.fields.is_empty() {
            "char _unused;".to_string()
        } else {
            info.fields
                .iter()
                .map(|(fname, fty)| format!("{} kd_{};", self.cty_of(*fty), fname))
                .collect::<Vec<_>>()
                .join(" ")
        };
        let cname = structs.c_name(id);
        self.line(&format!("typedef struct {{ {} }} {};", body, cname));
    }

    /// Emit one `kd_opt_<tag>` typedef plus its inline `_orelse` / `_unwrap`
    /// helpers. `_unwrap` panics (stderr + `exit(101)`) on null, per SPEC §11.3.
    fn emit_one_optional(&mut self, id: u32) {
        let structs = self.structs;
        let oname = structs.optional_c_name(id);
        let inner_cty = self.cty_of(structs.optional_inner(id));
        self.line(&format!(
            "typedef struct {{ bool has; {} val; }} {};",
            inner_cty, oname
        ));
        self.line(&format!(
            "static inline {} {}_orelse({} o, {} d) {{ return o.has ? o.val : d; }}",
            inner_cty, oname, oname, inner_cty
        ));
        self.line(&format!(
            "static inline {} {}_unwrap({} o) {{ if (!o.has) {{ fputs(\"panic: unwrapped a null optional\\n\", stderr); exit(101); }} return o.val; }}",
            inner_cty, oname, oname
        ));
    }

    /// Emit one `kd_err_<tag>` error-union typedef plus its inline `_catch`
    /// helper, per SPEC §12.3. The struct carries an `int32_t err` (0 = success,
    /// otherwise the failing error's 1-based code) and the payload `val`;
    /// `_catch` yields the payload on success or the eager default on error.
    fn emit_one_error_union(&mut self, id: u32) {
        let structs = self.structs;
        let ename = structs.error_union_c_name(id);
        let payload_cty = self.cty_of(structs.error_union_payload(id));
        self.line(&format!(
            "typedef struct {{ int32_t err; {} val; }} {};",
            payload_cty, ename
        ));
        self.line(&format!(
            "static inline {} {}_catch({} e, {} d) {{ return e.err == 0 ? e.val : d; }}",
            payload_cty, ename, ename, payload_cty
        ));
    }

    /// Emit one `kd_arr_<tag>_<N>` fixed-size-array typedef plus its inline
    /// bounds-checked `_get` helper, per SPEC §14.3. The array is a value type:
    /// wrapping the C array in a `struct { T data[N]; }` gives it C struct
    /// copy/pass/return semantics (so assignment, parameters and returns copy
    /// the whole array). `_get` panics (stderr + `exit(101)`) on an
    /// out-of-bounds index.
    fn emit_one_array(&mut self, id: u32) {
        let structs = self.structs;
        let elem_cty = self.cty_of(structs.array_elem(id));
        let len = structs.array_len(id);
        let cname = structs.array_c_name(id);
        self.line(&format!(
            "typedef struct {{ {} data[{}]; }} {};",
            elem_cty, len, cname
        ));
        self.line(&format!(
            "static inline {ec} {cn}_get({cn} a, int64_t i) {{ if (i < 0 || (uint64_t)i >= {n}) {{ fputs(\"panic: array index out of bounds\\n\", stderr); exit(101); }} return a.data[i]; }}",
            ec = elem_cty,
            cn = cname,
            n = len
        ));
    }

    /// Emit one `kd_slice_<tag>` slice typedef plus its inline bounds-checked
    /// `_get` helper, per SPEC §15.2. A slice is a non-owning `{ptr, len}` view
    /// over a backing array (or another slice); the backing storage's lifetime
    /// is the programmer's responsibility (raw, no borrow check). `_get` panics
    /// (stderr + `exit(101)`) on an out-of-bounds index.
    fn emit_one_slice(&mut self, id: u32) {
        let structs = self.structs;
        let elem_cty = self.cty_of(structs.slice_elem(id));
        let sname = structs.slice_c_name(id);
        self.line(&format!(
            "typedef struct {{ {} *ptr; uintptr_t len; }} {};",
            elem_cty, sname
        ));
        self.line(&format!(
            "static inline {ec} {sn}_get({sn} s, int64_t i) {{ if (i < 0 || (uint64_t)i >= s.len) {{ fputs(\"panic: slice index out of bounds\\n\", stderr); exit(101); }} return s.ptr[i]; }}",
            ec = elem_cty,
            sn = sname
        ));
        // The allocator helper for `[]T` (SPEC §16.2): `alloc(a, T, n)` lowers
        // to `<sn>_alloc(n)`, which `malloc`s `n` elements (panicking with
        // exit 101 on OOM for a non-zero `n`) and returns the owning view.
        self.line(&format!(
            "static inline {sn} {sn}_alloc(uintptr_t n) {{ {sn} s; s.ptr = malloc(n * sizeof({ec})); if (!s.ptr && n != 0) {{ fputs(\"panic: out of memory\\n\", stderr); exit(101); }} s.len = n; return s; }}",
            ec = elem_cty,
            sn = sname
        ));
    }

    /// Fold each top-level `const` initializer to a literal (C does not treat
    /// `const` objects as constant expressions) and emit it. Constants are
    /// processed in source order so later ones may reference earlier ones.
    fn emit_consts(&mut self, module: &Module) {
        let mut any = false;
        for item in &module.items {
            if let Item::Const(c) = item {
                // A v0.129 type-alias const — `const Alias = Name(C);` calling a
                // type-constructor (SPEC §25.3) — produced a struct, not a C
                // value, so it is NOT emitted as a C `const`. (Such a `Call`
                // initializer also fails `const_eval` below, so this is belt and
                // braces; the explicit skip keeps the intent clear.)
                if Self::is_type_alias_const(module, c) {
                    continue;
                }
                // The module is validated, so this evaluation always succeeds;
                // if it somehow does not we skip the const rather than panic.
                if let Ok(v) = crate::const_eval::eval(&c.value, &self.consts) {
                    // The C declaration type: the annotation when present, else
                    // the inferred type of the folded value (v0.121, SPEC §18.3).
                    // A `ConstVal::Int` infers `i64` (`int64_t`), a
                    // `ConstVal::Bool` infers `bool`.
                    let cty = match &c.ty {
                        Some(te) => self.cty(te),
                        None => self.cty_of(const_val_type(v)),
                    };
                    let lit = const_literal(v);
                    self.line(&format!("static const {} kd_{} = {};", cty, c.name, lit));
                    self.consts.insert(c.name.clone(), v);
                    any = true;
                }
            }
        }
        if any {
            self.blank();
        }
    }

    fn emit_forward_decls(&mut self, module: &Module) {
        let mut any = false;
        // Ordinary top-level functions first. A generic function is never
        // forward-declared under its plain name (SPEC §17.3) — only its
        // instances are (see `emit_instance_forward_decls`).
        for item in &module.items {
            if let Item::Func(f) = item {
                // A generic function is emitted per instantiation, never under
                // its plain name (§17.3); a type-constructor is compile-time only
                // and never emitted at all (§25.3).
                if Self::is_generic(f) || Self::is_type_ctor(f) {
                    continue;
                }
                let ret = self.cty(&f.ret);
                let params = self.format_params(&f.params);
                self.line(&format!("{} kd_{}({});", ret, f.name, params));
                any = true;
            }
        }
        // Forward-declare every generic instantiation alongside ordinary
        // functions, each under the active substitution (SPEC §17.3).
        if self.emit_instance_forward_decls() {
            any = true;
        }
        // Then every struct function, declared alongside ordinary ones. Each
        // lowers to a free C function `kd_<Struct>_<method>` whose `self`
        // parameter (if any) is an ordinary by-value struct parameter.
        for item in &module.items {
            if let Item::Struct(s) = item {
                // Bind `Self` so a `self: *Self` / `@This()` method signature
                // resolves in the forward declaration too (v0.136, §32.2).
                let self_id = self.structs.id_of(&s.name);
                if let Some(id) = self_id {
                    self.subst.insert("Self".to_string(), Type::Struct(id));
                }
                for m in &s.methods {
                    let ret = self.cty(&m.ret);
                    let params = self.format_params(&m.params);
                    self.line(&format!("{} kd_{}_{}({});", ret, s.name, m.name, params));
                    any = true;
                }
                if self_id.is_some() {
                    self.subst.remove("Self");
                }
            }
        }
        // Finally every generic-struct instance's methods (v0.130, SPEC §26.3),
        // forward-declared alongside ordinary struct methods so call order never
        // matters.
        if self.emit_struct_instance_forward_decls(module) {
            any = true;
        }
        if any {
            self.blank();
        }
    }

    /// Forward-declare every monomorphised generic-struct instance's methods
    /// (v0.130, SPEC §26.3). For each recorded [`crate::types::StructInstance`],
    /// the type-constructor's methods are declared under the substitution
    /// `{ type-param → arg, Self → Struct(id) }` (so their parameter / return
    /// types resolve to concrete types) and named `kd_<struct-name>_<method>` —
    /// matching the §10 struct-method lowering and [`Emitter::emit_method_call`].
    /// Mirrors [`Emitter::emit_instance_forward_decls`] for generic functions.
    /// Returns `true` if any were emitted. A fields-only instance (v0.129)
    /// declares no methods, so nothing is emitted for it.
    fn emit_struct_instance_forward_decls(&mut self, module: &Module) -> bool {
        let insts = self.structs.struct_instances().to_vec();
        let mut any = false;
        for inst in &insts {
            let ctor = match Self::instance_ctor(module, inst) {
                Some(c) => c,
                None => continue,
            };
            let methods = Self::ctor_methods(&ctor);
            if methods.is_empty() {
                continue;
            }
            let sname = self.structs.get(inst.struct_id).name.clone();
            self.set_instance_subst(&ctor, &inst.args, inst.struct_id);
            for m in &methods {
                let ret = self.cty(&m.ret);
                let params = self.format_params(&m.params);
                self.line(&format!("{} kd_{}_{}({});", ret, sname, m.name, params));
                any = true;
            }
            self.clear_subst();
        }
        any
    }

    fn emit_func_defs(&mut self, module: &Module) {
        // Ordinary top-level functions first, then struct functions, matching
        // the forward-declaration order. A generic function is not emitted under
        // its plain name (SPEC §17.3) — its instances are emitted below.
        for item in &module.items {
            if let Item::Func(f) = item {
                // Skip generic functions (emitted per instantiation, §17.3) and
                // type-constructors (compile-time only, never emitted, §25.3).
                if Self::is_generic(f) || Self::is_type_ctor(f) {
                    continue;
                }
                self.emit_func(f);
                self.blank();
            }
        }
        for item in &module.items {
            if let Item::Struct(s) = item {
                // Bind `Self` to this struct so a plain-struct method written
                // with `self: *Self` / `@This()` (v0.136, §32.2) resolves.
                let self_id = self.structs.id_of(&s.name);
                if let Some(id) = self_id {
                    self.subst.insert("Self".to_string(), Type::Struct(id));
                }
                for m in &s.methods {
                    let cname = format!("kd_{}_{}", s.name, m.name);
                    self.emit_func_named(m, &cname);
                    self.blank();
                }
                if self_id.is_some() {
                    self.subst.remove("Self");
                }
            }
        }
        // Emit one specialised C function per recorded instantiation (SPEC
        // §17.3), each under its concrete type-parameter substitution.
        self.emit_instance_defs();
        // Then one C function per generic-struct instance method (SPEC §26.3),
        // each under `{ type-param → arg, Self → Struct(id) }`.
        self.emit_struct_instance_defs(module);
    }

    /// Emit one C function per generic-struct instance method (v0.130, SPEC
    /// §26.3). For each recorded [`crate::types::StructInstance`], the
    /// type-constructor's methods are emitted via [`Emitter::emit_func_named`]
    /// (the §10 struct-method lowering) under the substitution `{ type-param →
    /// arg, Self → Struct(id) }`, so `Self` and the type parameter resolve to
    /// concrete types in the signature and body. A by-value `self: Self`
    /// parameter is an ordinary by-value struct parameter; the body reuses every
    /// existing statement / expression / `defer` lowering. The C name is
    /// `kd_<struct-name>_<method>`, matching the forward declaration and the
    /// call lowering. Mirrors [`Emitter::emit_instance_defs`] for generic
    /// functions; the type-constructor itself is never emitted (SPEC §25.3).
    fn emit_struct_instance_defs(&mut self, module: &Module) {
        let insts = self.structs.struct_instances().to_vec();
        for inst in &insts {
            let ctor = match Self::instance_ctor(module, inst) {
                Some(c) => c,
                None => continue,
            };
            let methods = Self::ctor_methods(&ctor);
            let sname = self.structs.get(inst.struct_id).name.clone();
            for m in &methods {
                self.set_instance_subst(&ctor, &inst.args, inst.struct_id);
                let cname = format!("kd_{}_{}", sname, m.name);
                self.emit_func_named(m, &cname);
                self.clear_subst();
                self.blank();
            }
        }
    }

    /// Forward-declare every recorded generic instantiation (SPEC §17.3), each
    /// under the instance's substitution so its runtime parameter types and
    /// return type resolve to concrete types. Returns `true` if any were
    /// emitted (so the caller can add the trailing blank line).
    fn emit_instance_forward_decls(&mut self) -> bool {
        let structs = self.structs;
        let insts = structs.instantiations().to_vec();
        let mut any = false;
        for inst in &insts {
            let f = match self.generics.get(&inst.fn_name).cloned() {
                Some(f) => f,
                // An instantiation of a function not present as a generic in
                // this module cannot occur for validated input; skip it.
                None => continue,
            };
            self.set_subst_for(&f, &inst.args);
            let ret = self.cty(&f.ret);
            let params = self.format_params(&f.params);
            let cname = self.structs.instantiation_c_name(inst);
            self.line(&format!("{} {}({});", ret, cname, params));
            self.clear_subst();
            any = true;
        }
        any
    }

    /// Emit one specialised C function body per recorded instantiation (SPEC
    /// §17.3). The substitution drives `cty` / `resolve_ty` / `type_of_expr`, so
    /// every type-parameter use in the runtime params, return type and body
    /// resolves to the concrete type; the body reuses all existing lowering.
    fn emit_instance_defs(&mut self) {
        let structs = self.structs;
        let insts = structs.instantiations().to_vec();
        for inst in &insts {
            let f = match self.generics.get(&inst.fn_name).cloned() {
                Some(f) => f,
                None => continue,
            };
            self.set_subst_for(&f, &inst.args);
            let cname = self.structs.instantiation_c_name(inst);
            self.emit_func_named(&f, &cname);
            self.clear_subst();
            self.blank();
        }
    }

    fn format_params(&self, params: &[Param]) -> String {
        // `comptime` type parameters are compile-time only — they never become
        // C parameters (SPEC §17.3). A generic instance emits its runtime
        // parameters under the active substitution; a fully-comptime parameter
        // list collapses to `void`.
        let runtime: Vec<&Param> = params.iter().filter(|p| !p.is_comptime).collect();
        if runtime.is_empty() {
            "void".to_string()
        } else {
            runtime
                .iter()
                .map(|p| format!("{} kd_{}", self.cty(&p.ty), p.name))
                .collect::<Vec<_>>()
                .join(", ")
        }
    }

    // -- type spelling ------------------------------------------------------

    /// Resolve a (validated) source type reference to a [`Type`]: a builtin via
    /// [`Type::from_name`], else a struct via the table, else `Void` for the
    /// impossible unresolved case so emission can never panic.
    fn resolve_ty(&self, t: &TypeExpr) -> Type {
        self.resolve_ty_in(t, &self.subst, &self.value_subst)
    }

    /// Resolve an array-size form to a concrete length, consulting `vsubst` for
    /// a comptime value-parameter size (`ArraySize::Param`, v0.128). A literal
    /// size resolves directly; an unbound parameter (impossible for validated
    /// input) falls back to `0` so emission never panics.
    fn array_size_in(&self, sz: &ArraySize, vsubst: &HashMap<String, i64>) -> usize {
        match sz {
            ArraySize::Lit(n) => *n as usize,
            ArraySize::Param(name) => vsubst.get(name).copied().unwrap_or(0) as usize,
        }
    }

    /// [`Emitter::array_size_in`] against the active value substitution. Used
    /// wherever an `ArraySize` is resolved under the current instance (e.g.
    /// `cty`), which is empty for non-generic code (so a literal size only).
    fn array_size(&self, sz: &ArraySize) -> usize {
        self.array_size_in(sz, &self.value_subst)
    }

    /// Like [`Emitter::resolve_ty`] but consults explicit substitutions: `subst`
    /// for the base type name (a comptime type parameter, v0.120) and `vsubst`
    /// for a comptime value-parameter array size (`[n]T`, v0.128). Used by the
    /// immutable `type_of_expr` / generic-call lowering to resolve a generic
    /// call's substituted types without touching `self.subst`/`self.value_subst`.
    fn resolve_ty_in(
        &self,
        t: &TypeExpr,
        subst: &HashMap<String, Type>,
        vsubst: &HashMap<String, i64>,
    ) -> Type {
        let base = self.base_type_in(&t.name, subst);
        if let Some(sz) = &t.array_len {
            // sema interned every `[N]T`; map the (element, length) pair back to
            // its `Type::Array(id)`. (`base` is the element type — `t.name` is
            // the element name when `array_len` is set.) A `[n]T` size resolves
            // through `vsubst` to the bound value first (SPEC §24.2).
            let len = self.array_size_in(sz, vsubst);
            self.structs
                .arrays()
                .find(|(_, elem, l)| *elem == base && *l == len)
                .map(|(id, _, _)| Type::Array(id))
                .unwrap_or(base)
        } else if t.optional {
            // sema interned every `?T` that appears, so the table holds it; map
            // the base inner type back to its `Type::Optional(id)`.
            self.structs
                .optionals()
                .find(|(_, inner)| *inner == base)
                .map(|(id, _)| Type::Optional(id))
                .unwrap_or(base)
        } else if t.error_union {
            // Likewise sema interned every `!T`; map the base payload back to
            // its `Type::ErrorUnion(id)`.
            self.structs
                .error_unions()
                .find(|(_, payload)| *payload == base)
                .map(|(id, _)| Type::ErrorUnion(id))
                .unwrap_or(base)
        } else if t.pointer {
            // `*T`: the pointee `base` was registered by `collect_ptr_types`, so
            // map it to a `Type::Ptr` with an emit-local id (offset by
            // `PTR_LOCAL_BASE` so it never collides with the table's ids).
            self.local_ptr_pointees
                .iter()
                .position(|x| *x == base)
                .map(|i| Type::Ptr(PTR_LOCAL_BASE + i as u32))
                .unwrap_or(Type::Ptr(PTR_LOCAL_BASE))
        } else if t.slice {
            // `[]T`: sema interned every slice, so map the element `base` back to
            // its `Type::Slice(id)` (mirrors the array/optional/error machinery).
            self.structs
                .slices()
                .find(|(_, elem)| *elem == base)
                .map(|(id, _)| Type::Slice(id))
                .unwrap_or(base)
        } else {
            base
        }
    }

    /// Resolve a bare source type name to a [`Type`]: a builtin via
    /// [`Type::from_name`], else a struct, else an enum, else `Void` for the
    /// impossible unresolved case. Shared by `resolve_ty` (for the base of a
    /// composite type) and by the pointer pre-pass.
    fn base_type(&self, name: &str) -> Type {
        self.base_type_in(name, &self.subst)
    }

    /// Like [`Emitter::base_type`] but consults an explicit `subst`. A name
    /// bound in `subst` is a generic type parameter and resolves to its
    /// concrete [`Type`] (SPEC §17.2); otherwise normal resolution applies.
    fn base_type_in(&self, name: &str, subst: &HashMap<String, Type>) -> Type {
        if let Some(&t) = subst.get(name) {
            return t;
        }
        Type::from_name(name)
            .or_else(|| self.structs.id_of(name).map(Type::Struct))
            .or_else(|| self.structs.enum_id_of(name).map(Type::Enum))
            .or_else(|| self.structs.union_id_of(name).map(Type::Union))
            // A type-alias name (`const IL = List(i32);`, v0.129) resolves to
            // the aliased type (a monomorphised struct).
            .or_else(|| self.structs.alias_of(name))
            .unwrap_or(Type::Void)
    }

    /// The pointee of a `Type::Ptr(id)`, whether `id` is an emit-local id (from
    /// `resolve_ty` / `&place`) or a table id (from a struct field, slice
    /// element, …). See [`PTR_LOCAL_BASE`].
    fn ptr_pointee_any(&self, id: u32) -> Type {
        if id >= PTR_LOCAL_BASE {
            self.local_ptr_pointees
                .get((id - PTR_LOCAL_BASE) as usize)
                .copied()
                .unwrap_or(Type::Void)
        } else {
            self.structs.ptr_pointee(id)
        }
    }

    /// The C type spelling for a resolved [`Type`]: a struct resolves through
    /// the table (`Type::c_name` would panic on it), an optional through
    /// `optional_c_name`; primitives use their builtin C name.
    /// A human-readable source name for a type, for `@typeName` (v0.136): a
    /// struct's interned name, else the primitive spelling.
    fn type_display_name(&self, t: Type) -> String {
        match t {
            Type::Struct(id) => self.structs.get(id).name.clone(),
            _ => t.name().to_string(),
        }
    }

    fn cty_of(&self, t: Type) -> String {
        match t {
            Type::Struct(id) => self.structs.c_name(id),
            Type::Optional(id) => self.structs.optional_c_name(id),
            Type::ErrorUnion(id) => self.structs.error_union_c_name(id),
            Type::Enum(id) => self.structs.enum_c_name(id),
            Type::Union(id) => self.structs.union_c_name(id),
            Type::Array(id) => self.structs.array_c_name(id),
            // `*T` has no typedef: its C spelling is `<pointee cty>*`.
            Type::Ptr(id) => format!("{}*", self.cty_of(self.ptr_pointee_any(id))),
            Type::Slice(id) => self.structs.slice_c_name(id),
            other => other.c_name().to_string(),
        }
    }

    /// The C type spelling for a source type reference. Builtins map through
    /// [`Type::c_name`]; struct names resolve to `kd_struct_<Name>` via the
    /// table; an unresolvable name (never reached for a validated module) falls
    /// back to `int64_t`.
    fn cty(&self, t: &TypeExpr) -> String {
        // A name bound in the active substitution is a generic type parameter:
        // resolve it to the concrete type, then apply the composite wrappers
        // below exactly as for an ordinary base type (SPEC §17.3).
        let base = if let Some(&s) = self.subst.get(&t.name) {
            s
        } else if let Some(prim) = Type::from_name(&t.name) {
            prim
        } else if let Some(id) = self.structs.id_of(&t.name) {
            Type::Struct(id)
        } else if let Some(id) = self.structs.enum_id_of(&t.name) {
            Type::Enum(id)
        } else if let Some(id) = self.structs.union_id_of(&t.name) {
            Type::Union(id)
        } else if let Some(ty) = self.structs.alias_of(&t.name) {
            // A type-alias name (`const IL = List(i32);`, v0.129).
            ty
        } else {
            Type::I64
        };
        if let Some(sz) = &t.array_len {
            // Matches `array_c_name(id)` (`kd_arr_<type_mangle(elem)>_<N>`)
            // without needing the interned id; `base` is the element type. A
            // `[n]T` size resolves through the active value substitution (SPEC
            // §24.3) so an instance's `[n]i32` spells `kd_arr_int32_t_<value>`.
            format!(
                "kd_arr_{}_{}",
                self.structs.type_mangle(base),
                self.array_size(sz)
            )
        } else if t.optional {
            // Matches `optional_c_name(id)` (`kd_opt_<type_mangle(inner)>`)
            // without needing the interned id.
            format!("kd_opt_{}", self.structs.type_mangle(base))
        } else if t.error_union {
            // Matches `error_union_c_name(id)` (`kd_err_<type_mangle(payload)>`).
            format!("kd_err_{}", self.structs.type_mangle(base))
        } else if t.pointer {
            // `*T` needs no typedef — its C spelling is `<pointee cty>*` and the
            // id is irrelevant to the name (`base` is the pointee here).
            format!("{}*", self.cty_of(base))
        } else if t.slice {
            // Matches `slice_c_name(id)` (`kd_slice_<type_mangle(elem)>`) without
            // needing the interned id; `base` is the element type.
            format!("kd_slice_{}", self.structs.type_mangle(base))
        } else {
            self.cty_of(base)
        }
    }

    // -- functions ----------------------------------------------------------

    fn emit_func(&mut self, f: &Func) {
        let cname = format!("kd_{}", f.name);
        self.emit_func_named(f, &cname);
    }

    /// Emit a function definition under the C name `c_name`. Ordinary functions
    /// pass `kd_<name>`; struct functions pass `kd_<Struct>_<method>` (so a
    /// `self` parameter is just an ordinary by-value struct parameter and the
    /// body reuses every statement/expr/`defer` lowering unchanged). Struct-
    /// typed parameters are recorded in the function scope so a method call on
    /// one of them resolves to its struct.
    fn emit_func_named(&mut self, f: &Func, c_name: &str) {
        self.scopes.clear();
        self.try_counter = 0;
        self.idx_counter = 0;
        self.if_counter = 0;
        self.str_counter = 0;
        self.for_counter = 0;
        self.current_ret = self.resolve_ty(&f.ret);
        let ret = self.cty(&f.ret);
        let params = self.format_params(&f.params);
        self.line(&format!("{} {}({}) {{", ret, c_name, params));
        let mut scope = Scope::function();
        for p in &f.params {
            // `comptime` parameters are not C value bindings (`format_params`
            // drops them). A comptime *type* parameter (`comptime T: type`) is
            // not a value at all and is skipped. A comptime *value* parameter
            // (`comptime n: usize`, v0.128) IS a compile-time constant of its
            // declared type: record its (substituted) type so a body reference
            // coerces correctly, while `emit_expr` substitutes the bound literal
            // in its place (so it never reads a non-existent C variable).
            if p.is_comptime {
                if Self::is_value_param(p) {
                    let pty = self.resolve_ty(&p.ty);
                    scope.var_types.insert(p.name.clone(), pty);
                }
                continue;
            }
            let pty = self.resolve_ty(&p.ty);
            scope.var_types.insert(p.name.clone(), pty);
        }
        self.emit_block(&f.body, scope);
        self.line("}");
    }

    // -- blocks -------------------------------------------------------------

    /// Emit the statements of `block` inside a fresh `scope`, then — if control
    /// fell through the end — flush that scope's defers (and, for a loop body,
    /// run the continue-expression). Returns `true` if the block diverged
    /// (ended in a `return`/`break`/`continue`), so callers can suppress the
    /// otherwise-mandatory fall-through flush. The opening `{` and closing `}`
    /// lines are emitted by the caller.
    fn emit_block(&mut self, block: &Block, scope: Scope) -> bool {
        self.indent += 1;
        self.scopes.push(scope);
        let mut diverged = false;
        for stmt in &block.stmts {
            diverged = self.emit_stmt(stmt);
            if diverged {
                break;
            }
        }
        if !diverged {
            self.flush_current_reversed(false);
            // A loop body runs its continue-clause at the end of each iteration
            // (the fall-through edge), after the body's defers.
            if self.scopes.last().map(|s| s.is_loop_body).unwrap_or(false) {
                let idx = self.scopes.len() - 1;
                self.emit_loop_cont(idx);
            }
        }
        self.scopes.pop();
        self.indent -= 1;
        diverged
    }

    // -- statements ---------------------------------------------------------

    /// Build the C store statement for a (possibly compound) assignment to an
    /// already side-effect-free / hoisted lvalue `target` (SPEC §27.3). A plain
    /// `=` (`op == None`) is `target = (val);`. A compound `op=` re-spells the
    /// place on both sides — `target = target <c-op> (val);` — which is correct
    /// precisely because `target` is side-effect-free (a var/field read, or the
    /// already-hoisted `__kd_idx` slot), so re-reading it does not re-evaluate
    /// any sub-expression.
    fn store_str(target: &str, op: Option<BinOp>, val: &str) -> String {
        match op {
            None => format!("{} = ({});", target, val),
            Some(binop) => format!("{} = {} {} ({});", target, target, binop.c_op(), val),
        }
    }

    /// Emit one statement. Returns `true` if it unconditionally transfers
    /// control (`return`/`break`/`continue`, or an `if`/block all of whose
    /// paths do).
    fn emit_stmt(&mut self, s: &Stmt) -> bool {
        match s {
            Stmt::Let {
                is_const,
                name,
                ty,
                value,
                ..
            } => {
                // The binding's type: the annotation when present, else the
                // inferred type of the initializer (v0.121, SPEC §18.3). A
                // validated inferred binding always has an inferable initializer
                // (`type_of_expr` returns `Some`); the `i64` fallback only guards
                // the impossible un-inferable case so emission never panics.
                let lty = match ty {
                    Some(te) => self.resolve_ty(te),
                    None => self.type_of_expr(value).unwrap_or(Type::I64),
                };
                // Coerce the initializer to that type (a `T` value, `null` or
                // `error.X` widens to `?T` / `!T` when annotated). For an inferred
                // binding the initializer already has type `lty`, so the coercion
                // is a no-op.
                let es = if let Expr::Try { expr, .. } = value {
                    // `var x = try e;` hoists the error propagation (which may
                    // early-return) and binds the unwrapped payload.
                    let payload = self.emit_try(expr);
                    self.coerce_str(&payload, self.try_payload_type(expr), lty)
                } else {
                    self.emit_coerced(value, lty)
                };
                // The C declaration type mirrors `lty`: the annotation's spelling
                // when present, else the inferred type's spelling.
                let ct = match ty {
                    Some(te) => self.cty(te),
                    None => self.cty_of(lty),
                };
                let prefix = if *is_const { "const " } else { "" };
                self.line(&format!("{}{} kd_{} = {};", prefix, ct, name, es));
                // Record the local's type so a later method call / coercion on
                // it resolves correctly.
                if let Some(scope) = self.scopes.last_mut() {
                    scope.var_types.insert(name.clone(), lty);
                }
                false
            }
            Stmt::Assign {
                name, op, value, ..
            } => {
                // The target is an assignable `var` local; coerce the RHS to its
                // declared type (so a `T`/`null` RHS widens to a `?T` target).
                let es = match self.lookup_var_type(name) {
                    Some(t) => self.emit_coerced(value, t),
                    None => self.emit_expr(value),
                };
                match op {
                    // Plain `=` (SPEC §4.3): `kd_<name> = <rhs>;`.
                    None => self.line(&format!("kd_{} = {};", name, es)),
                    // Compound `op=` (SPEC §27.3): `name op= rhs` ≡ `name = name op
                    // rhs`. A var read is free, so the place is just re-spelled on
                    // the RHS — `kd_<name> = kd_<name> <c-op> (<rhs>);`.
                    Some(binop) => self.line(&format!(
                        "kd_{} = kd_{} {} ({});",
                        name,
                        name,
                        binop.c_op(),
                        es
                    )),
                }
                false
            }
            Stmt::FieldAssign {
                place, op, value, ..
            } => {
                match place {
                    Expr::Index { base, index, .. } => {
                        // Index-assignment → a bounds-checked block: the index is
                        // hoisted into a fresh temporary, checked, then stored.
                        // The value is coerced to the element type so a
                        // `T`-coercible value widens. An array writes through
                        // `.data` and a fixed length (SPEC §14.3); a slice writes
                        // through `.ptr` and the runtime `.len` (SPEC §15.2).
                        //
                        // For a compound `a[i] op= e` (SPEC §27.3) the hoisted
                        // `__kd_idx{k}` is the *single* evaluation of the index, so
                        // re-spelling the element access on both sides of the
                        // store reads and writes the same slot without
                        // re-evaluating `i` — `target = target <c-op> (val);`.
                        let k = self.idx_counter;
                        self.idx_counter += 1;
                        let idx = self.emit_expr(index);
                        let base_str = self.emit_expr(base);
                        if let Some(Type::Slice(sid)) = self.type_of_expr(base) {
                            let val = self.emit_coerced(value, self.structs.slice_elem(sid));
                            let target = format!("({base}).ptr[__kd_idx{k}]", base = base_str, k = k);
                            let store = Self::store_str(&target, *op, &val);
                            self.line(&format!(
                                "{{ int64_t __kd_idx{k} = ({idx}); if (__kd_idx{k} < 0 || (uint64_t)__kd_idx{k} >= ({base}).len) {{ fputs(\"panic: slice index out of bounds\\n\", stderr); exit(101); }} {store} }}",
                                k = k,
                                idx = idx,
                                base = base_str,
                                store = store
                            ));
                        } else {
                            let (len, elem_ty) = match self.type_of_expr(base) {
                                Some(Type::Array(aid)) => {
                                    (self.structs.array_len(aid), Some(self.structs.array_elem(aid)))
                                }
                                // Unreachable for validated input (`base` is an array).
                                _ => (0, None),
                            };
                            let val = match elem_ty {
                                Some(t) => self.emit_coerced(value, t),
                                None => self.emit_expr(value),
                            };
                            let target = format!("({base}).data[__kd_idx{k}]", base = base_str, k = k);
                            let store = Self::store_str(&target, *op, &val);
                            self.line(&format!(
                                "{{ int64_t __kd_idx{k} = ({idx}); if (__kd_idx{k} < 0 || (uint64_t)__kd_idx{k} >= {len}) {{ fputs(\"panic: array index out of bounds\\n\", stderr); exit(101); }} {store} }}",
                                k = k,
                                idx = idx,
                                len = len,
                                store = store
                            ));
                        }
                    }
                    Expr::Deref { expr, .. } => {
                        // Deref-assignment `p.* = e;` → `*(<p>) = (<e>);` (SPEC
                        // §15.1). Coerce the RHS to the pointee type (the type of
                        // the `Deref` place). The pointer expression is
                        // side-effect-free, so a compound `p.* op= e` re-spells the
                        // dereference on both sides (SPEC §27.3).
                        let inner = self.emit_expr(expr);
                        let es = match self.type_of_expr(place) {
                            Some(t) => self.emit_coerced(value, t),
                            None => self.emit_expr(value),
                        };
                        let target = format!("*({})", inner);
                        self.line(&Self::store_str(&target, *op, &es));
                    }
                    _ => {
                        // `place` is a field-access chain (`a.b.c`); lowering it
                        // yields a C lvalue, so the assignment is a plain
                        // `(<place>) = (<value>);`. Coerce the RHS to the field's
                        // type (widening to `?T` if it is an optional field). A
                        // field access is side-effect-free, so a compound
                        // `s.f op= e` re-spells the place on both sides (SPEC §27.3).
                        let ps = self.emit_expr(place);
                        let es = match self.type_of_expr(place) {
                            Some(t) => self.emit_coerced(value, t),
                            None => self.emit_expr(value),
                        };
                        let target = format!("({})", ps);
                        self.line(&Self::store_str(&target, *op, &es));
                    }
                }
                false
            }
            Stmt::Expr(e) => self.emit_expr_stmt(e),
            Stmt::Return { value, .. } => {
                self.emit_return(value);
                true
            }
            Stmt::If {
                cond,
                capture,
                then,
                els,
                ..
            } => match capture {
                Some(name) => self.emit_if_capture(cond, name, then, els),
                None => self.emit_if(cond, then, els),
            },
            Stmt::While {
                cond, cont, body, ..
            } => {
                let cs = self.emit_expr(cond);
                self.line(&format!("while ({}) {{", cs));
                let cont_stmt = cont.as_ref().map(|b| (**b).clone());
                self.emit_block(body, Scope::loop_body(cont_stmt));
                self.line("}");
                // A `while` may iterate zero times or `break`, so the loop
                // statement itself never diverges.
                false
            }
            Stmt::For {
                iter,
                elem,
                index,
                body,
                ..
            } => self.emit_for(iter, elem, index, body),
            Stmt::Break(_) => {
                self.flush_to_loop_reversed();
                self.line("break;");
                true
            }
            Stmt::Continue(_) => {
                if let Some(i) = self.flush_to_loop_reversed() {
                    self.emit_loop_cont(i);
                }
                self.line("continue;");
                true
            }
            Stmt::Defer { stmt, .. } => {
                // Register only; the body runs at every scope exit (LIFO).
                if let Some(scope) = self.scopes.last_mut() {
                    scope.defers.push((false, (**stmt).clone()));
                }
                false
            }
            Stmt::ErrDefer { stmt, .. } => {
                // Register tagged as errdefer; runs only on error-return edges.
                if let Some(scope) = self.scopes.last_mut() {
                    scope.defers.push((true, (**stmt).clone()));
                }
                false
            }
            Stmt::Block(b) => self.emit_block(b, Scope::plain()),
            Stmt::Switch {
                scrutinee,
                arms,
                default,
                ..
            } => self.emit_switch(scrutinee, arms, default),
        }
    }

    /// Lower a `switch` to a C `switch`. The scrutinee's type resolves bare
    /// enum-literal labels (`.V`) to their C enumerator; qualified `Enum.V`
    /// labels and integer labels lower directly. Each arm's labels share one
    /// body block (reusing [`Emitter::emit_block`], so `defer`s inside an arm
    /// flush at the arm's block exit); a `break;` ends every arm so control
    /// never falls through to the next. An `else` arm becomes `default:`; sema
    /// proves enum switches exhaustive, so no `default:` is emitted otherwise.
    ///
    /// Returns `true` (diverges) when the switch is *total* and every arm — and
    /// the `else`, if present — diverges. A validated switch is always total
    /// (an enum switch is exhaustive or has an `else`; an integer switch
    /// requires an `else`), so this mirrors the `if`/`else` divergence rule.
    fn emit_switch(
        &mut self,
        scrutinee: &Expr,
        arms: &[SwitchArm],
        default: &Option<Block>,
    ) -> bool {
        let scrut_ty = self.type_of_expr(scrutinee);
        // A union scrutinee switches on the runtime `.tag` and may capture the
        // active variant's payload — a distinct lowering (SPEC §20.3).
        if let Some(Type::Union(uid)) = scrut_ty {
            return self.emit_union_switch(scrutinee, uid, arms, default);
        }
        let scrut = self.emit_expr(scrutinee);
        self.line(&format!("switch ({}) {{", scrut));
        self.indent += 1;
        let mut all_diverge = true;
        for arm in arms {
            let n = arm.labels.len();
            for (i, label) in arm.labels.iter().enumerate() {
                let lc = self.emit_switch_label(label, scrut_ty);
                // The final label of the arm opens the shared body block.
                if i + 1 < n {
                    self.line(&format!("case {}:", lc));
                } else {
                    self.line(&format!("case {}: {{", lc));
                }
            }
            // A label-less arm cannot be produced by the parser; guard so the
            // emitted C stays brace-balanced if one ever appears.
            if n == 0 {
                self.line("{");
            }
            let d = self.emit_block(&arm.body, Scope::plain());
            self.line("} break;");
            all_diverge = all_diverge && d;
        }
        if let Some(else_body) = default {
            self.line("default: {");
            let d = self.emit_block(else_body, Scope::plain());
            self.line("} break;");
            all_diverge = all_diverge && d;
        }
        self.indent -= 1;
        self.line("}");
        let total = default.is_some() || matches!(scrut_ty, Some(Type::Enum(_)));
        total && all_diverge
    }

    /// The C `case` label text for one `switch` arm pattern. A bare enum
    /// literal `.V` takes its enum from the scrutinee's type; a qualified
    /// `Enum.V` (a `Field`) and an integer literal already lower to a valid
    /// integer-constant case label via [`Emitter::emit_expr`].
    fn emit_switch_label(&mut self, label: &Expr, scrut_ty: Option<Type>) -> String {
        if let Expr::EnumLit { variant, .. } = label {
            if let Some(Type::Enum(eid)) = scrut_ty {
                return self.structs.enum_variant_c_name(eid, variant);
            }
        }
        self.emit_expr(label)
    }

    /// Lower a `switch` over a tagged union (SPEC §20.3). The C switch dispatches
    /// on the scrutinee's `.tag`; each label `.v` becomes the variant's 0-based
    /// tag as an integer `case`. When an arm carries a `|cap|` capture, the
    /// matched variant's payload is bound first — `<payload cty> kd_<cap> =
    /// (<u>).data.kd_<v>;` — and recorded in the arm's scope so its uses inside
    /// the body resolve. Each arm ends with `break;`; an `else` becomes
    /// `default:` (sema proves a union switch exhaustive, so no `default:` is
    /// emitted otherwise). A validated union switch is total, so it diverges iff
    /// every arm (and the `else`, if present) does.
    fn emit_union_switch(
        &mut self,
        scrutinee: &Expr,
        uid: u32,
        arms: &[SwitchArm],
        default: &Option<Block>,
    ) -> bool {
        let scrut = self.emit_expr(scrutinee);
        self.line(&format!("switch (({}).tag) {{", scrut));
        self.indent += 1;
        let mut all_diverge = true;
        for arm in arms {
            let n = arm.labels.len();
            // The variant named by the arm's (first) label drives both the tag
            // and, for a captured arm, the union member the payload is read from.
            let mut variant: Option<String> = None;
            for (i, label) in arm.labels.iter().enumerate() {
                if variant.is_none() {
                    if let Expr::EnumLit { variant: v, .. } = label {
                        variant = Some(v.clone());
                    }
                }
                let idx = self.union_label_index(label, uid);
                if i + 1 < n {
                    self.line(&format!("case {}:", idx));
                } else {
                    self.line(&format!("case {}: {{", idx));
                }
            }
            // A label-less arm cannot be produced by the parser; guard so the
            // emitted C stays brace-balanced if one ever appears.
            if n == 0 {
                self.line("{");
            }
            // Bind the captured payload (if any) and seed the arm scope with its
            // type, so the body's uses of `cap` resolve through `type_of_expr`.
            let mut scope = Scope::plain();
            if let (Some(cap), Some(v)) = (&arm.capture, &variant) {
                let payload = self
                    .structs
                    .union_get(uid)
                    .payload_type(v)
                    .unwrap_or(Type::Void);
                let cty = self.cty_of(payload);
                self.indent += 1;
                self.line(&format!("{} kd_{} = ({}).data.kd_{};", cty, cap, scrut, v));
                self.indent -= 1;
                scope.var_types.insert(cap.clone(), payload);
            }
            let d = self.emit_block(&arm.body, scope);
            self.line("} break;");
            all_diverge = all_diverge && d;
        }
        if let Some(else_body) = default {
            self.line("default: {");
            let d = self.emit_block(else_body, Scope::plain());
            self.line("} break;");
            all_diverge = all_diverge && d;
        }
        self.indent -= 1;
        self.line("}");
        // A validated union switch is total (every variant, or an `else`).
        all_diverge
    }

    /// The integer `case` label for one union-switch arm pattern: a variant
    /// literal `.v` resolves to its 0-based tag index. A non-variant label is
    /// unreachable for validated input; it falls back to ordinary lowering.
    fn union_label_index(&mut self, label: &Expr, uid: u32) -> String {
        if let Expr::EnumLit { variant, .. } = label {
            if let Some(idx) = self.structs.union_get(uid).variant_index(variant) {
                return idx.to_string();
            }
        }
        self.emit_expr(label)
    }

    /// Emit a `while` continue-clause statement (an assignment or expression).
    /// The parser restricts it to those two shapes, and it carries no `defer`
    /// or control-flow concerns, so it is emitted directly without the scope
    /// machinery `emit_stmt` uses.
    fn emit_cont(&mut self, c: &Stmt) {
        match c {
            Stmt::Assign {
                name, op, value, ..
            } => {
                let es = match self.lookup_var_type(name) {
                    Some(t) => self.emit_coerced(value, t),
                    None => self.emit_expr(value),
                };
                match op {
                    None => self.line(&format!("kd_{} = {};", name, es)),
                    // A compound continue-clause (`while (..) : (i += 1)`) lowers
                    // like any other compound name-assign (SPEC §27.3).
                    Some(binop) => self.line(&format!(
                        "kd_{} = kd_{} {} ({});",
                        name,
                        name,
                        binop.c_op(),
                        es
                    )),
                }
            }
            Stmt::Expr(e) => {
                let es = self.emit_expr(e);
                self.line(&format!("{};", es));
            }
            // The parser only ever produces Assign/Expr in this position.
            other => {
                let dbg = format!("/* unexpected continue-clause: {:?} */", other);
                self.line(&dbg);
            }
        }
    }

    /// Emit the continue-clause(s) of the loop-body scope at `idx`: the AST
    /// `cont` of a `while (..) : (cont)` and/or the raw C `cont_raw` of a `for`
    /// (its index increment, SPEC §29.2). Run on every loop edge — fall-through
    /// off the body end and before each `continue` — so a `for`'s index always
    /// advances even when the iteration `continue`s. A `while` has only `cont`
    /// and a `for` only `cont_raw`, so this never double-emits.
    fn emit_loop_cont(&mut self, idx: usize) {
        if let Some(c) = self.scopes[idx].cont.clone() {
            self.emit_cont(&c);
        }
        if let Some(raw) = self.scopes[idx].cont_raw.clone() {
            self.line(&raw);
        }
    }

    /// Lower a `for (iter) |elem| { … }` / `for (iter, 0..) |elem, index| { … }`
    /// over an array (`[N]T`) or slice (`[]T`) to an indexed `while` (SPEC
    /// §29.2). The iterable is evaluated **once** into `__kd_for{N}`; a `usize`
    /// index `__kd_fi{N}` walks `0 .. <len>` (`<len>` is the runtime `.len` of a
    /// slice / the compile-time length of an array). Each iteration first binds
    /// the element **by value** (`<T> kd_<elem> = <access>;`) and, for the index
    /// form, `usize kd_<index> = __kd_fi{N};`, then the body.
    ///
    /// The body runs in a loop-body [`Scope`] (so `defer`/`break`/`continue`
    /// behave) whose `cont_raw` is the index increment, so a `continue` still
    /// advances the index. A `for` may iterate zero times, so it never diverges
    /// (returns `false`).
    fn emit_for(
        &mut self,
        iter: &Expr,
        elem: &str,
        index: &Option<String>,
        body: &Block,
    ) -> bool {
        // The iterable's type selects the element access (`.ptr[i]` for a slice,
        // `.data[i]` for an array) and the length form (the runtime `.len` of a
        // slice / the literal length of an array). Validated input is always an
        // array or slice; an unexpected shape emits nothing so emission never
        // panics on a malformed AST.
        let (iter_cty, elem_ty, array_len, access_member) = match self.type_of_expr(iter) {
            Some(Type::Slice(sid)) => (
                self.structs.slice_c_name(sid),
                self.structs.slice_elem(sid),
                None,
                "ptr",
            ),
            Some(Type::Array(aid)) => (
                self.structs.array_c_name(aid),
                self.structs.array_elem(aid),
                Some(self.structs.array_len(aid)),
                "data",
            ),
            _ => return false,
        };

        let n = self.for_counter;
        self.for_counter += 1;
        let temp = format!("__kd_for{}", n);
        let iv = format!("__kd_fi{}", n);
        let usize_cty = self.cty_of(Type::Usize);
        let elem_cty = self.cty_of(elem_ty);
        let iter_str = self.emit_expr(iter);

        // Outer block: evaluate the iterable once, then declare the walking index.
        self.line("{");
        self.indent += 1;
        self.line(&format!("{} {} = {};", iter_cty, temp, iter_str));
        self.line(&format!("{} {} = 0;", usize_cty, iv));
        let len = match array_len {
            Some(l) => l.to_string(),
            None => format!("{}.len", temp),
        };
        self.line(&format!("while ({} < {}) {{", iv, len));

        // The loop body is a loop-body scope (so `defer`/`break`/`continue`
        // behave); its `cont_raw` is the index increment, so `continue` still
        // advances the index (SPEC §29.2). Record the element/index binding
        // types so body uses (method calls, coercion) resolve correctly.
        let mut scope = Scope::loop_body(None);
        scope.cont_raw = Some(format!("{} += 1;", iv));
        scope.var_types.insert(elem.to_string(), elem_ty);
        if let Some(ix) = index {
            scope.var_types.insert(ix.clone(), Type::Usize);
        }
        self.scopes.push(scope);
        self.indent += 1;

        // First the by-value element binding, then (the index form) the index.
        self.line(&format!(
            "{} kd_{} = {}.{}[{}];",
            elem_cty, elem, temp, access_member, iv
        ));
        if let Some(ix) = index {
            self.line(&format!("{} kd_{} = {};", usize_cty, ix, iv));
        }

        // Emit the body statements; on fall-through flush the body's defers and
        // run the index increment (mirroring `emit_block`'s loop handling).
        let mut diverged = false;
        for s in &body.stmts {
            diverged = self.emit_stmt(s);
            if diverged {
                break;
            }
        }
        if !diverged {
            self.flush_current_reversed(false);
            let idx = self.scopes.len() - 1;
            self.emit_loop_cont(idx);
        }
        self.scopes.pop();

        self.indent -= 1;
        self.line("}"); // close the `while` body
        self.indent -= 1;
        self.line("}"); // close the outer block
        // A `for` may iterate zero times, so the loop statement never diverges.
        false
    }

    fn emit_expr_stmt(&mut self, e: &Expr) -> bool {
        // In test mode `expect(c)` is a statement-level construct that returns
        // a failure code through the deferred-return path.
        if self.mode == EmitMode::Test {
            if let Expr::Call { callee, args, .. } = e {
                if callee == "expect" {
                    let cs = match args.first() {
                        Some(a) => self.emit_expr(a),
                        None => "0".to_string(),
                    };
                    self.line(&format!("if (!({})) {{", cs));
                    self.indent += 1;
                    self.flush_all_reversed(false);
                    self.line("return 1;");
                    self.indent -= 1;
                    self.line("}");
                    return false;
                }
            }
        }
        // v0.141 runtime-safety traps as statements / switch arms (SPEC §35.2):
        // lower to the bare `_Noreturn` call (no `, 0` comma form) and report
        // divergence so the enclosing block stops — no fall-through flush.
        match e {
            Expr::Unreachable { .. } => {
                self.line("kd_unreachable();");
                return true;
            }
            Expr::Builtin { name, args, .. } if name == "panic" => {
                let msg = match args.first() {
                    Some(a) => self.emit_expr(a),
                    None => "((kd_slice_uint8_t){0})".to_string(),
                };
                self.line(&format!("kd_panic({});", msg));
                return true;
            }
            _ => {}
        }
        // `try e;` as a bare statement: hoist the propagation, discard the
        // unwrapped payload.
        if let Expr::Try { expr, .. } = e {
            let val = self.emit_try(expr);
            self.line(&format!("(void)({});", val));
            return false;
        }
        let es = self.emit_expr(e);
        self.line(&format!("{};", es));
        false
    }

    fn emit_return(&mut self, value: &Option<Expr>) {
        let ret_ty = self.current_ret;
        // Compute the (coerced) C return-value string, or `None` for `return;`.
        // A `return try e;` first hoists the error propagation — which itself
        // early-returns on error — then returns the unwrapped payload coerced
        // back to the (error-union) return type.
        let val_str: Option<String> = match value {
            None => None,
            Some(Expr::Try { expr, .. }) => {
                let payload = self.emit_try(expr);
                Some(self.coerce_str(&payload, self.try_payload_type(expr), ret_ty))
            }
            Some(e) => Some(self.emit_coerced(e, ret_ty)),
        };
        // A `return error.X;` is an error-return edge, so `errdefer`s run too.
        // (`return try e;` propagates errors inside `emit_try`; the value it
        // then returns is the success payload, so this return is not an error
        // edge.)
        let include_err = matches!(value, Some(Expr::ErrorLit { .. }));
        self.finish_return(val_str, ret_ty, include_err);
    }

    /// Emit the actual `return` (with the deferred-temp dance) from a
    /// pre-computed, already-coerced value string. Shared by ordinary returns
    /// and `return try e;`.
    fn finish_return(&mut self, val_str: Option<String>, ret_ty: Type, include_err: bool) {
        let non_void = ret_ty != Type::Void;
        let active = self.any_defer_active(include_err);
        if active && non_void {
            // Evaluate the value into a temporary *before* running the defers,
            // since the defers may mutate state the value depends on.
            let es = val_str.unwrap_or_else(|| "0".to_string());
            let ret = self.cty_of(ret_ty);
            self.line(&format!("{} __kd_ret = ({});", ret, es));
            self.flush_all_reversed(include_err);
            self.line("return __kd_ret;");
        } else {
            if active {
                self.flush_all_reversed(include_err);
            }
            match val_str {
                Some(es) => self.line(&format!("return ({});", es)),
                None => self.line("return;"),
            }
        }
    }

    /// Lower a `try inner` at a statement position: hoist `inner` (an `!T`) into
    /// a fresh `__kd_tryN` temporary, propagate the error out of the enclosing
    /// function (flushing active defers first, per SPEC §12.3), and return the C
    /// expression (`__kd_tryN.val`) that yields the unwrapped payload.
    fn emit_try(&mut self, inner: &Expr) -> String {
        let n = self.try_counter;
        self.try_counter += 1;
        let temp = format!("__kd_try{}", n);
        // The temp holds the inner expression's own error-union value.
        let err_cty = match self.type_of_expr(inner) {
            Some(t @ Type::ErrorUnion(_)) => self.cty_of(t),
            // Validated input always resolves the inner; fall back to the
            // enclosing function's error-union type (same `{err,val}` layout).
            _ => self.cty_of(self.current_ret),
        };
        let es = self.emit_expr(inner);
        self.line(&format!("{} {} = {};", err_cty, temp, es));
        self.line(&format!("if ({}.err != 0) {{", temp));
        self.indent += 1;
        // An error propagation is an error-return edge: run errdefers too.
        self.flush_all_reversed(true);
        let ret_cty = self.cty_of(self.current_ret);
        self.line(&format!("return ({}){{ .err = {}.err }};", ret_cty, temp));
        self.indent -= 1;
        self.line("}");
        format!("{}.val", temp)
    }

    /// The payload type `T` of a `try inner` (i.e. the inner `!T`'s payload),
    /// used to coerce the unwrapped value back into a wider position. Falls back
    /// to the enclosing function's payload, which `try` always matches.
    fn try_payload_type(&self, inner: &Expr) -> Type {
        match self.type_of_expr(inner) {
            Some(Type::ErrorUnion(id)) => self.structs.error_union_payload(id),
            _ => match self.current_ret {
                Type::ErrorUnion(id) => self.structs.error_union_payload(id),
                other => other,
            },
        }
    }

    /// Coerce a raw C-expression string of source type `src` to `expected`,
    /// mirroring [`Emitter::emit_coerced`] but for a value that is already a
    /// string (e.g. a `try` payload). Widens `T` to `?T` / `!T`; an already-wide
    /// value (or a non-optional/non-error target) passes through unchanged.
    fn coerce_str(&self, raw: &str, src: Type, expected: Type) -> String {
        match expected {
            Type::Optional(oid) => {
                if matches!(src, Type::Optional(_)) {
                    raw.to_string()
                } else {
                    let oname = self.structs.optional_c_name(oid);
                    format!("(({}){{ .has = true, .val = {} }})", oname, raw)
                }
            }
            Type::ErrorUnion(eid) => {
                if matches!(src, Type::ErrorUnion(_)) {
                    raw.to_string()
                } else {
                    let ename = self.structs.error_union_c_name(eid);
                    format!("(({}){{ .err = 0, .val = {} }})", ename, raw)
                }
            }
            _ => raw.to_string(),
        }
    }

    /// Emit an optional-capture `if (opt) |name| { … } else { … }` (SPEC §21.1).
    /// The optional is evaluated once into a temp; on `.has`, the unwrapped
    /// payload binds `kd_<name>` in the then-branch. Returns `false`
    /// (conservatively non-diverging).
    fn emit_if_capture(
        &mut self,
        cond: &Expr,
        name: &str,
        then: &Block,
        els: &Option<Box<Stmt>>,
    ) -> bool {
        // Resolve the optional's C type + inner payload type.
        let (opt_cty, inner_ty) = match self.type_of_expr(cond) {
            Some(Type::Optional(id)) => {
                (self.structs.optional_c_name(id), self.structs.optional_inner(id))
            }
            // Validated input always makes `cond` an optional here; fall back to
            // a plain `if` so emission never panics on unexpected shapes.
            _ => return self.emit_if(cond, then, els),
        };
        let n = self.if_counter;
        self.if_counter += 1;
        let temp = format!("__kd_if{}", n);
        let cs = self.emit_expr(cond);
        let inner_cty = self.cty_of(inner_ty);
        self.line("{");
        self.indent += 1;
        self.line(&format!("{} {} = {};", opt_cty, temp, cs));
        self.line(&format!("if ({}.has) {{", temp));
        self.indent += 1;
        // Bind the unwrapped payload inside a scope so the then-block resolves
        // its type and `defer`s flush at the branch's exit.
        let mut scope = Scope::plain();
        scope.var_types.insert(name.to_string(), inner_ty);
        self.scopes.push(scope);
        self.line(&format!("{} kd_{} = {}.val;", inner_cty, name, temp));
        let mut diverged = false;
        for s in &then.stmts {
            diverged = self.emit_stmt(s);
            if diverged {
                break;
            }
        }
        if !diverged {
            self.flush_current_reversed(false);
        }
        self.scopes.pop();
        self.indent -= 1;
        match els {
            Some(boxed) => {
                self.line("} else {");
                self.indent += 1;
                self.emit_stmt(boxed.as_ref());
                self.indent -= 1;
                self.line("}");
            }
            None => self.line("}"),
        }
        self.indent -= 1;
        self.line("}");
        false
    }

    /// Emit an `if`/`else if`/`else` chain. Returns `true` only if there is a
    /// final `else` and every arm diverges.
    fn emit_if(&mut self, cond: &Expr, then: &Block, els: &Option<Box<Stmt>>) -> bool {
        // Flatten the `else if` chain so we can emit one C `if`/`else if`
        // ladder with matching braces.
        let mut conds: Vec<&Expr> = vec![cond];
        let mut blocks: Vec<&Block> = vec![then];
        let mut else_block: Option<&Block> = None;
        let mut else_single: Option<&Stmt> = None;
        let mut cur = els;
        loop {
            match cur {
                None => break,
                Some(boxed) => match boxed.as_ref() {
                    // Only flatten capture-less `else if`s; an `else if (opt) |v|`
                    // capture must go through emit_stmt → emit_if_capture, so it
                    // is treated as an else-statement here.
                    Stmt::If {
                        cond,
                        capture: None,
                        then,
                        els,
                        ..
                    } => {
                        conds.push(cond);
                        blocks.push(then);
                        cur = els;
                    }
                    Stmt::Block(b) => {
                        else_block = Some(b);
                        break;
                    }
                    other => {
                        else_single = Some(other);
                        break;
                    }
                },
            }
        }

        let mut all_diverge = true;
        for i in 0..conds.len() {
            let cs = self.emit_expr(conds[i]);
            if i == 0 {
                self.line(&format!("if ({}) {{", cs));
            } else {
                self.line(&format!("}} else if ({}) {{", cs));
            }
            let d = self.emit_block(blocks[i], Scope::plain());
            all_diverge = all_diverge && d;
        }

        if let Some(b) = else_block {
            self.line("} else {");
            let d = self.emit_block(b, Scope::plain());
            self.line("}");
            all_diverge && d
        } else if let Some(s) = else_single {
            self.line("} else {");
            self.indent += 1;
            let d = self.emit_stmt(s);
            self.indent -= 1;
            self.line("}");
            all_diverge && d
        } else {
            self.line("}");
            // No `else`: control can skip every arm, so this does not diverge.
            false
        }
    }

    // -- defer flushing -----------------------------------------------------

    /// Whether any scope has a deferred statement that would run on this exit:
    /// for a normal exit (`include_err = false`) only plain `defer`s count; for
    /// an error-return edge (`include_err = true`) `errdefer`s count too.
    fn any_defer_active(&self, include_err: bool) -> bool {
        self.scopes
            .iter()
            .any(|s| s.defers.iter().any(|(is_err, _)| include_err || !is_err))
    }

    /// Emit one scope's deferred statements in reverse registration order,
    /// skipping `errdefer`s unless `include_err`.
    fn flush_scope(&mut self, idx: usize, include_err: bool) {
        let defers = self.scopes[idx].defers.clone();
        for (is_err, s) in defers.iter().rev() {
            if include_err || !is_err {
                self.emit_stmt(s);
            }
        }
    }

    /// Flush the innermost scope's defers in reverse registration order.
    fn flush_current_reversed(&mut self, include_err: bool) {
        if !self.scopes.is_empty() {
            self.flush_scope(self.scopes.len() - 1, include_err);
        }
    }

    /// Flush every active scope, innermost first down to the function scope,
    /// each in reverse registration order. Used by deferred `return`, a failed
    /// `expect`, and (with `include_err`) error-return edges.
    fn flush_all_reversed(&mut self, include_err: bool) {
        for i in (0..self.scopes.len()).rev() {
            self.flush_scope(i, include_err);
        }
    }

    /// Flush scopes innermost-first down to and including the nearest loop-body
    /// scope (each reversed). Returns that loop-body scope's index, or `None`
    /// if there is no enclosing loop (which a validated module never hits).
    /// `break`/`continue` are normal exits, so `errdefer`s never run here.
    fn flush_to_loop_reversed(&mut self) -> Option<usize> {
        let n = self.scopes.len();
        let mut loop_idx = None;
        for i in (0..n).rev() {
            if self.scopes[i].is_loop_body {
                loop_idx = Some(i);
                break;
            }
        }
        let loop_idx = loop_idx?;
        for i in (loop_idx..n).rev() {
            self.flush_scope(i, false);
        }
        Some(loop_idx)
    }

    // -- expressions --------------------------------------------------------

    /// Lower an expression to a C expression string. Binary and unary
    /// sub-expressions are fully parenthesized so C precedence can never alter
    /// the meaning.
    fn emit_expr(&mut self, e: &Expr) -> String {
        match e {
            Expr::Int { value, .. } => value.to_string(),
            Expr::Bool { value, .. } => {
                if *value {
                    "true".to_string()
                } else {
                    "false".to_string()
                }
            }
            // A string literal is a `[]u8` slice over static bytes (SPEC §23.2):
            // a `kd_slice_uint8_t` compound literal whose `.ptr` is the C string
            // literal of the (byte-escaped) bytes and whose `.len` is the decoded
            // byte count. The `kd_slice_uint8_t` typedef is emitted because sema
            // interned the `[]u8` slice as this expression's type.
            Expr::StrLit { value, .. } => {
                format!(
                    "((kd_slice_uint8_t){{ .ptr = (uint8_t *){}, .len = {} }})",
                    c_string_literal(value),
                    value.as_bytes().len()
                )
            }
            Expr::Builtin { name, args, .. } => {
                // The single argument names a type (an `Ident`); resolve it
                // (substitution-aware) to its C type / display name (SPEC §32.1).
                let arg_name = match args.first() {
                    Some(Expr::Ident { name, .. }) => name.clone(),
                    _ => String::new(),
                };
                let ty = self.base_type(&arg_name);
                match name.as_str() {
                    "as" => {
                        // `@as(T, e)` → a C cast `((T)(e))` (v0.137, §33). `ty`
                        // (resolved from the first arg above) is the target type;
                        // the second arg is the value.
                        let val = match args.get(1) {
                            Some(e) => self.emit_expr(e),
                            None => "0".to_string(),
                        };
                        format!("(({})({}))", self.cty_of(ty), val)
                    }
                    "sizeOf" => format!("sizeof({})", self.cty_of(ty)),
                    "typeName" => {
                        // Print the bound type's name for a type parameter, else
                        // the name as written (a builtin, struct, or alias).
                        let display = if self.subst.contains_key(&arg_name) {
                            self.type_display_name(ty)
                        } else {
                            arg_name.clone()
                        };
                        format!(
                            "((kd_slice_uint8_t){{ .ptr = (uint8_t *){}, .len = {} }})",
                            c_string_literal(&display),
                            display.as_bytes().len()
                        )
                    }
                    // `@panic(msg)` in expression position (SPEC §35.2): the
                    // comma-expression `(kd_panic(<msg>), 0)`. `kd_panic` is
                    // `_Noreturn` (it `exit(101)`s), so the trailing `0` is dead;
                    // it satisfies the type only of an integer value position (a
                    // non-integer position is a later refinement). A statement
                    // position lowers without the `, 0` via `emit_expr_stmt`.
                    "panic" => {
                        let msg = match args.first() {
                            Some(e) => self.emit_expr(e),
                            None => "((kd_slice_uint8_t){0})".to_string(),
                        };
                        format!("(kd_panic({}), 0)", msg)
                    }
                    // Unknown builtins are rejected by sema; emit a placeholder.
                    _ => "0".to_string(),
                }
            }
            Expr::Ident { name, .. } => {
                // A reference to a comptime value parameter (`comptime n: usize`,
                // v0.128) emits the bound literal value — the parameter is not a
                // real C variable (SPEC §24.3). Everything else is the ordinary
                // `kd_<name>` local/parameter reference.
                if let Some(v) = self.value_subst.get(name) {
                    v.to_string()
                } else {
                    format!("kd_{}", name)
                }
            }
            Expr::Unary { op, expr, .. } => {
                let inner = self.emit_expr(expr);
                let opc = match op {
                    UnOp::Neg => "-",
                    UnOp::Not => "!",
                    // Bitwise complement (v0.132): `~x`, mirroring `-`/`!`.
                    UnOp::BitNot => "~",
                };
                format!("({}{})", opc, inner)
            }
            Expr::Binary { op, lhs, rhs, .. } => {
                let l = self.emit_binop_operand(lhs, rhs);
                let r = self.emit_binop_operand(rhs, lhs);
                format!("({} {} {})", l, op.c_op(), r)
            }
            Expr::Call { callee, args, .. } => {
                if callee == "print" {
                    // `print` of a `[]u8` (a string) writes the raw bytes plus a
                    // newline (SPEC §23.2). The slice is hoisted into a fresh
                    // `__kd_strN` temporary so the slice expression — which may
                    // have side effects or be costly — is evaluated exactly once
                    // before `fwrite`. Any other slice/type keeps the integer
                    // `kd_print` path below (sema rejects a non-int, non-string
                    // `print`, so only these two cases reach emit).
                    if let Some(arg) = args.first() {
                        if let Some(Type::Slice(sid)) = self.type_of_expr(arg) {
                            if self.structs.slice_elem(sid) == Type::U8 {
                                let s = self.emit_expr(arg);
                                let n = self.str_counter;
                                self.str_counter += 1;
                                return format!(
                                    "{{ kd_slice_uint8_t __kd_str{n} = ({s}); fwrite(__kd_str{n}.ptr, 1, __kd_str{n}.len, stdout); fputc('\\n', stdout); }}",
                                    n = n,
                                    s = s
                                );
                            }
                        }
                    }
                    let a = match args.first() {
                        Some(a) => self.emit_expr(a),
                        None => "0".to_string(),
                    };
                    format!("kd_print((long long)({}))", a)
                } else if callee == "expect" {
                    // `expect` returns void and is handled at the statement
                    // level; it can never legitimately appear as a value, but
                    // emit a harmless no-op so output stays well-formed.
                    "((void)0)".to_string()
                } else if callee == "c_allocator" {
                    // The malloc/free-backed allocator value (SPEC §16.2). The
                    // struct carries no state in v0.119, so a zero-initialised
                    // compound literal is the whole allocator.
                    "((kd_allocator){0})".to_string()
                } else if callee == "alloc" {
                    // `alloc(a, T, n) -> []T` (SPEC §16.2). arg0 (the allocator)
                    // is accepted but unused in v0.119; arg1 is an identifier
                    // naming the element type `T` (resolved exactly as sema
                    // resolves it); arg2 is the element count `n`. It lowers to
                    // the slice's inline `_alloc` helper — the `[]T` typedef and
                    // helper are emitted because sema interned the slice as the
                    // call's result type.
                    let elem = match args.get(1) {
                        Some(Expr::Ident { name, .. }) => self.base_type(name),
                        _ => Type::Void,
                    };
                    let tag = self.structs.type_mangle(elem);
                    let n = match args.get(2) {
                        Some(n) => self.emit_expr(n),
                        None => "0".to_string(),
                    };
                    format!("kd_slice_{}_alloc((uintptr_t)({}))", tag, n)
                } else if callee == "free" {
                    // `free(a, s) -> void` (SPEC §16.2). arg0 (the allocator) is
                    // unused; arg1 is the slice whose backing storage `malloc`
                    // returned. Releasing `.ptr` mirrors the `_alloc` helper.
                    let s = match args.get(1) {
                        Some(s) => self.emit_expr(s),
                        None => "0".to_string(),
                    };
                    format!("free(({}).ptr)", s)
                } else if let Some(gf) = self.generics.get(callee).cloned() {
                    // A call to a generic function (SPEC §17.3): the leading
                    // type-name args pick the instantiation; the call lowers to
                    // that instance's C name, passing ONLY the runtime args.
                    self.emit_generic_call(callee, &gf, args)
                } else {
                    // Coerce each argument to its parameter type, so a `T`/`null`
                    // argument widens to a `?T` parameter.
                    let params = self.fn_params.get(callee).cloned();
                    let mut arg_strs = Vec::with_capacity(args.len());
                    for (i, x) in args.iter().enumerate() {
                        let expected = params.as_ref().and_then(|p| p.get(i).copied());
                        let s = match expected {
                            Some(t) => self.emit_coerced(x, t),
                            None => self.emit_expr(x),
                        };
                        arg_strs.push(s);
                    }
                    format!("kd_{}({})", callee, arg_strs.join(", "))
                }
            }
            Expr::Comptime { expr, .. } => {
                // Fold to a literal when possible (validated input always
                // can); otherwise fall back to the inner expression, which the
                // C compiler will itself constant-fold.
                match crate::const_eval::eval(expr, &self.consts) {
                    Ok(v) => const_literal(v),
                    Err(_) => self.emit_expr(expr),
                }
            }
            Expr::Field { base, field, .. } => {
                // A qualified enum literal `Enum.Variant` reuses the field-access
                // shape: its base is an `Ident` naming an enum type. Lower it to
                // the C enumerator rather than a struct member access.
                if let Expr::Ident { name, .. } = base.as_ref() {
                    if let Some(eid) = self.structs.enum_id_of(name) {
                        return self.structs.enum_variant_c_name(eid, field);
                    }
                }
                // `a.len` on an array → the compile-time length as a `usize`
                // constant (SPEC §14.3). This precedes ordinary field access so
                // an array never falls through to a `.kd_len` member access.
                if field == "len" {
                    if let Some(Type::Array(aid)) = self.type_of_expr(base) {
                        return format!("((uintptr_t){})", self.structs.array_len(aid));
                    }
                    // `s.len` on a slice → its runtime `.len` field (SPEC §15.2).
                    if let Some(Type::Slice(_)) = self.type_of_expr(base) {
                        let b = self.emit_expr(base);
                        return format!("({}).len", b);
                    }
                }
                // Field access: `(<base>).kd_<field>`. The base is parenthesized
                // so a compound base expression (e.g. a literal or another access)
                // composes correctly: `((p).kd_a).kd_b`. When the base is a
                // pointer to a struct (`*Struct` — e.g. a `self: *Counter`
                // pointer receiver, v0.134), the access auto-derefs through the
                // pointer: `(*(<base>)).kd_<field>` (SPEC §30.1). General for any
                // `*Struct`, not just `self`; because the field-assignment place
                // lowering also reuses this path, `p.field = e` and `p.field op= e`
                // likewise write *through* the pointer at the same C lvalue.
                let b = self.emit_expr(base);
                if self.is_ptr_to_struct(base) {
                    format!("(*({})).kd_{}", b, field)
                } else {
                    format!("({}).kd_{}", b, field)
                }
            }
            Expr::StructLit { name, fields, .. } => {
                // A union construction `Name{ .v = e }` reuses the struct-literal
                // shape but lowers to a tagged compound literal (SPEC §20.3).
                // The table tells unions and structs apart by name.
                if let Some(uid) = self.structs.union_id_of(name) {
                    return self.emit_union_lit(uid, fields);
                }
                // C99 compound literal: `((kd_struct_<Name>){ .kd_<f> = <v>, ... })`.
                // A type-alias name (`IL{ ... }` where `const IL = List(i32);`,
                // v0.129) resolves to the aliased monomorphised struct's id; a
                // name bound in the active substitution — `Self` (or a type
                // parameter) inside a generic-struct instance method (v0.130) —
                // resolves to its substituted struct id.
                let resolved_id = self
                    .structs
                    .id_of(name)
                    .or_else(|| match self.subst.get(name) {
                        Some(Type::Struct(id)) => Some(*id),
                        _ => None,
                    })
                    .or_else(|| match self.structs.alias_of(name) {
                        Some(Type::Struct(id)) => Some(id),
                        _ => None,
                    });
                let (cname, sid) = match resolved_id {
                    Some(id) => (self.structs.c_name(id), Some(id)),
                    // Validated input always resolves; fall back to the canonical
                    // spelling so emission stays well-formed even if it does not.
                    None => (format!("kd_struct_{}", name), None),
                };
                if fields.is_empty() {
                    format!("(({}){{0}})", cname)
                } else {
                    // Coerce each initializer to its field type (widening a
                    // `T`/`null` value to a `?T` field).
                    let mut inits = Vec::with_capacity(fields.len());
                    for fi in fields {
                        let expected =
                            sid.and_then(|id| self.structs.get(id).field_type(&fi.name));
                        let v = match expected {
                            Some(t) => self.emit_coerced(&fi.value, t),
                            None => self.emit_expr(&fi.value),
                        };
                        inits.push(format!(".kd_{} = {}", fi.name, v));
                    }
                    format!("(({}){{ {} }})", cname, inits.join(", "))
                }
            }
            Expr::MethodCall {
                receiver,
                method,
                args,
                ..
            } => self.emit_method_call(receiver, method, args),
            Expr::Null { .. } => {
                // A bare `null` with no expected `?T` type is rejected by sema
                // (E0180); coercion handles every legitimate `null`. This arm is
                // unreachable for validated input — emit a harmless placeholder.
                "0".to_string()
            }
            Expr::Orelse { lhs, rhs, .. } => {
                // `x orelse y` → `kd_opt_<tag>_orelse(<x>, <y>)`; `y` is eager.
                let l = self.emit_expr(lhs);
                let r = self.emit_expr(rhs);
                match self.type_of_expr(lhs) {
                    Some(Type::Optional(oid)) => {
                        let oname = self.structs.optional_c_name(oid);
                        format!("{}_orelse({}, {})", oname, l, r)
                    }
                    // Unreachable for validated input (`lhs` is always `?T`).
                    _ => format!("({})", l),
                }
            }
            Expr::Unwrap { expr, .. } => {
                // `x.?` → `kd_opt_<tag>_unwrap(<x>)` (panics + exit 101 if null).
                let inner = self.emit_expr(expr);
                match self.type_of_expr(expr) {
                    Some(Type::Optional(oid)) => {
                        let oname = self.structs.optional_c_name(oid);
                        format!("{}_unwrap({})", oname, inner)
                    }
                    // Unreachable for validated input (`expr` is always `?T`).
                    _ => format!("({})", inner),
                }
            }
            Expr::ErrorLit { name, .. } => {
                // A bare `error.Name` reaches here only with no expected `!T` to
                // wrap into; coercion (`emit_coerced`) handles every legitimate
                // use, so this is unreachable for validated input. Emit the bare
                // 1-based error code so the output stays syntactically valid.
                let code = self.structs.error_code(name).unwrap_or(0);
                code.to_string()
            }
            Expr::EnumLit { variant, .. } => {
                // A bare `.Variant` gets its enum type from context — a coercion
                // target (`emit_coerced`), a `switch` label (`emit_switch_label`)
                // or a comparison sibling (`emit_binop_operand`) supplies it.
                // Reaching here means no context was available, which sema
                // rejects (E0215); emit a harmless `0` so output stays valid C.
                let _ = variant;
                "0".to_string()
            }
            Expr::ArrayLit { elem, elems, .. } => {
                // `[N]T{ e0, e1, … }` → `((kd_arr_<tag>_<N>){ .data = { e0, … } })`
                // (SPEC §14.3). `elem` is the *full* array type `[N]T`, so
                // `resolve_ty` yields `Type::Array(id)` directly; each element is
                // coerced to the element type (so a `T`-coercible element widens).
                match self.resolve_ty(elem) {
                    Type::Array(aid) => {
                        let cname = self.structs.array_c_name(aid);
                        let elem_ty = self.structs.array_elem(aid);
                        if elems.is_empty() {
                            // A zero-length array: a designated-init with no
                            // elements is not valid C, so zero-initialise.
                            format!("(({}){{0}})", cname)
                        } else {
                            let inits: Vec<String> = elems
                                .iter()
                                .map(|e| self.emit_coerced(e, elem_ty))
                                .collect();
                            format!("(({}){{ .data = {{ {} }} }})", cname, inits.join(", "))
                        }
                    }
                    // Unreachable for validated input (the literal's type is
                    // always an interned array). Emit a brace-init so the output
                    // stays syntactically plausible.
                    _ => {
                        let inits: Vec<String> =
                            elems.iter().map(|e| self.emit_expr(e)).collect();
                        format!("{{ {} }}", inits.join(", "))
                    }
                }
            }
            Expr::Index { base, index, .. } => {
                // `a[i]` / `s[i]` (read) → a bounds-checked inline helper call:
                // `kd_arr_<tag>_<N>_get` for an array (SPEC §14.3) or
                // `kd_slice_<tag>_get` for a slice (SPEC §15.2).
                let b = self.emit_expr(base);
                let i = self.emit_expr(index);
                match self.type_of_expr(base) {
                    Some(Type::Array(aid)) => {
                        let cname = self.structs.array_c_name(aid);
                        format!("{}_get({}, {})", cname, b, i)
                    }
                    Some(Type::Slice(sid)) => {
                        let cname = self.structs.slice_c_name(sid);
                        format!("{}_get({}, {})", cname, b, i)
                    }
                    // Unreachable for validated input (`base` is an array/slice).
                    _ => format!("({})[{}]", b, i),
                }
            }
            Expr::AddrOf { place, .. } => {
                // `&place` → `(&(<place>))` (SPEC §15.1). The place is an lvalue
                // (a `var`, field chain, index or deref); sema guarantees it.
                let p = self.emit_expr(place);
                format!("(&({}))", p)
            }
            Expr::Deref { expr, .. } => {
                // `p.*` (read) → `(*(<p>))` (SPEC §15.1).
                let inner = self.emit_expr(expr);
                format!("(*({}))", inner)
            }
            Expr::SliceExpr { base, lo, hi, .. } => self.emit_slice_expr(base, lo, hi),
            Expr::Try { expr, .. } => {
                // `try` is statement-level (SPEC §12.1) and is lowered by the
                // statement emitters (`emit_try`); sema rejects it in any other
                // expression position (E0191). This arm is unreachable for
                // validated input — emit the inner value so output stays valid.
                format!("({})", self.emit_expr(expr))
            }
            Expr::Catch { expr, default, .. } => {
                // `e catch d` → `kd_err_<tag>_catch(<e>, <d>)`; `d` is eager and
                // coerced to the payload type.
                let l = self.emit_expr(expr);
                match self.type_of_expr(expr) {
                    Some(Type::ErrorUnion(eid)) => {
                        let ename = self.structs.error_union_c_name(eid);
                        let payload = self.structs.error_union_payload(eid);
                        let r = self.emit_coerced(default, payload);
                        format!("{}_catch({}, {})", ename, l, r)
                    }
                    // Unreachable for validated input (`expr` is always `!T`).
                    _ => format!("({})", l),
                }
            }
            // An anonymous `struct {…}` **type value** (v0.129, SPEC §25) only
            // ever appears as the body of a type-constructor function, which the
            // orchestrator never emits (it is skipped like a generic, §25.3). So
            // this arm is unreachable for validated input; emit a harmless `0`
            // placeholder so the C stays well-formed even if it were reached.
            Expr::StructType { .. } => "0".to_string(),
            // `unreachable` in expression position (SPEC §35.2): the
            // comma-expression `(kd_unreachable(), 0)`. `kd_unreachable` is
            // `_Noreturn`, so the trailing `0` is dead — it only satisfies an
            // integer value position. A statement position lowers without the
            // `, 0` via `emit_expr_stmt`.
            Expr::Unreachable { .. } => "(kd_unreachable(), 0)".to_string(),
        }
    }

    /// Lower a union construction `Name{ .v = e }` to a tagged C compound
    /// literal (SPEC §20.3):
    /// `((kd_union_<Name>){ .tag = <idx>, .data = { .kd_<v> = <e> } })`, where
    /// `<idx>` is the variant's 0-based tag and `<e>` is coerced to the
    /// variant's payload type. A validated union literal names exactly one
    /// variant; the impossible empty case zero-initialises so the output stays
    /// valid C.
    fn emit_union_lit(&mut self, uid: u32, fields: &[crate::ast::FieldInit]) -> String {
        let cname = self.structs.union_c_name(uid);
        match fields.first() {
            Some(fi) => {
                // `variant_index` / `payload_type` return `Copy` values, so the
                // immutable table borrow ends before the `&mut self` coercion.
                let idx = self
                    .structs
                    .union_get(uid)
                    .variant_index(&fi.name)
                    .unwrap_or(0);
                let payload = self
                    .structs
                    .union_get(uid)
                    .payload_type(&fi.name)
                    .unwrap_or(Type::Void);
                let v = self.emit_coerced(&fi.value, payload);
                format!(
                    "(({}){{ .tag = {}, .data = {{ .kd_{} = {} }} }})",
                    cname, idx, fi.name, v
                )
            }
            // Unreachable for validated input (a union literal has one field).
            None => format!("(({}){{0}})", cname),
        }
    }

    /// Lower a slice expression `base[lo..hi]` (SPEC §15.2). The result is a
    /// `{ptr, len}` view (`kd_slice_<tag>`): from an array it points at `.data +
    /// lo`, from a slice at `.ptr + lo`, with `len = hi - lo`. The bounds
    /// (`0 <= lo <= hi <= cap`) are checked at runtime — a violation prints a
    /// panic and `exit(101)`. Because this is an *expression* (no statement
    /// context to host an `if`), the check is folded into a portable conditional
    /// whose failing branch never returns (`exit` is `_Noreturn`).
    fn emit_slice_expr(&mut self, base: &Expr, lo: &Expr, hi: &Expr) -> String {
        let base_str = self.emit_expr(base);
        let lo_str = self.emit_expr(lo);
        let hi_str = self.emit_expr(hi);
        // `(data_expr, cap_expr, elem)`: how to reach the backing storage and
        // its capacity, and the element type, for an array vs a slice base.
        let (data_expr, cap_expr, elem) = match self.type_of_expr(base) {
            Some(Type::Array(aid)) => (
                format!("({}).data", base_str),
                self.structs.array_len(aid).to_string(),
                self.structs.array_elem(aid),
            ),
            Some(Type::Slice(sid)) => (
                format!("({}).ptr", base_str),
                format!("({}).len", base_str),
                self.structs.slice_elem(sid),
            ),
            // Unreachable for validated input (`base` is an array or a slice).
            _ => (format!("({})", base_str), "0".to_string(), Type::Void),
        };
        let sname = self
            .structs
            .slices()
            .find(|(_, e)| *e == elem)
            .map(|(id, _)| self.structs.slice_c_name(id))
            .unwrap_or_else(|| format!("kd_slice_{}", self.structs.type_mangle(elem)));
        format!(
            "(( ({lo}) < 0 || ({hi}) < ({lo}) || ({hi}) > ({cap}) ) ? (fputs(\"panic: slice bounds out of range\\n\", stderr), exit(101), ({sn}){{0}}) : ({sn}){{ .ptr = {data} + ({lo}), .len = ({hi}) - ({lo}) }})",
            lo = lo_str,
            hi = hi_str,
            cap = cap_expr,
            sn = sname,
            data = data_expr
        )
    }

    /// Lower a call to a generic function `gf` (SPEC §17.3). The first `k` args
    /// (one per comptime type parameter) are type-name `Ident`s that pick the
    /// instantiation; they are resolved under the *current* substitution (so a
    /// type argument that is itself an enclosing instance's type parameter
    /// resolves transitively), used to build the instance's C name, and then
    /// dropped. Only the remaining runtime args are passed — each coerced to its
    /// substituted parameter type so a `T`/`null` value widens correctly.
    fn emit_generic_call(&mut self, callee: &str, gf: &Func, args: &[Expr]) -> String {
        let k = gf.params.iter().filter(|p| p.is_comptime).count();
        // The comptime arguments (type + value) pick the instantiation; build
        // the instance key + the inner substitutions the runtime parameter types
        // resolve under (SPEC §24.3).
        let (cargs, tinner, vinner) = self.comptime_args_and_subst(gf, args);
        let inst = Instantiation {
            fn_name: callee.to_string(),
            args: cargs,
        };
        let cname = self.structs.instantiation_c_name(&inst);
        // The instance's runtime parameter types, resolved under the inner
        // substitutions (comptime type params → concrete types, comptime value
        // params → concrete `[n]T` lengths), drive coercion of the runtime args.
        let runtime_param_tys: Vec<Type> = gf
            .params
            .iter()
            .filter(|p| !p.is_comptime)
            .map(|p| self.resolve_ty_in(&p.ty, &tinner, &vinner))
            .collect();
        let mut arg_strs = Vec::with_capacity(args.len().saturating_sub(k));
        for (i, x) in args.iter().skip(k).enumerate() {
            let s = match runtime_param_tys.get(i) {
                Some(t) => self.emit_coerced(x, *t),
                None => self.emit_expr(x),
            };
            arg_strs.push(s);
        }
        format!("{}({})", cname, arg_strs.join(", "))
    }

    /// Compute the comptime arguments of a generic call (SPEC §24.2) plus the
    /// inner type / value substitution maps that the instance's runtime
    /// parameter and return types resolve under. For each comptime parameter (in
    /// order): a `type` parameter takes the corresponding type-name identifier
    /// argument (resolved under the current substitution, so a transitively
    /// nested type parameter still resolves), and a value parameter
    /// const-evaluates its argument to an `i64` (the active value substitution
    /// participates, so an enclosing instance's value param resolves too). A
    /// non-constant value argument is impossible for validated input; it folds
    /// to `0` so emission never panics.
    fn comptime_args_and_subst(
        &self,
        gf: &Func,
        args: &[Expr],
    ) -> (Vec<ComptimeArg>, HashMap<String, Type>, HashMap<String, i64>) {
        // The const-eval environment: the top-level constants plus any active
        // comptime value parameters (so a value arg may reference an enclosing
        // instance's value param), mirroring how type args resolve transitively.
        let mut env = self.consts.clone();
        for (name, v) in &self.value_subst {
            env.insert(name.clone(), ConstVal::Int(*v));
        }
        let mut cargs = Vec::new();
        let mut tinner: HashMap<String, Type> = HashMap::new();
        let mut vinner: HashMap<String, i64> = HashMap::new();
        for (p, a) in gf.params.iter().filter(|p| p.is_comptime).zip(args.iter()) {
            if Self::is_value_param(p) {
                let v = match crate::const_eval::eval(a, &env) {
                    Ok(ConstVal::Int(n)) => n,
                    _ => 0,
                };
                cargs.push(ComptimeArg::Value(v));
                vinner.insert(p.name.clone(), v);
            } else {
                let t = match a {
                    Expr::Ident { name, .. } => self.base_type(name),
                    _ => Type::Void,
                };
                cargs.push(ComptimeArg::Type(t));
                tinner.insert(p.name.clone(), t);
            }
        }
        (cargs, tinner, vinner)
    }

    /// The substituted return type of a call to generic function `gf` with the
    /// given call `args` (the leading comptime args pick the substitution),
    /// resolved under the current substitution (SPEC §17.2 / §24.2). Lets
    /// `type_of_expr` / `struct_of_expr` infer a generic call's result type (so
    /// e.g. `var x: i32 = max(i32, a, b);` coerces correctly, `g(T, …).len`
    /// works, and a `[n]T` return resolves to its concrete-length array type).
    fn generic_call_ret(&self, gf: &Func, args: &[Expr]) -> Type {
        let (_, tinner, vinner) = self.comptime_args_and_subst(gf, args);
        self.resolve_ty_in(&gf.ret, &tinner, &vinner)
    }

    /// Lower one operand of a binary expression. This is ordinarily just
    /// [`Emitter::emit_expr`]; the sole exception is a bare enum literal `.V`,
    /// which has no intrinsic type — its enum is taken from the sibling operand
    /// so that e.g. `c == .Red` lowers `.Red` to the matching C enumerator.
    fn emit_binop_operand(&mut self, e: &Expr, sibling: &Expr) -> String {
        if matches!(e, Expr::EnumLit { .. }) {
            if let Some(t @ Type::Enum(_)) = self.type_of_expr(sibling) {
                return self.emit_coerced(e, t);
            }
        }
        self.emit_expr(e)
    }

    /// Lower a method / associated-function call to a free-function call.
    ///
    /// The call shape is decided exactly as sema decides it: if the receiver is
    /// an identifier naming a struct *type*, this is an associated call
    /// (`Counter.zero()` / `Counter.get(c)`) and only `args` are passed; the
    /// struct is that name. Otherwise it is a method call on a value, the
    /// receiver is emitted as the leading `self` argument, and the struct is
    /// resolved from the receiver expression's type. Either way the callee is
    /// `kd_<Struct>_<method>`.
    fn emit_method_call(&mut self, receiver: &Expr, method: &str, args: &[Expr]) -> String {
        let assoc = match receiver {
            // A direct struct name, or a type-alias name (`IntList.init(a)` where
            // `const IntList = ArrayList(i32);`, v0.130) → the struct's source
            // name, so the call lowers to `kd_<struct>_<method>` matching the
            // emitted instance method.
            Expr::Ident { name, .. } => self
                .structs
                .id_of(name)
                .map(|_| name.clone())
                .or_else(|| match self.structs.alias_of(name) {
                    Some(Type::Struct(id)) => Some(self.structs.get(id).name.clone()),
                    _ => None,
                })
                // `Self.assoc(...)` inside a generic-struct method: `Self` is in
                // the active substitution → the instantiated struct (v0.138).
                .or_else(|| match self.subst.get(name) {
                    Some(Type::Struct(id)) => Some(self.structs.get(*id).name.clone()),
                    _ => None,
                }),
            _ => None,
        };
        if let Some(struct_name) = assoc {
            // Associated call: args bind to *all* params (including an explicit
            // `self` in the `Counter.get(c)` form), so the receiver itself is
            // not passed. Coerce each arg against its parameter type.
            let params = self
                .method_params
                .get(&(struct_name.clone(), method.to_string()))
                .cloned();
            let arg_strs = self.emit_coerced_args(args, params.as_deref(), 0);
            format!("kd_{}_{}({})", struct_name, method, arg_strs.join(", "))
        } else {
            // Method call on a value: the receiver becomes the leading `self`
            // argument, then the remaining args (coerced against params[1..],
            // skipping the `self` parameter).
            let struct_name = self.struct_of_expr(receiver).unwrap_or_default();
            let params = self
                .method_params
                .get(&(struct_name.clone(), method.to_string()))
                .cloned();
            // A pointer-receiver method (v0.134, SPEC §30.2) has a `*Struct`
            // `self` parameter (param 0). Auto-ref / auto-deref the receiver so
            // the C `self` argument matches the parameter, whether the receiver
            // expression is itself a struct value or is already a pointer:
            //   value receiver + struct value    → pass the value (a copy)
            //   ptr   receiver + struct value     → pass `&value`  (address-of)
            //   value receiver + `*Struct` value  → pass `*ptr`    (deref)
            //   ptr   receiver + `*Struct` value  → pass the pointer unchanged
            // sema guarantees `&value` targets an addressable lvalue (a `var`,
            // field or index); the address-of mutation is real and updates the
            // caller's struct in place.
            let ptr_receiver =
                matches!(params.as_deref().and_then(|p| p.first()), Some(Type::Ptr(_)));
            let recv_is_ptr = self.is_ptr_to_struct(receiver);
            let recv = self.emit_expr(receiver);
            let self_str = match (ptr_receiver, recv_is_ptr) {
                (true, false) => format!("(&({}))", recv),
                (false, true) => format!("(*({}))", recv),
                _ => recv,
            };
            let arg_strs = self.emit_coerced_args(args, params.as_deref(), 1);
            let mut all = Vec::with_capacity(1 + arg_strs.len());
            all.push(self_str);
            all.extend(arg_strs);
            format!("kd_{}_{}({})", struct_name, method, all.join(", "))
        }
    }

    /// Lower `args`, coercing arg `i` to `params[i + offset]` when that
    /// parameter type is known (so a `T`/`null` argument widens to a `?T`
    /// parameter). `offset` is `1` for a method call on a value (to skip the
    /// `self` parameter) and `0` otherwise.
    fn emit_coerced_args(
        &mut self,
        args: &[Expr],
        params: Option<&[Type]>,
        offset: usize,
    ) -> Vec<String> {
        let mut out = Vec::with_capacity(args.len());
        for (i, a) in args.iter().enumerate() {
            let expected = params.and_then(|p| p.get(i + offset).copied());
            let s = match expected {
                Some(t) => self.emit_coerced(a, t),
                None => self.emit_expr(a),
            };
            out.push(s);
        }
        out
    }

    /// The source name of the struct an expression evaluates to, or `None` if it
    /// is not a struct (or cannot be determined). Used only to name the C
    /// function for a method call on a value. Resolves:
    /// - `Ident` — a struct-typed local/param recorded in the scope stack;
    /// - `Field` — the field's type within its base struct;
    /// - `StructLit` — the literal's own struct name;
    /// - `Call` — the called top-level function's return type;
    /// - `MethodCall` — the invoked struct function's return type.
    fn struct_of_expr(&self, e: &Expr) -> Option<String> {
        match e {
            Expr::Ident { name, .. } => self.lookup_var_struct(name),
            Expr::Field { base, field, .. } => {
                let base_struct = self.struct_of_expr(base)?;
                let id = self.structs.id_of(&base_struct)?;
                match self.structs.get(id).field_type(field)? {
                    Type::Struct(fid) => Some(self.structs.get(fid).name.clone()),
                    _ => None,
                }
            }
            Expr::StructLit { name, .. } => {
                // `Self` / a type parameter inside a generic-struct instance
                // method (v0.130), or a type-alias name (v0.129), names the
                // monomorphised struct — resolve it so a method call on the
                // literal uses the real `kd_<struct>_<method>` name. An ordinary
                // struct literal already carries its own struct name.
                if let Some(Type::Struct(id)) = self.subst.get(name) {
                    return Some(self.structs.get(*id).name.clone());
                }
                if let Some(Type::Struct(id)) = self.structs.alias_of(name) {
                    return Some(self.structs.get(id).name.clone());
                }
                Some(name.clone())
            }
            // A struct-typed array element: `a[i]` where the array's element is
            // a struct, so `a[i].method()` resolves to the element's struct.
            Expr::Index { base, .. } => match self.type_of_expr(base)? {
                Type::Array(aid) => match self.structs.array_elem(aid) {
                    Type::Struct(sid) => Some(self.structs.get(sid).name.clone()),
                    _ => None,
                },
                _ => None,
            },
            Expr::Call { callee, args, .. } => {
                // A generic call's struct is taken from its substituted return
                // type (SPEC §17.2); an ordinary call's from its recorded ret.
                if let Some(gf) = self.generics.get(callee) {
                    return match self.generic_call_ret(gf, args) {
                        Type::Struct(id) => Some(self.structs.get(id).name.clone()),
                        _ => None,
                    };
                }
                match self.fn_ret.get(callee)? {
                    Type::Struct(id) => Some(self.structs.get(*id).name.clone()),
                    _ => None,
                }
            }
            Expr::MethodCall {
                receiver, method, ..
            } => {
                // The struct on which `method` is invoked: an associated call's
                // type-name receiver, else the receiver expression's struct.
                let recv_struct = match receiver.as_ref() {
                    Expr::Ident { name, .. } if self.structs.id_of(name).is_some() => name.clone(),
                    _ => self.struct_of_expr(receiver)?,
                };
                match self.method_ret.get(&(recv_struct, method.clone()))? {
                    Type::Struct(id) => Some(self.structs.get(*id).name.clone()),
                    _ => None,
                }
            }
            _ => None,
        }
    }

    /// Find the struct name a (struct-typed) variable was recorded with,
    /// searching scopes innermost-first so a shadowing binding wins. A variable
    /// whose type is not a struct (a primitive or an optional) yields `None`.
    fn lookup_var_struct(&self, name: &str) -> Option<String> {
        match self.lookup_var_type(name)? {
            Type::Struct(id) => Some(self.structs.get(id).name.clone()),
            // A `*Struct` local/param (e.g. a `self: *Counter` pointer receiver,
            // v0.134) resolves to its pointee struct, so a method call on it
            // names the right `kd_<Struct>_<method>` C function and the receiver
            // auto-derefs/auto-refs (SPEC §30).
            Type::Ptr(pid) => match self.ptr_pointee_any(pid) {
                Type::Struct(id) => Some(self.structs.get(id).name.clone()),
                _ => None,
            },
            _ => None,
        }
    }

    /// True if `e` evaluates to a pointer-to-struct value (`*Struct`). Drives the
    /// v0.134 auto-deref of `p.field` and the auto-ref/auto-deref of a method
    /// receiver (SPEC §30). A non-pointer, or a pointer to a non-struct, is
    /// `false` (those keep their pre-v0.134 lowering).
    fn is_ptr_to_struct(&self, e: &Expr) -> bool {
        matches!(
            self.type_of_expr(e),
            Some(Type::Ptr(pid)) if matches!(self.ptr_pointee_any(pid), Type::Struct(_))
        )
    }

    /// The recorded type of a local/param, searching scopes innermost-first so
    /// a shadowing binding wins.
    fn lookup_var_type(&self, name: &str) -> Option<Type> {
        self.scopes
            .iter()
            .rev()
            .find_map(|s| s.var_types.get(name).copied())
    }

    // -- optional coercion --------------------------------------------------

    /// Best-effort static type of `e`, used to decide optional coercion. Returns
    /// `None` when it cannot be determined (e.g. a bare `null`); int literals are
    /// reported as `i64` (their default), which is sufficient because coercion
    /// only needs to tell "already an optional" apart from "a `T` value".
    ///
    /// Resolves identifiers via the scope's `var_types`, struct-literal /
    /// field-access types via the `StructTable`, call/method return types via
    /// the collected signatures, and `orelse` / `.?` as the inner `T`.
    fn type_of_expr(&self, e: &Expr) -> Option<Type> {
        match e {
            Expr::Int { .. } => Some(Type::I64),
            Expr::Bool { .. } => Some(Type::Bool),
            // A string literal has type `[]u8` (SPEC §23.1). The struct table is
            // immutable here, but sema already interned the `[]u8` slice (a
            // `StrLit` exists), so map back to its `Type::Slice(id)` by finding
            // the interned slice whose element is `u8`.
            Expr::StrLit { .. } => self
                .structs
                .slices()
                .find(|(_, e)| *e == Type::U8)
                .map(|(id, _)| Type::Slice(id)),
            // `@sizeOf(T)` → `usize`; `@typeName(T)` → `[]u8` (SPEC §32.1).
            Expr::Builtin { name, args, .. } => match name.as_str() {
                "sizeOf" => Some(Type::Usize),
                "typeName" => self
                    .structs
                    .slices()
                    .find(|(_, e)| *e == Type::U8)
                    .map(|(id, _)| Type::Slice(id)),
                // `@as(T, e)` has type `T` (the cast target).
                "as" => match args.first() {
                    Some(Expr::Ident { name, .. }) => Some(self.base_type(name)),
                    _ => None,
                },
                // `@panic(msg)` diverges (SPEC §35.1): no inherent type — it
                // adopts the expected type, and emission coerces via the
                // surrounding let/return annotation.
                "panic" => None,
                _ => None,
            },
            Expr::Ident { name, .. } => self.lookup_var_type(name),
            Expr::Unary { op, expr, .. } => match op {
                UnOp::Not => Some(Type::Bool),
                // `-x` and `~x` (bitwise complement, v0.132) keep the operand's
                // integer type.
                UnOp::Neg | UnOp::BitNot => self.type_of_expr(expr),
            },
            Expr::Binary { op, lhs, .. } => {
                if op.is_bool_result() {
                    Some(Type::Bool)
                } else {
                    // Arithmetic yields the (shared) operand type.
                    self.type_of_expr(lhs)
                }
            }
            Expr::Call { callee, args, .. } => {
                // The allocator builtins have synthetic result types (SPEC §16):
                // `c_allocator()` is an `Allocator`; `alloc(a, T, n)` is `[]T`,
                // resolved from the type-name identifier (arg1) and mapped to its
                // interned slice so a `var x = alloc(a, T, n);` infers `[]T`.
                if callee == "c_allocator" {
                    return Some(Type::Allocator);
                }
                if callee == "alloc" {
                    let elem = match args.get(1) {
                        Some(Expr::Ident { name, .. }) => self.base_type(name),
                        _ => return None,
                    };
                    return self
                        .structs
                        .slices()
                        .find(|(_, e)| *e == elem)
                        .map(|(id, _)| Type::Slice(id));
                }
                // A generic call's result is its substituted return type (SPEC
                // §17.2), so e.g. `max(i32, a, b)` reports `i32`.
                if let Some(gf) = self.generics.get(callee) {
                    return Some(self.generic_call_ret(gf, args));
                }
                self.fn_ret.get(callee).copied()
            }
            Expr::Comptime { expr, .. } => self.type_of_expr(expr),
            // A union literal `Name{ .v = e }` reuses the struct-literal shape;
            // the table distinguishes it, and it has type `Union(id)` (SPEC §20).
            Expr::StructLit { name, .. } => self
                .structs
                .union_id_of(name)
                .map(Type::Union)
                .or_else(|| self.structs.id_of(name).map(Type::Struct))
                // `Self` / a type parameter inside a generic-struct instance
                // method (v0.130) → its substituted struct.
                .or_else(|| self.subst.get(name).copied())
                // A type-alias name (`IL{ ... }`, v0.129) → the aliased struct.
                .or_else(|| self.structs.alias_of(name)),
            Expr::Field { base, field, .. } => {
                // A qualified enum literal `Enum.V` (base names an enum type) has
                // type `Enum(id)`; otherwise it is an ordinary struct-field access.
                if let Expr::Ident { name, .. } = base.as_ref() {
                    if let Some(eid) = self.structs.enum_id_of(name) {
                        return Some(Type::Enum(eid));
                    }
                }
                match self.type_of_expr(base)? {
                    Type::Struct(id) => self.structs.get(id).field_type(field),
                    // `p.field` on a `*Struct` auto-derefs to the pointee's field
                    // type (v0.134, SPEC §30.1) — so a field read/assign through a
                    // pointer receiver resolves and coerces like a value access.
                    Type::Ptr(pid) => match self.ptr_pointee_any(pid) {
                        Type::Struct(id) => self.structs.get(id).field_type(field),
                        _ => None,
                    },
                    // `a.len` on an array is a `usize` constant (SPEC §14.3).
                    Type::Array(_) if field == "len" => Some(Type::Usize),
                    // `s.len` on a slice is a `usize` (SPEC §15.2).
                    Type::Slice(_) if field == "len" => Some(Type::Usize),
                    _ => None,
                }
            }
            Expr::MethodCall {
                receiver, method, ..
            } => {
                let recv_struct = match receiver.as_ref() {
                    Expr::Ident { name, .. } if self.structs.id_of(name).is_some() => name.clone(),
                    _ => self.struct_of_expr(receiver)?,
                };
                self.method_ret.get(&(recv_struct, method.clone())).copied()
            }
            // A bare `null` has no intrinsic type — its `?T` comes from context.
            Expr::Null { .. } => None,
            // `orelse` / `.?` both produce the inner `T` of the optional operand.
            Expr::Orelse { lhs, .. } => match self.type_of_expr(lhs)? {
                Type::Optional(id) => Some(self.structs.optional_inner(id)),
                other => Some(other),
            },
            Expr::Unwrap { expr, .. } => match self.type_of_expr(expr)? {
                Type::Optional(id) => Some(self.structs.optional_inner(id)),
                other => Some(other),
            },
            // A bare `error.Name` has no intrinsic type — its `!T` comes from
            // context (it coerces to any error union).
            Expr::ErrorLit { .. } => None,
            // A bare `.Variant` has no intrinsic type — its enum comes from
            // context (the expected type or the `switch` scrutinee).
            Expr::EnumLit { .. } => None,
            // An array literal `[N]T{ … }` has the array type of its full `elem`
            // (`[N]T`); `resolve_ty` yields `Type::Array(id)` directly.
            Expr::ArrayLit { elem, .. } => match self.resolve_ty(elem) {
                t @ Type::Array(_) => Some(t),
                _ => None,
            },
            // `a[i]` / `s[i]` yields the element type of the array / slice.
            Expr::Index { base, .. } => match self.type_of_expr(base)? {
                Type::Array(id) => Some(self.structs.array_elem(id)),
                Type::Slice(id) => Some(self.structs.slice_elem(id)),
                _ => None,
            },
            // `try` / `catch` both produce the payload `T` of the `!T` operand.
            Expr::Try { expr, .. } => match self.type_of_expr(expr)? {
                Type::ErrorUnion(id) => Some(self.structs.error_union_payload(id)),
                other => Some(other),
            },
            Expr::Catch { expr, .. } => match self.type_of_expr(expr)? {
                Type::ErrorUnion(id) => Some(self.structs.error_union_payload(id)),
                other => Some(other),
            },
            // `&place` is `*T` where `T` is the place's type. Map that pointee to
            // an emit-local `Type::Ptr` id (`None` if the pointee is unknown or
            // the `*T` was never written as a source type, so not registered).
            Expr::AddrOf { place, .. } => {
                let pointee = self.type_of_expr(place)?;
                self.local_ptr_pointees
                    .iter()
                    .position(|x| *x == pointee)
                    .map(|i| Type::Ptr(PTR_LOCAL_BASE + i as u32))
            }
            // `p.*` yields the pointee type of the `*T` operand.
            Expr::Deref { expr, .. } => match self.type_of_expr(expr)? {
                Type::Ptr(id) => Some(self.ptr_pointee_any(id)),
                other => Some(other),
            },
            // `base[lo..hi]` yields `[]T` where `T` is the element of the sliced
            // array / slice.
            Expr::SliceExpr { base, .. } => {
                let elem = match self.type_of_expr(base)? {
                    Type::Array(aid) => self.structs.array_elem(aid),
                    Type::Slice(sid) => self.structs.slice_elem(sid),
                    _ => return None,
                };
                self.structs
                    .slices()
                    .find(|(_, e)| *e == elem)
                    .map(|(id, _)| Type::Slice(id))
            }
            // A `struct {…}` type value is compile-time only (v0.129); it never
            // reaches a runtime value position in validated input and so has no
            // runtime type — report `None` defensively.
            Expr::StructType { .. } => None,
            // `unreachable` diverges (SPEC §35.1): no inherent type — it adopts
            // the expected type, with emission coercing through the surrounding
            // context (let/return), exactly like `@panic`.
            Expr::Unreachable { .. } => None,
        }
    }

    /// Lower `e` to a C string, coercing it to `expected`. When `expected` is an
    /// optional `?T`, a `null` source widens to `{ .has = false }`, a value
    /// already of type `?T` passes through unchanged, and any other (`T`) value
    /// widens to `{ .has = true, .val = <e> }`. When `expected` is not an
    /// optional this is just [`Emitter::emit_expr`].
    fn emit_coerced(&mut self, e: &Expr, expected: Type) -> String {
        if let Type::Enum(eid) = expected {
            // A bare `.Variant` resolves to its C enumerator using the expected
            // enum. Any other enum-typed value (a qualified `Enum.V`, an
            // enum-typed local, a call result) already lowers correctly.
            if let Expr::EnumLit { variant, .. } = e {
                return self.structs.enum_variant_c_name(eid, variant);
            }
            return self.emit_expr(e);
        }
        if let Type::Optional(oid) = expected {
            let oname = self.structs.optional_c_name(oid);
            if matches!(e, Expr::Null { .. }) {
                return format!("(({}){{ .has = false }})", oname);
            }
            // Already an optional value? Pass it through. (A struct-equal but
            // differently-interned optional cannot occur — sema dedups them.)
            if matches!(self.type_of_expr(e), Some(Type::Optional(_))) {
                return self.emit_expr(e);
            }
            // Otherwise it is a `T` value being widened to `?T`.
            let inner = self.emit_expr(e);
            return format!("(({}){{ .has = true, .val = {} }})", oname, inner);
        }
        if let Type::ErrorUnion(eid) = expected {
            let ename = self.structs.error_union_c_name(eid);
            // An `error.Name` literal becomes a failure value carrying its code.
            if let Expr::ErrorLit { name, .. } = e {
                let code = self.structs.error_code(name).unwrap_or(0);
                return format!("(({}){{ .err = {} }})", ename, code);
            }
            // Already an error-union value? Pass it through. (sema dedups, so a
            // structurally-equal but differently-interned `!T` cannot occur.)
            if matches!(self.type_of_expr(e), Some(Type::ErrorUnion(_))) {
                return self.emit_expr(e);
            }
            // Otherwise it is a `T` value being widened to a success `!T`.
            let inner = self.emit_expr(e);
            return format!("(({}){{ .err = 0, .val = {} }})", ename, inner);
        }
        self.emit_expr(e)
    }

    // -- program / test entry points ---------------------------------------

    fn emit_program_main(&mut self, module: &Module) {
        let main_is_int = module
            .items
            .iter()
            .find_map(|it| match it {
                Item::Func(f) if f.name == "main" => {
                    Some(Type::from_name(&f.ret.name).map(|t| t.is_int()).unwrap_or(false))
                }
                _ => None,
            })
            .unwrap_or(false);
        let wire = if main_is_int {
            "return (int) kd_main();"
        } else {
            "kd_main(); return 0;"
        };
        self.out.push_str(&format!(
            "int main(int argc, char **argv){{ (void)argc;(void)argv; {} }}\n",
            wire
        ));
    }

    fn emit_test_harness(&mut self, module: &Module) {
        // Define each test function, then a driver `main`.
        let mut names: Vec<String> = Vec::new();
        for item in &module.items {
            if let Item::Test(t) = item {
                let idx = names.len();
                self.emit_test_fn(idx, t);
                self.blank();
                names.push(t.name.clone());
            }
        }

        let total = names.len();
        self.line("int main(int argc, char **argv) {");
        self.indent += 1;
        self.line("(void)argc; (void)argv;");
        self.line("int failures = 0;");
        for (i, name) in names.iter().enumerate() {
            let esc = c_escape(name);
            self.line(&format!("if (kd_test_{}() == 0) {{", i));
            self.indent += 1;
            self.line(&format!("fprintf(stderr, \"ok: %s\\n\", \"{}\");", esc));
            self.indent -= 1;
            self.line("} else {");
            self.indent += 1;
            self.line(&format!("fprintf(stderr, \"FAIL: %s\\n\", \"{}\");", esc));
            self.line("failures++;");
            self.indent -= 1;
            self.line("}");
        }
        self.line(&format!(
            "fprintf(stderr, \"%d/%d tests passed\\n\", {} - failures, {});",
            total, total
        ));
        self.line("return failures;");
        self.indent -= 1;
        self.line("}");
    }

    fn emit_test_fn(&mut self, idx: usize, t: &TestBlock) {
        self.scopes.clear();
        self.try_counter = 0;
        self.idx_counter = 0;
        self.if_counter = 0;
        self.for_counter = 0;
        self.current_ret = Type::I32; // the harness test functions return `int`
        self.line(&format!("static int kd_test_{}(void) {{", idx));
        self.indent += 1;
        self.scopes.push(Scope::function());
        let mut diverged = false;
        for stmt in &t.body.stmts {
            diverged = self.emit_stmt(stmt);
            if diverged {
                break;
            }
        }
        if !diverged {
            self.flush_current_reversed(false);
        }
        self.scopes.pop();
        self.line("return 0;");
        self.indent -= 1;
        self.line("}");
    }
}

/// The inferred [`Type`] of a folded constant value (v0.121): an integer
/// constant infers `i64`, a boolean constant infers `bool`. Used to pick the C
/// declaration type of an un-annotated top-level `const` (SPEC §18.2/§18.3).
fn const_val_type(v: ConstVal) -> Type {
    match v {
        ConstVal::Int(_) => Type::I64,
        ConstVal::Bool(_) => Type::Bool,
    }
}

/// Render a folded constant as a C literal.
fn const_literal(v: ConstVal) -> String {
    match v {
        ConstVal::Int(i) => i.to_string(),
        ConstVal::Bool(b) => {
            if b {
                "true".to_string()
            } else {
                "false".to_string()
            }
        }
    }
}

/// Escape a string for inclusion inside a C double-quoted literal.
fn c_escape(s: &str) -> String {
    let mut o = String::new();
    for ch in s.chars() {
        match ch {
            '\\' => o.push_str("\\\\"),
            '"' => o.push_str("\\\""),
            '\n' => o.push_str("\\n"),
            '\t' => o.push_str("\\t"),
            '\r' => o.push_str("\\r"),
            c => o.push(c),
        }
    }
    o
}

/// Render the bytes of a Kardashev string as a complete, double-quoted C string
/// literal (including the surrounding `"`), for the `[]u8` lowering (SPEC §23.2).
///
/// A string is `[]u8`, so escaping is byte-exact (not char-based): each byte of
/// `s` is rendered independently. Backslash and double-quote are escaped; `\n`,
/// `\t`, `\r` stay readable; every other byte outside the printable ASCII range
/// (`0x20..=0x7e`) becomes a two-digit `\xNN` hex escape.
///
/// A C hex escape consumes *all* following hex digits, so `"\x07f"` would mean
/// the single byte `0x7f`, not `0x07` then `'f'`. To keep each byte faithful,
/// when a `\xNN` escape is immediately followed by a byte that renders as a
/// literal hex digit, the string literal is split with `" "` (adjacent string
/// literals concatenate in C) so the escape cannot absorb that digit.
fn c_string_literal(s: &str) -> String {
    let mut o = String::from("\"");
    let mut prev_was_hex_escape = false;
    for &b in s.as_bytes() {
        match b {
            b'\\' => {
                o.push_str("\\\\");
                prev_was_hex_escape = false;
            }
            b'"' => {
                o.push_str("\\\"");
                prev_was_hex_escape = false;
            }
            b'\n' => {
                o.push_str("\\n");
                prev_was_hex_escape = false;
            }
            b'\t' => {
                o.push_str("\\t");
                prev_was_hex_escape = false;
            }
            b'\r' => {
                o.push_str("\\r");
                prev_was_hex_escape = false;
            }
            0x20..=0x7e => {
                // A literal hex digit right after a `\xNN` escape would extend
                // it; break the literal so the escape stops at two digits.
                if prev_was_hex_escape && b.is_ascii_hexdigit() {
                    o.push_str("\" \"");
                }
                o.push(b as char);
                prev_was_hex_escape = false;
            }
            _ => {
                o.push_str(&format!("\\x{:02x}", b));
                prev_was_hex_escape = true;
            }
        }
    }
    o.push('"');
    o
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{
        ArraySize, BinOp, Block, ConstDecl, ErrorSetDecl, Expr, FieldDecl, FieldInit, Func, Item,
        Module, Param, Stmt, StructDecl, SwitchArm, TestBlock, TypeExpr,
    };
    use crate::span::Span;
    use crate::types::{ComptimeArg, StructTable, Type};

    fn ty(name: &str) -> TypeExpr {
        TypeExpr {
            name: name.to_string(),
            optional: false,
            error_union: false,
            error_set: None,
            array_len: None,
            pointer: false,
            slice: false,
            span: Span::DUMMY,
        }
    }

    fn opt_ty(name: &str) -> TypeExpr {
        TypeExpr {
            name: name.to_string(),
            optional: true,
            error_union: false,
            error_set: None,
            array_len: None,
            pointer: false,
            slice: false,
            span: Span::DUMMY,
        }
    }

    /// An error union over the **implicit global** error set, `!name` (§12).
    fn err_ty(name: &str) -> TypeExpr {
        TypeExpr {
            name: name.to_string(),
            optional: false,
            error_union: true,
            error_set: None,
            array_len: None,
            pointer: false,
            slice: false,
            span: Span::DUMMY,
        }
    }

    /// An error union over a **named** error set, `set!name` (v0.139, §34). The
    /// runtime representation is identical to [`err_ty`] — the set name is a
    /// pure sema constraint and the backend must ignore it.
    fn set_err_ty(set: &str, name: &str) -> TypeExpr {
        TypeExpr {
            name: name.to_string(),
            optional: false,
            error_union: true,
            error_set: Some(set.to_string()),
            array_len: None,
            pointer: false,
            slice: false,
            span: Span::DUMMY,
        }
    }

    /// A fixed-size array type `[len]name` with a literal length
    /// (`array_len = Some(ArraySize::Lit(len))`, the element type name in
    /// `name`).
    fn arr_ty(name: &str, len: i64) -> TypeExpr {
        TypeExpr {
            name: name.to_string(),
            optional: false,
            error_union: false,
            error_set: None,
            array_len: Some(ArraySize::Lit(len)),
            pointer: false,
            slice: false,
            span: Span::DUMMY,
        }
    }

    /// A comptime-value-parameter-sized array type `[param]name`
    /// (`array_len = Some(ArraySize::Param(param))`, v0.128).
    fn arr_param_ty(name: &str, param: &str) -> TypeExpr {
        TypeExpr {
            name: name.to_string(),
            optional: false,
            error_union: false,
            error_set: None,
            array_len: Some(ArraySize::Param(param.to_string())),
            pointer: false,
            slice: false,
            span: Span::DUMMY,
        }
    }

    /// A pointer type `*name` (v0.118).
    fn ptr_ty(name: &str) -> TypeExpr {
        TypeExpr {
            name: name.to_string(),
            optional: false,
            error_union: false,
            error_set: None,
            array_len: None,
            pointer: true,
            slice: false,
            span: Span::DUMMY,
        }
    }

    /// A slice type `[]name` (v0.118).
    fn slice_ty(name: &str) -> TypeExpr {
        TypeExpr {
            name: name.to_string(),
            optional: false,
            error_union: false,
            error_set: None,
            array_len: None,
            pointer: false,
            slice: true,
            span: Span::DUMMY,
        }
    }

    fn ident(name: &str) -> Expr {
        Expr::Ident {
            name: name.to_string(),
            span: Span::DUMMY,
        }
    }

    fn int(v: i64) -> Expr {
        Expr::Int {
            value: v,
            span: Span::DUMMY,
        }
    }

    fn block(stmts: Vec<Stmt>) -> Block {
        Block {
            stmts,
            span: Span::DUMMY,
        }
    }

    fn ret(e: Expr) -> Stmt {
        Stmt::Return {
            value: Some(e),
            span: Span::DUMMY,
        }
    }

    fn defer(s: Stmt) -> Stmt {
        Stmt::Defer {
            stmt: Box::new(s),
            span: Span::DUMMY,
        }
    }

    fn print(e: Expr) -> Stmt {
        Stmt::Expr(Expr::Call {
            callee: "print".to_string(),
            args: vec![e],
            span: Span::DUMMY,
        })
    }

    #[test]
    fn simple_fn_emits_prelude_decl_and_body() {
        // pub fn add(a: i32, b: i32) i32 { return a + b; }
        let f = Func {
            is_pub: true,
            name: "add".to_string(),
            params: vec![
                Param {
                    name: "a".to_string(),
                    ty: ty("i32"),
                    is_comptime: false,
                    span: Span::DUMMY,
                },
                Param {
                    name: "b".to_string(),
                    ty: ty("i32"),
                    is_comptime: false,
                    span: Span::DUMMY,
                },
            ],
            ret: ty("i32"),
            body: block(vec![ret(Expr::Binary {
                op: BinOp::Add,
                lhs: Box::new(ident("a")),
                rhs: Box::new(ident("b")),
                span: Span::DUMMY,
            })]),
            span: Span::DUMMY,
        };
        let m = Module {
            items: vec![Item::Func(f)],
        };
        let out = emit(&m, &StructTable::new(), EmitMode::Program);

        assert!(
            out.contains("static void kd_print(long long v) { printf(\"%lld\\n\", v); }"),
            "prelude missing:\n{out}"
        );
        // Forward declaration with kd_-prefixed names and C types.
        assert!(
            out.contains("int32_t kd_add(int32_t kd_a, int32_t kd_b);"),
            "forward decl missing:\n{out}"
        );
        // Definition + parenthesized binary, parenthesized return.
        assert!(
            out.contains("int32_t kd_add(int32_t kd_a, int32_t kd_b) {"),
            "definition missing:\n{out}"
        );
        assert!(
            out.contains("return ((kd_a + kd_b));"),
            "return body wrong:\n{out}"
        );
        // No user main -> void-style wiring.
        assert!(
            out.contains("int main(int argc, char **argv){ (void)argc;(void)argv;"),
            "program main missing:\n{out}"
        );
    }

    #[test]
    fn deferred_return_uses_temp_and_lifo_flush() {
        // fn f() i32 { defer print(1); defer print(2); return 3; }
        let f = Func {
            is_pub: false,
            name: "f".to_string(),
            params: vec![],
            ret: ty("i32"),
            body: block(vec![
                defer(print(int(1))),
                defer(print(int(2))),
                ret(int(3)),
            ]),
            span: Span::DUMMY,
        };
        let m = Module {
            items: vec![Item::Func(f)],
        };
        let out = emit(&m, &StructTable::new(), EmitMode::Program);

        let temp = out.find("int32_t __kd_ret = (3);").expect("temp missing");
        let p2 = out.find("kd_print((long long)(2));").expect("defer 2 missing");
        let p1 = out.find("kd_print((long long)(1));").expect("defer 1 missing");
        let ret_at = out.find("return __kd_ret;").expect("return temp missing");

        // Value temp first, then defers in LIFO (2 before 1), then the return.
        assert!(temp < p2, "temp must precede defers:\n{out}");
        assert!(p2 < p1, "defers must flush LIFO (2 before 1):\n{out}");
        assert!(p1 < ret_at, "defers must precede return:\n{out}");
    }

    #[test]
    fn test_mode_emits_harness_shape() {
        // test "ok" { expect(true); }
        let t = TestBlock {
            name: "ok".to_string(),
            body: block(vec![Stmt::Expr(Expr::Call {
                callee: "expect".to_string(),
                args: vec![Expr::Bool {
                    value: true,
                    span: Span::DUMMY,
                }],
                span: Span::DUMMY,
            })]),
            span: Span::DUMMY,
        };
        let m = Module {
            items: vec![Item::Test(t)],
        };
        let out = emit(&m, &StructTable::new(), EmitMode::Test);

        assert!(
            out.contains("static int kd_test_0(void) {"),
            "test fn missing:\n{out}"
        );
        assert!(out.contains("if (!(true)) {"), "expect lowering missing:\n{out}");
        assert!(out.contains("return 1;"), "fail return missing:\n{out}");
        assert!(out.contains("return 0;"), "pass return missing:\n{out}");
        // Harness driver.
        assert!(
            out.contains("if (kd_test_0() == 0) {"),
            "harness dispatch missing:\n{out}"
        );
        assert!(
            out.contains("fprintf(stderr, \"ok: %s\\n\", \"ok\");"),
            "ok print missing:\n{out}"
        );
        assert!(
            out.contains("fprintf(stderr, \"FAIL: %s\\n\", \"ok\");"),
            "fail print missing:\n{out}"
        );
        assert!(
            out.contains("fprintf(stderr, \"%d/%d tests passed\\n\", 1 - failures, 1);"),
            "summary missing:\n{out}"
        );
        assert!(out.contains("return failures;"), "exit code missing:\n{out}");
        // No user main is wired in test mode.
        assert!(
            !out.contains("kd_main()"),
            "test mode must not wire user main:\n{out}"
        );
    }

    #[test]
    fn while_continue_runs_cont_then_continues() {
        // fn g() void { while (true) : (print(9)) { continue; } }
        let body = block(vec![Stmt::Continue(Span::DUMMY)]);
        let f = Func {
            is_pub: false,
            name: "g".to_string(),
            params: vec![],
            ret: ty("void"),
            body: block(vec![Stmt::While {
                cond: Expr::Bool {
                    value: true,
                    span: Span::DUMMY,
                },
                cont: Some(Box::new(Stmt::Expr(Expr::Call {
                    callee: "print".to_string(),
                    args: vec![int(9)],
                    span: Span::DUMMY,
                }))),
                body,
                span: Span::DUMMY,
            }]),
            span: Span::DUMMY,
        };
        let m = Module {
            items: vec![Item::Func(f)],
        };
        let out = emit(&m, &StructTable::new(), EmitMode::Program);

        assert!(out.contains("while (true) {"), "while missing:\n{out}");
        let cont_call = out.find("kd_print((long long)(9));").expect("cont missing");
        let cont_kw = out.find("continue;").expect("continue missing");
        assert!(
            cont_call < cont_kw,
            "cont-expr must run before continue:\n{out}"
        );
    }

    // -- struct codegen (v0.112) -------------------------------------------

    /// A `StructTable` with `Point { x: i32, y: i32 }` at id 0.
    fn point_table() -> StructTable {
        let mut t = StructTable::new();
        let id = t.intern("Point");
        t.set_fields(
            id,
            vec![("x".to_string(), Type::I32), ("y".to_string(), Type::I32)],
        );
        t
    }

    fn finit(name: &str, value: Expr) -> FieldInit {
        FieldInit {
            name: name.to_string(),
            value,
            span: Span::DUMMY,
        }
    }

    fn field(base: Expr, name: &str) -> Expr {
        Expr::Field {
            base: Box::new(base),
            field: name.to_string(),
            span: Span::DUMMY,
        }
    }

    #[test]
    fn struct_typedef_emitted_with_prefixed_fields() {
        // The typedefs come straight off the StructTable, in declaration order.
        let structs = point_table();
        let m = Module { items: vec![] };
        let out = emit(&m, &structs, EmitMode::Program);
        assert!(
            out.contains("typedef struct { int32_t kd_x; int32_t kd_y; } kd_struct_Point;"),
            "struct typedef missing/wrong:\n{out}"
        );
    }

    #[test]
    fn empty_struct_typedef_has_unused_member() {
        let mut structs = StructTable::new();
        let id = structs.intern("Unit");
        structs.set_fields(id, vec![]);
        let m = Module { items: vec![] };
        let out = emit(&m, &structs, EmitMode::Program);
        assert!(
            out.contains("typedef struct { char _unused; } kd_struct_Unit;"),
            "empty struct typedef missing/wrong:\n{out}"
        );
    }

    #[test]
    fn field_access_emits_dot_kd_member() {
        // fn getx(p: Point) i32 { return p.x; }
        let structs = point_table();
        let f = Func {
            is_pub: false,
            name: "getx".to_string(),
            params: vec![Param {
                name: "p".to_string(),
                ty: ty("Point"),
                is_comptime: false,
                span: Span::DUMMY,
            }],
            ret: ty("i32"),
            body: block(vec![ret(field(ident("p"), "x"))]),
            span: Span::DUMMY,
        };
        let m = Module {
            items: vec![Item::Func(f)],
        };
        let out = emit(&m, &structs, EmitMode::Program);
        // Struct params are by value, typed with the struct's typedef.
        assert!(
            out.contains("kd_struct_Point kd_p"),
            "struct param type wrong:\n{out}"
        );
        // Field access lowers to `(<base>).kd_<field>`.
        assert!(
            out.contains("(kd_p).kd_x"),
            "field access lowering wrong:\n{out}"
        );
    }

    #[test]
    fn struct_literal_emits_compound_literal() {
        // fn make() Point { return Point{ .x = 1, .y = 2 }; }
        let structs = point_table();
        let lit = Expr::StructLit {
            name: "Point".to_string(),
            fields: vec![finit("x", int(1)), finit("y", int(2))],
            span: Span::DUMMY,
        };
        let f = Func {
            is_pub: false,
            name: "make".to_string(),
            params: vec![],
            ret: ty("Point"),
            body: block(vec![ret(lit)]),
            span: Span::DUMMY,
        };
        let m = Module {
            items: vec![Item::Func(f)],
        };
        let out = emit(&m, &structs, EmitMode::Program);
        // A struct return type uses the typedef (by value).
        assert!(
            out.contains("kd_struct_Point kd_make("),
            "struct return type wrong:\n{out}"
        );
        // C99 compound literal with kd_-prefixed designators.
        assert!(
            out.contains("((kd_struct_Point){ .kd_x = 1, .kd_y = 2 })"),
            "struct literal lowering wrong:\n{out}"
        );
    }

    #[test]
    fn empty_struct_literal_uses_zero_init() {
        let mut structs = StructTable::new();
        let id = structs.intern("Unit");
        structs.set_fields(id, vec![]);
        // fn make() Unit { return Unit{}; }
        let f = Func {
            is_pub: false,
            name: "make".to_string(),
            params: vec![],
            ret: ty("Unit"),
            body: block(vec![ret(Expr::StructLit {
                name: "Unit".to_string(),
                fields: vec![],
                span: Span::DUMMY,
            })]),
            span: Span::DUMMY,
        };
        let m = Module {
            items: vec![Item::Func(f)],
        };
        let out = emit(&m, &structs, EmitMode::Program);
        assert!(
            out.contains("((kd_struct_Unit){0})"),
            "empty struct literal lowering wrong:\n{out}"
        );
    }

    #[test]
    fn field_assign_emits_assignment() {
        // fn set() void { var p: Point = Point{ .x = 0, .y = 0 }; p.x = 5; }
        let structs = point_table();
        let lit = Expr::StructLit {
            name: "Point".to_string(),
            fields: vec![finit("x", int(0)), finit("y", int(0))],
            span: Span::DUMMY,
        };
        let f = Func {
            is_pub: false,
            name: "set".to_string(),
            params: vec![],
            ret: ty("void"),
            body: block(vec![
                Stmt::Let {
                    is_const: false,
                    name: "p".to_string(),
                    ty: Some(ty("Point")),
                    value: lit,
                    span: Span::DUMMY,
                },
                Stmt::FieldAssign {
                    place: field(ident("p"), "x"),
                    op: None,
                    value: int(5),
                    span: Span::DUMMY,
                },
            ]),
            span: Span::DUMMY,
        };
        let m = Module {
            items: vec![Item::Func(f)],
        };
        let out = emit(&m, &structs, EmitMode::Program);
        // A struct-typed local uses the typedef.
        assert!(
            out.contains("kd_struct_Point kd_p ="),
            "struct local decl wrong:\n{out}"
        );
        // FieldAssign lowers to `(<place>) = (<value>);`.
        assert!(
            out.contains("((kd_p).kd_x) = (5);"),
            "field assign lowering wrong:\n{out}"
        );
    }

    #[test]
    fn nested_field_access_chains() {
        // A chain `a.b.c` nests left-associatively: `((kd_a).kd_b).kd_c`.
        let mut structs = StructTable::new();
        let inner = structs.intern("Inner");
        structs.set_fields(inner, vec![("c".to_string(), Type::I32)]);
        let outer = structs.intern("Outer");
        structs.set_fields(outer, vec![("b".to_string(), Type::Struct(inner))]);

        let f = Func {
            is_pub: false,
            name: "deep".to_string(),
            params: vec![Param {
                name: "a".to_string(),
                ty: ty("Outer"),
                is_comptime: false,
                span: Span::DUMMY,
            }],
            ret: ty("i32"),
            body: block(vec![ret(field(field(ident("a"), "b"), "c"))]),
            span: Span::DUMMY,
        };
        let m = Module {
            items: vec![Item::Func(f)],
        };
        let out = emit(&m, &structs, EmitMode::Program);
        // Both typedefs emit in declaration order; the inner one first.
        assert!(
            out.contains("typedef struct { int32_t kd_c; } kd_struct_Inner;"),
            "inner typedef wrong:\n{out}"
        );
        assert!(
            out.contains("typedef struct { kd_struct_Inner kd_b; } kd_struct_Outer;"),
            "outer typedef (struct field) wrong:\n{out}"
        );
        assert!(
            out.contains("((kd_a).kd_b).kd_c"),
            "nested field access lowering wrong:\n{out}"
        );
    }

    #[test]
    fn deferred_struct_return_uses_struct_temp() {
        // fn make() Point { defer print(1); return Point{ .x = 7, .y = 8 }; }
        // Exercises the return-temp path: `current_ret` must resolve to the
        // struct type (not a bogus `void`) so the temp carries the typedef.
        let structs = point_table();
        let lit = Expr::StructLit {
            name: "Point".to_string(),
            fields: vec![finit("x", int(7)), finit("y", int(8))],
            span: Span::DUMMY,
        };
        let f = Func {
            is_pub: false,
            name: "make".to_string(),
            params: vec![],
            ret: ty("Point"),
            body: block(vec![defer(print(int(1))), ret(lit)]),
            span: Span::DUMMY,
        };
        let m = Module {
            items: vec![Item::Func(f)],
        };
        let out = emit(&m, &structs, EmitMode::Program);
        assert!(
            out.contains("kd_struct_Point __kd_ret = (((kd_struct_Point){ .kd_x = 7, .kd_y = 8 }));"),
            "deferred struct return temp wrong:\n{out}"
        );
        assert!(
            out.contains("return __kd_ret;"),
            "deferred return temp missing:\n{out}"
        );
    }

    // -- struct methods & associated functions (v0.113) --------------------

    fn param(name: &str, ty_name: &str) -> Param {
        Param {
            name: name.to_string(),
            ty: ty(ty_name),
            is_comptime: false,
            span: Span::DUMMY,
        }
    }

    fn func(name: &str, params: Vec<Param>, ret_name: &str, body: Vec<Stmt>) -> Func {
        Func {
            is_pub: false,
            name: name.to_string(),
            params,
            ret: ty(ret_name),
            body: block(body),
            span: Span::DUMMY,
        }
    }

    fn method_call(receiver: Expr, method: &str, args: Vec<Expr>) -> Expr {
        Expr::MethodCall {
            receiver: Box::new(receiver),
            method: method.to_string(),
            args,
            span: Span::DUMMY,
        }
    }

    /// A `Counter { n: i32 }` struct table at id 0, mirroring the `methods`
    /// passed alongside it in the module (the table drives field/type
    /// resolution; the module drives function emission).
    fn counter_table() -> StructTable {
        let mut t = StructTable::new();
        let id = t.intern("Counter");
        t.set_fields(id, vec![("n".to_string(), Type::I32)]);
        t
    }

    /// `pub fn get(self: Counter) i32 { return self.n; }`
    fn counter_get() -> Func {
        let mut f = func(
            "get",
            vec![param("self", "Counter")],
            "i32",
            vec![ret(field(ident("self"), "n"))],
        );
        f.is_pub = true;
        f
    }

    /// `pub fn zero() Counter { return Counter{ .n = 0 }; }` (associated fn).
    fn counter_zero() -> Func {
        let mut f = func(
            "zero",
            vec![],
            "Counter",
            vec![ret(Expr::StructLit {
                name: "Counter".to_string(),
                fields: vec![finit("n", int(0))],
                span: Span::DUMMY,
            })],
        );
        f.is_pub = true;
        f
    }

    fn counter_struct(methods: Vec<Func>) -> Item {
        Item::Struct(StructDecl {
            is_pub: false,
            name: "Counter".to_string(),
            fields: vec![FieldDecl {
                name: "n".to_string(),
                ty: ty("i32"),
                span: Span::DUMMY,
            }],
            methods,
            span: Span::DUMMY,
        })
    }

    #[test]
    fn struct_method_emits_free_c_function() {
        // A method lowers to a free `kd_<Struct>_<method>` with `self` as an
        // ordinary by-value struct parameter, forward-declared and defined.
        let structs = counter_table();
        let m = Module {
            items: vec![counter_struct(vec![counter_get()])],
        };
        let out = emit(&m, &structs, EmitMode::Program);
        assert!(
            out.contains("int32_t kd_Counter_get(kd_struct_Counter kd_self);"),
            "method forward decl missing/wrong:\n{out}"
        );
        assert!(
            out.contains("int32_t kd_Counter_get(kd_struct_Counter kd_self) {"),
            "method definition missing/wrong:\n{out}"
        );
        // The body reuses ordinary field-access lowering.
        assert!(
            out.contains("(kd_self).kd_n"),
            "method body field access wrong:\n{out}"
        );
    }

    #[test]
    fn associated_fn_with_no_self_emits_void_params() {
        // An associated function has no `self`, so its C param list is `void`.
        let structs = counter_table();
        let m = Module {
            items: vec![counter_struct(vec![counter_zero()])],
        };
        let out = emit(&m, &structs, EmitMode::Program);
        assert!(
            out.contains("kd_struct_Counter kd_Counter_zero(void);"),
            "assoc fn forward decl missing/wrong:\n{out}"
        );
        assert!(
            out.contains("kd_struct_Counter kd_Counter_zero(void) {"),
            "assoc fn definition missing/wrong:\n{out}"
        );
    }

    #[test]
    fn method_call_passes_receiver_as_first_arg() {
        // fn use(c: Counter) i32 { return c.get(); }
        // The receiver `c` (a struct-typed param) is passed as the leading
        // `self` argument; the struct is resolved from `c`'s recorded type.
        let structs = counter_table();
        let user = func(
            "use",
            vec![param("c", "Counter")],
            "i32",
            vec![ret(method_call(ident("c"), "get", vec![]))],
        );
        let m = Module {
            items: vec![counter_struct(vec![counter_get()]), Item::Func(user)],
        };
        let out = emit(&m, &structs, EmitMode::Program);
        assert!(
            out.contains("kd_Counter_get(kd_c)"),
            "method call should pass receiver as self:\n{out}"
        );
    }

    #[test]
    fn method_call_with_extra_args_orders_self_first() {
        // pub fn bumped(self: Counter, by: i32) Counter { ... }
        // fn step(c: Counter) Counter { return c.bumped(3); }
        let structs = counter_table();
        let mut bumped = func(
            "bumped",
            vec![param("self", "Counter"), param("by", "i32")],
            "Counter",
            vec![ret(Expr::StructLit {
                name: "Counter".to_string(),
                fields: vec![finit(
                    "n",
                    Expr::Binary {
                        op: BinOp::Add,
                        lhs: Box::new(field(ident("self"), "n")),
                        rhs: Box::new(ident("by")),
                        span: Span::DUMMY,
                    },
                )],
                span: Span::DUMMY,
            })],
        );
        bumped.is_pub = true;
        let step = func(
            "step",
            vec![param("c", "Counter")],
            "Counter",
            vec![ret(method_call(ident("c"), "bumped", vec![int(3)]))],
        );
        let m = Module {
            items: vec![counter_struct(vec![bumped]), Item::Func(step)],
        };
        let out = emit(&m, &structs, EmitMode::Program);
        assert!(
            out.contains("kd_Counter_bumped(kd_c, 3)"),
            "method call must place self before args:\n{out}"
        );
    }

    #[test]
    fn associated_call_passes_only_args() {
        // fn make() Counter { return Counter.zero(); }
        // The receiver is the struct *type* name, so nothing is prepended.
        let structs = counter_table();
        let make = func(
            "make",
            vec![],
            "Counter",
            vec![ret(method_call(ident("Counter"), "zero", vec![]))],
        );
        let m = Module {
            items: vec![counter_struct(vec![counter_zero()]), Item::Func(make)],
        };
        let out = emit(&m, &structs, EmitMode::Program);
        assert!(
            out.contains("kd_Counter_zero()"),
            "associated call should pass only args:\n{out}"
        );
        // And must NOT inject a self argument.
        assert!(
            !out.contains("kd_Counter_zero(kd_"),
            "associated call must not pass a receiver:\n{out}"
        );
    }

    #[test]
    fn explicit_self_associated_call_passes_value_as_arg() {
        // fn peek(c: Counter) i32 { return Counter.get(c); }
        // The static form binds `c` to *all* params (the explicit `self`), so
        // it is emitted as an ordinary argument, not a prepended receiver.
        let structs = counter_table();
        let peek = func(
            "peek",
            vec![param("c", "Counter")],
            "i32",
            vec![ret(method_call(ident("Counter"), "get", vec![ident("c")]))],
        );
        let m = Module {
            items: vec![counter_struct(vec![counter_get()]), Item::Func(peek)],
        };
        let out = emit(&m, &structs, EmitMode::Program);
        assert!(
            out.contains("kd_Counter_get(kd_c)"),
            "explicit-self associated call wrong:\n{out}"
        );
    }

    // -- v0.134 pointer-receiver methods (true mutation) -------------------

    /// A pointer-typed param `name: *ty_name` (`self: *Counter`).
    fn ptr_param(name: &str, ty_name: &str) -> Param {
        Param {
            name: name.to_string(),
            ty: ptr_ty(ty_name),
            is_comptime: false,
            span: Span::DUMMY,
        }
    }

    /// `pub fn inc(self: *Counter) void { self.n += 1; }` — a pointer receiver
    /// that mutates the pointee in place via a compound assignment.
    fn counter_inc() -> Func {
        Func {
            is_pub: true,
            name: "inc".to_string(),
            params: vec![ptr_param("self", "Counter")],
            ret: ty("void"),
            body: block(vec![Stmt::FieldAssign {
                place: field(ident("self"), "n"),
                op: Some(BinOp::Add),
                value: int(1),
                span: Span::DUMMY,
            }]),
            span: Span::DUMMY,
        }
    }

    #[test]
    fn pointer_receiver_method_self_param_is_pointer() {
        // The `self: *Counter` parameter spells as a C pointer, and the body's
        // `self.n += 1` auto-derefs through it (SPEC §30.1, §30.3): the store
        // re-spells the same dereferenced lvalue on both sides (single eval).
        let structs = counter_table();
        let m = Module {
            items: vec![counter_struct(vec![counter_inc()])],
        };
        let out = emit(&m, &structs, EmitMode::Program);
        assert!(
            out.contains("void kd_Counter_inc(kd_struct_Counter* kd_self);"),
            "ptr-receiver forward decl should take a pointer self:\n{out}"
        );
        assert!(
            out.contains("void kd_Counter_inc(kd_struct_Counter* kd_self) {"),
            "ptr-receiver definition should take a pointer self:\n{out}"
        );
        // `self.n += 1` writes *through* the pointer (the place is re-spelled on
        // both sides; the field-assign wraps the dereferenced place in parens).
        assert!(
            out.contains("((*(kd_self)).kd_n) = ((*(kd_self)).kd_n) + (1);"),
            "compound assign through self must write through the pointer:\n{out}"
        );
    }

    #[test]
    fn pointer_receiver_call_on_value_takes_address() {
        // fn run() void { var c: Counter = Counter{ .n = 0 }; c.inc(); c.inc(); }
        // A pointer-receiver method called on a struct *value* auto-refs the
        // receiver: `kd_Counter_inc((&(kd_c)))` (SPEC §30.2) — the mutation is
        // real and updates `c` in place.
        let structs = counter_table();
        let run = Func {
            is_pub: false,
            name: "run".to_string(),
            params: vec![],
            ret: ty("void"),
            body: block(vec![
                Stmt::Let {
                    is_const: false,
                    name: "c".to_string(),
                    ty: Some(ty("Counter")),
                    value: Expr::StructLit {
                        name: "Counter".to_string(),
                        fields: vec![finit("n", int(0))],
                        span: Span::DUMMY,
                    },
                    span: Span::DUMMY,
                },
                Stmt::Expr(method_call(ident("c"), "inc", vec![])),
                Stmt::Expr(method_call(ident("c"), "inc", vec![])),
            ]),
            span: Span::DUMMY,
        };
        let m = Module {
            items: vec![counter_struct(vec![counter_inc()]), Item::Func(run)],
        };
        let out = emit(&m, &structs, EmitMode::Program);
        assert!(
            out.contains("kd_Counter_inc((&(kd_c)));"),
            "ptr-receiver call on a value must take its address:\n{out}"
        );
    }

    #[test]
    fn value_receiver_call_on_value_passes_copy_unchanged() {
        // A *value* receiver call is unchanged by v0.134: `c.get()` still passes
        // the struct value by copy — no address-of — so the original is never
        // mutated through it.
        let structs = counter_table();
        let user = func(
            "peek",
            vec![param("c", "Counter")],
            "i32",
            vec![ret(method_call(ident("c"), "get", vec![]))],
        );
        let m = Module {
            items: vec![
                counter_struct(vec![counter_get(), counter_inc()]),
                Item::Func(user),
            ],
        };
        let out = emit(&m, &structs, EmitMode::Program);
        assert!(
            out.contains("kd_Counter_get(kd_c)"),
            "value-receiver call must pass the value by copy:\n{out}"
        );
        // It must NOT take the address (that is the pointer-receiver lowering).
        assert!(
            !out.contains("kd_Counter_get((&("),
            "value-receiver call must not take an address:\n{out}"
        );
    }

    #[test]
    fn field_assign_and_read_through_pointer_param() {
        // fn bump(p: *Counter) void { p.n = 5; }   (a general `*Struct` param,
        // not just `self`) — both the write and a later read auto-deref through
        // the pointer (SPEC §30.1).
        let structs = counter_table();
        let bump = Func {
            is_pub: false,
            name: "bump".to_string(),
            params: vec![ptr_param("p", "Counter")],
            ret: ty("void"),
            body: block(vec![
                Stmt::FieldAssign {
                    place: field(ident("p"), "n"),
                    op: None,
                    value: int(5),
                    span: Span::DUMMY,
                },
                print(field(ident("p"), "n")),
            ]),
            span: Span::DUMMY,
        };
        let m = Module {
            items: vec![counter_struct(vec![]), Item::Func(bump)],
        };
        let out = emit(&m, &structs, EmitMode::Program);
        // Write through the pointer.
        assert!(
            out.contains("((*(kd_p)).kd_n) = (5);"),
            "field assign through a *Struct param must deref:\n{out}"
        );
        // Read through the pointer (the print arg auto-derefs).
        assert!(
            out.contains("(*(kd_p)).kd_n"),
            "field read through a *Struct param must deref:\n{out}"
        );
    }

    #[test]
    fn pointer_value_receiver_passes_pointer_unchanged() {
        // fn drive(p: *Counter) void { p.inc(); }
        // The receiver `p` is *already* a pointer and `inc` is a pointer
        // receiver, so it is passed straight through — no extra `&` (SPEC §30.2).
        let structs = counter_table();
        let drive = Func {
            is_pub: false,
            name: "drive".to_string(),
            params: vec![ptr_param("p", "Counter")],
            ret: ty("void"),
            body: block(vec![Stmt::Expr(method_call(ident("p"), "inc", vec![]))]),
            span: Span::DUMMY,
        };
        let m = Module {
            items: vec![counter_struct(vec![counter_inc()]), Item::Func(drive)],
        };
        let out = emit(&m, &structs, EmitMode::Program);
        assert!(
            out.contains("kd_Counter_inc(kd_p);"),
            "ptr receiver + ptr value must pass the pointer unchanged:\n{out}"
        );
        assert!(
            !out.contains("kd_Counter_inc((&("),
            "must not double-address an already-pointer receiver:\n{out}"
        );
    }

    #[test]
    fn pointer_value_value_receiver_derefs() {
        // fn peek(p: *Counter) i32 { return p.get(); }
        // `get` is a *value* receiver but `p` is a pointer, so the receiver is
        // dereferenced to pass the value by copy: `kd_Counter_get((*(kd_p)))`.
        let structs = counter_table();
        let peek = Func {
            is_pub: false,
            name: "peek".to_string(),
            params: vec![ptr_param("p", "Counter")],
            ret: ty("i32"),
            body: block(vec![ret(method_call(ident("p"), "get", vec![]))]),
            span: Span::DUMMY,
        };
        let m = Module {
            items: vec![counter_struct(vec![counter_get()]), Item::Func(peek)],
        };
        let out = emit(&m, &structs, EmitMode::Program);
        assert!(
            out.contains("kd_Counter_get((*(kd_p)))"),
            "value receiver + ptr value must deref the receiver:\n{out}"
        );
    }

    #[test]
    fn pointer_receiver_method_call_on_self_passes_self_unchanged() {
        // Inside a pointer-receiver method, calling another pointer-receiver
        // method on `self` passes `self` (already a pointer) unchanged:
        //   fn inc2(self: *Counter) void { self.inc(); self.inc(); }
        let structs = counter_table();
        let inc2 = Func {
            is_pub: true,
            name: "inc2".to_string(),
            params: vec![ptr_param("self", "Counter")],
            ret: ty("void"),
            body: block(vec![
                Stmt::Expr(method_call(ident("self"), "inc", vec![])),
                Stmt::Expr(method_call(ident("self"), "inc", vec![])),
            ]),
            span: Span::DUMMY,
        };
        let m = Module {
            items: vec![counter_struct(vec![counter_inc(), inc2])],
        };
        let out = emit(&m, &structs, EmitMode::Program);
        assert!(
            out.contains("kd_Counter_inc(kd_self);"),
            "ptr-receiver call on a ptr `self` must pass it unchanged:\n{out}"
        );
        assert!(
            !out.contains("kd_Counter_inc((&("),
            "must not address an already-pointer `self`:\n{out}"
        );
    }

    #[test]
    fn chained_method_call_resolves_struct_via_return_type() {
        // fn chain(c: Counter) i32 { return c.bumped(1).get(); }
        // The inner call returns `Counter`, so the outer `.get()` must resolve
        // to `kd_Counter_get`.
        let structs = counter_table();
        let mut bumped = func(
            "bumped",
            vec![param("self", "Counter"), param("by", "i32")],
            "Counter",
            vec![ret(ident("self"))],
        );
        bumped.is_pub = true;
        let chain = func(
            "chain",
            vec![param("c", "Counter")],
            "i32",
            vec![ret(method_call(
                method_call(ident("c"), "bumped", vec![int(1)]),
                "get",
                vec![],
            ))],
        );
        let m = Module {
            items: vec![
                counter_struct(vec![counter_get(), bumped]),
                Item::Func(chain),
            ],
        };
        let out = emit(&m, &structs, EmitMode::Program);
        assert!(
            out.contains("kd_Counter_get(kd_Counter_bumped(kd_c, 1))"),
            "chained method call should nest and resolve struct:\n{out}"
        );
    }

    #[test]
    fn method_call_on_struct_field_resolves_field_struct() {
        // Pair { a: Counter }; fn f(p: Pair) i32 { return p.a.get(); }
        // The receiver `p.a` is a struct-typed field, so `.get()` resolves to
        // `kd_Counter_get` with `(kd_p).kd_a` as the self argument.
        let mut structs = counter_table();
        let pair = structs.intern("Pair");
        let counter_id = structs.id_of("Counter").unwrap();
        structs.set_fields(pair, vec![("a".to_string(), Type::Struct(counter_id))]);

        let f = func(
            "f",
            vec![param("p", "Pair")],
            "i32",
            vec![ret(method_call(field(ident("p"), "a"), "get", vec![]))],
        );
        let pair_decl = Item::Struct(StructDecl {
            is_pub: false,
            name: "Pair".to_string(),
            fields: vec![FieldDecl {
                name: "a".to_string(),
                ty: ty("Counter"),
                span: Span::DUMMY,
            }],
            methods: vec![],
            span: Span::DUMMY,
        });
        let m = Module {
            items: vec![
                counter_struct(vec![counter_get()]),
                pair_decl,
                Item::Func(f),
            ],
        };
        let out = emit(&m, &structs, EmitMode::Program);
        assert!(
            out.contains("kd_Counter_get((kd_p).kd_a)"),
            "method call on a struct field should resolve the field's struct:\n{out}"
        );
    }

    // -- optionals (v0.114) -------------------------------------------------

    /// A `StructTable` with a single interned `?i32` (`kd_opt_int32_t`, id 0).
    fn opt_int_table() -> StructTable {
        let mut t = StructTable::new();
        t.intern_optional(Type::I32);
        t
    }

    #[test]
    fn optional_typedef_and_helpers_emitted() {
        // The typedef + inline helpers come straight off `StructTable::optionals`.
        let structs = opt_int_table();
        let m = Module { items: vec![] };
        let out = emit(&m, &structs, EmitMode::Program);
        // `<stdlib.h>` is in the prelude for `exit`.
        assert!(out.contains("#include <stdlib.h>"), "stdlib include missing:\n{out}");
        assert!(
            out.contains("typedef struct { bool has; int32_t val; } kd_opt_int32_t;"),
            "optional typedef missing/wrong:\n{out}"
        );
        assert!(
            out.contains(
                "static inline int32_t kd_opt_int32_t_orelse(kd_opt_int32_t o, int32_t d) { return o.has ? o.val : d; }"
            ),
            "orelse helper missing/wrong:\n{out}"
        );
        assert!(
            out.contains(
                "static inline int32_t kd_opt_int32_t_unwrap(kd_opt_int32_t o) { if (!o.has) { fputs(\"panic: unwrapped a null optional\\n\", stderr); exit(101); } return o.val; }"
            ),
            "unwrap helper missing/wrong:\n{out}"
        );
    }

    #[test]
    fn null_coerces_to_empty_optional() {
        // fn f() void { var x: ?i32 = null; }
        let structs = opt_int_table();
        let f = Func {
            is_pub: false,
            name: "f".to_string(),
            params: vec![],
            ret: ty("void"),
            body: block(vec![Stmt::Let {
                is_const: false,
                name: "x".to_string(),
                ty: Some(opt_ty("i32")),
                value: Expr::Null { span: Span::DUMMY },
                span: Span::DUMMY,
            }]),
            span: Span::DUMMY,
        };
        let m = Module {
            items: vec![Item::Func(f)],
        };
        let out = emit(&m, &structs, EmitMode::Program);
        // The local uses the optional typedef, and `null` widens to `.has = false`.
        assert!(
            out.contains("kd_opt_int32_t kd_x = ((kd_opt_int32_t){ .has = false });"),
            "null coercion wrong:\n{out}"
        );
    }

    #[test]
    fn value_coerces_to_present_optional() {
        // fn f() void { var x: ?i32 = 7; }  — a `T` value widens to `.has = true`.
        let structs = opt_int_table();
        let f = Func {
            is_pub: false,
            name: "f".to_string(),
            params: vec![],
            ret: ty("void"),
            body: block(vec![Stmt::Let {
                is_const: false,
                name: "x".to_string(),
                ty: Some(opt_ty("i32")),
                value: int(7),
                span: Span::DUMMY,
            }]),
            span: Span::DUMMY,
        };
        let m = Module {
            items: vec![Item::Func(f)],
        };
        let out = emit(&m, &structs, EmitMode::Program);
        assert!(
            out.contains("kd_opt_int32_t kd_x = ((kd_opt_int32_t){ .has = true, .val = 7 });"),
            "value coercion wrong:\n{out}"
        );
    }

    #[test]
    fn orelse_emits_helper_call() {
        // fn f(x: ?i32) i32 { return x orelse 0; }
        let structs = opt_int_table();
        let f = Func {
            is_pub: false,
            name: "f".to_string(),
            params: vec![Param {
                name: "x".to_string(),
                ty: opt_ty("i32"),
                is_comptime: false,
                span: Span::DUMMY,
            }],
            ret: ty("i32"),
            body: block(vec![ret(Expr::Orelse {
                lhs: Box::new(ident("x")),
                rhs: Box::new(int(0)),
                span: Span::DUMMY,
            })]),
            span: Span::DUMMY,
        };
        let m = Module {
            items: vec![Item::Func(f)],
        };
        let out = emit(&m, &structs, EmitMode::Program);
        // The `?i32` param is typed with the optional typedef.
        assert!(
            out.contains("int32_t kd_f(kd_opt_int32_t kd_x)"),
            "optional param type wrong:\n{out}"
        );
        // `orelse` lowers to the inline helper call.
        assert!(
            out.contains("kd_opt_int32_t_orelse(kd_x, 0)"),
            "orelse lowering wrong:\n{out}"
        );
    }

    #[test]
    fn unwrap_emits_helper_call() {
        // fn f(x: ?i32) i32 { return x.?; }
        let structs = opt_int_table();
        let f = Func {
            is_pub: false,
            name: "f".to_string(),
            params: vec![Param {
                name: "x".to_string(),
                ty: opt_ty("i32"),
                is_comptime: false,
                span: Span::DUMMY,
            }],
            ret: ty("i32"),
            body: block(vec![ret(Expr::Unwrap {
                expr: Box::new(ident("x")),
                span: Span::DUMMY,
            })]),
            span: Span::DUMMY,
        };
        let m = Module {
            items: vec![Item::Func(f)],
        };
        let out = emit(&m, &structs, EmitMode::Program);
        assert!(
            out.contains("kd_opt_int32_t_unwrap(kd_x)"),
            "unwrap lowering wrong:\n{out}"
        );
    }

    #[test]
    fn already_optional_value_passes_through() {
        // fn f(x: ?i32) void { var y: ?i32 = x; }
        // An expression already of type `?i32` is not re-wrapped.
        let structs = opt_int_table();
        let f = Func {
            is_pub: false,
            name: "f".to_string(),
            params: vec![Param {
                name: "x".to_string(),
                ty: opt_ty("i32"),
                is_comptime: false,
                span: Span::DUMMY,
            }],
            ret: ty("void"),
            body: block(vec![Stmt::Let {
                is_const: false,
                name: "y".to_string(),
                ty: Some(opt_ty("i32")),
                value: ident("x"),
                span: Span::DUMMY,
            }]),
            span: Span::DUMMY,
        };
        let m = Module {
            items: vec![Item::Func(f)],
        };
        let out = emit(&m, &structs, EmitMode::Program);
        assert!(
            out.contains("kd_opt_int32_t kd_y = kd_x;"),
            "already-optional value should pass through unchanged:\n{out}"
        );
    }

    #[test]
    fn optional_struct_field_coerces_in_literal() {
        // const Box = struct { v: ?i32 };
        // fn make() Box { return Box{ .v = 5 }; }
        // The field-init value `5` widens to the field's `?i32` type.
        let mut structs = StructTable::new();
        let oid = structs.intern_optional(Type::I32);
        let bid = structs.intern("Box");
        structs.set_fields(bid, vec![("v".to_string(), Type::Optional(oid))]);

        let f = Func {
            is_pub: false,
            name: "make".to_string(),
            params: vec![],
            ret: ty("Box"),
            body: block(vec![ret(Expr::StructLit {
                name: "Box".to_string(),
                fields: vec![finit("v", int(5))],
                span: Span::DUMMY,
            })]),
            span: Span::DUMMY,
        };
        let box_decl = Item::Struct(StructDecl {
            is_pub: false,
            name: "Box".to_string(),
            fields: vec![FieldDecl {
                name: "v".to_string(),
                ty: opt_ty("i32"),
                span: Span::DUMMY,
            }],
            methods: vec![],
            span: Span::DUMMY,
        });
        let m = Module {
            items: vec![box_decl, Item::Func(f)],
        };
        let out = emit(&m, &structs, EmitMode::Program);
        // The struct typedef uses the optional typedef for the field.
        assert!(
            out.contains("typedef struct { kd_opt_int32_t kd_v; } kd_struct_Box;"),
            "optional struct field typedef wrong:\n{out}"
        );
        // The literal initializer is widened.
        assert!(
            out.contains(".kd_v = ((kd_opt_int32_t){ .has = true, .val = 5 })"),
            "optional field-init coercion wrong:\n{out}"
        );
    }

    #[test]
    fn optional_return_coerces_value() {
        // fn f() ?i32 { return 9; }  — the `T` return value widens to `?i32`.
        let structs = opt_int_table();
        let f = Func {
            is_pub: false,
            name: "f".to_string(),
            params: vec![],
            ret: opt_ty("i32"),
            body: block(vec![ret(int(9))]),
            span: Span::DUMMY,
        };
        let m = Module {
            items: vec![Item::Func(f)],
        };
        let out = emit(&m, &structs, EmitMode::Program);
        assert!(
            out.contains("kd_opt_int32_t kd_f(void)"),
            "optional return type wrong:\n{out}"
        );
        assert!(
            out.contains("return (((kd_opt_int32_t){ .has = true, .val = 9 }));"),
            "optional return coercion wrong:\n{out}"
        );
    }

    // -- error unions (v0.115) ----------------------------------------------

    /// A `StructTable` with a single interned `!i32` (`kd_err_int32_t`, id 0).
    fn err_int_table() -> StructTable {
        let mut t = StructTable::new();
        t.intern_error_union(Type::I32);
        t
    }

    fn error_lit(name: &str) -> Expr {
        Expr::ErrorLit {
            name: name.to_string(),
            span: Span::DUMMY,
        }
    }

    fn call(callee: &str, args: Vec<Expr>) -> Expr {
        Expr::Call {
            callee: callee.to_string(),
            args,
            span: Span::DUMMY,
        }
    }

    fn try_expr(inner: Expr) -> Expr {
        Expr::Try {
            expr: Box::new(inner),
            span: Span::DUMMY,
        }
    }

    #[test]
    fn error_union_typedef_and_catch_emitted() {
        // The typedef + inline `_catch` come straight off `error_unions`.
        let structs = err_int_table();
        let m = Module { items: vec![] };
        let out = emit(&m, &structs, EmitMode::Program);
        assert!(
            out.contains("typedef struct { int32_t err; int32_t val; } kd_err_int32_t;"),
            "error-union typedef missing/wrong:\n{out}"
        );
        assert!(
            out.contains(
                "static inline int32_t kd_err_int32_t_catch(kd_err_int32_t e, int32_t d) { return e.err == 0 ? e.val : d; }"
            ),
            "catch helper missing/wrong:\n{out}"
        );
    }

    #[test]
    fn error_lit_coerces_to_err_code() {
        // fn f() !i32 { return error.Oops; }  — the error literal carries its code.
        let mut structs = StructTable::new();
        structs.intern_error_union(Type::I32);
        structs.intern_error("Oops"); // 1-based code 1

        let f = Func {
            is_pub: false,
            name: "f".to_string(),
            params: vec![],
            ret: err_ty("i32"),
            body: block(vec![ret(error_lit("Oops"))]),
            span: Span::DUMMY,
        };
        let m = Module {
            items: vec![Item::Func(f)],
        };
        let out = emit(&m, &structs, EmitMode::Program);
        // The `!i32` return type uses the error-union typedef.
        assert!(
            out.contains("kd_err_int32_t kd_f(void)"),
            "error-union return type wrong:\n{out}"
        );
        // `error.Oops` widens to a failure value carrying its 1-based code.
        assert!(
            out.contains("return (((kd_err_int32_t){ .err = 1 }));"),
            "error literal coercion wrong:\n{out}"
        );
    }

    #[test]
    fn value_coerces_to_success_error_union() {
        // fn f() !i32 { return 9; }  — a `T` value widens to a success `!i32`.
        let structs = err_int_table();
        let f = Func {
            is_pub: false,
            name: "f".to_string(),
            params: vec![],
            ret: err_ty("i32"),
            body: block(vec![ret(int(9))]),
            span: Span::DUMMY,
        };
        let m = Module {
            items: vec![Item::Func(f)],
        };
        let out = emit(&m, &structs, EmitMode::Program);
        assert!(
            out.contains("return (((kd_err_int32_t){ .err = 0, .val = 9 }));"),
            "success-value coercion wrong:\n{out}"
        );
    }

    #[test]
    fn catch_emits_helper_call() {
        // fn f(x: !i32) i32 { return x catch 0; }
        let structs = err_int_table();
        let f = Func {
            is_pub: false,
            name: "f".to_string(),
            params: vec![Param {
                name: "x".to_string(),
                ty: err_ty("i32"),
                is_comptime: false,
                span: Span::DUMMY,
            }],
            ret: ty("i32"),
            body: block(vec![ret(Expr::Catch {
                expr: Box::new(ident("x")),
                default: Box::new(int(0)),
                span: Span::DUMMY,
            })]),
            span: Span::DUMMY,
        };
        let m = Module {
            items: vec![Item::Func(f)],
        };
        let out = emit(&m, &structs, EmitMode::Program);
        // The `!i32` param is typed with the error-union typedef.
        assert!(
            out.contains("int32_t kd_f(kd_err_int32_t kd_x)"),
            "error-union param type wrong:\n{out}"
        );
        // `catch` lowers to the inline helper call.
        assert!(
            out.contains("kd_err_int32_t_catch(kd_x, 0)"),
            "catch lowering wrong:\n{out}"
        );
    }

    #[test]
    fn try_let_emits_temp_if_and_propagation() {
        // fn g() !i32 { return 1; }
        // fn f() !i32 { var x = try g(); return x; }
        let structs = err_int_table();
        let g = Func {
            is_pub: false,
            name: "g".to_string(),
            params: vec![],
            ret: err_ty("i32"),
            body: block(vec![ret(int(1))]),
            span: Span::DUMMY,
        };
        let f = Func {
            is_pub: false,
            name: "f".to_string(),
            params: vec![],
            ret: err_ty("i32"),
            body: block(vec![
                Stmt::Let {
                    is_const: false,
                    name: "x".to_string(),
                    ty: Some(ty("i32")),
                    value: try_expr(call("g", vec![])),
                    span: Span::DUMMY,
                },
                ret(ident("x")),
            ]),
            span: Span::DUMMY,
        };
        let m = Module {
            items: vec![Item::Func(g), Item::Func(f)],
        };
        let out = emit(&m, &structs, EmitMode::Program);
        // The temp holds the inner call's error-union value.
        assert!(
            out.contains("kd_err_int32_t __kd_try0 = kd_g();"),
            "try temp hoist wrong:\n{out}"
        );
        // On error, propagate it out of the enclosing function.
        assert!(
            out.contains("if (__kd_try0.err != 0) {"),
            "try error check missing:\n{out}"
        );
        assert!(
            out.contains("return (kd_err_int32_t){ .err = __kd_try0.err };"),
            "try error propagation wrong:\n{out}"
        );
        // The bound local takes the unwrapped payload.
        assert!(
            out.contains("int32_t kd_x = __kd_try0.val;"),
            "try payload binding wrong:\n{out}"
        );
    }

    #[test]
    fn try_return_propagates_and_wraps_payload() {
        // fn g() !i32 { return 1; }
        // fn f() !i32 { return try g(); }
        let structs = err_int_table();
        let g = Func {
            is_pub: false,
            name: "g".to_string(),
            params: vec![],
            ret: err_ty("i32"),
            body: block(vec![ret(int(1))]),
            span: Span::DUMMY,
        };
        let f = Func {
            is_pub: false,
            name: "f".to_string(),
            params: vec![],
            ret: err_ty("i32"),
            body: block(vec![ret(try_expr(call("g", vec![])))]),
            span: Span::DUMMY,
        };
        let m = Module {
            items: vec![Item::Func(g), Item::Func(f)],
        };
        let out = emit(&m, &structs, EmitMode::Program);
        assert!(
            out.contains("kd_err_int32_t __kd_try0 = kd_g();"),
            "try temp hoist wrong:\n{out}"
        );
        assert!(
            out.contains("return (kd_err_int32_t){ .err = __kd_try0.err };"),
            "try error propagation wrong:\n{out}"
        );
        // The success path wraps the unwrapped payload back into `!i32`.
        assert!(
            out.contains("return (((kd_err_int32_t){ .err = 0, .val = __kd_try0.val }));"),
            "try success wrap wrong:\n{out}"
        );
    }

    #[test]
    fn try_statement_discards_payload() {
        // fn g() !i32 { return 1; }
        // fn f() !i32 { try g(); return 0; }
        let structs = err_int_table();
        let g = Func {
            is_pub: false,
            name: "g".to_string(),
            params: vec![],
            ret: err_ty("i32"),
            body: block(vec![ret(int(1))]),
            span: Span::DUMMY,
        };
        let f = Func {
            is_pub: false,
            name: "f".to_string(),
            params: vec![],
            ret: err_ty("i32"),
            body: block(vec![Stmt::Expr(try_expr(call("g", vec![]))), ret(int(0))]),
            span: Span::DUMMY,
        };
        let m = Module {
            items: vec![Item::Func(g), Item::Func(f)],
        };
        let out = emit(&m, &structs, EmitMode::Program);
        assert!(
            out.contains("kd_err_int32_t __kd_try0 = kd_g();"),
            "try temp hoist wrong:\n{out}"
        );
        // The payload is discarded.
        assert!(
            out.contains("(void)(__kd_try0.val);"),
            "try statement discard wrong:\n{out}"
        );
    }

    #[test]
    fn try_return_flushes_defers_on_error_path() {
        // fn g() !i32 { return 1; }
        // fn f() !i32 { defer print(7); return try g(); }
        // The error path must flush active defers before propagating.
        let structs = err_int_table();
        let g = Func {
            is_pub: false,
            name: "g".to_string(),
            params: vec![],
            ret: err_ty("i32"),
            body: block(vec![ret(int(1))]),
            span: Span::DUMMY,
        };
        let f = Func {
            is_pub: false,
            name: "f".to_string(),
            params: vec![],
            ret: err_ty("i32"),
            body: block(vec![defer(print(int(7))), ret(try_expr(call("g", vec![])))]),
            span: Span::DUMMY,
        };
        let m = Module {
            items: vec![Item::Func(g), Item::Func(f)],
        };
        let out = emit(&m, &structs, EmitMode::Program);
        let check = out.find("if (__kd_try0.err != 0) {").expect("error check missing");
        let flush = out[check..]
            .find("kd_print((long long)(7));")
            .map(|i| check + i)
            .expect("defer flush on error path missing");
        let prop = out[check..]
            .find("return (kd_err_int32_t){ .err = __kd_try0.err };")
            .map(|i| check + i)
            .expect("error propagation missing");
        // Inside the error branch the defer runs before the propagation return.
        assert!(flush < prop, "defer must flush before propagation:\n{out}");
    }

    // -- named error sets (v0.139, SPEC §34) -------------------------------
    //
    // A named set `Set!T` is purely a sema constraint: it lowers to the SAME
    // `{ int32_t err; <T> val; }` as the implicit global `!T`, interned by
    // payload (§34.3). The backend must therefore IGNORE `TypeExpr.error_set`,
    // and an `Item::ErrorSet` must emit nothing (compile-time only).

    #[test]
    fn named_error_set_return_lowers_identically_to_global() {
        // fn f() FileErr!i32 { return 9; }  must emit byte-identical C to
        // fn f() !i32 { return 9; } — `error_set` is invisible to codegen.
        let structs = err_int_table();
        let make = |rty: TypeExpr| Module {
            items: vec![Item::Func(Func {
                is_pub: false,
                name: "f".to_string(),
                params: vec![],
                ret: rty,
                body: block(vec![ret(int(9))]),
                span: Span::DUMMY,
            })],
        };
        let global = emit(&make(err_ty("i32")), &structs, EmitMode::Program);
        let named = emit(&make(set_err_ty("FileErr", "i32")), &structs, EmitMode::Program);
        assert_eq!(
            global, named,
            "a named-set error union must lower identically to the global `!T`"
        );
        // Sanity: the shared lowering is the canonical success widening + typedef.
        assert!(
            named.contains("kd_err_int32_t kd_f(void)"),
            "named-set return type must use the payload-keyed typedef:\n{named}"
        );
        assert!(
            named.contains("return (((kd_err_int32_t){ .err = 0, .val = 9 }));"),
            "named-set success-value coercion wrong:\n{named}"
        );
    }

    #[test]
    fn named_error_set_error_lit_and_catch_lower_correctly() {
        // const FileErr = error{ NotFound, Denied };
        // fn f() FileErr!i32 { return error.NotFound; }
        // fn g(x: FileErr!i32) i32 { return x catch 0; }
        let mut structs = StructTable::new();
        structs.intern_error_union(Type::I32);
        structs.intern_error("NotFound"); // 1-based global code 1
        structs.intern_error("Denied"); // code 2

        let eset = Item::ErrorSet(ErrorSetDecl {
            is_pub: false,
            name: "FileErr".to_string(),
            members: vec!["NotFound".to_string(), "Denied".to_string()],
            span: Span::DUMMY,
        });
        let f = Func {
            is_pub: false,
            name: "f".to_string(),
            params: vec![],
            ret: set_err_ty("FileErr", "i32"),
            body: block(vec![ret(error_lit("NotFound"))]),
            span: Span::DUMMY,
        };
        let g = Func {
            is_pub: false,
            name: "g".to_string(),
            params: vec![Param {
                name: "x".to_string(),
                ty: set_err_ty("FileErr", "i32"),
                is_comptime: false,
                span: Span::DUMMY,
            }],
            ret: ty("i32"),
            body: block(vec![ret(Expr::Catch {
                expr: Box::new(ident("x")),
                default: Box::new(int(0)),
                span: Span::DUMMY,
            })]),
            span: Span::DUMMY,
        };
        let m = Module {
            items: vec![eset, Item::Func(f), Item::Func(g)],
        };
        let out = emit(&m, &structs, EmitMode::Program);
        // `FileErr!i32` uses the same payload-keyed error-union typedef as `!i32`.
        assert!(
            out.contains("kd_err_int32_t kd_f(void)"),
            "named-set return type wrong:\n{out}"
        );
        // `error.NotFound` widens to a failure value carrying its global code.
        assert!(
            out.contains("return (((kd_err_int32_t){ .err = 1 }));"),
            "named-set error-literal coercion wrong:\n{out}"
        );
        // The `FileErr!i32` parameter is typed with the error-union typedef.
        assert!(
            out.contains("int32_t kd_g(kd_err_int32_t kd_x)"),
            "named-set param type wrong:\n{out}"
        );
        // `x catch 0` lowers to the set-agnostic inline catch helper.
        assert!(
            out.contains("kd_err_int32_t_catch(kd_x, 0)"),
            "named-set catch lowering wrong:\n{out}"
        );
        // The set declaration emits nothing — its name never reaches the C.
        assert!(
            !out.contains("FileErr"),
            "Item::ErrorSet must emit nothing:\n{out}"
        );
    }

    #[test]
    fn named_error_set_try_propagation_identical_to_global() {
        // fn g() <ret> { return 1; }  fn f() <ret> { return try g(); }
        // with <ret> = `!i32` then `FileErr!i32`: identical propagation lowering.
        let structs = err_int_table();
        let make = |rty: TypeExpr| {
            let g = Func {
                is_pub: false,
                name: "g".to_string(),
                params: vec![],
                ret: rty.clone(),
                body: block(vec![ret(int(1))]),
                span: Span::DUMMY,
            };
            let f = Func {
                is_pub: false,
                name: "f".to_string(),
                params: vec![],
                ret: rty,
                body: block(vec![ret(try_expr(call("g", vec![])))]),
                span: Span::DUMMY,
            };
            Module {
                items: vec![Item::Func(g), Item::Func(f)],
            }
        };
        let global = emit(&make(err_ty("i32")), &structs, EmitMode::Program);
        let named = emit(&make(set_err_ty("FileErr", "i32")), &structs, EmitMode::Program);
        assert_eq!(
            global, named,
            "named-set `try` propagation must lower identically to the global `!T`"
        );
        assert!(
            named.contains("kd_err_int32_t __kd_try0 = kd_g();"),
            "named-set try temp hoist wrong:\n{named}"
        );
        assert!(
            named.contains("return (kd_err_int32_t){ .err = __kd_try0.err };"),
            "named-set try error propagation wrong:\n{named}"
        );
    }

    #[test]
    fn item_error_set_emits_nothing() {
        // A module that is `const FileErr = error{ A, B };` plus an empty `main`
        // must emit exactly what the same module WITHOUT the set emits.
        let structs = StructTable::new();
        let main = Func {
            is_pub: false,
            name: "main".to_string(),
            params: vec![],
            ret: ty("void"),
            body: block(vec![]),
            span: Span::DUMMY,
        };
        let with_set = Module {
            items: vec![
                Item::ErrorSet(ErrorSetDecl {
                    is_pub: true,
                    name: "FileErr".to_string(),
                    members: vec!["A".to_string(), "B".to_string()],
                    span: Span::DUMMY,
                }),
                Item::Func(main.clone()),
            ],
        };
        let without_set = Module {
            items: vec![Item::Func(main)],
        };
        let a = emit(&with_set, &structs, EmitMode::Program);
        let b = emit(&without_set, &structs, EmitMode::Program);
        assert_eq!(a, b, "an Item::ErrorSet must add nothing to the emitted C");
        assert!(
            !a.contains("FileErr"),
            "the set name must not appear in the emitted C:\n{a}"
        );
    }

    // -- enums & switch (v0.116) -------------------------------------------

    fn enum_lit(variant: &str) -> Expr {
        Expr::EnumLit {
            variant: variant.to_string(),
            span: Span::DUMMY,
        }
    }

    fn assign(name: &str, value: Expr) -> Stmt {
        Stmt::Assign {
            name: name.to_string(),
            op: None,
            value,
            span: Span::DUMMY,
        }
    }

    fn arm(labels: Vec<Expr>, body: Vec<Stmt>) -> SwitchArm {
        SwitchArm {
            labels,
            capture: None,
            body: block(body),
            span: Span::DUMMY,
        }
    }

    /// A union-switch arm `.variant => |cap| { body }` that captures the
    /// matched variant's payload (v0.124).
    fn cap_arm(labels: Vec<Expr>, cap: &str, body: Vec<Stmt>) -> SwitchArm {
        SwitchArm {
            labels,
            capture: Some(cap.to_string()),
            body: block(body),
            span: Span::DUMMY,
        }
    }

    /// A `StructTable` with `Color = enum { Red, Green, Blue }` at enum id 0.
    fn color_enum_table() -> StructTable {
        let mut t = StructTable::new();
        let id = t.intern_enum("Color");
        t.set_enum_variants(
            id,
            vec!["Red".to_string(), "Green".to_string(), "Blue".to_string()],
        );
        t
    }

    #[test]
    fn enum_typedef_emitted_with_indexed_enumerators() {
        // The typedef comes straight off the enum table, variants 0-based.
        let structs = color_enum_table();
        let m = Module { items: vec![] };
        let out = emit(&m, &structs, EmitMode::Program);
        assert!(
            out.contains(
                "typedef enum { kd_enum_Color_Red = 0, kd_enum_Color_Green = 1, kd_enum_Color_Blue = 2 } kd_enum_Color;"
            ),
            "enum typedef missing/wrong:\n{out}"
        );
    }

    #[test]
    fn qualified_enum_literal_lowers_to_enumerator() {
        // fn pick() Color { return Color.Green; }
        let structs = color_enum_table();
        let f = Func {
            is_pub: false,
            name: "pick".to_string(),
            params: vec![],
            ret: ty("Color"),
            body: block(vec![ret(field(ident("Color"), "Green"))]),
            span: Span::DUMMY,
        };
        let m = Module {
            items: vec![Item::Func(f)],
        };
        let out = emit(&m, &structs, EmitMode::Program);
        // An enum return type uses the enum typedef.
        assert!(
            out.contains("kd_enum_Color kd_pick(void)"),
            "enum return type wrong:\n{out}"
        );
        // `Color.Green` lowers to the enumerator (not a `.kd_Green` field access).
        assert!(
            out.contains("return (kd_enum_Color_Green);"),
            "qualified enum literal lowering wrong:\n{out}"
        );
    }

    #[test]
    fn unqualified_enum_literal_coerces_via_expected_type() {
        // fn f() void { var c: Color = .Blue; }
        let structs = color_enum_table();
        let f = Func {
            is_pub: false,
            name: "f".to_string(),
            params: vec![],
            ret: ty("void"),
            body: block(vec![Stmt::Let {
                is_const: false,
                name: "c".to_string(),
                ty: Some(ty("Color")),
                value: enum_lit("Blue"),
                span: Span::DUMMY,
            }]),
            span: Span::DUMMY,
        };
        let m = Module {
            items: vec![Item::Func(f)],
        };
        let out = emit(&m, &structs, EmitMode::Program);
        // The enum-typed local uses the enum typedef; `.Blue` resolves via the
        // expected type to its enumerator.
        assert!(
            out.contains("kd_enum_Color kd_c = kd_enum_Color_Blue;"),
            "unqualified enum literal coercion wrong:\n{out}"
        );
    }

    #[test]
    fn switch_emits_c_switch_with_cases_break_and_default() {
        // fn f(c: Color) i32 {
        //     var r: i32 = 0;
        //     switch (c) {
        //         .Red => { r = 1; }
        //         .Green, .Blue => { r = 2; }
        //         else => { r = 3; }
        //     }
        //     return r;
        // }
        let structs = color_enum_table();
        let sw = Stmt::Switch {
            scrutinee: ident("c"),
            arms: vec![
                arm(vec![enum_lit("Red")], vec![assign("r", int(1))]),
                arm(
                    vec![enum_lit("Green"), enum_lit("Blue")],
                    vec![assign("r", int(2))],
                ),
            ],
            default: Some(block(vec![assign("r", int(3))])),
            span: Span::DUMMY,
        };
        let f = Func {
            is_pub: false,
            name: "f".to_string(),
            params: vec![param("c", "Color")],
            ret: ty("i32"),
            body: block(vec![
                Stmt::Let {
                    is_const: false,
                    name: "r".to_string(),
                    ty: Some(ty("i32")),
                    value: int(0),
                    span: Span::DUMMY,
                },
                sw,
                ret(ident("r")),
            ]),
            span: Span::DUMMY,
        };
        let m = Module {
            items: vec![Item::Func(f)],
        };
        let out = emit(&m, &structs, EmitMode::Program);
        // The enum-typed param uses the enum typedef.
        assert!(
            out.contains("kd_enum_Color kd_c"),
            "enum param type wrong:\n{out}"
        );
        // The C switch header dispatches on the (kd_-prefixed) scrutinee.
        assert!(out.contains("switch (kd_c) {"), "switch header missing:\n{out}");
        // The single-label arm opens its body block.
        assert!(
            out.contains("case kd_enum_Color_Red: {"),
            "first case (body) missing:\n{out}"
        );
        // Shared labels: the first is a bare `case`, the last opens the body.
        assert!(
            out.contains("case kd_enum_Color_Green:"),
            "shared label 1 missing:\n{out}"
        );
        assert!(
            out.contains("case kd_enum_Color_Blue: {"),
            "shared label 2 (body) missing:\n{out}"
        );
        // Each arm ends with a break so control never falls through.
        assert!(out.contains("} break;"), "arm break missing:\n{out}");
        // The source `else` becomes a C `default:`.
        assert!(out.contains("default: {"), "default arm missing:\n{out}");
    }

    #[test]
    fn enum_switch_without_else_emits_no_default() {
        // fn f(c: Color) i32 {
        //     switch (c) { .Red => { return 1; } .Green => { return 2; } .Blue => { return 3; } }
        // }
        // An exhaustive enum switch with no `else` emits no `default:`.
        let structs = color_enum_table();
        let sw = Stmt::Switch {
            scrutinee: ident("c"),
            arms: vec![
                arm(vec![enum_lit("Red")], vec![ret(int(1))]),
                arm(vec![enum_lit("Green")], vec![ret(int(2))]),
                arm(vec![enum_lit("Blue")], vec![ret(int(3))]),
            ],
            default: None,
            span: Span::DUMMY,
        };
        let f = Func {
            is_pub: false,
            name: "f".to_string(),
            params: vec![param("c", "Color")],
            ret: ty("i32"),
            body: block(vec![sw]),
            span: Span::DUMMY,
        };
        let m = Module {
            items: vec![Item::Func(f)],
        };
        let out = emit(&m, &structs, EmitMode::Program);
        assert!(out.contains("switch (kd_c) {"), "switch header missing:\n{out}");
        assert!(
            out.contains("case kd_enum_Color_Blue: {"),
            "blue case missing:\n{out}"
        );
        assert!(
            !out.contains("default:"),
            "exhaustive enum switch must not emit a default:\n{out}"
        );
    }

    #[test]
    fn integer_switch_emits_int_cases_and_default() {
        // fn f(n: i32) i32 {
        //     var r: i32 = 0;
        //     switch (n) { 1 => { r = 10; } 2, 3 => { r = 20; } else => { r = 0; } }
        //     return r;
        // }
        let structs = StructTable::new();
        let sw = Stmt::Switch {
            scrutinee: ident("n"),
            arms: vec![
                arm(vec![int(1)], vec![assign("r", int(10))]),
                arm(vec![int(2), int(3)], vec![assign("r", int(20))]),
            ],
            default: Some(block(vec![assign("r", int(0))])),
            span: Span::DUMMY,
        };
        let f = Func {
            is_pub: false,
            name: "f".to_string(),
            params: vec![param("n", "i32")],
            ret: ty("i32"),
            body: block(vec![
                Stmt::Let {
                    is_const: false,
                    name: "r".to_string(),
                    ty: Some(ty("i32")),
                    value: int(0),
                    span: Span::DUMMY,
                },
                sw,
                ret(ident("r")),
            ]),
            span: Span::DUMMY,
        };
        let m = Module {
            items: vec![Item::Func(f)],
        };
        let out = emit(&m, &structs, EmitMode::Program);
        assert!(out.contains("switch (kd_n) {"), "switch header missing:\n{out}");
        assert!(out.contains("case 1: {"), "int case 1 missing:\n{out}");
        assert!(out.contains("case 2:"), "shared int label missing:\n{out}");
        assert!(out.contains("case 3: {"), "int case 3 (body) missing:\n{out}");
        assert!(out.contains("default: {"), "int switch default missing:\n{out}");
    }

    #[test]
    fn switch_arm_flushes_defer_at_arm_exit() {
        // fn f(c: Color) void {
        //     switch (c) { .Red => { defer print(7); print(1); } else => {} }
        // }
        // A defer registered inside an arm flushes at that arm's block exit
        // (before the trailing `break;`), in LIFO order — reusing emit_block.
        let structs = color_enum_table();
        let sw = Stmt::Switch {
            scrutinee: ident("c"),
            arms: vec![arm(
                vec![enum_lit("Red")],
                vec![defer(print(int(7))), print(int(1))],
            )],
            default: Some(block(vec![])),
            span: Span::DUMMY,
        };
        let f = Func {
            is_pub: false,
            name: "f".to_string(),
            params: vec![param("c", "Color")],
            ret: ty("void"),
            body: block(vec![sw]),
            span: Span::DUMMY,
        };
        let m = Module {
            items: vec![Item::Func(f)],
        };
        let out = emit(&m, &structs, EmitMode::Program);
        let body = out.find("kd_print((long long)(1));").expect("arm body missing");
        let deferred = out[body..]
            .find("kd_print((long long)(7));")
            .map(|i| body + i)
            .expect("deferred call missing");
        let brk = out[body..]
            .find("} break;")
            .map(|i| body + i)
            .expect("arm break missing");
        // The body runs, then the defer flushes, then the arm's `break`.
        assert!(body < deferred, "defer must flush after body:\n{out}");
        assert!(deferred < brk, "defer must flush before the break:\n{out}");
    }

    #[test]
    fn enum_struct_field_typedef_orders_enum_first() {
        // const Pixel = struct { c: Color };  — a struct embedding an enum.
        // The enum typedef must precede the struct typedef that embeds it.
        let mut structs = color_enum_table();
        let color_id = structs.enum_id_of("Color").unwrap();
        let pid = structs.intern("Pixel");
        structs.set_fields(pid, vec![("c".to_string(), Type::Enum(color_id))]);
        let m = Module { items: vec![] };
        let out = emit(&m, &structs, EmitMode::Program);
        let enum_at = out
            .find("typedef enum { kd_enum_Color_Red = 0")
            .expect("enum typedef missing");
        let struct_at = out
            .find("typedef struct { kd_enum_Color kd_c; } kd_struct_Pixel;")
            .expect("struct-with-enum-field typedef missing/wrong");
        assert!(
            enum_at < struct_at,
            "enum typedef must precede the struct that embeds it:\n{out}"
        );
    }

    // -- tagged unions & switch capture (v0.124) ---------------------------

    /// A `StructTable` with `Shape = union(enum) { circle: i32, rect: i64 }` at
    /// union id 0.
    fn shape_union_table() -> StructTable {
        let mut t = StructTable::new();
        let id = t.intern_union("Shape");
        t.set_union_variants(
            id,
            vec![
                ("circle".to_string(), Type::I32),
                ("rect".to_string(), Type::I64),
            ],
        );
        t
    }

    /// A union construction `Name{ .variant = value }` (reuses the struct-
    /// literal AST shape, SPEC §20.1).
    fn union_lit(name: &str, variant: &str, value: Expr) -> Expr {
        Expr::StructLit {
            name: name.to_string(),
            fields: vec![finit(variant, value)],
            span: Span::DUMMY,
        }
    }

    #[test]
    fn union_typedef_emitted_as_tagged_struct() {
        // The typedef comes straight off the union table: a tag plus an
        // anonymous C union of every payload, keyed `data.kd_<variant>`.
        let structs = shape_union_table();
        let m = Module { items: vec![] };
        let out = emit(&m, &structs, EmitMode::Program);
        assert!(
            out.contains(
                "typedef struct { int32_t tag; union { int32_t kd_circle; int64_t kd_rect; } data; } kd_union_Shape;"
            ),
            "union typedef missing/wrong:\n{out}"
        );
    }

    #[test]
    fn union_construction_emits_tagged_compound_literal() {
        // fn make() Shape { return Shape{ .circle = 5 }; }
        let structs = shape_union_table();
        let f = func(
            "make",
            vec![],
            "Shape",
            vec![ret(union_lit("Shape", "circle", int(5)))],
        );
        let m = Module {
            items: vec![Item::Func(f)],
        };
        let out = emit(&m, &structs, EmitMode::Program);
        // The union return type uses the union typedef.
        assert!(
            out.contains("kd_union_Shape kd_make(void)"),
            "union return type wrong:\n{out}"
        );
        // The first variant lowers to `.tag = 0` and the named union member.
        assert!(
            out.contains("((kd_union_Shape){ .tag = 0, .data = { .kd_circle = 5 } })"),
            "union construction lowering wrong:\n{out}"
        );
    }

    #[test]
    fn union_construction_second_variant_uses_its_tag_and_member() {
        // fn make() Shape { return Shape{ .rect = 10 }; }  — `.rect` is tag 1.
        let structs = shape_union_table();
        let f = func(
            "make",
            vec![],
            "Shape",
            vec![ret(union_lit("Shape", "rect", int(10)))],
        );
        let m = Module {
            items: vec![Item::Func(f)],
        };
        let out = emit(&m, &structs, EmitMode::Program);
        assert!(
            out.contains("((kd_union_Shape){ .tag = 1, .data = { .kd_rect = 10 } })"),
            "second-variant union construction wrong:\n{out}"
        );
    }

    #[test]
    fn union_switch_dispatches_on_tag_and_binds_capture() {
        // fn pick(s: Shape) i64 {
        //     switch (s) {
        //         .circle => |r| { return r; }
        //         .rect   => |w| { return w; }
        //     }
        // }
        let structs = shape_union_table();
        let sw = Stmt::Switch {
            scrutinee: ident("s"),
            arms: vec![
                cap_arm(vec![enum_lit("circle")], "r", vec![ret(ident("r"))]),
                cap_arm(vec![enum_lit("rect")], "w", vec![ret(ident("w"))]),
            ],
            default: None,
            span: Span::DUMMY,
        };
        let f = func("pick", vec![param("s", "Shape")], "i64", vec![sw]);
        let m = Module {
            items: vec![Item::Func(f)],
        };
        let out = emit(&m, &structs, EmitMode::Program);
        // A union-typed param uses the union typedef.
        assert!(
            out.contains("kd_union_Shape kd_s"),
            "union param type wrong:\n{out}"
        );
        // The C switch dispatches on the runtime `.tag`.
        assert!(
            out.contains("switch ((kd_s).tag) {"),
            "tag dispatch header missing:\n{out}"
        );
        // Variant labels become their 0-based tag index.
        assert!(out.contains("case 0: {"), "circle tag case missing:\n{out}");
        assert!(out.contains("case 1: {"), "rect tag case missing:\n{out}");
        // Each capture binds the matched variant's payload from `.data.kd_<v>`,
        // typed by that variant's payload (i32 for circle, i64 for rect).
        assert!(
            out.contains("int32_t kd_r = (kd_s).data.kd_circle;"),
            "circle capture binding wrong:\n{out}"
        );
        assert!(
            out.contains("int64_t kd_w = (kd_s).data.kd_rect;"),
            "rect capture binding wrong:\n{out}"
        );
        // Each arm ends with a break.
        assert!(out.contains("} break;"), "arm break missing:\n{out}");
        // An exhaustive union switch with no `else` emits no `default:`.
        assert!(
            !out.contains("default:"),
            "exhaustive union switch must not emit a default:\n{out}"
        );
    }

    #[test]
    fn union_switch_else_lowers_to_default() {
        // fn pick(s: Shape) i64 {
        //     var r: i64 = 0;
        //     switch (s) { .circle => |c| { r = c; } else => { r = 9; } }
        //     return r;
        // }
        let structs = shape_union_table();
        let sw = Stmt::Switch {
            scrutinee: ident("s"),
            arms: vec![cap_arm(
                vec![enum_lit("circle")],
                "c",
                vec![assign("r", ident("c"))],
            )],
            default: Some(block(vec![assign("r", int(9))])),
            span: Span::DUMMY,
        };
        let f = func(
            "pick",
            vec![param("s", "Shape")],
            "i64",
            vec![
                Stmt::Let {
                    is_const: false,
                    name: "r".to_string(),
                    ty: Some(ty("i64")),
                    value: int(0),
                    span: Span::DUMMY,
                },
                sw,
                ret(ident("r")),
            ],
        );
        let m = Module {
            items: vec![Item::Func(f)],
        };
        let out = emit(&m, &structs, EmitMode::Program);
        assert!(
            out.contains("switch ((kd_s).tag) {"),
            "tag dispatch header missing:\n{out}"
        );
        assert!(out.contains("case 0: {"), "circle case missing:\n{out}");
        assert!(
            out.contains("int32_t kd_c = (kd_s).data.kd_circle;"),
            "capture binding missing:\n{out}"
        );
        // The source `else` becomes a C `default:`.
        assert!(out.contains("default: {"), "else must lower to default:\n{out}");
    }

    #[test]
    fn union_payload_struct_typedef_emitted_before_union() {
        // const Wrap = union(enum) { p: Point, n: i32 };
        // The struct payload typedef must precede the union that embeds it.
        let mut structs = point_table();
        let pid = structs.id_of("Point").unwrap();
        let uid = structs.intern_union("Wrap");
        structs.set_union_variants(
            uid,
            vec![
                ("p".to_string(), Type::Struct(pid)),
                ("n".to_string(), Type::I32),
            ],
        );
        let m = Module { items: vec![] };
        let out = emit(&m, &structs, EmitMode::Program);
        let point_at = out
            .find("} kd_struct_Point;")
            .expect("Point struct typedef missing");
        let wrap_at = out
            .find("} kd_union_Wrap;")
            .expect("Wrap union typedef missing");
        assert!(
            point_at < wrap_at,
            "payload struct typedef must precede the union that embeds it:\n{out}"
        );
        // The union embeds the struct payload by its C name inside `data`.
        assert!(
            out.contains("union { kd_struct_Point kd_p; int32_t kd_n; } data; } kd_union_Wrap;"),
            "union payload members wrong:\n{out}"
        );
    }

    // -- fixed-size arrays (v0.117) ----------------------------------------

    /// A `StructTable` with a single interned `[3]i32` (`kd_arr_int32_t_3`).
    fn arr_int_table() -> StructTable {
        let mut t = StructTable::new();
        t.intern_array(Type::I32, 3);
        t
    }

    fn index(base: Expr, idx: Expr) -> Expr {
        Expr::Index {
            base: Box::new(base),
            index: Box::new(idx),
            span: Span::DUMMY,
        }
    }

    #[test]
    fn array_typedef_and_get_emitted() {
        // The typedef + inline bounds-checked `_get` come straight off the
        // interned array table.
        let structs = arr_int_table();
        let m = Module { items: vec![] };
        let out = emit(&m, &structs, EmitMode::Program);
        assert!(
            out.contains("typedef struct { int32_t data[3]; } kd_arr_int32_t_3;"),
            "array typedef missing/wrong:\n{out}"
        );
        assert!(
            out.contains(
                "static inline int32_t kd_arr_int32_t_3_get(kd_arr_int32_t_3 a, int64_t i) { if (i < 0 || (uint64_t)i >= 3) { fputs(\"panic: array index out of bounds\\n\", stderr); exit(101); } return a.data[i]; }"
            ),
            "array _get helper missing/wrong:\n{out}"
        );
    }

    #[test]
    fn array_literal_emits_compound_literal() {
        // fn make() [3]i32 { return [3]i32{ 1, 2, 3 }; }
        let structs = arr_int_table();
        let lit = Expr::ArrayLit {
            elem: arr_ty("i32", 3),
            elems: vec![int(1), int(2), int(3)],
            span: Span::DUMMY,
        };
        let f = Func {
            is_pub: false,
            name: "make".to_string(),
            params: vec![],
            ret: arr_ty("i32", 3),
            body: block(vec![ret(lit)]),
            span: Span::DUMMY,
        };
        let m = Module {
            items: vec![Item::Func(f)],
        };
        let out = emit(&m, &structs, EmitMode::Program);
        // The array return type uses the array typedef (by value).
        assert!(
            out.contains("kd_arr_int32_t_3 kd_make(void)"),
            "array return type wrong:\n{out}"
        );
        // C99 compound literal initialising the wrapped `data` member.
        assert!(
            out.contains("((kd_arr_int32_t_3){ .data = { 1, 2, 3 } })"),
            "array literal lowering wrong:\n{out}"
        );
    }

    #[test]
    fn index_read_emits_get_call() {
        // fn at(a: [3]i32, i: i32) i32 { return a[i]; }
        let structs = arr_int_table();
        let f = Func {
            is_pub: false,
            name: "at".to_string(),
            params: vec![
                Param {
                    name: "a".to_string(),
                    ty: arr_ty("i32", 3),
                    is_comptime: false,
                    span: Span::DUMMY,
                },
                param("i", "i32"),
            ],
            ret: ty("i32"),
            body: block(vec![ret(index(ident("a"), ident("i")))]),
            span: Span::DUMMY,
        };
        let m = Module {
            items: vec![Item::Func(f)],
        };
        let out = emit(&m, &structs, EmitMode::Program);
        // The array param is by value, typed with the array typedef.
        assert!(
            out.contains("kd_arr_int32_t_3 kd_a"),
            "array param type wrong:\n{out}"
        );
        // Indexing lowers to the bounds-checked `_get` helper call.
        assert!(
            out.contains("kd_arr_int32_t_3_get(kd_a, kd_i)"),
            "index read lowering wrong:\n{out}"
        );
    }

    #[test]
    fn array_len_emits_uintptr_constant() {
        // fn n(a: [3]i32) usize { return a.len; }
        let structs = arr_int_table();
        let f = Func {
            is_pub: false,
            name: "n".to_string(),
            params: vec![Param {
                name: "a".to_string(),
                ty: arr_ty("i32", 3),
                is_comptime: false,
                span: Span::DUMMY,
            }],
            ret: ty("usize"),
            body: block(vec![ret(field(ident("a"), "len"))]),
            span: Span::DUMMY,
        };
        let m = Module {
            items: vec![Item::Func(f)],
        };
        let out = emit(&m, &structs, EmitMode::Program);
        // `a.len` is the compile-time length as a `usize` constant — not a
        // `.kd_len` member access.
        assert!(
            out.contains("return (((uintptr_t)3));"),
            "array len lowering wrong:\n{out}"
        );
        assert!(
            !out.contains(".kd_len"),
            "array len must not lower to a struct field access:\n{out}"
        );
    }

    #[test]
    fn index_assign_emits_bounds_checked_block() {
        // fn set(a: [3]i32) void { a[0] = 5; }
        let structs = arr_int_table();
        let f = Func {
            is_pub: false,
            name: "set".to_string(),
            params: vec![Param {
                name: "a".to_string(),
                ty: arr_ty("i32", 3),
                is_comptime: false,
                span: Span::DUMMY,
            }],
            ret: ty("void"),
            body: block(vec![Stmt::FieldAssign {
                place: index(ident("a"), int(0)),
                op: None,
                value: int(5),
                span: Span::DUMMY,
            }]),
            span: Span::DUMMY,
        };
        let m = Module {
            items: vec![Item::Func(f)],
        };
        let out = emit(&m, &structs, EmitMode::Program);
        assert!(
            out.contains(
                "{ int64_t __kd_idx0 = (0); if (__kd_idx0 < 0 || (uint64_t)__kd_idx0 >= 3) { fputs(\"panic: array index out of bounds\\n\", stderr); exit(101); } (kd_a).data[__kd_idx0] = (5); }"
            ),
            "index assign lowering wrong:\n{out}"
        );
    }

    // -- v0.131 compound assignment (SPEC §27.3) ----------------------------

    fn compound_assign(name: &str, op: BinOp, value: Expr) -> Stmt {
        Stmt::Assign {
            name: name.to_string(),
            op: Some(op),
            value,
            span: Span::DUMMY,
        }
    }

    #[test]
    fn compound_name_assign_lowers_to_self_op() {
        // fn go() void { var x: i32 = 0; x += 2; x -= 1; x *= 4; x /= 3; x %= 5; }
        // Each compound `x op= e` re-spells the var on the RHS: a var read is
        // free, so `kd_x = kd_x <c-op> (e);`.
        let f = Func {
            is_pub: false,
            name: "go".to_string(),
            params: vec![],
            ret: ty("void"),
            body: block(vec![
                Stmt::Let {
                    is_const: false,
                    name: "x".to_string(),
                    ty: Some(ty("i32")),
                    value: int(0),
                    span: Span::DUMMY,
                },
                compound_assign("x", BinOp::Add, int(2)),
                compound_assign("x", BinOp::Sub, int(1)),
                compound_assign("x", BinOp::Mul, int(4)),
                compound_assign("x", BinOp::Div, int(3)),
                compound_assign("x", BinOp::Rem, int(5)),
            ]),
            span: Span::DUMMY,
        };
        let m = Module {
            items: vec![Item::Func(f)],
        };
        let out = emit(&m, &StructTable::new(), EmitMode::Program);
        assert!(out.contains("kd_x = kd_x + (2);"), "+= lowering wrong:\n{out}");
        assert!(out.contains("kd_x = kd_x - (1);"), "-= lowering wrong:\n{out}");
        assert!(out.contains("kd_x = kd_x * (4);"), "*= lowering wrong:\n{out}");
        assert!(out.contains("kd_x = kd_x / (3);"), "/= lowering wrong:\n{out}");
        assert!(out.contains("kd_x = kd_x % (5);"), "%= lowering wrong:\n{out}");
    }

    #[test]
    fn compound_index_assign_hoists_index_once() {
        // fn go(a: [3]i32) void { a[2] += 5; }
        // The bounds-checked block hoists the index into a single `__kd_idx0`;
        // the compound store reads and writes that one slot — `i` is evaluated
        // exactly once (SPEC §27.3).
        let structs = arr_int_table();
        let f = Func {
            is_pub: false,
            name: "go".to_string(),
            params: vec![Param {
                name: "a".to_string(),
                ty: arr_ty("i32", 3),
                is_comptime: false,
                span: Span::DUMMY,
            }],
            ret: ty("void"),
            body: block(vec![Stmt::FieldAssign {
                place: index(ident("a"), int(2)),
                op: Some(BinOp::Add),
                value: int(5),
                span: Span::DUMMY,
            }]),
            span: Span::DUMMY,
        };
        let m = Module {
            items: vec![Item::Func(f)],
        };
        let out = emit(&m, &structs, EmitMode::Program);
        // Read and write go through the same hoisted index temp.
        assert!(
            out.contains("(kd_a).data[__kd_idx0] = (kd_a).data[__kd_idx0] + (5);"),
            "compound index store wrong:\n{out}"
        );
        // The index expression is hoisted exactly once (single evaluation).
        assert_eq!(
            out.matches("int64_t __kd_idx0 =").count(),
            1,
            "index must be hoisted exactly once:\n{out}"
        );
    }

    #[test]
    fn compound_slice_index_assign_hoists_index_once() {
        // fn go(s: []i32) void { s[1] -= 4; }  — slices write through `.ptr`.
        let structs = slice_int_table();
        let f = Func {
            is_pub: false,
            name: "go".to_string(),
            params: vec![Param {
                name: "s".to_string(),
                ty: slice_ty("i32"),
                is_comptime: false,
                span: Span::DUMMY,
            }],
            ret: ty("void"),
            body: block(vec![Stmt::FieldAssign {
                place: index(ident("s"), int(1)),
                op: Some(BinOp::Sub),
                value: int(4),
                span: Span::DUMMY,
            }]),
            span: Span::DUMMY,
        };
        let m = Module {
            items: vec![Item::Func(f)],
        };
        let out = emit(&m, &structs, EmitMode::Program);
        assert!(
            out.contains("(kd_s).ptr[__kd_idx0] = (kd_s).ptr[__kd_idx0] - (4);"),
            "compound slice store wrong:\n{out}"
        );
        assert_eq!(
            out.matches("int64_t __kd_idx0 =").count(),
            1,
            "index must be hoisted exactly once:\n{out}"
        );
    }

    #[test]
    fn compound_field_assign_respells_place() {
        // fn set() void { var p: Point = Point{.x=0,.y=0}; p.x *= 3; }
        // A field access is side-effect-free, so the place is re-spelled on both
        // sides of the store (SPEC §27.3).
        let structs = point_table();
        let lit = Expr::StructLit {
            name: "Point".to_string(),
            fields: vec![finit("x", int(0)), finit("y", int(0))],
            span: Span::DUMMY,
        };
        let f = Func {
            is_pub: false,
            name: "set".to_string(),
            params: vec![],
            ret: ty("void"),
            body: block(vec![
                Stmt::Let {
                    is_const: false,
                    name: "p".to_string(),
                    ty: Some(ty("Point")),
                    value: lit,
                    span: Span::DUMMY,
                },
                Stmt::FieldAssign {
                    place: field(ident("p"), "x"),
                    op: Some(BinOp::Mul),
                    value: int(3),
                    span: Span::DUMMY,
                },
            ]),
            span: Span::DUMMY,
        };
        let m = Module {
            items: vec![Item::Func(f)],
        };
        let out = emit(&m, &structs, EmitMode::Program);
        assert!(
            out.contains("((kd_p).kd_x) = ((kd_p).kd_x) * (3);"),
            "compound field store wrong:\n{out}"
        );
    }

    #[test]
    fn compound_continue_clause_lowers_self_op() {
        // fn go() void { var i: i32 = 0; while (i < 3) : (i += 1) {} }
        // The continue-clause `i += 1` lowers like any compound name-assign.
        let f = Func {
            is_pub: false,
            name: "go".to_string(),
            params: vec![],
            ret: ty("void"),
            body: block(vec![
                Stmt::Let {
                    is_const: false,
                    name: "i".to_string(),
                    ty: Some(ty("i32")),
                    value: int(0),
                    span: Span::DUMMY,
                },
                Stmt::While {
                    cond: Expr::Binary {
                        op: BinOp::Lt,
                        lhs: Box::new(ident("i")),
                        rhs: Box::new(int(3)),
                        span: Span::DUMMY,
                    },
                    cont: Some(Box::new(compound_assign("i", BinOp::Add, int(1)))),
                    body: block(vec![]),
                    span: Span::DUMMY,
                },
            ]),
            span: Span::DUMMY,
        };
        let m = Module {
            items: vec![Item::Func(f)],
        };
        let out = emit(&m, &StructTable::new(), EmitMode::Program);
        assert!(
            out.contains("kd_i = kd_i + (1);"),
            "compound continue-clause lowering wrong:\n{out}"
        );
    }

    #[test]
    fn empty_array_literal_uses_zero_init() {
        // fn make() [0]i32 { return [0]i32{}; }  — a zero-length array.
        let mut structs = StructTable::new();
        structs.intern_array(Type::I32, 0);
        let f = Func {
            is_pub: false,
            name: "make".to_string(),
            params: vec![],
            ret: arr_ty("i32", 0),
            body: block(vec![ret(Expr::ArrayLit {
                elem: arr_ty("i32", 0),
                elems: vec![],
                span: Span::DUMMY,
            })]),
            span: Span::DUMMY,
        };
        let m = Module {
            items: vec![Item::Func(f)],
        };
        let out = emit(&m, &structs, EmitMode::Program);
        assert!(
            out.contains("((kd_arr_int32_t_0){0})"),
            "empty array literal lowering wrong:\n{out}"
        );
    }

    #[test]
    fn struct_field_array_typedef_orders_array_first() {
        // const Buf = struct { xs: [2]i32 };  — a struct embedding an array.
        // The array typedef must precede the struct typedef that embeds it.
        let mut structs = StructTable::new();
        let aid = structs.intern_array(Type::I32, 2);
        let bid = structs.intern("Buf");
        structs.set_fields(bid, vec![("xs".to_string(), Type::Array(aid))]);
        let m = Module { items: vec![] };
        let out = emit(&m, &structs, EmitMode::Program);
        let arr_at = out
            .find("typedef struct { int32_t data[2]; } kd_arr_int32_t_2;")
            .expect("array typedef missing");
        let struct_at = out
            .find("typedef struct { kd_arr_int32_t_2 kd_xs; } kd_struct_Buf;")
            .expect("struct-with-array-field typedef missing/wrong");
        assert!(
            arr_at < struct_at,
            "array typedef must precede the struct that embeds it:\n{out}"
        );
    }

    #[test]
    fn array_of_struct_orders_struct_first() {
        // A `[2]Point` array of a struct: the struct typedef must precede the
        // array typedef that embeds it by value.
        let mut structs = point_table();
        let pid = structs.id_of("Point").unwrap();
        structs.intern_array(Type::Struct(pid), 2);
        let m = Module { items: vec![] };
        let out = emit(&m, &structs, EmitMode::Program);
        let struct_at = out
            .find("kd_struct_Point;")
            .expect("Point struct typedef missing");
        let arr_at = out
            .find("typedef struct { kd_struct_Point data[2]; } kd_arr_struct_Point_2;")
            .expect("array-of-struct typedef missing/wrong");
        assert!(
            struct_at < arr_at,
            "struct typedef must precede the array that embeds it:\n{out}"
        );
    }

    // -- pointers & slices (v0.118) ----------------------------------------

    fn addr_of(place: Expr) -> Expr {
        Expr::AddrOf {
            place: Box::new(place),
            span: Span::DUMMY,
        }
    }

    fn deref(e: Expr) -> Expr {
        Expr::Deref {
            expr: Box::new(e),
            span: Span::DUMMY,
        }
    }

    fn slice_expr(base: Expr, lo: Expr, hi: Expr) -> Expr {
        Expr::SliceExpr {
            base: Box::new(base),
            lo: Box::new(lo),
            hi: Box::new(hi),
            span: Span::DUMMY,
        }
    }

    /// A `StructTable` with a single interned `[]i32` (`kd_slice_int32_t`).
    fn slice_int_table() -> StructTable {
        let mut t = StructTable::new();
        t.intern_slice(Type::I32);
        t
    }

    #[test]
    fn pointer_param_cty_is_pointer_to_elem() {
        // fn f(p: *i32) i32 { return p.*; }
        let f = Func {
            is_pub: false,
            name: "f".to_string(),
            params: vec![Param {
                name: "p".to_string(),
                ty: ptr_ty("i32"),
                is_comptime: false,
                span: Span::DUMMY,
            }],
            ret: ty("i32"),
            body: block(vec![ret(deref(ident("p")))]),
            span: Span::DUMMY,
        };
        let m = Module {
            items: vec![Item::Func(f)],
        };
        let out = emit(&m, &StructTable::new(), EmitMode::Program);
        // `*i32` spells as `int32_t*` (pointers need no typedef).
        assert!(out.contains("int32_t* kd_p"), "pointer param cty wrong:\n{out}");
        // `p.*` (read) lowers to `(*(<p>))`.
        assert!(out.contains("(*(kd_p))"), "deref read lowering wrong:\n{out}");
    }

    #[test]
    fn addr_of_lowers_to_ampersand() {
        // fn g(x: i32) i32 { var p: *i32 = &x; return p.*; }
        let f = Func {
            is_pub: false,
            name: "g".to_string(),
            params: vec![param("x", "i32")],
            ret: ty("i32"),
            body: block(vec![
                Stmt::Let {
                    is_const: false,
                    name: "p".to_string(),
                    ty: Some(ptr_ty("i32")),
                    value: addr_of(ident("x")),
                    span: Span::DUMMY,
                },
                ret(deref(ident("p"))),
            ]),
            span: Span::DUMMY,
        };
        let m = Module {
            items: vec![Item::Func(f)],
        };
        let out = emit(&m, &StructTable::new(), EmitMode::Program);
        // The pointer local uses the `T*` spelling; `&x` lowers to `(&(<x>))`.
        assert!(
            out.contains("int32_t* kd_p = (&(kd_x));"),
            "addr-of lowering wrong:\n{out}"
        );
        assert!(out.contains("(*(kd_p))"), "deref lowering wrong:\n{out}");
    }

    #[test]
    fn deref_assign_lowers_to_star_assignment() {
        // fn s(p: *i32) void { p.* = 5; }
        let f = Func {
            is_pub: false,
            name: "s".to_string(),
            params: vec![Param {
                name: "p".to_string(),
                ty: ptr_ty("i32"),
                is_comptime: false,
                span: Span::DUMMY,
            }],
            ret: ty("void"),
            body: block(vec![Stmt::FieldAssign {
                place: deref(ident("p")),
                op: None,
                value: int(5),
                span: Span::DUMMY,
            }]),
            span: Span::DUMMY,
        };
        let m = Module {
            items: vec![Item::Func(f)],
        };
        let out = emit(&m, &StructTable::new(), EmitMode::Program);
        assert!(
            out.contains("*(kd_p) = (5);"),
            "deref-assign lowering wrong:\n{out}"
        );
    }

    #[test]
    fn slice_typedef_and_get_emitted() {
        // The typedef + inline bounds-checked `_get` come straight off the
        // interned slice table.
        let structs = slice_int_table();
        let m = Module { items: vec![] };
        let out = emit(&m, &structs, EmitMode::Program);
        assert!(
            out.contains("typedef struct { int32_t *ptr; uintptr_t len; } kd_slice_int32_t;"),
            "slice typedef missing/wrong:\n{out}"
        );
        assert!(
            out.contains(
                "static inline int32_t kd_slice_int32_t_get(kd_slice_int32_t s, int64_t i) { if (i < 0 || (uint64_t)i >= s.len) { fputs(\"panic: slice index out of bounds\\n\", stderr); exit(101); } return s.ptr[i]; }"
            ),
            "slice _get helper missing/wrong:\n{out}"
        );
    }

    #[test]
    fn slice_from_array_emits_compound_literal() {
        // fn sl(a: [3]i32) []i32 { return a[0..2]; }
        let mut structs = StructTable::new();
        structs.intern_array(Type::I32, 3);
        structs.intern_slice(Type::I32);
        let f = Func {
            is_pub: false,
            name: "sl".to_string(),
            params: vec![Param {
                name: "a".to_string(),
                ty: arr_ty("i32", 3),
                is_comptime: false,
                span: Span::DUMMY,
            }],
            ret: slice_ty("i32"),
            body: block(vec![ret(slice_expr(ident("a"), int(0), int(2)))]),
            span: Span::DUMMY,
        };
        let m = Module {
            items: vec![Item::Func(f)],
        };
        let out = emit(&m, &structs, EmitMode::Program);
        // The slice return type uses the slice typedef.
        assert!(
            out.contains("kd_slice_int32_t kd_sl("),
            "slice return type wrong:\n{out}"
        );
        // The slice points into the array's `.data` with `len = hi - lo`.
        assert!(
            out.contains("(kd_slice_int32_t){ .ptr = (kd_a).data + (0), .len = (2) - (0) }"),
            "slice-from-array lowering wrong:\n{out}"
        );
        // The bounds check is against the array's fixed length.
        assert!(
            out.contains("(2) > (3)"),
            "slice bounds check (vs array length) missing:\n{out}"
        );
    }

    #[test]
    fn slice_from_slice_points_through_ptr() {
        // fn re(s: []i32) []i32 { return s[1..2]; }  — slicing a slice reads `.ptr`.
        let structs = slice_int_table();
        let f = Func {
            is_pub: false,
            name: "re".to_string(),
            params: vec![Param {
                name: "s".to_string(),
                ty: slice_ty("i32"),
                is_comptime: false,
                span: Span::DUMMY,
            }],
            ret: slice_ty("i32"),
            body: block(vec![ret(slice_expr(ident("s"), int(1), int(2)))]),
            span: Span::DUMMY,
        };
        let m = Module {
            items: vec![Item::Func(f)],
        };
        let out = emit(&m, &structs, EmitMode::Program);
        // A slice-of-a-slice points through `.ptr` and bounds-checks against `.len`.
        assert!(
            out.contains("(kd_slice_int32_t){ .ptr = (kd_s).ptr + (1), .len = (2) - (1) }"),
            "slice-from-slice lowering wrong:\n{out}"
        );
        assert!(
            out.contains("(2) > ((kd_s).len)"),
            "slice-from-slice bounds check (vs .len) missing:\n{out}"
        );
    }

    #[test]
    fn slice_index_read_emits_get_call() {
        // fn at(s: []i32, i: i32) i32 { return s[i]; }
        let structs = slice_int_table();
        let f = Func {
            is_pub: false,
            name: "at".to_string(),
            params: vec![
                Param {
                    name: "s".to_string(),
                    ty: slice_ty("i32"),
                    is_comptime: false,
                    span: Span::DUMMY,
                },
                param("i", "i32"),
            ],
            ret: ty("i32"),
            body: block(vec![ret(index(ident("s"), ident("i")))]),
            span: Span::DUMMY,
        };
        let m = Module {
            items: vec![Item::Func(f)],
        };
        let out = emit(&m, &structs, EmitMode::Program);
        // The slice param is typed with the slice typedef.
        assert!(
            out.contains("kd_slice_int32_t kd_s"),
            "slice param type wrong:\n{out}"
        );
        // Indexing lowers to the bounds-checked `_get` helper call.
        assert!(
            out.contains("kd_slice_int32_t_get(kd_s, kd_i)"),
            "slice index read lowering wrong:\n{out}"
        );
    }

    #[test]
    fn slice_len_emits_dot_len() {
        // fn n(s: []i32) usize { return s.len; }
        let structs = slice_int_table();
        let f = Func {
            is_pub: false,
            name: "n".to_string(),
            params: vec![Param {
                name: "s".to_string(),
                ty: slice_ty("i32"),
                is_comptime: false,
                span: Span::DUMMY,
            }],
            ret: ty("usize"),
            body: block(vec![ret(field(ident("s"), "len"))]),
            span: Span::DUMMY,
        };
        let m = Module {
            items: vec![Item::Func(f)],
        };
        let out = emit(&m, &structs, EmitMode::Program);
        // `s.len` reads the slice's runtime `.len` field — not a `.kd_len` member.
        assert!(
            out.contains("return ((kd_s).len);"),
            "slice len lowering wrong:\n{out}"
        );
        assert!(
            !out.contains(".kd_len"),
            "slice len must not lower to a struct field access:\n{out}"
        );
    }

    #[test]
    fn slice_index_assign_emits_bounds_checked_block() {
        // fn set(s: []i32) void { s[0] = 9; }
        let structs = slice_int_table();
        let f = Func {
            is_pub: false,
            name: "set".to_string(),
            params: vec![Param {
                name: "s".to_string(),
                ty: slice_ty("i32"),
                is_comptime: false,
                span: Span::DUMMY,
            }],
            ret: ty("void"),
            body: block(vec![Stmt::FieldAssign {
                place: index(ident("s"), int(0)),
                op: None,
                value: int(9),
                span: Span::DUMMY,
            }]),
            span: Span::DUMMY,
        };
        let m = Module {
            items: vec![Item::Func(f)],
        };
        let out = emit(&m, &structs, EmitMode::Program);
        assert!(
            out.contains(
                "{ int64_t __kd_idx0 = (0); if (__kd_idx0 < 0 || (uint64_t)__kd_idx0 >= (kd_s).len) { fputs(\"panic: slice index out of bounds\\n\", stderr); exit(101); } (kd_s).ptr[__kd_idx0] = (9); }"
            ),
            "slice index assign lowering wrong:\n{out}"
        );
    }

    #[test]
    fn slice_typedef_orders_after_element_struct() {
        // A `[]Point` slice of a struct: the struct typedef must precede the
        // slice typedef that names its element by value.
        let mut structs = point_table();
        let pid = structs.id_of("Point").unwrap();
        structs.intern_slice(Type::Struct(pid));
        let m = Module { items: vec![] };
        let out = emit(&m, &structs, EmitMode::Program);
        let struct_at = out
            .find("kd_struct_Point;")
            .expect("Point struct typedef missing");
        let slice_at = out
            .find("typedef struct { kd_struct_Point *ptr; uintptr_t len; } kd_slice_struct_Point;")
            .expect("slice-of-struct typedef missing/wrong");
        assert!(
            struct_at < slice_at,
            "struct typedef must precede the slice that names it:\n{out}"
        );
    }

    #[test]
    fn struct_pointer_field_orders_pointee_first() {
        // const A = struct { b: *B };  const B = struct { v: i32 };
        // Even though A is interned first, B's typedef must precede A's: A's
        // definition names `kd_struct_B*`, so that typedef must be in scope.
        let mut structs = StructTable::new();
        let aid = structs.intern("A");
        let bid = structs.intern("B");
        let pb = structs.intern_ptr(Type::Struct(bid));
        structs.set_fields(aid, vec![("b".to_string(), Type::Ptr(pb))]);
        structs.set_fields(bid, vec![("v".to_string(), Type::I32)]);
        let m = Module { items: vec![] };
        let out = emit(&m, &structs, EmitMode::Program);
        // A pointer field spells `kd_struct_B*` (a table pointer id, resolved
        // through the table — not the emit-local registry).
        assert!(
            out.contains("typedef struct { kd_struct_B* kd_b; } kd_struct_A;"),
            "pointer field typedef wrong:\n{out}"
        );
        let b_at = out.find("} kd_struct_B;").expect("B typedef missing");
        let a_at = out.find("} kd_struct_A;").expect("A typedef missing");
        assert!(
            b_at < a_at,
            "pointee struct must be declared before the struct that points to it:\n{out}"
        );
    }

    // -- the Allocator interface + heap (v0.119, SPEC §16) ------------------

    #[test]
    fn allocator_typedef_emitted_in_prelude() {
        // The `kd_allocator` typedef is unconditional prelude (SPEC §16.2):
        // it appears even when no slice/allocator is used in the program.
        let m = Module { items: vec![] };
        let out = emit(&m, &StructTable::new(), EmitMode::Program);
        assert!(
            out.contains("typedef struct { int _unused; } kd_allocator;"),
            "kd_allocator typedef missing from prelude:\n{out}"
        );
    }

    #[test]
    fn slice_alloc_helper_emitted() {
        // Each interned slice gets its inline `_alloc` heap helper (SPEC §16.2),
        // beside the typedef + `_get` accessor.
        let structs = slice_int_table();
        let m = Module { items: vec![] };
        let out = emit(&m, &structs, EmitMode::Program);
        assert!(
            out.contains(
                "static inline kd_slice_int32_t kd_slice_int32_t_alloc(uintptr_t n) { kd_slice_int32_t s; s.ptr = malloc(n * sizeof(int32_t)); if (!s.ptr && n != 0) { fputs(\"panic: out of memory\\n\", stderr); exit(101); } s.len = n; return s; }"
            ),
            "slice _alloc helper missing/wrong:\n{out}"
        );
    }

    #[test]
    fn c_allocator_emits_compound_literal() {
        // fn a() Allocator { return c_allocator(); }
        let f = Func {
            is_pub: false,
            name: "a".to_string(),
            params: vec![],
            ret: ty("Allocator"),
            body: block(vec![ret(call("c_allocator", vec![]))]),
            span: Span::DUMMY,
        };
        let m = Module {
            items: vec![Item::Func(f)],
        };
        let out = emit(&m, &StructTable::new(), EmitMode::Program);
        // The `Allocator` return type spells as the prelude typedef.
        assert!(
            out.contains("kd_allocator kd_a(void)"),
            "allocator return type wrong:\n{out}"
        );
        // `c_allocator()` lowers to the zero-initialised compound literal.
        assert!(
            out.contains("((kd_allocator){0})"),
            "c_allocator lowering wrong:\n{out}"
        );
    }

    #[test]
    fn alloc_emits_slice_alloc_call() {
        // fn mk(a: Allocator, n: usize) []i32 { return alloc(a, i32, n); }
        let structs = slice_int_table();
        let f = Func {
            is_pub: false,
            name: "mk".to_string(),
            params: vec![param("a", "Allocator"), param("n", "usize")],
            ret: slice_ty("i32"),
            body: block(vec![ret(call(
                "alloc",
                vec![ident("a"), ident("i32"), ident("n")],
            ))]),
            span: Span::DUMMY,
        };
        let m = Module {
            items: vec![Item::Func(f)],
        };
        let out = emit(&m, &structs, EmitMode::Program);
        // `alloc(a, i32, n)` lowers to the `[]i32` heap helper; the allocator
        // argument is accepted but dropped (unused in v0.119), and `n` is cast.
        assert!(
            out.contains("kd_slice_int32_t_alloc((uintptr_t)(kd_n))"),
            "alloc lowering wrong:\n{out}"
        );
        // The call's result type is `[]i32`, so the slice typedef is the return.
        assert!(
            out.contains("kd_slice_int32_t kd_mk("),
            "alloc result (slice) type wrong:\n{out}"
        );
    }

    #[test]
    fn free_emits_ptr_free() {
        // fn fr(a: Allocator, s: []i32) void { free(a, s); }
        let structs = slice_int_table();
        let f = Func {
            is_pub: false,
            name: "fr".to_string(),
            params: vec![
                param("a", "Allocator"),
                Param {
                    name: "s".to_string(),
                    ty: slice_ty("i32"),
                    is_comptime: false,
                    span: Span::DUMMY,
                },
            ],
            ret: ty("void"),
            body: block(vec![Stmt::Expr(call("free", vec![ident("a"), ident("s")]))]),
            span: Span::DUMMY,
        };
        let m = Module {
            items: vec![Item::Func(f)],
        };
        let out = emit(&m, &structs, EmitMode::Program);
        // `free(a, s)` releases the slice's backing storage; the allocator arg
        // is dropped.
        assert!(
            out.contains("free((kd_s).ptr);"),
            "free lowering wrong:\n{out}"
        );
    }

    #[test]
    fn alloc_result_type_infers_slice() {
        // `alloc(a, i32, 2).len` proves `type_of_expr` infers the slice type for
        // an `alloc` call: the `.len` lowers to the slice's runtime `.len` field
        // (it would not if the call's type were unknown).
        let structs = slice_int_table();
        let f = Func {
            is_pub: false,
            name: "ln".to_string(),
            params: vec![param("a", "Allocator")],
            ret: ty("usize"),
            body: block(vec![ret(field(
                call("alloc", vec![ident("a"), ident("i32"), int(2)]),
                "len",
            ))]),
            span: Span::DUMMY,
        };
        let m = Module {
            items: vec![Item::Func(f)],
        };
        let out = emit(&m, &structs, EmitMode::Program);
        assert!(
            out.contains("(kd_slice_int32_t_alloc((uintptr_t)(2))).len"),
            "alloc result type not inferred as a slice (`.len` not lowered):\n{out}"
        );
    }

    // -- comptime generics (v0.120, SPEC §17) ------------------------------

    /// A `comptime IDENT: type` type parameter (`is_comptime = true`).
    fn comptime_param(name: &str) -> Param {
        Param {
            name: name.to_string(),
            ty: ty("type"),
            is_comptime: true,
            span: Span::DUMMY,
        }
    }

    /// `fn max(comptime T: type, a: T, b: T) T { if (a > b) { return a; } return b; }`
    fn generic_max() -> Func {
        func(
            "max",
            vec![comptime_param("T"), param("a", "T"), param("b", "T")],
            "T",
            vec![
                Stmt::If {
                    capture: None,
                    cond: Expr::Binary {
                        op: BinOp::Gt,
                        lhs: Box::new(ident("a")),
                        rhs: Box::new(ident("b")),
                        span: Span::DUMMY,
                    },
                    then: block(vec![ret(ident("a"))]),
                    els: None,
                    span: Span::DUMMY,
                },
                ret(ident("b")),
            ],
        )
    }

    /// `fn first(comptime T: type, x: ?T) T { return x orelse 0; }`
    fn generic_first() -> Func {
        Func {
            is_pub: false,
            name: "first".to_string(),
            params: vec![
                comptime_param("T"),
                Param {
                    name: "x".to_string(),
                    ty: opt_ty("T"),
                    is_comptime: false,
                    span: Span::DUMMY,
                },
            ],
            ret: ty("T"),
            body: block(vec![ret(Expr::Orelse {
                lhs: Box::new(ident("x")),
                rhs: Box::new(int(0)),
                span: Span::DUMMY,
            })]),
            span: Span::DUMMY,
        }
    }

    #[test]
    fn generic_fn_monomorphised_at_i32() {
        // fn pick(x: i32, y: i32) i32 { return max(i32, x, y); }
        let mut structs = StructTable::new();
        assert!(
            structs.intern_instantiation("max", vec![ComptimeArg::Type(Type::I32)]),
            "first instantiation should be newly recorded"
        );
        let user = func(
            "pick",
            vec![param("x", "i32"), param("y", "i32")],
            "i32",
            vec![ret(call("max", vec![ident("i32"), ident("x"), ident("y")]))],
        );
        let m = Module {
            items: vec![Item::Func(generic_max()), Item::Func(user)],
        };
        let out = emit(&m, &structs, EmitMode::Program);
        // The instance is forward-declared and defined under its mangled C
        // name, with the type parameter resolved to `int32_t`.
        assert!(
            out.contains("int32_t kd_max__int32_t(int32_t kd_a, int32_t kd_b);"),
            "instance forward decl missing/wrong:\n{out}"
        );
        assert!(
            out.contains("int32_t kd_max__int32_t(int32_t kd_a, int32_t kd_b) {"),
            "instance definition missing/wrong:\n{out}"
        );
        // The body lowers under the substitution (comparison + returns).
        assert!(
            out.contains("((kd_a > kd_b))"),
            "instance body comparison wrong:\n{out}"
        );
        // The call drops the leading type arg and targets the instance C name
        // with ONLY the runtime args.
        assert!(
            out.contains("kd_max__int32_t(kd_x, kd_y)"),
            "generic call should use the instance name with only runtime args:\n{out}"
        );
        // The generic function is NEVER emitted under its plain name.
        assert!(
            !out.contains("kd_max("),
            "a generic function must not be emitted under its plain name:\n{out}"
        );
    }

    #[test]
    fn generic_fn_two_instantiations_emit_two_functions() {
        // The same generic, recorded at `i32` and `i64`, yields two C functions.
        let mut structs = StructTable::new();
        structs.intern_instantiation("max", vec![ComptimeArg::Type(Type::I32)]);
        structs.intern_instantiation("max", vec![ComptimeArg::Type(Type::I64)]);
        let m = Module {
            items: vec![Item::Func(generic_max())],
        };
        let out = emit(&m, &structs, EmitMode::Program);
        assert!(
            out.contains("int32_t kd_max__int32_t(int32_t kd_a, int32_t kd_b) {"),
            "i32 instance missing:\n{out}"
        );
        assert!(
            out.contains("int64_t kd_max__int64_t(int64_t kd_a, int64_t kd_b) {"),
            "i64 instance missing:\n{out}"
        );
        // Still never under the plain name.
        assert!(
            !out.contains("kd_max("),
            "plain generic name must not be emitted:\n{out}"
        );
    }

    #[test]
    fn generic_call_result_type_drives_coercion() {
        // fn pick(x: i32, y: i32) ?i32 { return max(i32, x, y); }
        // `type_of_expr(max(i32, …))` must be `i32` so the `T` result widens to
        // `?i32` at the return (SPEC §17.2 substituted return type).
        let mut structs = StructTable::new();
        structs.intern_optional(Type::I32);
        structs.intern_instantiation("max", vec![ComptimeArg::Type(Type::I32)]);
        let user = Func {
            is_pub: false,
            name: "pick".to_string(),
            params: vec![param("x", "i32"), param("y", "i32")],
            ret: opt_ty("i32"),
            body: block(vec![ret(call(
                "max",
                vec![ident("i32"), ident("x"), ident("y")],
            ))]),
            span: Span::DUMMY,
        };
        let m = Module {
            items: vec![Item::Func(generic_max()), Item::Func(user)],
        };
        let out = emit(&m, &structs, EmitMode::Program);
        assert!(
            out.contains(
                "((kd_opt_int32_t){ .has = true, .val = kd_max__int32_t(kd_x, kd_y) })"
            ),
            "generic call result not inferred as `i32` (no `?i32` widening):\n{out}"
        );
    }

    #[test]
    fn generic_runtime_arg_coerces_under_substitution() {
        // fn first(comptime T: type, x: ?T) T { return x orelse 0; }
        // fn use(v: i32) i32 { return first(i32, v); }
        // The `?T` param resolves to `?i32` under the substitution, so the
        // instance body uses the `?i32` helper and the runtime arg `v` (an
        // `i32`) widens to the present optional at the call site.
        let mut structs = StructTable::new();
        structs.intern_optional(Type::I32);
        structs.intern_instantiation("first", vec![ComptimeArg::Type(Type::I32)]);
        let user = func(
            "use",
            vec![param("v", "i32")],
            "i32",
            vec![ret(call("first", vec![ident("i32"), ident("v")]))],
        );
        let m = Module {
            items: vec![Item::Func(generic_first()), Item::Func(user)],
        };
        let out = emit(&m, &structs, EmitMode::Program);
        // The instance's runtime param is the substituted optional type; its
        // return type is the concrete `int32_t`.
        assert!(
            out.contains("int32_t kd_first__int32_t(kd_opt_int32_t kd_x) {"),
            "instance optional param / return type wrong:\n{out}"
        );
        // The body uses the substituted optional helper.
        assert!(
            out.contains("kd_opt_int32_t_orelse(kd_x, 0)"),
            "instance body orelse lowering wrong:\n{out}"
        );
        // The runtime arg widens to `?i32` at the call site.
        assert!(
            out.contains(
                "kd_first__int32_t(((kd_opt_int32_t){ .has = true, .val = kd_v }))"
            ),
            "runtime arg should widen to the substituted optional param:\n{out}"
        );
    }

    // -- comptime value parameters (v0.128, SPEC §24) ----------------------

    /// A `comptime IDENT: usize` value parameter (`is_comptime = true`, v0.128).
    fn comptime_value_param(name: &str) -> Param {
        Param {
            name: name.to_string(),
            ty: ty("usize"),
            is_comptime: true,
            span: Span::DUMMY,
        }
    }

    /// `fn make(comptime n: usize, a: [n]i32) [n]i32 { return a; }` — a generic
    /// function whose runtime parameter and return type are sized by the
    /// comptime value parameter `n`.
    fn generic_make() -> Func {
        Func {
            is_pub: false,
            name: "make".to_string(),
            params: vec![
                comptime_value_param("n"),
                Param {
                    name: "a".to_string(),
                    ty: arr_param_ty("i32", "n"),
                    is_comptime: false,
                    span: Span::DUMMY,
                },
            ],
            ret: arr_param_ty("i32", "n"),
            body: block(vec![ret(ident("a"))]),
            span: Span::DUMMY,
        }
    }

    #[test]
    fn comptime_value_param_sizes_array_and_call_drops_value_arg() {
        // fn make(comptime n: usize, a: [n]i32) [n]i32 { return a; }
        // fn use2(a: [2]i32) [2]i32 { return make(2, a); }
        // Instantiated at n = 2: the `[n]i32` param/return resolve to `[2]i32`,
        // the instance is `kd_make__2`, and the call drops the value arg.
        let mut structs = StructTable::new();
        structs.intern_array(Type::I32, 2);
        structs.intern_instantiation("make", vec![ComptimeArg::Value(2)]);
        let user = Func {
            is_pub: false,
            name: "use2".to_string(),
            params: vec![Param {
                name: "a".to_string(),
                ty: arr_ty("i32", 2),
                is_comptime: false,
                span: Span::DUMMY,
            }],
            ret: arr_ty("i32", 2),
            body: block(vec![ret(call("make", vec![int(2), ident("a")]))]),
            span: Span::DUMMY,
        };
        let m = Module {
            items: vec![Item::Func(generic_make()), Item::Func(user)],
        };
        let out = emit(&m, &structs, EmitMode::Program);
        // The `[2]i32` array type is emitted (length resolved from n = 2).
        assert!(
            out.contains("typedef struct { int32_t data[2]; } kd_arr_int32_t_2;"),
            "array type of length 2 missing:\n{out}"
        );
        // The instance is forward-declared and defined as `kd_make__2`, the
        // `[n]i32` param/return resolved to the `[2]i32` C type.
        assert!(
            out.contains("kd_arr_int32_t_2 kd_make__2(kd_arr_int32_t_2 kd_a);"),
            "instance forward decl missing/wrong:\n{out}"
        );
        assert!(
            out.contains("kd_arr_int32_t_2 kd_make__2(kd_arr_int32_t_2 kd_a) {"),
            "instance definition missing/wrong:\n{out}"
        );
        // The call drops the comptime value arg and targets the mangled instance
        // with ONLY the runtime arg.
        assert!(
            out.contains("kd_make__2(kd_a)"),
            "generic call should use the mangled instance name with only runtime args:\n{out}"
        );
        // Never emitted under the plain generic name.
        assert!(
            !out.contains("kd_make("),
            "a generic function must not be emitted under its plain name:\n{out}"
        );
    }

    #[test]
    fn comptime_value_param_reference_emits_literal() {
        // fn size(comptime n: usize) usize { return n; }  instantiated at n = 5.
        // A body reference to `n` emits the bound literal (it is not a C var).
        let mut structs = StructTable::new();
        structs.intern_instantiation("size", vec![ComptimeArg::Value(5)]);
        let f = Func {
            is_pub: false,
            name: "size".to_string(),
            params: vec![comptime_value_param("n")],
            ret: ty("usize"),
            body: block(vec![ret(ident("n"))]),
            span: Span::DUMMY,
        };
        let m = Module {
            items: vec![Item::Func(f)],
        };
        let out = emit(&m, &structs, EmitMode::Program);
        // The value-only param list collapses to `void`; instance is `kd_size__5`.
        assert!(
            out.contains("uintptr_t kd_size__5(void) {"),
            "value-param instance signature wrong:\n{out}"
        );
        // The reference to `n` emits the bound literal `5`, not a `kd_n` variable.
        assert!(
            out.contains("return (5);"),
            "value-param reference should emit the bound literal:\n{out}"
        );
        assert!(
            !out.contains("kd_n"),
            "a comptime value param is not a real C variable:\n{out}"
        );
        assert!(
            !out.contains("kd_size("),
            "a generic function must not be emitted under its plain name:\n{out}"
        );
    }

    #[test]
    fn comptime_value_two_instantiations_distinct_arrays() {
        // The same generic `make` recorded at n = 2 and n = 4 yields two
        // instances over two distinct array types.
        let mut structs = StructTable::new();
        structs.intern_array(Type::I32, 2);
        structs.intern_array(Type::I32, 4);
        structs.intern_instantiation("make", vec![ComptimeArg::Value(2)]);
        structs.intern_instantiation("make", vec![ComptimeArg::Value(4)]);
        let m = Module {
            items: vec![Item::Func(generic_make())],
        };
        let out = emit(&m, &structs, EmitMode::Program);
        assert!(
            out.contains("kd_arr_int32_t_2 kd_make__2(kd_arr_int32_t_2 kd_a) {"),
            "n = 2 instance missing:\n{out}"
        );
        assert!(
            out.contains("kd_arr_int32_t_4 kd_make__4(kd_arr_int32_t_4 kd_a) {"),
            "n = 4 instance missing:\n{out}"
        );
    }

    #[test]
    fn literal_sized_array_unchanged_by_value_params() {
        // A non-generic `fn id(a: [3]i32) [3]i32 { return a; }` still lowers
        // exactly as in v0.117 — value params do not perturb literal arrays.
        let mut structs = StructTable::new();
        structs.intern_array(Type::I32, 3);
        let f = Func {
            is_pub: false,
            name: "id".to_string(),
            params: vec![Param {
                name: "a".to_string(),
                ty: arr_ty("i32", 3),
                is_comptime: false,
                span: Span::DUMMY,
            }],
            ret: arr_ty("i32", 3),
            body: block(vec![ret(ident("a"))]),
            span: Span::DUMMY,
        };
        let m = Module {
            items: vec![Item::Func(f)],
        };
        let out = emit(&m, &structs, EmitMode::Program);
        assert!(
            out.contains("typedef struct { int32_t data[3]; } kd_arr_int32_t_3;"),
            "literal array typedef missing/changed:\n{out}"
        );
        assert!(
            out.contains("kd_arr_int32_t_3 kd_id(kd_arr_int32_t_3 kd_a) {"),
            "literal-array function signature changed:\n{out}"
        );
    }

    // -- v0.121: type inference for `var` / `const` ------------------------

    #[test]
    fn inferred_var_int_emits_int64_decl() {
        // fn f() void { var x = 42; }  — no annotation, inferred `i64`.
        let f = func(
            "f",
            vec![],
            "void",
            vec![Stmt::Let {
                is_const: false,
                name: "x".to_string(),
                ty: None,
                value: int(42),
                span: Span::DUMMY,
            }],
        );
        let m = Module {
            items: vec![Item::Func(f)],
        };
        let out = emit(&m, &StructTable::new(), EmitMode::Program);
        assert!(
            out.contains("int64_t kd_x = 42;"),
            "inferred int var should emit an int64_t decl:\n{out}"
        );
    }

    #[test]
    fn inferred_var_bool_emits_bool_decl() {
        // fn f() void { var b = true; }  — inferred `bool`.
        let f = func(
            "f",
            vec![],
            "void",
            vec![Stmt::Let {
                is_const: false,
                name: "b".to_string(),
                ty: None,
                value: Expr::Bool {
                    value: true,
                    span: Span::DUMMY,
                },
                span: Span::DUMMY,
            }],
        );
        let m = Module {
            items: vec![Item::Func(f)],
        };
        let out = emit(&m, &StructTable::new(), EmitMode::Program);
        assert!(
            out.contains("bool kd_b = true;"),
            "inferred bool var should emit a bool decl:\n{out}"
        );
    }

    #[test]
    fn inferred_var_struct_emits_typedef_name() {
        // fn f() void { var p = Point{ .x = 1, .y = 2 }; }  — inferred `Point`,
        // so the C declaration type is the struct typedef name.
        let structs = point_table();
        let lit = Expr::StructLit {
            name: "Point".to_string(),
            fields: vec![finit("x", int(1)), finit("y", int(2))],
            span: Span::DUMMY,
        };
        let f = func(
            "f",
            vec![],
            "void",
            vec![Stmt::Let {
                is_const: false,
                name: "p".to_string(),
                ty: None,
                value: lit,
                span: Span::DUMMY,
            }],
        );
        let m = Module {
            items: vec![Item::Func(f)],
        };
        let out = emit(&m, &structs, EmitMode::Program);
        assert!(
            out.contains(
                "kd_struct_Point kd_p = ((kd_struct_Point){ .kd_x = 1, .kd_y = 2 });"
            ),
            "inferred struct var should emit the struct typedef name:\n{out}"
        );
    }

    #[test]
    fn inferred_var_type_recorded_for_later_use() {
        // fn f(o: ?i32) void { var x = o; x = null; }
        // `x` is inferred `?i32` from `o`; the recorded inferred type must drive
        // the later `null` re-assignment to widen to the empty optional (which
        // proves the inferred type landed in `var_types`).
        let structs = opt_int_table();
        let f = Func {
            is_pub: false,
            name: "f".to_string(),
            params: vec![Param {
                name: "o".to_string(),
                ty: opt_ty("i32"),
                is_comptime: false,
                span: Span::DUMMY,
            }],
            ret: ty("void"),
            body: block(vec![
                Stmt::Let {
                    is_const: false,
                    name: "x".to_string(),
                    ty: None,
                    value: ident("o"),
                    span: Span::DUMMY,
                },
                Stmt::Assign {
                    name: "x".to_string(),
                    op: None,
                    value: Expr::Null { span: Span::DUMMY },
                    span: Span::DUMMY,
                },
            ]),
            span: Span::DUMMY,
        };
        let m = Module {
            items: vec![Item::Func(f)],
        };
        let out = emit(&m, &structs, EmitMode::Program);
        // The inferred declaration uses the optional typedef.
        assert!(
            out.contains("kd_opt_int32_t kd_x = kd_o;"),
            "inferred optional var should use the optional typedef:\n{out}"
        );
        // The later `null` re-assignment widens using the recorded inferred type.
        assert!(
            out.contains("kd_x = ((kd_opt_int32_t){ .has = false });"),
            "later use should resolve the inferred var's type:\n{out}"
        );
    }

    #[test]
    fn inferred_top_level_const_infers_c_type() {
        // const N = 7;  /  const B = true;  — un-annotated top-level consts
        // (v0.121) infer `i64` / `bool` from the folded comptime value.
        let m = Module {
            items: vec![
                Item::Const(crate::ast::ConstDecl {
                    is_pub: false,
                    name: "N".to_string(),
                    ty: None,
                    value: int(7),
                    span: Span::DUMMY,
                }),
                Item::Const(crate::ast::ConstDecl {
                    is_pub: false,
                    name: "B".to_string(),
                    ty: None,
                    value: Expr::Bool {
                        value: true,
                        span: Span::DUMMY,
                    },
                    span: Span::DUMMY,
                }),
            ],
        };
        let out = emit(&m, &StructTable::new(), EmitMode::Program);
        assert!(
            out.contains("static const int64_t kd_N = 7;"),
            "inferred int const should be int64_t:\n{out}"
        );
        assert!(
            out.contains("static const bool kd_B = true;"),
            "inferred bool const should be bool:\n{out}"
        );
    }

    // -- v0.127 strings -----------------------------------------------------

    fn str_lit(value: &str) -> Expr {
        Expr::StrLit {
            value: value.to_string(),
            span: Span::DUMMY,
        }
    }

    /// A `let s = e;` (inferred type, immutable binding); used to observe the
    /// lowering + inferred C declaration type of a string-typed initializer.
    fn let_infer(name: &str, value: Expr) -> Stmt {
        Stmt::Let {
            is_const: false,
            name: name.to_string(),
            ty: None,
            value,
            span: Span::DUMMY,
        }
    }

    /// A struct table that has interned the `[]u8` slice (exactly what sema does
    /// the moment a string literal appears), so its typedef + the `Type::Slice`
    /// id for `[]u8` are available to emission.
    fn u8_slice_table() -> StructTable {
        let mut t = StructTable::new();
        t.intern_slice(Type::U8);
        t
    }

    #[test]
    fn strlit_emits_compound_slice_literal() {
        // fn f() void { let s = "hi"; }
        let f = func("f", vec![], "void", vec![let_infer("s", str_lit("hi"))]);
        let m = Module {
            items: vec![Item::Func(f)],
        };
        let out = emit(&m, &u8_slice_table(), EmitMode::Program);
        // The initializer is a `kd_slice_uint8_t` compound literal over the C
        // string `"hi"` with the right byte length, and the inferred binding
        // type is the `[]u8` slice typedef.
        assert!(
            out.contains(
                "kd_slice_uint8_t kd_s = ((kd_slice_uint8_t){ .ptr = (uint8_t *)\"hi\", .len = 2 });"
            ),
            "strlit compound slice literal missing:\n{out}"
        );
        // The `[]u8` slice typedef was emitted (interned by sema / the table).
        assert!(
            out.contains("} kd_slice_uint8_t;"),
            "[]u8 slice typedef missing:\n{out}"
        );
    }

    #[test]
    fn strlit_len_counts_bytes_not_chars() {
        // A multi-byte UTF-8 string ("é" = 0xc3 0xa9) reports its byte length,
        // and its non-ASCII bytes are emitted as \xNN escapes.
        let f = func("f", vec![], "void", vec![let_infer("s", str_lit("é"))]);
        let m = Module {
            items: vec![Item::Func(f)],
        };
        let out = emit(&m, &u8_slice_table(), EmitMode::Program);
        assert!(
            out.contains(".ptr = (uint8_t *)\"\\xc3\\xa9\", .len = 2"),
            "byte-exact length / hex escaping wrong:\n{out}"
        );
    }

    #[test]
    fn print_of_strlit_emits_fwrite_and_newline() {
        // fn f() void { print("hi"); }
        let f = func("f", vec![], "void", vec![print(str_lit("hi"))]);
        let m = Module {
            items: vec![Item::Func(f)],
        };
        let out = emit(&m, &u8_slice_table(), EmitMode::Program);
        // The slice is hoisted to a temp, then written with a trailing newline.
        assert!(
            out.contains("kd_slice_uint8_t __kd_str0 = (((kd_slice_uint8_t){ .ptr = (uint8_t *)\"hi\", .len = 2 }));"),
            "string-print temp hoist missing:\n{out}"
        );
        assert!(
            out.contains("fwrite(__kd_str0.ptr, 1, __kd_str0.len, stdout); fputc('\\n', stdout);"),
            "string-print fwrite/newline missing:\n{out}"
        );
        // The integer print helper must NOT be used for a string argument.
        assert!(
            !out.contains("kd_print((long long)"),
            "string print must not use the integer kd_print path:\n{out}"
        );
    }

    #[test]
    fn print_of_string_local_hoists_via_type_of_expr() {
        // fn f() void { let s = "hi"; print(s); } — the arg is an `Ident` whose
        // type (resolved through the scope) is `[]u8`, so it still takes the
        // string-print path rather than the integer one.
        let f = func(
            "f",
            vec![],
            "void",
            vec![let_infer("s", str_lit("hi")), print(ident("s"))],
        );
        let m = Module {
            items: vec![Item::Func(f)],
        };
        let out = emit(&m, &u8_slice_table(), EmitMode::Program);
        assert!(
            out.contains("kd_slice_uint8_t __kd_str0 = (kd_s);"),
            "string-local print should hoist the slice variable:\n{out}"
        );
        assert!(
            out.contains("fwrite(__kd_str0.ptr, 1, __kd_str0.len, stdout); fputc('\\n', stdout);"),
            "string-local print fwrite/newline missing:\n{out}"
        );
    }

    #[test]
    fn print_of_int_still_uses_kd_print() {
        // fn f() void { print(7); } — unchanged integer path.
        let f = func("f", vec![], "void", vec![print(int(7))]);
        let m = Module {
            items: vec![Item::Func(f)],
        };
        let out = emit(&m, &u8_slice_table(), EmitMode::Program);
        assert!(
            out.contains("kd_print((long long)(7));"),
            "integer print path changed:\n{out}"
        );
        assert!(
            !out.contains("fwrite("),
            "integer print must not emit fwrite:\n{out}"
        );
    }

    #[test]
    fn two_string_prints_get_distinct_temps() {
        // Each `print(s)` uses the monotonic str_counter, so two in one function
        // get distinct temp names (__kd_str0, __kd_str1).
        let f = func(
            "f",
            vec![],
            "void",
            vec![print(str_lit("a")), print(str_lit("b"))],
        );
        let m = Module {
            items: vec![Item::Func(f)],
        };
        let out = emit(&m, &u8_slice_table(), EmitMode::Program);
        assert!(out.contains("__kd_str0"), "first temp missing:\n{out}");
        assert!(out.contains("__kd_str1"), "second temp missing:\n{out}");
    }

    #[test]
    fn c_string_literal_escapes_specials_and_breaks_hex() {
        // Backslash / quote / readable control escapes.
        assert_eq!(c_string_literal("a\\b\"c\n\t"), "\"a\\\\b\\\"c\\n\\t\"");
        // A non-printable byte becomes \xNN; a following literal hex digit forces
        // a string-literal split so the escape isn't extended.
        // bytes: 0x07, 'f'  ->  "\x07" "f"
        assert_eq!(c_string_literal("\u{7}f"), "\"\\x07\" \"f\"");
        // A non-hex-digit char after a \xNN escape needs no split.
        // bytes: 0x07, 'g'  ->  "\x07g"
        assert_eq!(c_string_literal("\u{7}g"), "\"\\x07g\"");
        // The empty string is a valid empty C literal.
        assert_eq!(c_string_literal(""), "\"\"");
    }

    // -- generic structs / type-returning functions (v0.129) ----------------

    /// A bare `type` return type (`fn Name(...) type`), the marker of a
    /// type-constructor.
    fn type_kw() -> TypeExpr {
        ty("type")
    }

    /// The post-sema shape of:
    /// ```text
    /// fn Box(comptime T: type) type { return struct { v: T }; }
    /// const IB = Box(i32);
    /// fn main() void { var b: IB = IB{ .v = 5 }; }
    /// ```
    /// sema identifies `Box` as a type-constructor (not checked/emitted as an
    /// ordinary fn), instantiates it at `i32` into a monomorphised struct named
    /// `Box__int32_t` (field `v: i32`) held in the [`StructTable`], and resolves
    /// the alias `IB` to that struct — so emit receives the canonical struct
    /// name in the `var`/struct-literal positions and lowers them as a normal
    /// struct (SPEC §25.3). The `Box` body keeps the `Expr::StructType` value to
    /// exercise the type-constructor skip on a realistic module.
    fn box_program() -> (Module, StructTable) {
        let mut structs = StructTable::new();
        let sid = structs.intern("Box__int32_t");
        structs.set_fields(sid, vec![("v".to_string(), Type::I32)]);

        let box_ctor = Func {
            is_pub: false,
            name: "Box".to_string(),
            params: vec![Param {
                name: "T".to_string(),
                ty: type_kw(),
                is_comptime: true,
                span: Span::DUMMY,
            }],
            ret: type_kw(),
            body: block(vec![ret(Expr::StructType {
                fields: vec![FieldDecl {
                    name: "v".to_string(),
                    ty: ty("T"),
                    span: Span::DUMMY,
                }],
                methods: vec![],
                span: Span::DUMMY,
            })]),
            span: Span::DUMMY,
        };

        let alias = Item::Const(ConstDecl {
            is_pub: false,
            name: "IB".to_string(),
            ty: None,
            value: Expr::Call {
                callee: "Box".to_string(),
                args: vec![ident("i32")],
                span: Span::DUMMY,
            },
            span: Span::DUMMY,
        });

        let main = Func {
            is_pub: false,
            name: "main".to_string(),
            params: vec![],
            ret: ty("void"),
            body: block(vec![Stmt::Let {
                is_const: false,
                name: "b".to_string(),
                ty: Some(ty("Box__int32_t")),
                value: Expr::StructLit {
                    name: "Box__int32_t".to_string(),
                    fields: vec![finit("v", int(5))],
                    span: Span::DUMMY,
                },
                span: Span::DUMMY,
            }]),
            span: Span::DUMMY,
        };

        let m = Module {
            items: vec![Item::Func(box_ctor), alias, Item::Func(main)],
        };
        (m, structs)
    }

    #[test]
    fn type_constructor_and_alias_are_compile_time_only() {
        let (m, structs) = box_program();
        let out = emit(&m, &structs, EmitMode::Program);

        // The monomorphised struct emits as an ordinary C typedef.
        assert!(
            out.contains("typedef struct { int32_t kd_v; } kd_struct_Box__int32_t;"),
            "monomorphised struct typedef missing/wrong:\n{out}"
        );
        // The type-constructor itself is compile-time only — never a C function
        // (neither forward-declared nor defined). `kd_struct_Box__int32_t` must
        // not be mistaken for a `kd_Box(` function decl, so check the call form.
        assert!(
            !out.contains("kd_Box("),
            "type-constructor was emitted as a C function:\n{out}"
        );
        assert!(
            !out.contains(") type") && !out.contains("type kd_"),
            "a `type` return type leaked into the C output:\n{out}"
        );
        // The type-alias const named a struct, not a value — no C `const` for it.
        assert!(
            !out.contains("kd_IB"),
            "type-alias const was emitted as a C value const:\n{out}"
        );
        // The alias-typed `var` and struct literal lower as a normal struct.
        assert!(
            out.contains("kd_struct_Box__int32_t kd_b = ((kd_struct_Box__int32_t){ .kd_v = 5 });"),
            "alias-typed var/struct-literal lowering wrong:\n{out}"
        );
    }

    #[test]
    fn parameterless_type_constructor_is_also_skipped() {
        // Defensive: a type-constructor is recognised by its `type` return type
        // alone, so even a (non-generic) `fn F() type { return struct {}; }` —
        // which `is_generic` would not catch — is never emitted to C. Such a
        // function carries no valid C return type, so emitting it would be wrong.
        let f = Func {
            is_pub: false,
            name: "F".to_string(),
            params: vec![],
            ret: ty("type"),
            body: block(vec![ret(Expr::StructType {
                fields: vec![],
                methods: vec![],
                span: Span::DUMMY,
            })]),
            span: Span::DUMMY,
        };
        let main = Func {
            is_pub: false,
            name: "main".to_string(),
            params: vec![],
            ret: ty("void"),
            body: block(vec![]),
            span: Span::DUMMY,
        };
        let m = Module {
            items: vec![Item::Func(f), Item::Func(main)],
        };
        let out = emit(&m, &StructTable::new(), EmitMode::Program);
        assert!(
            !out.contains("kd_F("),
            "parameterless type-constructor was emitted:\n{out}"
        );
        assert!(
            !out.contains("type kd_F"),
            "a `type` return type leaked into the C output:\n{out}"
        );
    }

    // -- generic-struct methods + container foundation (v0.130) --------------

    /// The post-sema shape of:
    /// ```text
    /// fn Box(comptime T: type) type {
    ///     return struct {
    ///         v: T,
    ///         fn get(self: Self) T { return self.v; }
    ///         fn replaced(self: Self, nv: T) Self { return Self{ .v = nv }; }
    ///     };
    /// }
    /// const IB = Box(i32);
    /// fn main() void {
    ///     var b: IB = IB{ .v = 7 };
    ///     print(b.get());            // 7
    ///     var c: IB = b.replaced(9);
    ///     print(c.get());            // 9
    /// }
    /// ```
    /// sema interns the monomorphised struct `Box__int32_t` (field `v: i32`),
    /// records it as a generic-struct instance of `Box` at `i32`, and binds the
    /// alias `IB` to it. The backend must emit the constructor's methods per
    /// instance under `{ T → i32, Self → Struct(Box__int32_t) }` as free C
    /// functions `kd_Box__int32_t_<method>` (SPEC §26.3), while the
    /// type-constructor `Box` itself stays compile-time only (§25.3).
    fn box_with_methods_program() -> (Module, StructTable) {
        let mut structs = StructTable::new();
        let sid = structs.intern("Box__int32_t");
        structs.set_fields(sid, vec![("v".to_string(), Type::I32)]);
        structs.record_struct_instance(sid, "Box", vec![Type::I32]);
        structs.add_alias("IB", Type::Struct(sid));

        let get = Func {
            is_pub: false,
            name: "get".to_string(),
            params: vec![param("self", "Self")],
            ret: ty("T"),
            body: block(vec![ret(field(ident("self"), "v"))]),
            span: Span::DUMMY,
        };
        let replaced = Func {
            is_pub: false,
            name: "replaced".to_string(),
            params: vec![param("self", "Self"), param("nv", "T")],
            ret: ty("Self"),
            body: block(vec![ret(Expr::StructLit {
                name: "Self".to_string(),
                fields: vec![finit("v", ident("nv"))],
                span: Span::DUMMY,
            })]),
            span: Span::DUMMY,
        };

        let box_ctor = Func {
            is_pub: false,
            name: "Box".to_string(),
            params: vec![Param {
                name: "T".to_string(),
                ty: type_kw(),
                is_comptime: true,
                span: Span::DUMMY,
            }],
            ret: type_kw(),
            body: block(vec![ret(Expr::StructType {
                fields: vec![FieldDecl {
                    name: "v".to_string(),
                    ty: ty("T"),
                    span: Span::DUMMY,
                }],
                methods: vec![get, replaced],
                span: Span::DUMMY,
            })]),
            span: Span::DUMMY,
        };

        let alias = Item::Const(ConstDecl {
            is_pub: false,
            name: "IB".to_string(),
            ty: None,
            value: Expr::Call {
                callee: "Box".to_string(),
                args: vec![ident("i32")],
                span: Span::DUMMY,
            },
            span: Span::DUMMY,
        });

        let main = Func {
            is_pub: false,
            name: "main".to_string(),
            params: vec![],
            ret: ty("void"),
            body: block(vec![
                Stmt::Let {
                    is_const: false,
                    name: "b".to_string(),
                    ty: Some(ty("IB")),
                    value: Expr::StructLit {
                        name: "IB".to_string(),
                        fields: vec![finit("v", int(7))],
                        span: Span::DUMMY,
                    },
                    span: Span::DUMMY,
                },
                print(method_call(ident("b"), "get", vec![])),
                Stmt::Let {
                    is_const: false,
                    name: "c".to_string(),
                    ty: Some(ty("IB")),
                    value: method_call(ident("b"), "replaced", vec![int(9)]),
                    span: Span::DUMMY,
                },
                print(method_call(ident("c"), "get", vec![])),
            ]),
            span: Span::DUMMY,
        };

        let m = Module {
            items: vec![Item::Func(box_ctor), alias, Item::Func(main)],
        };
        (m, structs)
    }

    #[test]
    fn generic_struct_method_emits_instance_function_and_lowers_call() {
        let (m, structs) = box_with_methods_program();
        let out = emit(&m, &structs, EmitMode::Program);

        // The monomorphised struct still emits as an ordinary C typedef (§25.3).
        assert!(
            out.contains("typedef struct { int32_t kd_v; } kd_struct_Box__int32_t;"),
            "monomorphised struct typedef missing/wrong:\n{out}"
        );
        // The instance's `get` is forward-declared and defined as a free C
        // function returning the substituted `T` (= int32_t), with a by-value
        // `self: Self` (= the instantiated struct) parameter (§26.3, §10).
        assert!(
            out.contains("int32_t kd_Box__int32_t_get(kd_struct_Box__int32_t kd_self);"),
            "instance method forward-decl missing/wrong:\n{out}"
        );
        assert!(
            out.contains("int32_t kd_Box__int32_t_get(kd_struct_Box__int32_t kd_self) {"),
            "instance method definition missing/wrong:\n{out}"
        );
        assert!(
            out.contains("return ((kd_self).kd_v);"),
            "instance method body wrong:\n{out}"
        );
        // `replaced` returns `Self` (= the instantiated struct) and constructs a
        // `Self{…}` literal — `Self` resolves to the struct in both positions.
        assert!(
            out.contains(
                "kd_struct_Box__int32_t kd_Box__int32_t_replaced(kd_struct_Box__int32_t kd_self, int32_t kd_nv) {"
            ),
            "Self return / param type did not resolve:\n{out}"
        );
        assert!(
            out.contains("return (((kd_struct_Box__int32_t){ .kd_v = kd_nv }));"),
            "Self{{…}} literal in a method body did not resolve:\n{out}"
        );
        // The method call lowers to the instance C function; `self` is the
        // receiver, extra args follow it.
        assert!(
            out.contains("kd_print((long long)(kd_Box__int32_t_get(kd_b)))"),
            "method call did not lower to the instance function:\n{out}"
        );
        assert!(
            out.contains("kd_Box__int32_t_replaced(kd_b, 9)"),
            "method-with-arg call did not lower correctly:\n{out}"
        );
        // The type-constructor itself is never emitted to C (§25.3).
        assert!(
            !out.contains("kd_Box("),
            "type-constructor was emitted as a C function:\n{out}"
        );
        assert!(
            !out.contains(") type") && !out.contains("type kd_"),
            "a `type` return type leaked into the C output:\n{out}"
        );
        // `Self` must never leak into the C name space.
        assert!(
            !out.contains("kd_struct_Self") && !out.contains("kd_Self_"),
            "the contextual `Self` leaked into the emitted C:\n{out}"
        );
    }

    #[test]
    fn fields_only_generic_struct_instance_emits_no_methods() {
        // Preservation (v0.129): a recorded instance whose constructor declares
        // *no* methods (the fields-only generic struct) emits exactly what
        // v0.129 did — the struct typedef and nothing else. The empty-`methods`
        // iteration is a no-op.
        let mut structs = StructTable::new();
        let sid = structs.intern("Box__int32_t");
        structs.set_fields(sid, vec![("v".to_string(), Type::I32)]);
        structs.record_struct_instance(sid, "Box", vec![Type::I32]);

        let box_ctor = Func {
            is_pub: false,
            name: "Box".to_string(),
            params: vec![Param {
                name: "T".to_string(),
                ty: type_kw(),
                is_comptime: true,
                span: Span::DUMMY,
            }],
            ret: type_kw(),
            body: block(vec![ret(Expr::StructType {
                fields: vec![FieldDecl {
                    name: "v".to_string(),
                    ty: ty("T"),
                    span: Span::DUMMY,
                }],
                methods: vec![],
                span: Span::DUMMY,
            })]),
            span: Span::DUMMY,
        };
        let main = Func {
            is_pub: false,
            name: "main".to_string(),
            params: vec![],
            ret: ty("void"),
            body: block(vec![]),
            span: Span::DUMMY,
        };
        let m = Module {
            items: vec![Item::Func(box_ctor), Item::Func(main)],
        };
        let out = emit(&m, &structs, EmitMode::Program);

        assert!(
            out.contains("typedef struct { int32_t kd_v; } kd_struct_Box__int32_t;"),
            "monomorphised struct typedef missing:\n{out}"
        );
        assert!(
            !out.contains("kd_Box__int32_t_"),
            "a fields-only instance must emit no method functions:\n{out}"
        );
        assert!(
            !out.contains("kd_Box("),
            "type-constructor was emitted:\n{out}"
        );
    }

    // -- multiple type parameters (v0.135) -----------------------------------

    /// The post-sema shape of:
    /// ```text
    /// fn Pair(comptime A: type, comptime B: type) type {
    ///     return struct {
    ///         a: A,
    ///         b: B,
    ///         fn first(self: Self) A { return self.a; }
    ///         fn second(self: Self) B { return self.b; }
    ///         fn sum_widths(self: Self) i64 { return self.a + self.b; }
    ///     };
    /// }
    /// const IB = Pair(i32, i64);
    /// fn main() void {
    ///     var p: IB = IB{ .a = 3, .b = 9 };
    ///     print(p.first());      // 3   (A → i32)
    ///     print(p.second());     // 9   (B → i64)
    ///     print(p.sum_widths()); // 12  (uses both fields)
    /// }
    /// ```
    /// sema interns the monomorphised struct `Pair__int32_t_int64_t` (fields
    /// `a: i32`, `b: i64`) and records it as an instance of `Pair` with
    /// `args = [i32, i64]`. The backend must build the method substitution by
    /// **zipping** the constructor's two comptime type parameters with those two
    /// args (`A → i32`, `B → i64`, plus `Self → Struct(id)`), so `first` returns
    /// `int32_t`, `second` returns `int64_t`, and a per-field body resolves both
    /// fields (SPEC §31.2).
    fn pair_with_methods_program() -> (Module, StructTable) {
        let mut structs = StructTable::new();
        let sid = structs.intern("Pair__int32_t_int64_t");
        structs.set_fields(
            sid,
            vec![("a".to_string(), Type::I32), ("b".to_string(), Type::I64)],
        );
        structs.record_struct_instance(sid, "Pair", vec![Type::I32, Type::I64]);
        structs.add_alias("IB", Type::Struct(sid));

        let first = Func {
            is_pub: false,
            name: "first".to_string(),
            params: vec![param("self", "Self")],
            ret: ty("A"),
            body: block(vec![ret(field(ident("self"), "a"))]),
            span: Span::DUMMY,
        };
        let second = Func {
            is_pub: false,
            name: "second".to_string(),
            params: vec![param("self", "Self")],
            ret: ty("B"),
            body: block(vec![ret(field(ident("self"), "b"))]),
            span: Span::DUMMY,
        };
        let sum_widths = Func {
            is_pub: false,
            name: "sum_widths".to_string(),
            params: vec![param("self", "Self")],
            ret: ty("i64"),
            body: block(vec![ret(Expr::Binary {
                op: BinOp::Add,
                lhs: Box::new(field(ident("self"), "a")),
                rhs: Box::new(field(ident("self"), "b")),
                span: Span::DUMMY,
            })]),
            span: Span::DUMMY,
        };

        let pair_ctor = Func {
            is_pub: false,
            name: "Pair".to_string(),
            params: vec![
                Param {
                    name: "A".to_string(),
                    ty: type_kw(),
                    is_comptime: true,
                    span: Span::DUMMY,
                },
                Param {
                    name: "B".to_string(),
                    ty: type_kw(),
                    is_comptime: true,
                    span: Span::DUMMY,
                },
            ],
            ret: type_kw(),
            body: block(vec![ret(Expr::StructType {
                fields: vec![
                    FieldDecl {
                        name: "a".to_string(),
                        ty: ty("A"),
                        span: Span::DUMMY,
                    },
                    FieldDecl {
                        name: "b".to_string(),
                        ty: ty("B"),
                        span: Span::DUMMY,
                    },
                ],
                methods: vec![first, second, sum_widths],
                span: Span::DUMMY,
            })]),
            span: Span::DUMMY,
        };

        let alias = Item::Const(ConstDecl {
            is_pub: false,
            name: "IB".to_string(),
            ty: None,
            value: Expr::Call {
                callee: "Pair".to_string(),
                args: vec![ident("i32"), ident("i64")],
                span: Span::DUMMY,
            },
            span: Span::DUMMY,
        });

        let main = Func {
            is_pub: false,
            name: "main".to_string(),
            params: vec![],
            ret: ty("void"),
            body: block(vec![
                Stmt::Let {
                    is_const: false,
                    name: "p".to_string(),
                    ty: Some(ty("IB")),
                    value: Expr::StructLit {
                        name: "IB".to_string(),
                        fields: vec![finit("a", int(3)), finit("b", int(9))],
                        span: Span::DUMMY,
                    },
                    span: Span::DUMMY,
                },
                print(method_call(ident("p"), "first", vec![])),
                print(method_call(ident("p"), "second", vec![])),
                print(method_call(ident("p"), "sum_widths", vec![])),
            ]),
            span: Span::DUMMY,
        };

        let m = Module {
            items: vec![Item::Func(pair_ctor), alias, Item::Func(main)],
        };
        (m, structs)
    }

    #[test]
    fn two_type_param_constructor_zips_params_with_args() {
        let (m, structs) = pair_with_methods_program();
        let out = emit(&m, &structs, EmitMode::Program);

        // The monomorphised struct emits as an ordinary typedef with *both*
        // fields resolved to their concrete types.
        assert!(
            out.contains(
                "typedef struct { int32_t kd_a; int64_t kd_b; } kd_struct_Pair__int32_t_int64_t;"
            ),
            "two-field monomorphised struct typedef missing/wrong:\n{out}"
        );
        // `first` returns `A` — the FIRST type parameter — so it must resolve to
        // int32_t. If the params were not zipped positionally (e.g. all bound to
        // the last arg, the pre-v0.135 single-arg behaviour) this would be
        // int64_t and the assertion fails.
        assert!(
            out.contains(
                "int32_t kd_Pair__int32_t_int64_t_first(kd_struct_Pair__int32_t_int64_t kd_self);"
            ),
            "`first` did not resolve A → int32_t (param zip wrong):\n{out}"
        );
        assert!(
            out.contains(
                "int32_t kd_Pair__int32_t_int64_t_first(kd_struct_Pair__int32_t_int64_t kd_self) {"
            ),
            "`first` definition missing/wrong:\n{out}"
        );
        // `second` returns `B` — the SECOND type parameter — so it must resolve
        // to int64_t. Together with `first` this proves the positional zip.
        assert!(
            out.contains(
                "int64_t kd_Pair__int32_t_int64_t_second(kd_struct_Pair__int32_t_int64_t kd_self);"
            ),
            "`second` did not resolve B → int64_t (param zip wrong):\n{out}"
        );
        // A method using *both* fields resolves each through the struct.
        assert!(
            out.contains(
                "int64_t kd_Pair__int32_t_int64_t_sum_widths(kd_struct_Pair__int32_t_int64_t kd_self) {"
            ),
            "`sum_widths` definition missing/wrong:\n{out}"
        );
        assert!(
            out.contains("(kd_self).kd_a") && out.contains("(kd_self).kd_b"),
            "`sum_widths` body did not read both fields:\n{out}"
        );
        // Method calls lower to the instance C functions.
        assert!(
            out.contains("kd_Pair__int32_t_int64_t_first(kd_p)")
                && out.contains("kd_Pair__int32_t_int64_t_second(kd_p)")
                && out.contains("kd_Pair__int32_t_int64_t_sum_widths(kd_p)"),
            "method calls did not lower to the instance functions:\n{out}"
        );
        // The type-constructor itself is never emitted (§25.3).
        assert!(
            !out.contains("kd_Pair("),
            "type-constructor was emitted as a C function:\n{out}"
        );
        // `Self` must never leak into the C name space.
        assert!(
            !out.contains("kd_struct_Self") && !out.contains("kd_Self_"),
            "the contextual `Self` leaked into the emitted C:\n{out}"
        );
    }

    #[test]
    fn single_type_param_constructor_still_emits_identically() {
        // Regression: the v0.129/v0.130 single-type-parameter case must be
        // byte-for-byte unchanged under the new (vector-based) zip — the
        // length-1 zip binds the single param exactly as before.
        let (m, structs) = box_with_methods_program();
        let out = emit(&m, &structs, EmitMode::Program);
        assert!(
            out.contains("typedef struct { int32_t kd_v; } kd_struct_Box__int32_t;"),
            "single-param struct typedef regressed:\n{out}"
        );
        assert!(
            out.contains("int32_t kd_Box__int32_t_get(kd_struct_Box__int32_t kd_self) {"),
            "single-param method emission regressed:\n{out}"
        );
        assert!(
            out.contains(
                "kd_struct_Box__int32_t kd_Box__int32_t_replaced(kd_struct_Box__int32_t kd_self, int32_t kd_nv) {"
            ),
            "single-param `Self` resolution regressed:\n{out}"
        );
    }

    #[test]
    fn two_type_param_program_compiles_and_prints_field_values() {
        // End-to-end through the backend the emit_c module owns: emit C for a
        // program that builds `Pair(i32, i64)`, sets both fields, and reads each
        // back via methods, then compile with `cc` and run it, asserting the
        // printed values. Driving emit → cc → run from the AST+table exercises
        // the whole instance-method lowering without depending on sema/parser.
        let (m, structs) = pair_with_methods_program();
        let c = emit(&m, &structs, EmitMode::Program);

        let exe = std::env::temp_dir().join(format!(
            "kardc_emit_v135_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        crate::backend::cc_build(&c, &exe, &crate::backend::BuildOptions::default())
            .expect("emitted C for a two-type-param generic struct should compile");
        let output = std::process::Command::new(&exe)
            .output()
            .expect("the compiled program should run");
        let _ = std::fs::remove_file(&exe);

        assert!(output.status.success(), "program exited non-zero:\n{c}");
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert_eq!(
            stdout, "3\n9\n12\n",
            "field values printed wrong (expected first=3, second=9, sum=12):\nstdout={stdout}\n--- C ---\n{c}"
        );
    }

    // ---- v0.132: bitwise & shift operators -------------------------------

    #[test]
    fn bitwise_and_shift_binops_emit_c_operators() {
        // For each binary bitwise/shift op, `fn go(a:i32,b:i32) i32 { return
        // a OP b; }` lowers to `(kd_a <c-op> kd_b)` via BinOp::c_op() — the
        // existing `Expr::Binary` path already routes through `c_op()`, so the
        // new ops need no special-casing (SPEC §28.3).
        let cases = [
            (BinOp::BitAnd, "(kd_a & kd_b)"),
            (BinOp::BitOr, "(kd_a | kd_b)"),
            (BinOp::BitXor, "(kd_a ^ kd_b)"),
            (BinOp::Shl, "(kd_a << kd_b)"),
            (BinOp::Shr, "(kd_a >> kd_b)"),
        ];
        for (op, expected) in cases {
            let f = func(
                "go",
                vec![param("a", "i32"), param("b", "i32")],
                "i32",
                vec![ret(Expr::Binary {
                    op,
                    lhs: Box::new(ident("a")),
                    rhs: Box::new(ident("b")),
                    span: Span::DUMMY,
                })],
            );
            let m = Module {
                items: vec![Item::Func(f)],
            };
            let out = emit(&m, &StructTable::new(), EmitMode::Program);
            assert!(
                out.contains(expected),
                "{op:?} should emit {expected}:\n{out}"
            );
        }
    }

    #[test]
    fn bitnot_unary_emits_tilde() {
        // fn go(a: i32) i32 { return ~a; }  →  `(~kd_a)`, mirroring `-`/`!`.
        let f = func(
            "go",
            vec![param("a", "i32")],
            "i32",
            vec![ret(Expr::Unary {
                op: UnOp::BitNot,
                expr: Box::new(ident("a")),
                span: Span::DUMMY,
            })],
        );
        let m = Module {
            items: vec![Item::Func(f)],
        };
        let out = emit(&m, &StructTable::new(), EmitMode::Program);
        assert!(out.contains("(~kd_a)"), "~a should emit (~kd_a):\n{out}");
    }

    #[test]
    fn bit_twiddle_expression_lowers_with_precedence_parens() {
        // fn go(a,b,c: i32) i32 { return ((a << 4) | (b & 7)) ^ ~c; }
        // Each sub-expression is parenthesised, so the C output preserves the
        // intended grouping regardless of C's operator precedence.
        let shifted = Expr::Binary {
            op: BinOp::Shl,
            lhs: Box::new(ident("a")),
            rhs: Box::new(int(4)),
            span: Span::DUMMY,
        };
        let masked = Expr::Binary {
            op: BinOp::BitAnd,
            lhs: Box::new(ident("b")),
            rhs: Box::new(int(7)),
            span: Span::DUMMY,
        };
        let ored = Expr::Binary {
            op: BinOp::BitOr,
            lhs: Box::new(shifted),
            rhs: Box::new(masked),
            span: Span::DUMMY,
        };
        let notc = Expr::Unary {
            op: UnOp::BitNot,
            expr: Box::new(ident("c")),
            span: Span::DUMMY,
        };
        let xored = Expr::Binary {
            op: BinOp::BitXor,
            lhs: Box::new(ored),
            rhs: Box::new(notc),
            span: Span::DUMMY,
        };
        let f = func(
            "go",
            vec![param("a", "i32"), param("b", "i32"), param("c", "i32")],
            "i32",
            vec![ret(xored)],
        );
        let m = Module {
            items: vec![Item::Func(f)],
        };
        let out = emit(&m, &StructTable::new(), EmitMode::Program);
        assert!(
            out.contains("(((kd_a << 4) | (kd_b & 7)) ^ (~kd_c))"),
            "bit-twiddle expression lowering wrong:\n{out}"
        );
    }

    // -- v0.133 `for` loops over arrays & slices (SPEC §29.2) ---------------

    /// A `for (iter) |elem| { body }` / `for (iter, 0..) |elem, index| { body }`
    /// statement.
    fn for_stmt(iter: Expr, elem: &str, index: Option<&str>, body: Vec<Stmt>) -> Stmt {
        Stmt::For {
            iter,
            elem: elem.to_string(),
            index: index.map(|s| s.to_string()),
            body: block(body),
            span: Span::DUMMY,
        }
    }

    /// A `fn name(p: <pty>) void { <body> }` with a single (possibly composite)
    /// parameter type.
    fn fn_one_param(name: &str, p: &str, pty: TypeExpr, body: Vec<Stmt>) -> Func {
        Func {
            is_pub: false,
            name: name.to_string(),
            params: vec![Param {
                name: p.to_string(),
                ty: pty,
                is_comptime: false,
                span: Span::DUMMY,
            }],
            ret: ty("void"),
            body: block(body),
            span: Span::DUMMY,
        }
    }

    #[test]
    fn for_over_array_emits_indexed_while_with_byvalue_elem() {
        // fn go(a: [3]i32) void { for (a) |x| { print(x); } }
        let structs = arr_int_table();
        let f = fn_one_param(
            "go",
            "a",
            arr_ty("i32", 3),
            vec![for_stmt(ident("a"), "x", None, vec![print(ident("x"))])],
        );
        let m = Module {
            items: vec![Item::Func(f)],
        };
        let out = emit(&m, &structs, EmitMode::Program);
        // The iterable is copied once into the `__kd_for0` temp; a `usize`
        // (`uintptr_t`) index walks `0 .. 3` (the array's literal length).
        assert!(
            out.contains("kd_arr_int32_t_3 __kd_for0 = kd_a;"),
            "iterable temp wrong:\n{out}"
        );
        assert!(
            out.contains("uintptr_t __kd_fi0 = 0;"),
            "index var wrong:\n{out}"
        );
        assert!(
            out.contains("while (__kd_fi0 < 3) {"),
            "loop condition wrong:\n{out}"
        );
        // The element binds by value through `.data[i]`.
        assert!(
            out.contains("int32_t kd_x = __kd_for0.data[__kd_fi0];"),
            "by-value element binding wrong:\n{out}"
        );
        // The index is incremented at the end of the body (fall-through edge).
        assert!(
            out.contains("__kd_fi0 += 1;"),
            "index increment missing:\n{out}"
        );
        // No index capture was written, so no `kd_` index local is emitted.
        assert!(
            !out.contains("kd_i = __kd_fi0;"),
            "unexpected index binding for the non-index form:\n{out}"
        );
        // The body lowering is unchanged (`print(x)` over the element copy).
        assert!(
            out.contains("kd_print((long long)(kd_x));"),
            "body lowering wrong:\n{out}"
        );
    }

    #[test]
    fn for_index_form_binds_usize_index() {
        // fn go(a: [3]i32) void { for (a, 0..) |x, i| { print(i); } }
        let structs = arr_int_table();
        let f = fn_one_param(
            "go",
            "a",
            arr_ty("i32", 3),
            vec![for_stmt(ident("a"), "x", Some("i"), vec![print(ident("i"))])],
        );
        let m = Module {
            items: vec![Item::Func(f)],
        };
        let out = emit(&m, &structs, EmitMode::Program);
        // The index form binds a `usize` (`uintptr_t`) local to the walking index.
        assert!(
            out.contains("uintptr_t kd_i = __kd_fi0;"),
            "index capture binding wrong:\n{out}"
        );
        // The element still binds by value before the index.
        assert!(
            out.contains("int32_t kd_x = __kd_for0.data[__kd_fi0];"),
            "element binding wrong:\n{out}"
        );
        let elem_at = out.find("kd_x = __kd_for0.data").expect("elem binding present");
        let idx_at = out.find("kd_i = __kd_fi0;").expect("index binding present");
        assert!(elem_at < idx_at, "element must bind before index:\n{out}");
    }

    #[test]
    fn for_over_slice_uses_ptr_and_runtime_len() {
        // fn go(s: []i32) void { for (s) |x| { print(x); } }
        let structs = slice_int_table();
        let f = fn_one_param(
            "go",
            "s",
            slice_ty("i32"),
            vec![for_stmt(ident("s"), "x", None, vec![print(ident("x"))])],
        );
        let m = Module {
            items: vec![Item::Func(f)],
        };
        let out = emit(&m, &structs, EmitMode::Program);
        // A slice iterable is copied once; the bound is the runtime `.len`.
        assert!(
            out.contains("kd_slice_int32_t __kd_for0 = kd_s;"),
            "slice iterable temp wrong:\n{out}"
        );
        assert!(
            out.contains("while (__kd_fi0 < __kd_for0.len) {"),
            "slice loop condition must use the runtime `.len`:\n{out}"
        );
        // The element binds by value through `.ptr[i]`.
        assert!(
            out.contains("int32_t kd_x = __kd_for0.ptr[__kd_fi0];"),
            "slice element binding must use `.ptr[i]`:\n{out}"
        );
    }

    #[test]
    fn for_continue_still_increments_index() {
        // fn go(a: [3]i32) void { for (a) |x| { continue; } }
        // A `continue` must advance the index, so the increment is emitted on
        // the `continue` edge, immediately before the C `continue;`.
        let structs = arr_int_table();
        let f = fn_one_param(
            "go",
            "a",
            arr_ty("i32", 3),
            vec![for_stmt(
                ident("a"),
                "x",
                None,
                vec![Stmt::Continue(Span::DUMMY)],
            )],
        );
        let m = Module {
            items: vec![Item::Func(f)],
        };
        let out = emit(&m, &structs, EmitMode::Program);
        let inc_at = out.find("__kd_fi0 += 1;").expect("increment present");
        let cont_at = out.find("continue;").expect("continue present");
        assert!(
            inc_at < cont_at,
            "the index increment must run before `continue;`:\n{out}"
        );
        // The unconditional `continue` diverges, so the fall-through increment is
        // suppressed — exactly one increment is emitted (the `continue` edge).
        assert_eq!(
            out.matches("__kd_fi0 += 1;").count(),
            1,
            "exactly one increment expected (no duplicate on a diverging body):\n{out}"
        );
    }

    // ---- v0.141: @panic / unreachable runtime-safety traps ----------------
    // (Reuses the existing `str_lit` / `u8_slice_table` test helpers above.)

    /// `@panic(msg)` — an `Expr::Builtin { name: "panic" }` (SPEC §35).
    fn panic_call(msg: Expr) -> Expr {
        Expr::Builtin {
            name: "panic".to_string(),
            args: vec![msg],
            span: Span::DUMMY,
        }
    }

    #[test]
    fn panic_statement_emits_kd_panic_over_string_slice_and_diverges() {
        // fn main() void { @panic("boom"); }
        let m = Module {
            items: vec![Item::Func(func(
                "main",
                vec![],
                "void",
                vec![Stmt::Expr(panic_call(str_lit("boom")))],
            ))],
        };
        let out = emit(&m, &u8_slice_table(), EmitMode::Program);
        // The `_Noreturn` helper is declared once a `[]u8` slice exists.
        assert!(
            out.contains(
                "_Noreturn void kd_panic(kd_slice_uint8_t m) { fwrite(m.ptr, 1, m.len, stderr); fputc(0x0a, stderr); exit(101); }"
            ),
            "kd_panic helper missing:\n{out}"
        );
        // Ordering: the helper names `kd_slice_uint8_t`, so the typedef must
        // precede it (why `kd_panic` lives at the tail of the type-def section,
        // not in the prelude).
        let typedef_at = out
            .find("} kd_slice_uint8_t;")
            .expect("the []u8 slice typedef must be emitted");
        let helper_at = out
            .find("_Noreturn void kd_panic(")
            .expect("the kd_panic helper must be emitted");
        assert!(
            typedef_at < helper_at,
            "kd_panic must follow the kd_slice_uint8_t typedef:\n{out}"
        );
        // The statement lowers to the bare call over the string slice — NOT the
        // `(.., 0)` comma form — and so diverges.
        assert!(
            out.contains(
                "kd_panic(((kd_slice_uint8_t){ .ptr = (uint8_t *)\"boom\", .len = 4 }));"
            ),
            "panic statement lowering wrong:\n{out}"
        );
        assert!(
            !out.contains("(kd_panic(((kd_slice_uint8_t){ .ptr = (uint8_t *)\"boom\", .len = 4 })), 0)"),
            "a statement-position @panic must not use the comma-expression form:\n{out}"
        );
    }

    #[test]
    fn panic_expression_uses_comma_form() {
        // `@panic` directly through emit_expr (a value position) keeps the
        // `(kd_panic(<msg>), 0)` comma form, whose dead `0` satisfies an integer
        // value position.
        let t = StructTable::new();
        let mut em = Emitter::new(EmitMode::Program, &t);
        let s = em.emit_expr(&panic_call(str_lit("x")));
        assert_eq!(
            s,
            "(kd_panic(((kd_slice_uint8_t){ .ptr = (uint8_t *)\"x\", .len = 1 })), 0)"
        );
        // `unreachable` likewise.
        let u = em.emit_expr(&Expr::Unreachable { span: Span::DUMMY });
        assert_eq!(u, "(kd_unreachable(), 0)");
    }

    #[test]
    fn unreachable_statement_emits_kd_unreachable_and_diverges() {
        // fn main() void { unreachable; }
        let m = Module {
            items: vec![Item::Func(func(
                "main",
                vec![],
                "void",
                vec![Stmt::Expr(Expr::Unreachable { span: Span::DUMMY })],
            ))],
        };
        let out = emit(&m, &StructTable::new(), EmitMode::Program);
        // The prelude helper is always present (no type dependency).
        assert!(
            out.contains(
                "_Noreturn void kd_unreachable(void) { fputs(\"reached unreachable code\\n\", stderr); exit(101); }"
            ),
            "kd_unreachable prelude helper missing:\n{out}"
        );
        // The statement lowers to the bare call.
        assert!(
            out.contains("kd_unreachable();"),
            "unreachable statement lowering wrong:\n{out}"
        );
    }

    #[test]
    fn panic_program_exits_101_and_prints_message_to_stderr() {
        // End-to-end: emit → cc → run a program that hits `@panic("boom")`,
        // asserting exit code 101 and the message (plus newline) on stderr.
        let m = Module {
            items: vec![Item::Func(func(
                "main",
                vec![],
                "void",
                vec![Stmt::Expr(panic_call(str_lit("boom")))],
            ))],
        };
        let c = emit(&m, &u8_slice_table(), EmitMode::Program);
        let exe = std::env::temp_dir().join(format!(
            "kardc_emit_v141_panic_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        crate::backend::cc_build(&c, &exe, &crate::backend::BuildOptions::default())
            .expect("emitted C for an @panic program should compile");
        let output = std::process::Command::new(&exe)
            .output()
            .expect("the compiled program should run");
        let _ = std::fs::remove_file(&exe);
        assert_eq!(
            output.status.code(),
            Some(101),
            "an @panic must exit 101:\n--- C ---\n{c}"
        );
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert_eq!(
            stderr, "boom\n",
            "panic message wrong on stderr:\nstderr={stderr}\n--- C ---\n{c}"
        );
    }

    #[test]
    fn unreachable_in_else_arm_compiles() {
        // fn main() void { switch (0) { 0 => return; else => unreachable; } }
        let m = Module {
            items: vec![Item::Func(func(
                "main",
                vec![],
                "void",
                vec![Stmt::Switch {
                    scrutinee: int(0),
                    arms: vec![SwitchArm {
                        labels: vec![int(0)],
                        capture: None,
                        body: block(vec![Stmt::Return {
                            value: None,
                            span: Span::DUMMY,
                        }]),
                        span: Span::DUMMY,
                    }],
                    default: Some(block(vec![Stmt::Expr(Expr::Unreachable {
                        span: Span::DUMMY,
                    })])),
                    span: Span::DUMMY,
                }],
            ))],
        };
        let c = emit(&m, &StructTable::new(), EmitMode::Program);
        assert!(
            c.contains("kd_unreachable();"),
            "else-arm unreachable lowering missing:\n{c}"
        );
        // The emitted switch is valid C.
        let exe = std::env::temp_dir().join(format!(
            "kardc_emit_v141_switch_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        crate::backend::cc_build(&c, &exe, &crate::backend::BuildOptions::default())
            .expect("a switch with an `else => unreachable` arm should compile");
        let _ = std::fs::remove_file(&exe);
    }
}
