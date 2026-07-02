// emit.ks — self-host stage 3 (v0.161): a C emitter for the SCALAR SUBSET,
// written in kardashev, mirroring `crates/kardc/src/emit_c.rs` decision for
// decision so that — for every subset program — the emitted C is
// BYTE-IDENTICAL to the Rust emitter's `EmitMode::Program` output.
//
// The subset (the "growing subset" of ROADMAP v0.159.0+, first slice):
//
//   - types: `i32`, `i64`, `bool`, `void` — bare names only (no `?`/`!`/`*`/
//     `[]`/`[N]`/`Name(..)` forms);
//   - items: top-level `fn` (non-generic) and top-level `const`;
//   - statements: `var`/`const` lets, (compound) name-assignment, `if`/
//     `else if`/`else`, `while` with continue-clause, unlabeled `break`/
//     `continue`, `defer`, `return`, bare blocks, expression statements;
//   - expressions: integer/bool literals, names, unary `-`/`!`/`~`, the full
//     binary ladder (arithmetic, comparison, `and`/`or`, bitwise, shifts),
//     free-function calls, `print`, `expect`, `comptime` folds.
//
// Everything else is OUT of the subset. `es_detect` walks the AST in a fixed
// depth-first order and reports the FIRST unsupported construct as a
// `(word, position)` pair; the differential driver prints it as
// `SKIP <word> <pos>` and the Rust test mirrors the same walk, so subset
// membership itself is differentially tested on every corpus file.
//
// Like the Rust emitter, this one works off the plain parsed AST — there is
// no sema here. Emission of a program that sema would REJECT is therefore
// unspecified-but-total: it must never crash or loop, but its output is only
// compared for programs the Rust pipeline validates (the differential test
// carries the explicit list of subset-shaped-but-invalid corpus files).
//
// Mirrored decisions (emit_c.rs / const_eval.rs):
//   - the fixed 10-line prelude + one blank line (`emit_prelude`);
//   - `static const` top-level consts, folded by a `const_eval` mirror in
//     source order (a failing fold SKIPS the const, exactly like the Rust
//     "skip rather than panic" arm), then one blank line if any were emitted;
//   - dead-function elimination (SPEC §43.1): a worklist transitive closure
//     of called names rooted at `main`; forward declarations and definitions
//     both consult the same liveness;
//   - declaration/definition formatting: `<cty> kd_<name>(<params>)`, empty
//     parameter lists spelled `void`, 4-space indentation, one blank line
//     after the forward-declaration block and after every definition;
//   - statement/expression lowering: fully parenthesized operators, `print`
//     → `kd_print((long long)(<e>))`, `expect` in value position →
//     `((void)0)`, compound assignment re-spelling the place on both sides;
//   - `defer` lowering (SPEC §4.4): a scope stack; fall-through flushes the
//     current scope in reverse registration order, `return` flushes all
//     scopes (hoisting a non-void value into `<cty> __kd_ret = (<e>);`
//     first), `break`/`continue` flush to the nearest loop-body scope, and
//     the `while` continue-clause runs after those defers and before the C
//     `continue;`;
//   - local type inference (`type_of_expr` mirror): int literal → `i64`,
//     bool → `bool`, name → the scope stack, unary/binary by operator shape,
//     call → the collected return type; an un-inferable initializer falls
//     back to `i64` — including the Rust emitter's own quirks (a top-level
//     const referenced as an initializer infers `i64`, not its own type);
//   - `int main(int argc, char **argv){ (void)argc;(void)argv; <wire> }`
//     where `<wire>` is `return (int) kd_main();` for an integer `main`,
//     else `kd_main(); return 0;`.
//
// Known, accepted divergence: the const-fold mirrors Rust's WRAPPING i64
// arithmetic with plain kardashev `i64` ops (plus explicit guards for the
// `i64::MIN / -1`, `i64::MIN % -1` and `-i64::MIN` traps and the shift-amount
// mask `& 63`). A `comptime` overflow therefore folds identically on every
// production target, but is formally implementation-defined here rather than
// two's-complement-guaranteed as in Rust.

@import("ast.ks");
@import("std");

// --- type codes ----------------------------------------------------------------
//
// The mirror of `types.rs::Type` restricted to the subset. `ET_NONE` mirrors
// a `None` from `Type::from_name` / `type_of_expr` (the "no type" outcome);
// it is distinct from `ET_VOID`, which is a real type.

pub const ET_VOID: i64 = 0;
pub const ET_I32: i64 = 1;
pub const ET_I64: i64 = 2;
pub const ET_BOOL: i64 = 3;
pub const ET_NONE: i64 = 4;

/// `Type::from_name` over the subset: the four spellings map to their codes,
/// anything else is `ET_NONE` (the caller decides the fallback, mirroring the
/// two distinct Rust fallbacks: `resolve_ty` → void, `cty` → `int64_t`).
pub fn et_from_name(name: []u8) i64 {
    if (str_eq(name, "i32")) { return ET_I32; }
    if (str_eq(name, "i64")) { return ET_I64; }
    if (str_eq(name, "bool")) { return ET_BOOL; }
    if (str_eq(name, "void")) { return ET_VOID; }
    return ET_NONE;
}

/// `Type::c_name` over the subset. `ET_NONE` never reaches C spelling through
/// `et_cty_of` in a detector-approved program; spell it `int64_t` (the same
/// defensive fallback the Rust `cty` uses for an unresolvable name).
pub fn et_c_name(t: i64) []u8 {
    if (t == ET_I32) { return "int32_t"; }
    if (t == ET_I64) { return "int64_t"; }
    if (t == ET_BOOL) { return "bool"; }
    if (t == ET_VOID) { return "void"; }
    return "int64_t";
}

/// `Type::is_int` over the subset (only `i32`/`i64` exist here).
pub fn et_is_int(t: i64) bool {
    return t == ET_I32 or t == ET_I64;
}

// --- operator spellings ----------------------------------------------------------
//
// `BinOp::c_op` / `is_bool_result` and the unary spellings, keyed by the
// `OPC_*` / `UOP_*` codes of `ast.ks`.

pub fn es_c_op(op: i64) []u8 {
    if (op == OPC_ADD) { return "+"; }
    if (op == OPC_SUB) { return "-"; }
    if (op == OPC_MUL) { return "*"; }
    if (op == OPC_DIV) { return "/"; }
    if (op == OPC_REM) { return "%"; }
    if (op == OPC_EQ) { return "=="; }
    if (op == OPC_NE) { return "!="; }
    if (op == OPC_LT) { return "<"; }
    if (op == OPC_LE) { return "<="; }
    if (op == OPC_GT) { return ">"; }
    if (op == OPC_GE) { return ">="; }
    if (op == OPC_AND) { return "&&"; }
    if (op == OPC_OR) { return "||"; }
    if (op == OPC_BAND) { return "&"; }
    if (op == OPC_BOR) { return "|"; }
    if (op == OPC_BXOR) { return "^"; }
    if (op == OPC_SHL) { return "<<"; }
    return ">>";
}

pub fn es_is_bool_result(op: i64) bool {
    return (op >= OPC_EQ and op <= OPC_GE) or op == OPC_AND or op == OPC_OR;
}

// --- subset detection ------------------------------------------------------------
//
// A fixed depth-first walk over the arena, recording the FIRST unsupported
// construct as a `(word, pos)` pair. The walk order is part of the contract:
// items in source order; per function, parameters (flag, then type), return
// type, body; per statement/expression, children in their `a`/`b`/`c` field
// order. `crates/kardc/tests/selfhost_emit.rs` mirrors this walk over the
// Rust AST word for word — the differential compares both the verdict and
// the position.

