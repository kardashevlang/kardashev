//! C backend: a validated AST → portable, deterministic C11 source text.
//!
//! `defer` statements are lowered here: each scope tracks its deferred
//! statements and flushes them in LIFO (reverse registration) order at every
//! exit edge — fall-through off the end of a block, `return`, `break` and
//! `continue` (and, in test mode, a failed `expect`). Identical input always
//! produces byte-identical output.

use std::collections::HashMap;

use crate::ast::{Block, Expr, Func, Item, Module, Param, Stmt, TestBlock, TypeExpr, UnOp};
use crate::const_eval::ConstVal;
use crate::types::{StructTable, Type};

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

/// A lexical scope active during emission. Each one accumulates the `defer`
/// statement bodies registered within it (in registration order) and notes
/// whether it is the body of a `while` loop (so `break`/`continue` know where
/// to stop flushing). A loop-body scope also carries the loop's optional
/// continue-expression.
struct Scope {
    defers: Vec<Stmt>,
    is_loop_body: bool,
    cont: Option<Stmt>,
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
        }
    }

    /// Pre-pass: record the resolved return type of every top-level function and
    /// every struct function. These let [`Emitter::struct_of_expr`] follow a
    /// receiver chain that passes through a call to find the struct whose
    /// function a method call invokes. Pure bookkeeping — emits nothing.
    fn collect_signatures(&mut self, module: &Module) {
        for item in &module.items {
            match item {
                Item::Func(f) => {
                    let ret = self.resolve_ty(&f.ret);
                    self.fn_ret.insert(f.name.clone(), ret);
                    let ptys: Vec<Type> = f.params.iter().map(|p| self.resolve_ty(&p.ty)).collect();
                    self.fn_params.insert(f.name.clone(), ptys);
                }
                Item::Struct(s) => {
                    for m in &s.methods {
                        let ret = self.resolve_ty(&m.ret);
                        self.method_ret.insert((s.name.clone(), m.name.clone()), ret);
                        let ptys: Vec<Type> =
                            m.params.iter().map(|p| self.resolve_ty(&p.ty)).collect();
                        self.method_params
                            .insert((s.name.clone(), m.name.clone()), ptys);
                    }
                }
                _ => {}
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
        // `<stdlib.h>` provides `exit`, used by the `?T` unwrap panic helper.
        self.out.push_str("#include <stdlib.h>\n");
        self.out
            .push_str("static void kd_print(long long v) { printf(\"%lld\\n\", v); }\n");
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
        {
            return;
        }

        // A definition node: a struct, an interned optional, or an interned
        // error union.
        #[derive(Clone, Copy, PartialEq, Eq, Hash)]
        enum Node {
            Struct(u32),
            Opt(u32),
            ErrU(u32),
        }
        fn dep_of(t: Type) -> Option<Node> {
            match t {
                Type::Struct(s) => Some(Node::Struct(s)),
                Type::Optional(o) => Some(Node::Opt(o)),
                Type::ErrorUnion(e) => Some(Node::ErrU(e)),
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
                        if let Some(d) = dep_of(*fty) {
                            visit(d, structs, seen, order);
                        }
                    }
                }
                Node::Opt(o) => {
                    if let Some(d) = dep_of(structs.optional_inner(o)) {
                        visit(d, structs, seen, order);
                    }
                }
                Node::ErrU(e) => {
                    if let Some(d) = dep_of(structs.error_union_payload(e)) {
                        visit(d, structs, seen, order);
                    }
                }
            }
            order.push(n);
        }

        let mut seen = HashSet::new();
        let mut order = Vec::new();
        for (id, _) in structs.iter() {
            visit(Node::Struct(id), structs, &mut seen, &mut order);
        }
        for (id, _) in structs.optionals() {
            visit(Node::Opt(id), structs, &mut seen, &mut order);
        }
        for (id, _) in structs.error_unions() {
            visit(Node::ErrU(id), structs, &mut seen, &mut order);
        }

        for n in order {
            match n {
                Node::Struct(id) => self.emit_one_struct(id),
                Node::Opt(id) => self.emit_one_optional(id),
                Node::ErrU(id) => self.emit_one_error_union(id),
            }
        }
        self.blank();
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

    /// Fold each top-level `const` initializer to a literal (C does not treat
    /// `const` objects as constant expressions) and emit it. Constants are
    /// processed in source order so later ones may reference earlier ones.
    fn emit_consts(&mut self, module: &Module) {
        let mut any = false;
        for item in &module.items {
            if let Item::Const(c) = item {
                // The module is validated, so this evaluation always succeeds;
                // if it somehow does not we skip the const rather than panic.
                if let Ok(v) = crate::const_eval::eval(&c.value, &self.consts) {
                    let cty = self.cty(&c.ty);
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
        // Ordinary top-level functions first.
        for item in &module.items {
            if let Item::Func(f) = item {
                let ret = self.cty(&f.ret);
                let params = self.format_params(&f.params);
                self.line(&format!("{} kd_{}({});", ret, f.name, params));
                any = true;
            }
        }
        // Then every struct function, declared alongside ordinary ones. Each
        // lowers to a free C function `kd_<Struct>_<method>` whose `self`
        // parameter (if any) is an ordinary by-value struct parameter.
        for item in &module.items {
            if let Item::Struct(s) = item {
                for m in &s.methods {
                    let ret = self.cty(&m.ret);
                    let params = self.format_params(&m.params);
                    self.line(&format!("{} kd_{}_{}({});", ret, s.name, m.name, params));
                    any = true;
                }
            }
        }
        if any {
            self.blank();
        }
    }

    fn emit_func_defs(&mut self, module: &Module) {
        // Ordinary top-level functions first, then struct functions, matching
        // the forward-declaration order.
        for item in &module.items {
            if let Item::Func(f) = item {
                self.emit_func(f);
                self.blank();
            }
        }
        for item in &module.items {
            if let Item::Struct(s) = item {
                for m in &s.methods {
                    let cname = format!("kd_{}_{}", s.name, m.name);
                    self.emit_func_named(m, &cname);
                    self.blank();
                }
            }
        }
    }

    fn format_params(&self, params: &[Param]) -> String {
        if params.is_empty() {
            "void".to_string()
        } else {
            params
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
        let base = Type::from_name(&t.name)
            .or_else(|| self.structs.id_of(&t.name).map(Type::Struct))
            .unwrap_or(Type::Void);
        if t.optional {
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
        } else {
            base
        }
    }

    /// The C type spelling for a resolved [`Type`]: a struct resolves through
    /// the table (`Type::c_name` would panic on it), an optional through
    /// `optional_c_name`; primitives use their builtin C name.
    fn cty_of(&self, t: Type) -> String {
        match t {
            Type::Struct(id) => self.structs.c_name(id),
            Type::Optional(id) => self.structs.optional_c_name(id),
            Type::ErrorUnion(id) => self.structs.error_union_c_name(id),
            other => other.c_name().to_string(),
        }
    }

    /// The C type spelling for a source type reference. Builtins map through
    /// [`Type::c_name`]; struct names resolve to `kd_struct_<Name>` via the
    /// table; an unresolvable name (never reached for a validated module) falls
    /// back to `int64_t`.
    fn cty(&self, t: &TypeExpr) -> String {
        let base = if let Some(prim) = Type::from_name(&t.name) {
            prim
        } else if let Some(id) = self.structs.id_of(&t.name) {
            Type::Struct(id)
        } else {
            Type::I64
        };
        if t.optional {
            // Matches `optional_c_name(id)` (`kd_opt_<type_mangle(inner)>`)
            // without needing the interned id.
            format!("kd_opt_{}", self.structs.type_mangle(base))
        } else if t.error_union {
            // Matches `error_union_c_name(id)` (`kd_err_<type_mangle(payload)>`).
            format!("kd_err_{}", self.structs.type_mangle(base))
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
        self.current_ret = self.resolve_ty(&f.ret);
        let ret = self.cty(&f.ret);
        let params = self.format_params(&f.params);
        self.line(&format!("{} {}({}) {{", ret, c_name, params));
        let mut scope = Scope::function();
        for p in &f.params {
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
            self.flush_current_reversed();
            // A loop body runs its continue-clause at the end of each iteration
            // (the fall-through edge), after the body's defers.
            let cont = {
                let top = self.scopes.last().expect("scope present");
                if top.is_loop_body {
                    top.cont.clone()
                } else {
                    None
                }
            };
            if let Some(c) = cont {
                self.emit_cont(&c);
            }
        }
        self.scopes.pop();
        self.indent -= 1;
        diverged
    }

    // -- statements ---------------------------------------------------------

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
                // Coerce the initializer to the declared type (a `T` value,
                // `null` or `error.X` widens to `?T` / `!T` when annotated).
                let lty = self.resolve_ty(ty);
                let es = if let Expr::Try { expr, .. } = value {
                    // `var x = try e;` hoists the error propagation (which may
                    // early-return) and binds the unwrapped payload.
                    let payload = self.emit_try(expr);
                    self.coerce_str(&payload, self.try_payload_type(expr), lty)
                } else {
                    self.emit_coerced(value, lty)
                };
                let ct = self.cty(ty);
                let prefix = if *is_const { "const " } else { "" };
                self.line(&format!("{}{} kd_{} = {};", prefix, ct, name, es));
                // Record the local's type so a later method call / coercion on
                // it resolves correctly.
                if let Some(scope) = self.scopes.last_mut() {
                    scope.var_types.insert(name.clone(), lty);
                }
                false
            }
            Stmt::Assign { name, value, .. } => {
                // The target is an assignable `var` local; coerce the RHS to its
                // declared type (so a `T`/`null` RHS widens to a `?T` target).
                let es = match self.lookup_var_type(name) {
                    Some(t) => self.emit_coerced(value, t),
                    None => self.emit_expr(value),
                };
                self.line(&format!("kd_{} = {};", name, es));
                false
            }
            Stmt::FieldAssign { place, value, .. } => {
                // `place` is a field-access chain (`a.b.c`); lowering it yields a
                // C lvalue, so the assignment is a plain `(<place>) = (<value>);`.
                // Coerce the RHS to the field's type (widening to `?T` if it is
                // an optional field).
                let ps = self.emit_expr(place);
                let es = match self.type_of_expr(place) {
                    Some(t) => self.emit_coerced(value, t),
                    None => self.emit_expr(value),
                };
                self.line(&format!("({}) = ({});", ps, es));
                false
            }
            Stmt::Expr(e) => self.emit_expr_stmt(e),
            Stmt::Return { value, .. } => {
                self.emit_return(value);
                true
            }
            Stmt::If {
                cond, then, els, ..
            } => self.emit_if(cond, then, els),
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
            Stmt::Break(_) => {
                self.flush_to_loop_reversed();
                self.line("break;");
                true
            }
            Stmt::Continue(_) => {
                if let Some(i) = self.flush_to_loop_reversed() {
                    if let Some(c) = self.scopes[i].cont.clone() {
                        self.emit_cont(&c);
                    }
                }
                self.line("continue;");
                true
            }
            Stmt::Defer { stmt, .. } => {
                // Register only; the body runs at scope exit, not now.
                if let Some(scope) = self.scopes.last_mut() {
                    scope.defers.push((**stmt).clone());
                }
                false
            }
            Stmt::Block(b) => self.emit_block(b, Scope::plain()),
        }
    }

    /// Emit a `while` continue-clause statement (an assignment or expression).
    /// The parser restricts it to those two shapes, and it carries no `defer`
    /// or control-flow concerns, so it is emitted directly without the scope
    /// machinery `emit_stmt` uses.
    fn emit_cont(&mut self, c: &Stmt) {
        match c {
            Stmt::Assign { name, value, .. } => {
                let es = match self.lookup_var_type(name) {
                    Some(t) => self.emit_coerced(value, t),
                    None => self.emit_expr(value),
                };
                self.line(&format!("kd_{} = {};", name, es));
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
                    self.flush_all_reversed();
                    self.line("return 1;");
                    self.indent -= 1;
                    self.line("}");
                    return false;
                }
            }
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
        self.finish_return(val_str, ret_ty);
    }

    /// Emit the actual `return` (with the deferred-temp dance) from a
    /// pre-computed, already-coerced value string. Shared by ordinary returns
    /// and `return try e;`.
    fn finish_return(&mut self, val_str: Option<String>, ret_ty: Type) {
        let non_void = ret_ty != Type::Void;
        let active = self.any_defer_active();
        if active && non_void {
            // Evaluate the value into a temporary *before* running the defers,
            // since the defers may mutate state the value depends on.
            let es = val_str.unwrap_or_else(|| "0".to_string());
            let ret = self.cty_of(ret_ty);
            self.line(&format!("{} __kd_ret = ({});", ret, es));
            self.flush_all_reversed();
            self.line("return __kd_ret;");
        } else {
            if active {
                self.flush_all_reversed();
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
        self.flush_all_reversed();
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
                    Stmt::If {
                        cond, then, els, ..
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

    fn any_defer_active(&self) -> bool {
        self.scopes.iter().any(|s| !s.defers.is_empty())
    }

    /// Flush the innermost scope's defers in reverse registration order.
    fn flush_current_reversed(&mut self) {
        if let Some(scope) = self.scopes.last() {
            let defers = scope.defers.clone();
            for s in defers.iter().rev() {
                self.emit_stmt(s);
            }
        }
    }

    /// Flush every active scope, innermost first down to the function scope,
    /// each in reverse registration order. Used by deferred `return` and by a
    /// failed `expect`.
    fn flush_all_reversed(&mut self) {
        let n = self.scopes.len();
        for i in (0..n).rev() {
            let defers = self.scopes[i].defers.clone();
            for s in defers.iter().rev() {
                self.emit_stmt(s);
            }
        }
    }

    /// Flush scopes innermost-first down to and including the nearest loop-body
    /// scope (each reversed). Returns that loop-body scope's index, or `None`
    /// if there is no enclosing loop (which a validated module never hits).
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
            let defers = self.scopes[i].defers.clone();
            for s in defers.iter().rev() {
                self.emit_stmt(s);
            }
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
            Expr::Ident { name, .. } => format!("kd_{}", name),
            Expr::Unary { op, expr, .. } => {
                let inner = self.emit_expr(expr);
                let opc = match op {
                    UnOp::Neg => "-",
                    UnOp::Not => "!",
                };
                format!("({}{})", opc, inner)
            }
            Expr::Binary { op, lhs, rhs, .. } => {
                let l = self.emit_expr(lhs);
                let r = self.emit_expr(rhs);
                format!("({} {} {})", l, op.c_op(), r)
            }
            Expr::Call { callee, args, .. } => {
                if callee == "print" {
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
                // Field access: `(<base>).kd_<field>`. The base is parenthesized
                // so a compound base expression (e.g. a literal or another access)
                // composes correctly: `((p).kd_a).kd_b`.
                let b = self.emit_expr(base);
                format!("({}).kd_{}", b, field)
            }
            Expr::StructLit { name, fields, .. } => {
                // C99 compound literal: `((kd_struct_<Name>){ .kd_<f> = <v>, ... })`.
                let (cname, sid) = match self.structs.id_of(name) {
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
        }
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
            Expr::Ident { name, .. } => self.structs.id_of(name).map(|_| name.clone()),
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
            let self_str = self.emit_expr(receiver);
            let struct_name = self.struct_of_expr(receiver).unwrap_or_default();
            let params = self
                .method_params
                .get(&(struct_name.clone(), method.to_string()))
                .cloned();
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
            Expr::StructLit { name, .. } => Some(name.clone()),
            Expr::Call { callee, .. } => match self.fn_ret.get(callee)? {
                Type::Struct(id) => Some(self.structs.get(*id).name.clone()),
                _ => None,
            },
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
            _ => None,
        }
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
            Expr::Ident { name, .. } => self.lookup_var_type(name),
            Expr::Unary { op, expr, .. } => match op {
                UnOp::Not => Some(Type::Bool),
                UnOp::Neg => self.type_of_expr(expr),
            },
            Expr::Binary { op, lhs, .. } => {
                if op.is_bool_result() {
                    Some(Type::Bool)
                } else {
                    // Arithmetic yields the (shared) operand type.
                    self.type_of_expr(lhs)
                }
            }
            Expr::Call { callee, .. } => self.fn_ret.get(callee).copied(),
            Expr::Comptime { expr, .. } => self.type_of_expr(expr),
            Expr::StructLit { name, .. } => self.structs.id_of(name).map(Type::Struct),
            Expr::Field { base, field, .. } => match self.type_of_expr(base)? {
                Type::Struct(id) => self.structs.get(id).field_type(field),
                _ => None,
            },
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
            // `try` / `catch` both produce the payload `T` of the `!T` operand.
            Expr::Try { expr, .. } => match self.type_of_expr(expr)? {
                Type::ErrorUnion(id) => Some(self.structs.error_union_payload(id)),
                other => Some(other),
            },
            Expr::Catch { expr, .. } => match self.type_of_expr(expr)? {
                Type::ErrorUnion(id) => Some(self.structs.error_union_payload(id)),
                other => Some(other),
            },
        }
    }

    /// Lower `e` to a C string, coercing it to `expected`. When `expected` is an
    /// optional `?T`, a `null` source widens to `{ .has = false }`, a value
    /// already of type `?T` passes through unchanged, and any other (`T`) value
    /// widens to `{ .has = true, .val = <e> }`. When `expected` is not an
    /// optional this is just [`Emitter::emit_expr`].
    fn emit_coerced(&mut self, e: &Expr, expected: Type) -> String {
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
            self.flush_current_reversed();
        }
        self.scopes.pop();
        self.line("return 0;");
        self.indent -= 1;
        self.line("}");
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{
        BinOp, Block, Expr, FieldDecl, FieldInit, Func, Item, Module, Param, Stmt, StructDecl,
        TestBlock, TypeExpr,
    };
    use crate::span::Span;
    use crate::types::{StructTable, Type};

    fn ty(name: &str) -> TypeExpr {
        TypeExpr {
            name: name.to_string(),
            optional: false,
            error_union: false,
            span: Span::DUMMY,
        }
    }

    fn opt_ty(name: &str) -> TypeExpr {
        TypeExpr {
            name: name.to_string(),
            optional: true,
            error_union: false,
            span: Span::DUMMY,
        }
    }

    fn err_ty(name: &str) -> TypeExpr {
        TypeExpr {
            name: name.to_string(),
            optional: false,
            error_union: true,
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
                    span: Span::DUMMY,
                },
                Param {
                    name: "b".to_string(),
                    ty: ty("i32"),
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
                    ty: ty("Point"),
                    value: lit,
                    span: Span::DUMMY,
                },
                Stmt::FieldAssign {
                    place: field(ident("p"), "x"),
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
                ty: opt_ty("i32"),
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
                ty: opt_ty("i32"),
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
                span: Span::DUMMY,
            }],
            ret: ty("void"),
            body: block(vec![Stmt::Let {
                is_const: false,
                name: "y".to_string(),
                ty: opt_ty("i32"),
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
                    ty: ty("i32"),
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
}
