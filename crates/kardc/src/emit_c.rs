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
    em.emit_prelude();
    em.emit_structs();
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
}

impl Scope {
    fn plain() -> Scope {
        Scope {
            defers: Vec::new(),
            is_loop_body: false,
            cont: None,
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
        self.out
            .push_str("static void kd_print(long long v) { printf(\"%lld\\n\", v); }\n");
        self.blank();
    }

    /// Emit one C `typedef struct { ... } kd_struct_<Name>;` per struct, in
    /// declaration (id) order — exactly the table's iteration order, so a
    /// field of a previously-declared struct type is always already in scope.
    /// An empty struct gets a `char _unused;` member so it stays valid C.
    fn emit_structs(&mut self) {
        // Copy the reference so the iteration borrows the table (lifetime `'a`)
        // rather than `self`, leaving `self` free for `cty_of` / `line`.
        let structs = self.structs;
        if structs.is_empty() {
            return;
        }
        for (id, info) in structs.iter() {
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
        self.blank();
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
        for item in &module.items {
            if let Item::Func(f) = item {
                let ret = self.cty(&f.ret);
                let params = self.format_params(&f.params);
                self.line(&format!("{} kd_{}({});", ret, f.name, params));
                any = true;
            }
        }
        if any {
            self.blank();
        }
    }

    fn emit_func_defs(&mut self, module: &Module) {
        for item in &module.items {
            if let Item::Func(f) = item {
                self.emit_func(f);
                self.blank();
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
        Type::from_name(&t.name)
            .or_else(|| self.structs.id_of(&t.name).map(Type::Struct))
            .unwrap_or(Type::Void)
    }

    /// The C type spelling for a resolved [`Type`]: a struct resolves through
    /// the table (`Type::c_name` would panic on it); primitives use their
    /// builtin C name.
    fn cty_of(&self, t: Type) -> String {
        match t {
            Type::Struct(id) => self.structs.c_name(id),
            other => other.c_name().to_string(),
        }
    }

    /// The C type spelling for a source type reference. Builtins map through
    /// [`Type::c_name`]; struct names resolve to `kd_struct_<Name>` via the
    /// table; an unresolvable name (never reached for a validated module) falls
    /// back to `int64_t`.
    fn cty(&self, t: &TypeExpr) -> String {
        if let Some(prim) = Type::from_name(&t.name) {
            prim.c_name().to_string()
        } else if let Some(id) = self.structs.id_of(&t.name) {
            self.structs.c_name(id)
        } else {
            "int64_t".to_string()
        }
    }

    // -- functions ----------------------------------------------------------

    fn emit_func(&mut self, f: &Func) {
        self.scopes.clear();
        self.current_ret = self.resolve_ty(&f.ret);
        let ret = self.cty(&f.ret);
        let params = self.format_params(&f.params);
        self.line(&format!("{} kd_{}({}) {{", ret, f.name, params));
        self.emit_block(&f.body, Scope::function());
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
                let es = self.emit_expr(value);
                let ct = self.cty(ty);
                let prefix = if *is_const { "const " } else { "" };
                self.line(&format!("{}{} kd_{} = {};", prefix, ct, name, es));
                false
            }
            Stmt::Assign { name, value, .. } => {
                let es = self.emit_expr(value);
                self.line(&format!("kd_{} = {};", name, es));
                false
            }
            Stmt::FieldAssign { place, value, .. } => {
                // `place` is a field-access chain (`a.b.c`); lowering it yields a
                // C lvalue, so the assignment is a plain `(<place>) = (<value>);`.
                let ps = self.emit_expr(place);
                let es = self.emit_expr(value);
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
                let es = self.emit_expr(value);
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
        let es = self.emit_expr(e);
        self.line(&format!("{};", es));
        false
    }

    fn emit_return(&mut self, value: &Option<Expr>) {
        let non_void = self.current_ret != Type::Void;
        let active = self.any_defer_active();
        if active && non_void {
            // Evaluate the value into a temporary *before* running the defers,
            // since the defers may mutate state the value depends on.
            let es = match value {
                Some(e) => self.emit_expr(e),
                None => "0".to_string(),
            };
            let ret = self.cty_of(self.current_ret);
            self.line(&format!("{} __kd_ret = ({});", ret, es));
            self.flush_all_reversed();
            self.line("return __kd_ret;");
        } else {
            if active {
                self.flush_all_reversed();
            }
            match value {
                Some(e) => {
                    let es = self.emit_expr(e);
                    self.line(&format!("return ({});", es));
                }
                None => self.line("return;"),
            }
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
                    let a = args
                        .iter()
                        .map(|x| self.emit_expr(x))
                        .collect::<Vec<_>>()
                        .join(", ");
                    format!("kd_{}({})", callee, a)
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
                let cname = match self.structs.id_of(name) {
                    Some(id) => self.structs.c_name(id),
                    // Validated input always resolves; fall back to the canonical
                    // spelling so emission stays well-formed even if it does not.
                    None => format!("kd_struct_{}", name),
                };
                if fields.is_empty() {
                    format!("(({}){{0}})", cname)
                } else {
                    let inits = fields
                        .iter()
                        .map(|fi| {
                            let v = self.emit_expr(&fi.value);
                            format!(".kd_{} = {}", fi.name, v)
                        })
                        .collect::<Vec<_>>()
                        .join(", ");
                    format!("(({}){{ {} }})", cname, inits)
                }
            }
        }
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
        BinOp, Block, Expr, FieldInit, Func, Item, Module, Param, Stmt, TestBlock, TypeExpr,
    };
    use crate::span::Span;
    use crate::types::{StructTable, Type};

    fn ty(name: &str) -> TypeExpr {
        TypeExpr {
            name: name.to_string(),
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
}