pub const Det = struct {
    src: []u8,
    nodes: []Node,
    found: bool,
    word: []u8,
    pos: usize,

    fn init(src: []u8, nodes: []Node) Self {
        return Det{ .src = src, .nodes = nodes, .found = false, .word = "", .pos = 0 };
    }

    /// Record the first finding; later ones are ignored.
    fn hit(self: *Self, word: []u8, pos: usize) void {
        if (self.found) { return; }
        self.found = true;
        self.word = word;
        self.pos = pos;
    }

    /// A type reference: any composite form is out, then the base name must
    /// be one of the four subset spellings. (`@This()` carries no source
    /// name; it reports `type-name` exactly like the Rust mirror, whose
    /// synthesized name `Self` is not a subset spelling.)
    fn check_type(self: *Self, n: i32) void {
        if (self.found or n < 0) { return; }
        var u: usize = @as(usize, n);
        var fl: i64 = self.nodes[u].flags;
        var forms: i64 = F_OPT | F_ERR | F_PTR | F_SLICE | F_ARRLIT | F_ARRPARAM | F_ERRSET | F_APP | F_ESETTHIS;
        if ((fl & forms) != 0) {
            self.hit("type-form", self.nodes[u].off);
            return;
        }
        if ((fl & F_THIS) != 0) {
            self.hit("type-name", self.nodes[u].off);
            return;
        }
        var name: []u8 = self.src[self.nodes[u].xoff .. self.nodes[u].xoff + self.nodes[u].xlen];
        if (et_from_name(name) == ET_NONE) {
            self.hit("type-name", self.nodes[u].off);
        }
    }

    fn check_expr_list(self: *Self, head: i32) void {
        var cur: i32 = head;
        while (cur >= 0) {
            if (self.found) { return; }
            self.check_expr(cur);
            cur = self.nodes[@as(usize, cur)].next;
        }
    }

    fn check_expr(self: *Self, n: i32) void {
        if (self.found or n < 0) { return; }
        var u: usize = @as(usize, n);
        var k: u8 = self.nodes[u].kind;
        var off: usize = self.nodes[u].off;
        if (k == ND_INT or k == ND_BOOL or k == ND_IDENT) { return; }
        if (k == ND_UNARY) {
            self.check_expr(self.nodes[u].a);
            return;
        }
        if (k == ND_BIN) {
            self.check_expr(self.nodes[u].a);
            self.check_expr(self.nodes[u].b);
            return;
        }
        if (k == ND_CALL) {
            var callee: []u8 = self.src[self.nodes[u].xoff .. self.nodes[u].xoff + self.nodes[u].xlen];
            if (str_eq(callee, "alloc") or str_eq(callee, "free") or str_eq(callee, "c_allocator")) {
                self.hit("builtin-call", off);
                return;
            }
            self.check_expr_list(self.nodes[u].a);
            return;
        }
        if (k == ND_COMPTIME) {
            self.check_expr(self.nodes[u].a);
            return;
        }
        if (k == ND_FLOAT) { self.hit("float", off); return; }
        if (k == ND_STR) { self.hit("string", off); return; }
        if (k == ND_BUILTIN) { self.hit("builtin", off); return; }
        if (k == ND_SLIT) { self.hit("struct-lit", off); return; }
        if (k == ND_STRUCTTYPE) { self.hit("struct-type", off); return; }
        if (k == ND_FIELD) { self.hit("field", off); return; }
        if (k == ND_MCALL) { self.hit("method-call", off); return; }
        if (k == ND_NULL) { self.hit("null", off); return; }
        if (k == ND_ORELSE) { self.hit("orelse", off); return; }
        if (k == ND_UNWRAP) { self.hit("unwrap", off); return; }
        if (k == ND_ERRLIT) { self.hit("error-lit", off); return; }
        if (k == ND_ENUMLIT) { self.hit("enum-lit", off); return; }
        if (k == ND_ALIT) { self.hit("array-lit", off); return; }
        if (k == ND_INDEX) { self.hit("index", off); return; }
        if (k == ND_ADDROF) { self.hit("addrof", off); return; }
        if (k == ND_DEREF) { self.hit("deref", off); return; }
        if (k == ND_SLICEX) { self.hit("slice-expr", off); return; }
        if (k == ND_TRY) { self.hit("try", off); return; }
        if (k == ND_CATCH) { self.hit("catch", off); return; }
        if (k == ND_UNREACHABLE) { self.hit("unreachable", off); return; }
        // Any other kind in expression position is a walker bug; surface it
        // as a mismatch rather than silently accepting.
        self.hit("bad-expr", off);
    }

    fn check_block(self: *Self, n: i32) void {
        if (self.found or n < 0) { return; }
        var cur: i32 = self.nodes[@as(usize, n)].a;
        while (cur >= 0) {
            if (self.found) { return; }
            self.check_stmt(cur);
            cur = self.nodes[@as(usize, cur)].next;
        }
    }

    fn check_stmt(self: *Self, n: i32) void {
        if (self.found or n < 0) { return; }
        var u: usize = @as(usize, n);
        var k: u8 = self.nodes[u].kind;
        var fl: i64 = self.nodes[u].flags;
        var off: usize = self.nodes[u].off;
        if (k == ND_LET) {
            self.check_type(self.nodes[u].a);
            self.check_expr(self.nodes[u].b);
            return;
        }
        if (k == ND_ASSIGN) {
            self.check_expr(self.nodes[u].a);
            return;
        }
        if (k == ND_PASSIGN) { self.hit("place-assign", off); return; }
        if (k == ND_RETURN) {
            self.check_expr(self.nodes[u].a);
            return;
        }
        if (k == ND_IF) {
            if ((fl & F_CAP) != 0) { self.hit("capture", off); return; }
            self.check_expr(self.nodes[u].a);
            self.check_block(self.nodes[u].b);
            self.check_stmt(self.nodes[u].c);
            return;
        }
        if (k == ND_WHILE) {
            if ((fl & F_LABEL) != 0) { self.hit("label", off); return; }
            self.check_expr(self.nodes[u].a);
            self.check_stmt(self.nodes[u].b);
            self.check_block(self.nodes[u].c);
            return;
        }
        if (k == ND_FOR) { self.hit("for", off); return; }
        if (k == ND_BREAK or k == ND_CONTINUE) {
            if ((fl & F_LABEL) != 0) { self.hit("label", off); }
            return;
        }
        if (k == ND_DEFER) {
            self.check_stmt(self.nodes[u].a);
            return;
        }
        if (k == ND_ERRDEFER) { self.hit("errdefer", off); return; }
        if (k == ND_BLOCK) {
            self.check_block(n);
            return;
        }
        if (k == ND_SWITCH) { self.hit("switch", off); return; }
        // An expression statement.
        self.check_expr(n);
    }

    fn check_fn(self: *Self, n: i32) void {
        if (self.found) { return; }
        var u: usize = @as(usize, n);
        var p: i32 = self.nodes[u].a;
        while (p >= 0) {
            if (self.found) { return; }
            var pu: usize = @as(usize, p);
            if ((self.nodes[pu].flags & F_COMPTIME) != 0) {
                self.hit("generic-param", self.nodes[pu].off);
                return;
            }
            self.check_type(self.nodes[pu].a);
            p = self.nodes[pu].next;
        }
        self.check_type(self.nodes[u].b);
        self.check_block(self.nodes[u].c);
    }
};

/// Subset verdict for a parsed module (`root` = the item-chain head). The
/// FIRST check is for a top-level `fn main` — a module without one cannot be
/// a Program-mode subset program (the Rust pipeline rejects it as E0150
/// before emission), reported as `nomain` at position 0.
pub fn es_detect(src: []u8, nodes: []Node, root: i32) Det {
    var d: Det = Det.init(src, nodes);
    var has_main: bool = false;
    var cur: i32 = root;
    while (cur >= 0) {
        var u: usize = @as(usize, cur);
        if (nodes[u].kind == ND_FN) {
            var name: []u8 = src[nodes[u].xoff .. nodes[u].xoff + nodes[u].xlen];
            if (str_eq(name, "main")) { has_main = true; }
        }
        cur = nodes[u].next;
    }
    if (!has_main) {
        d.hit("nomain", 0);
        return d;
    }
    cur = root;
    while (cur >= 0) {
        if (d.found) { return d; }
        var u: usize = @as(usize, cur);
        var k: u8 = nodes[u].kind;
        if (k == ND_FN) {
            d.check_fn(cur);
        } else if (k == ND_CONST) {
            d.check_type(nodes[u].a);
            d.check_expr(nodes[u].b);
        } else if (k == ND_TEST) {
            d.hit("test", nodes[u].off);
        } else if (k == ND_STRUCT) {
            d.hit("struct", nodes[u].off);
        } else if (k == ND_ENUM) {
            d.hit("enum", nodes[u].off);
        } else if (k == ND_UNION) {
            d.hit("union", nodes[u].off);
        } else if (k == ND_IMPORT) {
            d.hit("import", nodes[u].off);
        } else if (k == ND_ERRSET) {
            d.hit("errorset", nodes[u].off);
        } else {
            d.hit("bad-item", nodes[u].off);
        }
        cur = nodes[u].next;
    }
    return d;
}

// --- constant evaluation ----------------------------------------------------------
//
// The `const_eval::eval` mirror over the subset value kinds. A result is
// `(ok, isb, val)`: `ok = false` is any `E013x` outcome (the caller only
// needs the fact of failure — a failing top-level const is skipped, a
// failing `comptime` falls back to expression lowering, both exactly as in
// Rust). Integer arithmetic wraps as `i64` (with explicit guards where C
// would trap, see the header).

pub const EvRes = struct {
    ok: bool,
    isb: bool,
    val: i64,
};

fn ev_err() EvRes {
    return EvRes{ .ok = false, .isb = false, .val = 0 };
}

fn ev_int(v: i64) EvRes {
    return EvRes{ .ok = true, .isb = false, .val = v };
}

fn ev_bool(v: i64) EvRes {
    return EvRes{ .ok = true, .isb = true, .val = v };
}

/// The most negative `i64`, spelled without a negative literal.
fn ev_i64_min() i64 {
    return (0 - 9223372036854775807) - 1;
}

// --- the emitter -------------------------------------------------------------------

/// One lexical scope active during emission (`emit_c.rs::Scope`). The defers
/// and locals of every scope live in the emitter's flat `defers`/`vts`
/// stacks; a scope records where its span begins (`dstart`/`vstart`), so a
/// scope's own entries are `[start, next scope's start)` — pushes only ever
/// target the innermost scope, so the spans stay contiguous.
pub const EmScope = struct {
    is_loop: bool,
    cont: i32,
    dstart: i64,
    vstart: i64,
};

/// One local/param type record: the source name (a span) and its type code.
pub const VtEnt = struct {
    off: usize,
    len: usize,
    ty: i64,
};

/// One top-level function signature: name span, resolved return type code,
/// its arena node, and the §43.1 liveness verdict.
pub const FnSig = struct {
    off: usize,
    len: usize,
    ret: i64,
    node: i32,
    live: bool,
};

/// One folded top-level constant: name span, kind, value.
pub const CEnt = struct {
    off: usize,
    len: usize,
    isb: bool,
    val: i64,
};

/// A pending name in the liveness worklist (a span into the source).
pub const PendName = struct {
    off: usize,
    len: usize,
};

pub const Em = struct {
    src: []u8,
    nodes: []Node,
    root: i32,
    // Output buffer (grown by doubling).
    out: []u8,
    out_len: usize,
    indent: i64,
    // Scope stack + the flat defer/local stacks it indexes into.
    scopes: []EmScope,
    sc_len: usize,
    defers: []i32,
    df_len: usize,
    vts: []VtEnt,
    vt_len: usize,
    // Collected signatures and folded consts.
    fns: []FnSig,
    fn_len: usize,
    consts: []CEnt,
    ct_len: usize,
    // Return type of the function being emitted.
    cur_ret: i64,

    fn init(a: Allocator, src: []u8, nodes: []Node, root: i32) Self {
        return Em{
            .src = src,
            .nodes = nodes,
            .root = root,
            .out = alloc(a, u8, 4096),
            .out_len = 0,
            .indent = 0,
            .scopes = alloc(a, EmScope, 16),
            .sc_len = 0,
            .defers = alloc(a, i32, 16),
            .df_len = 0,
            .vts = alloc(a, VtEnt, 32),
            .vt_len = 0,
            .fns = alloc(a, FnSig, 16),
            .fn_len = 0,
            .consts = alloc(a, CEnt, 16),
            .ct_len = 0,
            .cur_ret = ET_VOID,
        };
    }

    // -- raw output -----------------------------------------------------------

    fn putc(self: *Self, a: Allocator, b: u8) void {
        if (self.out_len == self.out.len) {
            var grown: []u8 = alloc(a, u8, self.out.len * 2);
            var i: usize = 0;
            while (i < self.out_len) : (i += 1) { grown[i] = self.out[i]; }
            free(a, self.out);
            self.out = grown;
        }
        self.out[self.out_len] = b;
        self.out_len += 1;
    }

    fn put(self: *Self, a: Allocator, s: []u8) void {
        var i: usize = 0;
        while (i < s.len) : (i += 1) { self.putc(a, s[i]); }
    }

    /// `Emitter::line`: indentation, the text, a newline.
    fn line(self: *Self, a: Allocator, s: []u8) void {
        var i: i64 = 0;
        while (i < self.indent) : (i += 1) { self.put(a, "    "); }
        self.put(a, s);
        self.putc(a, 10);
    }

    /// `Emitter::blank`: one bare newline.
    fn blank(self: *Self, a: Allocator) void {
        self.putc(a, 10);
    }

    // -- name/text helpers ------------------------------------------------------

    /// The primary name text of node `n` (its `x` span).
    fn xname(self: *Self, n: i32) []u8 {
        var u: usize = @as(usize, n);
        return self.src[self.nodes[u].xoff .. self.nodes[u].xoff + self.nodes[u].xlen];
    }

    // -- stack growth -----------------------------------------------------------

    fn push_scope(self: *Self, a: Allocator, is_loop: bool, cont: i32) void {
        if (self.sc_len == self.scopes.len) {
            var grown: []EmScope = alloc(a, EmScope, self.scopes.len * 2);
            var i: usize = 0;
            while (i < self.sc_len) : (i += 1) { grown[i] = self.scopes[i]; }
            free(a, self.scopes);
            self.scopes = grown;
        }
        self.scopes[self.sc_len] = EmScope{
            .is_loop = is_loop,
            .cont = cont,
            .dstart = @as(i64, self.df_len),
            .vstart = @as(i64, self.vt_len),
        };
        self.sc_len += 1;
    }

    /// Pop the innermost scope, dropping its defers and locals.
    fn pop_scope(self: *Self) void {
        var top: usize = self.sc_len - 1;
        self.df_len = @as(usize, self.scopes[top].dstart);
        self.vt_len = @as(usize, self.scopes[top].vstart);
        self.sc_len -= 1;
    }

    fn push_defer(self: *Self, a: Allocator, n: i32) void {
        if (self.df_len == self.defers.len) {
            var grown: []i32 = alloc(a, i32, self.defers.len * 2);
            var i: usize = 0;
            while (i < self.df_len) : (i += 1) { grown[i] = self.defers[i]; }
            free(a, self.defers);
            self.defers = grown;
        }
        self.defers[self.df_len] = n;
        self.df_len += 1;
    }

    fn push_vt(self: *Self, a: Allocator, off: usize, len: usize, ty: i64) void {
        if (self.vt_len == self.vts.len) {
            var grown: []VtEnt = alloc(a, VtEnt, self.vts.len * 2);
            var i: usize = 0;
            while (i < self.vt_len) : (i += 1) { grown[i] = self.vts[i]; }
            free(a, self.vts);
            self.vts = grown;
        }
        self.vts[self.vt_len] = VtEnt{ .off = off, .len = len, .ty = ty };
        self.vt_len += 1;
    }

    fn push_fn(self: *Self, a: Allocator, off: usize, len: usize, ret: i64, node: i32) void {
        if (self.fn_len == self.fns.len) {
            var grown: []FnSig = alloc(a, FnSig, self.fns.len * 2);
            var i: usize = 0;
            while (i < self.fn_len) : (i += 1) { grown[i] = self.fns[i]; }
            free(a, self.fns);
            self.fns = grown;
        }
        self.fns[self.fn_len] = FnSig{ .off = off, .len = len, .ret = ret, .node = node, .live = false };
        self.fn_len += 1;
    }

    fn push_const(self: *Self, a: Allocator, off: usize, len: usize, isb: bool, val: i64) void {
        if (self.ct_len == self.consts.len) {
            var grown: []CEnt = alloc(a, CEnt, self.consts.len * 2);
            var i: usize = 0;
            while (i < self.ct_len) : (i += 1) { grown[i] = self.consts[i]; }
            free(a, self.consts);
            self.consts = grown;
        }
        self.consts[self.ct_len] = CEnt{ .off = off, .len = len, .isb = isb, .val = val };
        self.ct_len += 1;
    }

    // -- lookups ------------------------------------------------------------------

    /// `Emitter::lookup_var_type`: innermost binding of `name` wins.
    fn vt_lookup(self: *Self, name: []u8) i64 {
        var i: i64 = @as(i64, self.vt_len) - 1;
        while (i >= 0) : (i -= 1) {
            var u: usize = @as(usize, i);
            var ent: []u8 = self.src[self.vts[u].off .. self.vts[u].off + self.vts[u].len];
            if (str_eq(ent, name)) { return self.vts[u].ty; }
        }
        return ET_NONE;
    }

    /// The collected return type of the top-level `fn` named `name`, or
    /// `ET_NONE` (mirrors an `fn_ret` map miss).
    fn fn_ret_of(self: *Self, name: []u8) i64 {
        var i: usize = 0;
        while (i < self.fn_len) : (i += 1) {
            var ent: []u8 = self.src[self.fns[i].off .. self.fns[i].off + self.fns[i].len];
            if (str_eq(ent, name)) { return self.fns[i].ret; }
        }
        return ET_NONE;
    }

    /// The folded constant named `name`: `ok = false` mirrors an unknown /
    /// not-yet-folded const (`E0131`).
    fn const_lookup(self: *Self, name: []u8) EvRes {
        var i: usize = 0;
        while (i < self.ct_len) : (i += 1) {
            var ent: []u8 = self.src[self.consts[i].off .. self.consts[i].off + self.consts[i].len];
            if (str_eq(ent, name)) {
                return EvRes{ .ok = true, .isb = self.consts[i].isb, .val = self.consts[i].val };
            }
        }
        return ev_err();
    }

    // -- type resolution -----------------------------------------------------------

    /// `Emitter::resolve_ty`: the base name through `from_name`, else the
    /// `Void` fallback (struct/enum/... paths are empty in the subset).
    fn resolve_ty(self: *Self, n: i32) i64 {
        var t: i64 = et_from_name(self.xname(n));
        if (t == ET_NONE) { return ET_VOID; }
        return t;
    }

    /// `Emitter::cty`: the base name through `from_name`, else the `int64_t`
    /// fallback.
    fn cty(self: *Self, n: i32) []u8 {
        var t: i64 = et_from_name(self.xname(n));
        if (t == ET_NONE) { return "int64_t"; }
        return et_c_name(t);
    }

    // -- const evaluation -------------------------------------------------------------

    /// `const_eval::eval` over the arena (see the module header for the
    /// wrapping-arithmetic contract).
    fn eval(self: *Self, n: i32) EvRes {
        if (n < 0) { return ev_err(); }
        var u: usize = @as(usize, n);
        var k: u8 = self.nodes[u].kind;
        if (k == ND_INT) { return ev_int(self.nodes[u].val); }
        if (k == ND_BOOL) { return ev_bool(self.nodes[u].val); }
        if (k == ND_IDENT) { return self.const_lookup(self.xname(n)); }
        if (k == ND_COMPTIME) { return self.eval(self.nodes[u].a); }
        if (k == ND_UNARY) {
            var v: EvRes = self.eval(self.nodes[u].a);
            if (!v.ok) { return v; }
            var op: i64 = self.nodes[u].val;
            if (op == UOP_NEG) {
                if (v.isb) { return ev_err(); }
                if (v.val == ev_i64_min()) { return ev_int(v.val); }
                return ev_int(0 - v.val);
            }
            if (op == UOP_NOT) {
                if (!v.isb) { return ev_err(); }
                if (v.val == 0) { return ev_bool(1); }
                return ev_bool(0);
            }
            // UOP_BNOT
            if (v.isb) { return ev_err(); }
            return ev_int(~v.val);
        }
        if (k == ND_BIN) {
            var l: EvRes = self.eval(self.nodes[u].a);
            if (!l.ok) { return l; }
            var r: EvRes = self.eval(self.nodes[u].b);
            if (!r.ok) { return r; }
            return self.eval_binary(self.nodes[u].val, l, r);
        }
        // Calls and every other shape are not compile-time constants.
        return ev_err();
    }

    fn eval_binary(self: *Self, op: i64, l: EvRes, r: EvRes) EvRes {
        if (op == OPC_ADD or op == OPC_SUB or op == OPC_MUL or op == OPC_DIV or op == OPC_REM) {
            if (l.isb or r.isb) { return ev_err(); }
            if (op == OPC_ADD) { return ev_int(l.val + r.val); }
            if (op == OPC_SUB) { return ev_int(l.val - r.val); }
            if (op == OPC_MUL) { return ev_int(l.val * r.val); }
            if (r.val == 0) { return ev_err(); }
            // The lone case where Rust's wrapping division diverges from C.
            if (l.val == ev_i64_min() and r.val == 0 - 1) {
                if (op == OPC_DIV) { return ev_int(l.val); }
                return ev_int(0);
            }
            if (op == OPC_DIV) { return ev_int(l.val / r.val); }
            return ev_int(l.val % r.val);
        }
        if (op == OPC_EQ or op == OPC_NE) {
            if (l.isb != r.isb) { return ev_err(); }
            var eq: bool = l.val == r.val;
            if (op == OPC_NE) { eq = !eq; }
            if (eq) { return ev_bool(1); }
            return ev_bool(0);
        }
        if (op == OPC_LT or op == OPC_LE or op == OPC_GT or op == OPC_GE) {
            // Bools compare as 0/1 integers, mirroring `ConstVal::Bool as i64`.
            if (l.isb != r.isb) { return ev_err(); }
            var v: bool = false;
            if (op == OPC_LT) { v = l.val < r.val; }
            if (op == OPC_LE) { v = l.val <= r.val; }
            if (op == OPC_GT) { v = l.val > r.val; }
            if (op == OPC_GE) { v = l.val >= r.val; }
            if (v) { return ev_bool(1); }
            return ev_bool(0);
        }
        if (op == OPC_AND or op == OPC_OR) {
            if (!l.isb or !r.isb) { return ev_err(); }
            var b: bool = false;
            if (op == OPC_AND) { b = l.val != 0 and r.val != 0; }
            if (op == OPC_OR) { b = l.val != 0 or r.val != 0; }
            if (b) { return ev_bool(1); }
            return ev_bool(0);
        }
        // Bitwise / shifts.
        if (l.isb or r.isb) { return ev_err(); }
        if (op == OPC_BAND) { return ev_int(l.val & r.val); }
        if (op == OPC_BOR) { return ev_int(l.val | r.val); }
        if (op == OPC_BXOR) { return ev_int(l.val ^ r.val); }
        // Shift amounts mask to 0..63, mirroring `wrapping_shl`/`wrapping_shr`.
        if (op == OPC_SHL) { return ev_int(l.val << (r.val & 63)); }
        return ev_int(l.val >> (r.val & 63));
    }

    /// `const_literal`: a folded value as C source.
    fn const_literal(self: *Self, a: Allocator, v: EvRes) []u8 {
        if (v.isb) {
            if (v.val != 0) { return "true"; }
            return "false";
        }
        var sb: StrBuilder = StrBuilder.init(a);
        sb.append_i64(a, v.val);
        var s: []u8 = sb.build(a);
        sb.deinit(a);
        return s;
    }

    // -- type_of_expr ------------------------------------------------------------------

    /// `Emitter::type_of_expr` over the subset: the best-effort static type,
    /// `ET_NONE` for "cannot be determined" — including the mirrored quirk
    /// that a top-level const name is NOT resolvable here (only locals and
    /// params are), so an initializer referencing one infers `i64`.
    fn type_of_expr(self: *Self, n: i32) i64 {
        if (n < 0) { return ET_NONE; }
        var u: usize = @as(usize, n);
        var k: u8 = self.nodes[u].kind;
        if (k == ND_INT) { return ET_I64; }
        if (k == ND_BOOL) { return ET_BOOL; }
        if (k == ND_IDENT) { return self.vt_lookup(self.xname(n)); }
        if (k == ND_UNARY) {
            if (self.nodes[u].val == UOP_NOT) { return ET_BOOL; }
            return self.type_of_expr(self.nodes[u].a);
        }
        if (k == ND_BIN) {
            if (es_is_bool_result(self.nodes[u].val)) { return ET_BOOL; }
            return self.type_of_expr(self.nodes[u].a);
        }
        if (k == ND_CALL) {
            return self.fn_ret_of(self.xname(n));
        }
        if (k == ND_COMPTIME) { return self.type_of_expr(self.nodes[u].a); }
        return ET_NONE;
    }

    // -- expressions --------------------------------------------------------------------

    /// `Emitter::emit_expr`: lower an expression to a C expression string.
    /// Scalar coercion is the identity, so `emit_coerced` collapses to this.
    fn emit_expr(self: *Self, a: Allocator, n: i32) []u8 {
        var u: usize = @as(usize, n);
        var k: u8 = self.nodes[u].kind;
        if (k == ND_INT) {
            var sb: StrBuilder = StrBuilder.init(a);
            sb.append_i64(a, self.nodes[u].val);
            var s: []u8 = sb.build(a);
            sb.deinit(a);
            return s;
        }
        if (k == ND_BOOL) {
            if (self.nodes[u].val != 0) { return "true"; }
            return "false";
        }
        if (k == ND_IDENT) {
            var sb: StrBuilder = StrBuilder.init(a);
            sb.append(a, "kd_");
            sb.append(a, self.xname(n));
            var s: []u8 = sb.build(a);
            sb.deinit(a);
            return s;
        }
        if (k == ND_UNARY) {
            var inner: []u8 = self.emit_expr(a, self.nodes[u].a);
            var sb: StrBuilder = StrBuilder.init(a);
            sb.append(a, "(");
            var op: i64 = self.nodes[u].val;
            if (op == UOP_NEG) { sb.append(a, "-"); }
            if (op == UOP_NOT) { sb.append(a, "!"); }
            if (op == UOP_BNOT) { sb.append(a, "~"); }
            sb.append(a, inner);
            sb.append(a, ")");
            var s: []u8 = sb.build(a);
            sb.deinit(a);
            return s;
        }
        if (k == ND_BIN) {
            var l: []u8 = self.emit_expr(a, self.nodes[u].a);
            var r: []u8 = self.emit_expr(a, self.nodes[u].b);
            var sb: StrBuilder = StrBuilder.init(a);
            sb.append(a, "(");
            sb.append(a, l);
            sb.append(a, " ");
            sb.append(a, es_c_op(self.nodes[u].val));
            sb.append(a, " ");
            sb.append(a, r);
            sb.append(a, ")");
            var s: []u8 = sb.build(a);
            sb.deinit(a);
            return s;
        }
        if (k == ND_CALL) {
            var callee: []u8 = self.xname(n);
            if (str_eq(callee, "print")) {
                // `print(<int>)` → `kd_print((long long)(<e>))` (the slice /
                // f64 routes cannot appear in the subset).
                var arg: i32 = self.nodes[u].a;
                var astr: []u8 = "0";
                if (arg >= 0) { astr = self.emit_expr(a, arg); }
                var sb: StrBuilder = StrBuilder.init(a);
                sb.append(a, "kd_print((long long)(");
                sb.append(a, astr);
                sb.append(a, "))");
                var s: []u8 = sb.build(a);
                sb.deinit(a);
                return s;
            }
            if (str_eq(callee, "expect")) {
                // Value-position `expect` is a no-op placeholder (Program
                // mode; sema rejects it, output must stay well-formed).
                return "((void)0)";
            }
            if (str_eq(callee, "c_allocator")) {
                // Unreachable behind the detector; mirrored for totality.
                return "((kd_allocator){0})";
            }
            var sb: StrBuilder = StrBuilder.init(a);
            sb.append(a, "kd_");
            sb.append(a, callee);
            sb.append(a, "(");
            var cur: i32 = self.nodes[u].a;
            var first: bool = true;
            while (cur >= 0) {
                if (!first) { sb.append(a, ", "); }
                first = false;
                var e: []u8 = self.emit_expr(a, cur);
                sb.append(a, e);
                cur = self.nodes[@as(usize, cur)].next;
            }
            sb.append(a, ")");
            var s: []u8 = sb.build(a);
            sb.deinit(a);
            return s;
        }
        if (k == ND_COMPTIME) {
            // Fold to a literal when possible; else fall back to the inner
            // expression (the C compiler folds it itself).
            var v: EvRes = self.eval(self.nodes[u].a);
            if (v.ok) { return self.const_literal(a, v); }
            return self.emit_expr(a, self.nodes[u].a);
        }
        // Unreachable behind the detector: keep the output well-formed.
        return "0";
    }

    // -- defer flushing --------------------------------------------------------------

    /// Whether any scope holds a deferred statement (`any_defer_active`;
    /// the subset has no `errdefer`, so there is no error-edge variant).
    fn any_defer_active(self: *Self) bool {
        return self.df_len > 0;
    }

    /// The end of scope `idx`'s defer span: the next scope's start, or the
    /// stack top for the innermost scope.
    fn defer_end(self: *Self, idx: usize) i64 {
        if (idx + 1 < self.sc_len) { return self.scopes[idx + 1].dstart; }
        return @as(i64, self.df_len);
    }

    /// `flush_scope`: one scope's defers in reverse registration order. The
    /// span is snapshotted first (Rust clones the list), so a defer body
    /// that itself registers defers cannot extend the flush.
    fn flush_scope(self: *Self, a: Allocator, idx: usize) void {
        var lo: i64 = self.scopes[idx].dstart;
        var hi: i64 = self.defer_end(idx);
        var i: i64 = hi - 1;
        while (i >= lo) : (i -= 1) {
            var st: i32 = self.defers[@as(usize, i)];
            var d: bool = self.emit_stmt(a, st);
            // The divergence verdict of a flushed defer body is discarded,
            // exactly as in Rust (`emit_stmt(s);` in `flush_scope`).
            if (d) { }
        }
    }

    fn flush_current(self: *Self, a: Allocator) void {
        if (self.sc_len > 0) { self.flush_scope(a, self.sc_len - 1); }
    }

    fn flush_all(self: *Self, a: Allocator) void {
        var i: i64 = @as(i64, self.sc_len) - 1;
        while (i >= 0) : (i -= 1) {
            self.flush_scope(a, @as(usize, i));
        }
    }

    /// Flush innermost-first down to and including the nearest loop-body
    /// scope; returns its index, or -1 when there is no enclosing loop (a
    /// sema-invalid `break`/`continue` — nothing is flushed, mirroring the
    /// early `None` return).
    fn flush_to_loop(self: *Self, a: Allocator) i64 {
        var loop_idx: i64 = 0 - 1;
        var i: i64 = @as(i64, self.sc_len) - 1;
        while (i >= 0) : (i -= 1) {
            if (self.scopes[@as(usize, i)].is_loop) {
                loop_idx = i;
                break;
            }
        }
        if (loop_idx < 0) { return loop_idx; }
        i = @as(i64, self.sc_len) - 1;
        while (i >= loop_idx) : (i -= 1) {
            self.flush_scope(a, @as(usize, i));
        }
        return loop_idx;
    }

    // -- statements ---------------------------------------------------------------------

    /// `emit_cont`: a `while` continue-clause (an assignment or expression).
    fn emit_cont(self: *Self, a: Allocator, n: i32) void {
        var u: usize = @as(usize, n);
        var k: u8 = self.nodes[u].kind;
        if (k == ND_ASSIGN) {
            self.emit_assign(a, n);
            return;
        }
        // The parser only produces an assignment or an expression here.
        var es: []u8 = self.emit_expr(a, n);
        var sb: StrBuilder = StrBuilder.init(a);
        sb.append(a, es);
        sb.append(a, ";");
        var s: []u8 = sb.build(a);
        sb.deinit(a);
        self.line(a, s);
    }

    /// `emit_loop_cont`: the continue-clause of the loop-body scope at
    /// `idx`, if any (the subset has no `for`, so no raw clause).
    fn emit_loop_cont(self: *Self, a: Allocator, idx: usize) void {
        var c: i32 = self.scopes[idx].cont;
        if (c >= 0) { self.emit_cont(a, c); }
    }

    /// The (compound) name-assignment lowering, shared by `Stmt::Assign` and
    /// the continue-clause: `kd_x = (<e>);` / `kd_x = kd_x <op> (<e>);`.
    fn emit_assign(self: *Self, a: Allocator, n: i32) void {
        var u: usize = @as(usize, n);
        var name: []u8 = self.xname(n);
        var es: []u8 = self.emit_expr(a, self.nodes[u].a);
        var op: i64 = self.nodes[u].val;
        var sb: StrBuilder = StrBuilder.init(a);
        sb.append(a, "kd_");
        sb.append(a, name);
        sb.append(a, " = ");
        if (op >= 0) {
            sb.append(a, "kd_");
            sb.append(a, name);
            sb.append(a, " ");
            sb.append(a, es_c_op(op));
            sb.append(a, " (");
            sb.append(a, es);
            sb.append(a, ");");
        } else {
            sb.append(a, es);
            sb.append(a, ";");
        }
        var s: []u8 = sb.build(a);
        sb.deinit(a);
        self.line(a, s);
    }

    /// `finish_return`: the deferred-temp dance. `has_val` distinguishes
    /// `return;` from `return <e>;` (`es` is meaningful only when set).
    fn finish_return(self: *Self, a: Allocator, has_val: bool, es: []u8) void {
        var non_void: bool = self.cur_ret != ET_VOID;
        var active: bool = self.any_defer_active();
        if (active and non_void) {
            // Evaluate into a temporary before the defers run; a missing
            // value falls back to `0` (the `unwrap_or` arm — sema-invalid
            // input only).
            var v: []u8 = "0";
            if (has_val) { v = es; }
            var sb: StrBuilder = StrBuilder.init(a);
            sb.append(a, et_c_name(self.cur_ret));
            sb.append(a, " __kd_ret = (");
            sb.append(a, v);
            sb.append(a, ");");
            var s: []u8 = sb.build(a);
            sb.deinit(a);
            self.line(a, s);
            self.flush_all(a);
            self.line(a, "return __kd_ret;");
            return;
        }
        if (active) { self.flush_all(a); }
        if (has_val) {
            var sb2: StrBuilder = StrBuilder.init(a);
            sb2.append(a, "return (");
            sb2.append(a, es);
            sb2.append(a, ");");
            var s2: []u8 = sb2.build(a);
            sb2.deinit(a);
            self.line(a, s2);
            return;
        }
        self.line(a, "return;");
    }

    /// `emit_if`: flatten the `else if` chain into one C ladder. Returns
    /// whether every arm AND a final `else` diverge.
    fn emit_if(self: *Self, a: Allocator, n: i32) bool {
        var u: usize = @as(usize, n);
        var cs: []u8 = self.emit_expr(a, self.nodes[u].a);
        var sb: StrBuilder = StrBuilder.init(a);
        sb.append(a, "if (");
        sb.append(a, cs);
        sb.append(a, ") {");
        var s: []u8 = sb.build(a);
        sb.deinit(a);
        self.line(a, s);
        var all: bool = true;
        var d: bool = self.emit_block(a, self.nodes[u].b, false, 0 - 1);
        if (!d) { all = false; }
        var els: i32 = self.nodes[u].c;
        while (els >= 0) {
            var eu: usize = @as(usize, els);
            var ek: u8 = self.nodes[eu].kind;
            if (ek == ND_IF and (self.nodes[eu].flags & F_CAP) == 0) {
                var cs2: []u8 = self.emit_expr(a, self.nodes[eu].a);
                var sb2: StrBuilder = StrBuilder.init(a);
                sb2.append(a, "} else if (");
                sb2.append(a, cs2);
                sb2.append(a, ") {");
                var s2: []u8 = sb2.build(a);
                sb2.deinit(a);
                self.line(a, s2);
                var d2: bool = self.emit_block(a, self.nodes[eu].b, false, 0 - 1);
                if (!d2) { all = false; }
                els = self.nodes[eu].c;
            } else if (ek == ND_BLOCK) {
                self.line(a, "} else {");
                var d3: bool = self.emit_block(a, els, false, 0 - 1);
                self.line(a, "}");
                return all and d3;
            } else {
                // A single-statement `else` (unreachable in the subset
                // grammar; mirrored for totality).
                self.line(a, "} else {");
                self.indent += 1;
                var d4: bool = self.emit_stmt(a, els);
                self.indent -= 1;
                self.line(a, "}");
                return all and d4;
            }
        }
        self.line(a, "}");
        // No `else`: control can skip every arm.
        return false;
    }

    /// `Emitter::emit_stmt`. Returns true if the statement unconditionally
    /// transfers control.
    fn emit_stmt(self: *Self, a: Allocator, n: i32) bool {
        var u: usize = @as(usize, n);
        var k: u8 = self.nodes[u].kind;
        if (k == ND_LET) {
            // The binding's type: annotation, else inferred (i64 fallback).
            var ann: i32 = self.nodes[u].a;
            var lty: i64 = ET_NONE;
            var ct: []u8 = "";
            if (ann >= 0) {
                lty = self.resolve_ty(ann);
                ct = self.cty(ann);
            } else {
                lty = self.type_of_expr(self.nodes[u].b);
                if (lty == ET_NONE) { lty = ET_I64; }
                ct = et_c_name(lty);
            }
            var es: []u8 = self.emit_expr(a, self.nodes[u].b);
            var sb: StrBuilder = StrBuilder.init(a);
            if ((self.nodes[u].flags & F_CONST) != 0) { sb.append(a, "const "); }
            sb.append(a, ct);
            sb.append(a, " kd_");
            sb.append(a, self.xname(n));
            sb.append(a, " = ");
            sb.append(a, es);
            sb.append(a, ";");
            var s: []u8 = sb.build(a);
            sb.deinit(a);
            self.line(a, s);
            self.push_vt(a, self.nodes[u].xoff, self.nodes[u].xlen, lty);
            return false;
        }
        if (k == ND_ASSIGN) {
            self.emit_assign(a, n);
            return false;
        }
        if (k == ND_RETURN) {
            var v: i32 = self.nodes[u].a;
            if (v >= 0) {
                var es: []u8 = self.emit_expr(a, v);
                self.finish_return(a, true, es);
            } else {
                self.finish_return(a, false, "");
            }
            return true;
        }
        if (k == ND_IF) {
            return self.emit_if(a, n);
        }
        if (k == ND_WHILE) {
            var cs: []u8 = self.emit_expr(a, self.nodes[u].a);
            var sb: StrBuilder = StrBuilder.init(a);
            sb.append(a, "while (");
            sb.append(a, cs);
            sb.append(a, ") {");
            var s: []u8 = sb.build(a);
            sb.deinit(a);
            self.line(a, s);
            var d: bool = self.emit_block(a, self.nodes[u].c, true, self.nodes[u].b);
            if (d) { }
            self.line(a, "}");
            // A `while` may iterate zero times, so it never diverges.
            return false;
        }
        if (k == ND_BREAK) {
            var i: i64 = self.flush_to_loop(a);
            if (i >= 0) { }
            self.line(a, "break;");
            return true;
        }
        if (k == ND_CONTINUE) {
            var j: i64 = self.flush_to_loop(a);
            if (j >= 0) { self.emit_loop_cont(a, @as(usize, j)); }
            self.line(a, "continue;");
            return true;
        }
        if (k == ND_DEFER) {
            // Register only; the body re-lowers at every exit edge.
            self.push_defer(a, self.nodes[u].a);
            return false;
        }
        if (k == ND_BLOCK) {
            // A bare block is its own C scope.
            self.line(a, "{");
            var d: bool = self.emit_block(a, n, false, 0 - 1);
            self.line(a, "}");
            return d;
        }
        // An expression statement: `<e>;`.
        var es2: []u8 = self.emit_expr(a, n);
        var sb2: StrBuilder = StrBuilder.init(a);
        sb2.append(a, es2);
        sb2.append(a, ";");
        var s2: []u8 = sb2.build(a);
        sb2.deinit(a);
        self.line(a, s2);
        return false;
    }

    /// `Emitter::emit_block`: statements inside a fresh scope; fall-through
    /// flushes that scope's defers, a loop body then runs its
    /// continue-clause. The braces belong to the caller.
    fn emit_block(self: *Self, a: Allocator, block: i32, is_loop: bool, cont: i32) bool {
        self.indent += 1;
        self.push_scope(a, is_loop, cont);
        var diverged: bool = false;
        var cur: i32 = self.nodes[@as(usize, block)].a;
        while (cur >= 0) {
            diverged = self.emit_stmt(a, cur);
            if (diverged) { break; }
            cur = self.nodes[@as(usize, cur)].next;
        }
        if (!diverged) {
            self.flush_current(a);
        }
        var top: usize = self.sc_len - 1;
        if (self.scopes[top].is_loop and !diverged) {
            self.emit_loop_cont(a, top);
        }
        self.pop_scope();
        self.indent -= 1;
        return diverged;
    }

    // -- functions ----------------------------------------------------------------------

    /// `format_params` into `sb`: `void` for an empty list, else
    /// `<cty> kd_<name>` joined by `, `.
    fn put_params(self: *Self, a: Allocator, sb: *StrBuilder, fnode: i32) void {
        var p: i32 = self.nodes[@as(usize, fnode)].a;
        if (p < 0) {
            sb.append(a, "void");
            return;
        }
        var first: bool = true;
        while (p >= 0) {
            var pu: usize = @as(usize, p);
            if (!first) { sb.append(a, ", "); }
            first = false;
            sb.append(a, self.cty(self.nodes[pu].a));
            sb.append(a, " kd_");
            sb.append(a, self.src[self.nodes[pu].xoff .. self.nodes[pu].xoff + self.nodes[pu].xlen]);
            p = self.nodes[pu].next;
        }
    }

    /// `emit_func` (+ `emit_func_named`): reset per-function state, open the
    /// signature line, seed the function scope with the parameter types,
    /// emit the body, close.
    fn emit_func(self: *Self, a: Allocator, fnode: i32) void {
        var u: usize = @as(usize, fnode);
        // Reset the scope machinery (per-function, like `scopes.clear()`).
        self.sc_len = 0;
        self.df_len = 0;
        self.vt_len = 0;
        self.cur_ret = self.resolve_ty(self.nodes[u].b);
        var sb: StrBuilder = StrBuilder.init(a);
        sb.append(a, self.cty(self.nodes[u].b));
        sb.append(a, " kd_");
        sb.append(a, self.xname(fnode));
        sb.append(a, "(");
        self.put_params(a, &sb, fnode);
        sb.append(a, ") {");
        var s: []u8 = sb.build(a);
        sb.deinit(a);
        self.line(a, s);
        // The function scope, seeded with the parameters.
        self.push_scope(a, false, 0 - 1);
        var p: i32 = self.nodes[u].a;
        while (p >= 0) {
            var pu: usize = @as(usize, p);
            self.push_vt(a, self.nodes[pu].xoff, self.nodes[pu].xlen, self.resolve_ty(self.nodes[pu].a));
            p = self.nodes[pu].next;
        }
        // The body statements run inside the function scope itself — mirror
        // `emit_block(&f.body, scope)` by inlining its fall-through flush.
        self.indent += 1;
        var diverged: bool = false;
        var cur: i32 = self.nodes[@as(usize, self.nodes[u].c)].a;
        while (cur >= 0) {
            diverged = self.emit_stmt(a, cur);
            if (diverged) { break; }
            cur = self.nodes[@as(usize, cur)].next;
        }
        if (!diverged) {
            self.flush_current(a);
        }
        self.pop_scope();
        self.indent -= 1;
        self.line(a, "}");
    }

    // -- liveness (SPEC §43.1) -------------------------------------------------------

    /// Collect every free-call name in a statement subtree into the pending
    /// worklist (the `collect_called_names` visitor: `Call{callee}` only —
    /// the subset has no method calls).
    fn collect_calls_expr(self: *Self, a: Allocator, pend: *PendList, n: i32) void {
        if (n < 0) { return; }
        var u: usize = @as(usize, n);
        var k: u8 = self.nodes[u].kind;
        if (k == ND_CALL) {
            pend.push(a, self.nodes[u].xoff, self.nodes[u].xlen);
            var cur: i32 = self.nodes[u].a;
            while (cur >= 0) {
                self.collect_calls_expr(a, pend, cur);
                cur = self.nodes[@as(usize, cur)].next;
            }
            return;
        }
        if (k == ND_UNARY or k == ND_COMPTIME) {
            self.collect_calls_expr(a, pend, self.nodes[u].a);
            return;
        }
        if (k == ND_BIN) {
            self.collect_calls_expr(a, pend, self.nodes[u].a);
            self.collect_calls_expr(a, pend, self.nodes[u].b);
            return;
        }
    }

    fn collect_calls_block(self: *Self, a: Allocator, pend: *PendList, block: i32) void {
        if (block < 0) { return; }
        var cur: i32 = self.nodes[@as(usize, block)].a;
        while (cur >= 0) {
            self.collect_calls_stmt(a, pend, cur);
            cur = self.nodes[@as(usize, cur)].next;
        }
    }

    fn collect_calls_stmt(self: *Self, a: Allocator, pend: *PendList, n: i32) void {
        if (n < 0) { return; }
        var u: usize = @as(usize, n);
        var k: u8 = self.nodes[u].kind;
        if (k == ND_LET or k == ND_ASSIGN) {
            var v: i32 = self.nodes[u].b;
            if (k == ND_ASSIGN) { v = self.nodes[u].a; }
            self.collect_calls_expr(a, pend, v);
            return;
        }
        if (k == ND_RETURN) {
            self.collect_calls_expr(a, pend, self.nodes[u].a);
            return;
        }
        if (k == ND_IF) {
            self.collect_calls_expr(a, pend, self.nodes[u].a);
            self.collect_calls_block(a, pend, self.nodes[u].b);
            self.collect_calls_stmt(a, pend, self.nodes[u].c);
            return;
        }
        if (k == ND_WHILE) {
            self.collect_calls_expr(a, pend, self.nodes[u].a);
            self.collect_calls_stmt(a, pend, self.nodes[u].b);
            self.collect_calls_block(a, pend, self.nodes[u].c);
            return;
        }
        if (k == ND_DEFER) {
            self.collect_calls_stmt(a, pend, self.nodes[u].a);
            return;
        }
        if (k == ND_BLOCK) {
            self.collect_calls_block(a, pend, n);
            return;
        }
        if (k == ND_BREAK or k == ND_CONTINUE) { return; }
        // An expression statement.
        self.collect_calls_expr(a, pend, n);
    }

    /// `live_functions` for the subset: the worklist closure of called names
    /// rooted at `main`. A name goes live once; going live marks EVERY
    /// top-level `fn` of that name and walks each of their bodies. The
    /// synthetic root name `main` is encoded as the (0, 0) span, which
    /// `pend_text` decodes back to the text `main`.
    fn compute_live(self: *Self, a: Allocator) void {
        var pend: PendList = PendList.init(a);
        var done: PendList = PendList.init(a);
        pend.push(a, 0, 0);
        while (pend.len > 0) {
            pend.len -= 1;
            var noff: usize = pend.offs[pend.len];
            var nlen: usize = pend.lens[pend.len];
            var name: []u8 = self.pend_text(noff, nlen);
            if (done.contains(self.src, name)) { continue; }
            done.push(a, noff, nlen);
            // Mark and walk every function of this name.
            var i: usize = 0;
            while (i < self.fn_len) : (i += 1) {
                var fname: []u8 = self.src[self.fns[i].off .. self.fns[i].off + self.fns[i].len];
                if (str_eq(fname, name)) {
                    self.fns[i].live = true;
                    var fu: usize = @as(usize, self.fns[i].node);
                    self.collect_calls_block(a, &pend, self.nodes[fu].c);
                }
            }
        }
        pend.deinit(a);
        done.deinit(a);
    }

    /// The text of a pending name: a span into `src` — except the synthetic
    /// root `main`, marked by the (0, 0) span (no source bytes spell it: the
    /// module may call `main` nowhere).
    fn pend_text(self: *Self, off: usize, len: usize) []u8 {
        if (len == 0) { return "main"; }
        return self.src[off .. off + len];
    }

    // -- top-level passes -----------------------------------------------------------------

    /// `collect_signatures` for the subset: name span + resolved return type
    /// of every top-level `fn`.
    fn collect_signatures(self: *Self, a: Allocator) void {
        var cur: i32 = self.root;
        while (cur >= 0) {
            var u: usize = @as(usize, cur);
            if (self.nodes[u].kind == ND_FN) {
                self.push_fn(a, self.nodes[u].xoff, self.nodes[u].xlen, self.resolve_ty(self.nodes[u].b), cur);
            }
            cur = self.nodes[u].next;
        }
    }

    fn emit_prelude(self: *Self, a: Allocator) void {
        self.put(a, "#include <stdint.h>\n");
        self.put(a, "#include <stdbool.h>\n");
        self.put(a, "#include <stdio.h>\n");
        self.put(a, "#include <stdlib.h>\n");
        self.put(a, "#include <string.h>\n");
        self.put(a, "#include <time.h>\n");
        self.put(a, "typedef struct { int _unused; } kd_allocator;\n");
        self.put(a, "static void kd_print(long long v) { printf(\"%lld\\n\", v); }\n");
        self.put(a, "static void kd_print_f64(double x) { printf(\"%g\\n\", x); }\n");
        self.put(a, "_Noreturn void kd_unreachable(void) { fputs(\"reached unreachable code\\n\", stderr); exit(101); }\n");
        self.blank(a);
    }

    /// `emit_consts`: fold each top-level const in source order; a failing
    /// fold skips the const (never a crash); a trailing blank if any.
    fn emit_consts(self: *Self, a: Allocator) void {
        var any: bool = false;
        var cur: i32 = self.root;
        while (cur >= 0) {
            var u: usize = @as(usize, cur);
            if (self.nodes[u].kind == ND_CONST) {
                var v: EvRes = self.eval(self.nodes[u].b);
                if (v.ok) {
                    var ct: []u8 = "";
                    if (self.nodes[u].a >= 0) {
                        ct = self.cty(self.nodes[u].a);
                    } else if (v.isb) {
                        ct = "bool";
                    } else {
                        ct = "int64_t";
                    }
                    var sb: StrBuilder = StrBuilder.init(a);
                    sb.append(a, "static const ");
                    sb.append(a, ct);
                    sb.append(a, " kd_");
                    sb.append(a, self.xname(cur));
                    sb.append(a, " = ");
                    sb.append(a, self.const_literal(a, v));
                    sb.append(a, ";");
                    var s: []u8 = sb.build(a);
                    sb.deinit(a);
                    self.line(a, s);
                    self.push_const(a, self.nodes[u].xoff, self.nodes[u].xlen, v.isb, v.val);
                    any = true;
                }
            }
            cur = self.nodes[u].next;
        }
        if (any) { self.blank(a); }
    }

    /// `emit_forward_decls`: one line per live function, then a blank.
    fn emit_forward_decls(self: *Self, a: Allocator) void {
        var any: bool = false;
        var i: usize = 0;
        while (i < self.fn_len) : (i += 1) {
            if (!self.fns[i].live) { continue; }
            var fnode: i32 = self.fns[i].node;
            var u: usize = @as(usize, fnode);
            var sb: StrBuilder = StrBuilder.init(a);
            sb.append(a, self.cty(self.nodes[u].b));
            sb.append(a, " kd_");
            sb.append(a, self.xname(fnode));
            sb.append(a, "(");
            self.put_params(a, &sb, fnode);
            sb.append(a, ");");
            var s: []u8 = sb.build(a);
            sb.deinit(a);
            self.line(a, s);
            any = true;
        }
        if (any) { self.blank(a); }
    }

    /// `emit_func_defs`: every live function, each followed by a blank.
    fn emit_func_defs(self: *Self, a: Allocator) void {
        var i: usize = 0;
        while (i < self.fn_len) : (i += 1) {
            if (!self.fns[i].live) { continue; }
            self.emit_func(a, self.fns[i].node);
            self.blank(a);
        }
    }

    /// `emit_program_main`: the C entry point wiring `kd_main`.
    fn emit_program_main(self: *Self, a: Allocator) void {
        var is_int: bool = false;
        var cur: i32 = self.root;
        while (cur >= 0) {
            var u: usize = @as(usize, cur);
            if (self.nodes[u].kind == ND_FN and str_eq(self.xname(cur), "main")) {
                var rt: i64 = et_from_name(self.xname(self.nodes[u].b));
                is_int = et_is_int(rt);
                break;
            }
            cur = self.nodes[u].next;
        }
        if (is_int) {
            self.put(a, "int main(int argc, char **argv){ (void)argc;(void)argv; return (int) kd_main(); }\n");
        } else {
            self.put(a, "int main(int argc, char **argv){ (void)argc;(void)argv; kd_main(); return 0; }\n");
        }
    }

    /// The whole `emit_c::emit` pass sequence for `EmitMode::Program`. The
    /// result is `self.out[0 .. self.out_len]`.
    fn run(self: *Self, a: Allocator) void {
        self.collect_signatures(a);
        self.compute_live(a);
        self.emit_prelude(a);
        // `emit_type_defs` emits nothing: the subset interns no composite
        // types (no structs, optionals, error unions, enums, arrays, slices).
        self.emit_consts(a);
        self.emit_forward_decls(a);
        self.emit_func_defs(a);
        self.emit_program_main(a);
    }
};

/// The liveness worklist / done-set: parallel span arrays. The synthetic
/// root name `main` is encoded as the (0, 0) span (see `Em.pend_text`).
pub const PendList = struct {
    offs: []usize,
    lens: []usize,
    len: usize,

    fn init(a: Allocator) Self {
        return PendList{ .offs = alloc(a, usize, 16), .lens = alloc(a, usize, 16), .len = 0 };
    }

    fn push(self: *Self, a: Allocator, off: usize, len: usize) void {
        if (self.len == self.offs.len) {
            var goffs: []usize = alloc(a, usize, self.offs.len * 2);
            var glens: []usize = alloc(a, usize, self.lens.len * 2);
            var i: usize = 0;
            while (i < self.len) : (i += 1) {
                goffs[i] = self.offs[i];
                glens[i] = self.lens[i];
            }
            free(a, self.offs);
            free(a, self.lens);
            self.offs = goffs;
            self.lens = glens;
        }
        self.offs[self.len] = off;
        self.lens[self.len] = len;
        self.len += 1;
    }

    /// Whether `name` is already recorded (`src` decodes the stored spans;
    /// the (0,0) span decodes to `main`).
    fn contains(self: *Self, src: []u8, name: []u8) bool {
        var i: usize = 0;
        while (i < self.len) : (i += 1) {
            var ent: []u8 = "main";
            if (self.lens[i] != 0) {
                ent = src[self.offs[i] .. self.offs[i] + self.lens[i]];
            }
            if (str_eq(ent, name)) { return true; }
        }
        return false;
    }

    fn deinit(self: *Self, a: Allocator) void {
        free(a, self.offs);
        free(a, self.lens);
    }
};

/// Convenience entry point: emit `EmitMode::Program` C for a parsed subset
/// module. The caller must have run `es_detect` first (a non-subset module
/// yields unspecified — but total — output).
pub fn es_emit_program(a: Allocator, src: []u8, nodes: []Node, root: i32) []u8 {
    var em: Em = Em.init(a, src, nodes, root);
    em.run(a);
    return em.out[0 .. em.out_len];
}
