// sema.ks — self-host stage 27 (v0.186): the sema mirror OPENS.
//
// The Rust `sema::check` is a ~13k-line pass stack; mirroring it lands in
// stages exactly like the emitter did (stages 3–26). Stage 27 covers the
// SCALAR CORE over SINGLE-FILE modules — the v0.111 procedural language
// (§2/§3) at every integer width:
//
//   items       `fn` (no comptime params), top-level `const`, `test`
//   types       bare `i8 i16 i32 i64 u8 u16 u32 u64 usize bool void`
//   statements  let (annotated + inferred), (compound) name-assignment,
//               if/else, while (+ continue-clause), unlabeled
//               break/continue, defer, bare blocks, return, expression
//               statements
//   exprs       int/bool literals, names, unary `- ! ~`, the full binary
//               ladder, free calls, `print`, `expect`, `comptime`
//
// Everything else — imports/multi-file (the flatten mirror is modres.ks's
// v0.167 territory; sema over a FLATTENED module is a later stage), f64,
// strings, aggregates, optionals/error unions, generics, switch/for,
// labels, captures, the allocator builtins — is OUT of the stage-27
// subset, detected by `ss_detect`'s fixed depth-first walk (mirrored
// word-for-word by the Rust twin in `selfhost_sema.rs`).
//
// For an in-subset module the checker `ss_verdict` replays the Rust pass
// order and reports the FIRST diagnostic — code and byte position — or OK:
//
//   Pass 1  function items in source order: redefining a builtin (`print`
//           `expect` `c_allocator` `alloc` `free`) is E0101 at the fn
//           span; signatures (param/return types) register for calls.
//   Pass 2  top-level consts in source order: an initializer that CALLS a
//           registered fn is E0311 at the call ("not a type-constructor");
//           otherwise the const-eval mirror folds it (E0130 non-constant /
//           E0131 unknown const / E0132 type error, at the offending
//           node), then an annotated type is checked against the folded
//           value's kind (E0110 at the initializer).
//   Pass 3  fn + test bodies in source order, with the scope stack, the
//           §3 type rules (E0110 at the Rust span choices: operand errors
//           at the operand, same-type mismatches at the operator node,
//           assignment immutability at the statement, initializer/return
//           mismatches at the value), unknown names E0100, break/continue
//           outside a loop E0120, `expect` outside a test E0140, and
//           integer-literal polymorphism (a flexible literal adopts the
//           expected type / the sibling operand's type / i64) — including
//           `check_int_operands`'s exact anchoring ORDER (a flexible lhs
//           anchors on a concrete rhs, which is then checked FIRST).
//
// Because the mirror reports only the FIRST diagnostic, it SHORT-CIRCUITS:
// every check guards on the failure flag, so the Rust recovery paths
// (which keep checking to accumulate more diagnostics) never need
// replaying — nothing they do can change the first diagnostic.
//
// The differential contract lives in `crates/kardc/tests/selfhost_sema.rs`:
// the Rust reference classifies every corpus file with the REAL
// `sema::check` (not a hand mirror), so `DIAG <code> <pos>` lines pin the
// production sema byte-for-byte.

@import("ast.ks");
@import("std");

// --- the scalar type codes ----------------------------------------------------

pub const SY_NONE: i64 = 0 - 1;
pub const SY_I32: i64 = 0;
pub const SY_I64: i64 = 1;
pub const SY_BOOL: i64 = 2;
pub const SY_VOID: i64 = 3;
pub const SY_U8: i64 = 4;
pub const SY_USIZE: i64 = 5;
pub const SY_I8: i64 = 6;
pub const SY_I16: i64 = 7;
pub const SY_U16: i64 = 8;
pub const SY_U32: i64 = 9;
pub const SY_U64: i64 = 10;

/// `Type::from_name` over the stage-27 scalar set (`f64` is deliberately
/// NOT here — floats join a later sema stage; the detector keeps them out).
pub fn sy_from_name(name: []u8) i64 {
    if (str_eq(name, "i32")) { return SY_I32; }
    if (str_eq(name, "i64")) { return SY_I64; }
    if (str_eq(name, "bool")) { return SY_BOOL; }
    if (str_eq(name, "void")) { return SY_VOID; }
    if (str_eq(name, "u8")) { return SY_U8; }
    if (str_eq(name, "usize")) { return SY_USIZE; }
    if (str_eq(name, "i8")) { return SY_I8; }
    if (str_eq(name, "i16")) { return SY_I16; }
    if (str_eq(name, "u16")) { return SY_U16; }
    if (str_eq(name, "u32")) { return SY_U32; }
    if (str_eq(name, "u64")) { return SY_U64; }
    return SY_NONE;
}

/// `Type::is_int`: every scalar except `bool`/`void` (and the none marker).
pub fn sy_is_int(t: i64) bool {
    if (t == SY_NONE) { return false; }
    if (t == SY_BOOL) { return false; }
    if (t == SY_VOID) { return false; }
    return true;
}

/// `Type::is_signed` over the subset ints.
pub fn sy_is_signed(t: i64) bool {
    return t == SY_I8 or t == SY_I16 or t == SY_I32 or t == SY_I64;
}

// --- the stage-27 subset detector ---------------------------------------------

/// The detector's verdict: `found` with the FIRST out-of-subset construct's
/// word + byte position, in a fixed depth-first walk (items in source
/// order; per fn params → return → body; per statement/expression,
/// children in field order — the Rust twin transcribes this exactly).
pub const SsDet = struct {
    src: []u8,
    nodes: []Node,
    found: bool,
    word: []u8,
    pos: usize,

    fn init(src: []u8, nodes: []Node) SsDet {
        return SsDet{ .src = src, .nodes = nodes, .found = false, .word = "", .pos = 0 };
    }

    fn hit(self: *SsDet, word: []u8, pos: usize) void {
        if (self.found) { return; }
        self.found = true;
        self.word = word;
        self.pos = pos;
    }

    fn dname(self: *SsDet, n: i32) []u8 {
        var u: usize = @as(usize, n);
        return self.src[self.nodes[u].xoff .. self.nodes[u].xoff + self.nodes[u].xlen];
    }

    /// A type reference: BARE scalar spellings only — any composite FORM
    /// (`? ! * [] [N] [n] Set! Name(…)`) is `type-form`, any other
    /// spelling (f64, `type`, `Self` — and `@This()`, whose synthesized
    /// name is `Self` on the Rust side and an empty span here, both
    /// unknown spellings) is `type-name`. `F_THIS`/`F_ESETTHIS` are
    /// deliberately NOT form bits: the Rust twin cannot distinguish a
    /// written `Self` from a desugared `@This()`, so both sides classify
    /// by the (unknown) name.
    fn d_type(self: *SsDet, n: i32) void {
        if (self.found or n < 0) { return; }
        var u: usize = @as(usize, n);
        var composite: i64 = F_OPT | F_ERR | F_PTR | F_SLICE | F_ARRLIT | F_ARRPARAM | F_ERRSET | F_APP;
        if ((self.nodes[u].flags & composite) != 0) {
            self.hit("type-form", self.nodes[u].off);
            return;
        }
        if (sy_from_name(self.dname(n)) == SY_NONE) {
            self.hit("type-name", self.nodes[u].off);
        }
    }

    fn d_expr(self: *SsDet, n: i32) void {
        if (self.found or n < 0) { return; }
        var u: usize = @as(usize, n);
        var k: u8 = self.nodes[u].kind;
        if (k == ND_INT or k == ND_BOOL or k == ND_IDENT) { return; }
        if (k == ND_UNARY or k == ND_COMPTIME) {
            self.d_expr(self.nodes[u].a);
            return;
        }
        if (k == ND_BIN) {
            self.d_expr(self.nodes[u].a);
            self.d_expr(self.nodes[u].b);
            return;
        }
        if (k == ND_CALL) {
            // The allocator builtins pull `Allocator`/slice types into
            // sema — out of the scalar stage (a call to a USER fn of these
            // names is still out: sema's builtin arms match the name
            // first, and the detector must agree with itself on both
            // sides, not with the resolution).
            var callee: []u8 = self.dname(n);
            if (str_eq(callee, "c_allocator") or str_eq(callee, "alloc") or str_eq(callee, "free")) {
                self.hit("call", self.nodes[u].off);
                return;
            }
            var arg: i32 = self.nodes[u].a;
            while (arg >= 0) {
                self.d_expr(arg);
                if (self.found) { return; }
                arg = self.nodes[@as(usize, arg)].next;
            }
            return;
        }
        // Floats, strings, aggregates, optionals, error unions, builtins,
        // `unreachable`, method calls, … — every other expression shape.
        self.hit("expr", self.nodes[u].off);
    }

    fn d_block(self: *SsDet, n: i32) void {
        if (self.found or n < 0) { return; }
        var s: i32 = self.nodes[@as(usize, n)].a;
        while (s >= 0) {
            self.d_stmt(s);
            if (self.found) { return; }
            s = self.nodes[@as(usize, s)].next;
        }
    }

    fn d_stmt(self: *SsDet, n: i32) void {
        if (self.found or n < 0) { return; }
        var u: usize = @as(usize, n);
        var k: u8 = self.nodes[u].kind;
        if (k == ND_LET) {
            if (self.nodes[u].a >= 0) { self.d_type(self.nodes[u].a); }
            if (self.found) { return; }
            self.d_expr(self.nodes[u].b);
            return;
        }
        if (k == ND_ASSIGN) {
            self.d_expr(self.nodes[u].a);
            return;
        }
        if (k == ND_RETURN) {
            if (self.nodes[u].a >= 0) { self.d_expr(self.nodes[u].a); }
            return;
        }
        if (k == ND_IF) {
            if ((self.nodes[u].flags & F_CAP) != 0) {
                self.hit("capture", self.nodes[u].off);
                return;
            }
            self.d_expr(self.nodes[u].a);
            if (self.found) { return; }
            self.d_block(self.nodes[u].b);
            if (self.found) { return; }
            if (self.nodes[u].c >= 0) { self.d_stmt(self.nodes[u].c); }
            return;
        }
        if (k == ND_WHILE) {
            if ((self.nodes[u].flags & F_LABEL) != 0) {
                self.hit("label", self.nodes[u].off);
                return;
            }
            self.d_expr(self.nodes[u].a);
            if (self.found) { return; }
            if (self.nodes[u].b >= 0) {
                self.d_stmt(self.nodes[u].b);
                if (self.found) { return; }
            }
            self.d_block(self.nodes[u].c);
            return;
        }
        if (k == ND_BREAK or k == ND_CONTINUE) {
            if ((self.nodes[u].flags & F_LABEL) != 0) {
                self.hit("label", self.nodes[u].off);
            }
            return;
        }
        if (k == ND_DEFER) {
            self.d_stmt(self.nodes[u].a);
            return;
        }
        if (k == ND_BLOCK) {
            self.d_block(n);
            return;
        }
        // Out-of-subset statement FORMS get one word; everything else in a
        // statement slot is an expression statement and walks as one.
        if (k == ND_PASSIGN or k == ND_FOR or k == ND_SWITCH or k == ND_ERRDEFER) {
            self.hit("stmt", self.nodes[u].off);
            return;
        }
        self.d_expr(n);
    }
};

/// Detect the FIRST out-of-subset construct over the flattened item chain
/// (`root` = head). An `@import` item anywhere in the chain — the root's
/// own, a sub-file's, or the std import — puts the whole module out
/// (stage 27 is single-file; sema over a flattened module is a later
/// stage).
pub fn ss_detect(src: []u8, nodes: []Node, root: i32) SsDet {
    var d: SsDet = SsDet.init(src, nodes);
    var cur: i32 = root;
    while (cur >= 0) {
        var u: usize = @as(usize, cur);
        var k: u8 = nodes[u].kind;
        if (k == ND_IMPORT) {
            d.hit("import", nodes[u].off);
            return d;
        }
        if (k == ND_STRUCT or k == ND_ENUM or k == ND_UNION or k == ND_ERRSET) {
            d.hit("item", nodes[u].off);
            return d;
        }
        if (k == ND_FN) {
            var p: i32 = nodes[u].a;
            while (p >= 0) {
                var pu: usize = @as(usize, p);
                if ((nodes[pu].flags & F_COMPTIME) != 0) {
                    d.hit("generic-param", nodes[pu].off);
                    return d;
                }
                d.d_type(nodes[pu].a);
                if (d.found) { return d; }
                p = nodes[pu].next;
            }
            d.d_type(nodes[u].b);
            if (d.found) { return d; }
            d.d_block(nodes[u].c);
            if (d.found) { return d; }
        }
        if (k == ND_CONST) {
            if (nodes[u].a >= 0) {
                d.d_type(nodes[u].a);
                if (d.found) { return d; }
            }
            d.d_expr(nodes[u].b);
            if (d.found) { return d; }
        }
        if (k == ND_TEST) {
            d.d_block(nodes[u].a);
            if (d.found) { return d; }
        }
        cur = nodes[u].next;
    }
    return d;
}

// --- the checker ---------------------------------------------------------------

/// Mutability classes (`sema::Mut`): only `M_VAR` places assign.
const M_VAR: i64 = 0;
const M_PARAM: i64 = 1;
const M_CONST: i64 = 2;

/// A folded constant (`const_eval::ConstVal`): `kind` 1 = int, 2 = bool,
/// 0 = the evaluation failed (the diagnostic is already recorded).
const EvOut = struct {
    kind: i64,
    val: i64,
};

/// The lookup result: `found`, the binding's type and mutability.
const LkOut = struct {
    found: bool,
    ty: i64,
    m: i64,
};

/// The `check_int_operands` pair (either side `SY_NONE` = poisoned).
const TyPair = struct {
    lt: i64,
    rt: i64,
};

/// The verdict of `ss_verdict`: `code == 0` is OK, otherwise the FIRST
/// diagnostic's numeric code (110 = E0110, …) at byte `pos`.
pub const SsOut = struct {
    code: i64,
    pos: usize,
};

/// The stage-27 checker state. Fixed-capacity tables (subset corpus files
/// are small; the caps are far above any real input, and registration
/// stops silently at the cap — unreachable in practice).
const Ck = struct {
    src: []u8,
    nodes: []Node,
    // The first diagnostic (0 = none yet) — set once, never overwritten.
    dcode: i64,
    dpos: usize,
    // Registered fn signatures: name span + the ND_FN node (params/return
    // resolve on demand — the subset guarantees scalar spellings).
    fn_noff: []usize,
    fn_nlen: []usize,
    fn_node: []i32,
    fn_count: usize,
    // Folded top-level consts: name span, value kind (1 int / 2 bool),
    // value, and declared-or-inferred type.
    co_noff: []usize,
    co_nlen: []usize,
    co_kind: []i64,
    co_val: []i64,
    co_ty: []i64,
    co_count: usize,
    // The scope stack: a flat binding array (name span, type, mutability)
    // plus per-scope start indexes; lookup scans newest-first, so
    // shadowing and same-scope redefinition behave like the Rust
    // per-scope-HashMap (the latest definition wins).
    sb_noff: []usize,
    sb_nlen: []usize,
    sb_ty: []i64,
    sb_mut: []i64,
    sb_count: usize,
    sc_start: []usize,
    sc_count: usize,
    in_test: bool,
    loop_depth: i64,
    ret_ty: i64,

    fn init(a: Allocator, src: []u8, nodes: []Node) Ck {
        return Ck{
            .src = src,
            .nodes = nodes,
            .dcode = 0,
            .dpos = 0,
            .fn_noff = alloc(a, usize, 1024),
            .fn_nlen = alloc(a, usize, 1024),
            .fn_node = alloc(a, i32, 1024),
            .fn_count = 0,
            .co_noff = alloc(a, usize, 1024),
            .co_nlen = alloc(a, usize, 1024),
            .co_kind = alloc(a, i64, 1024),
            .co_val = alloc(a, i64, 1024),
            .co_ty = alloc(a, i64, 1024),
            .co_count = 0,
            .sb_noff = alloc(a, usize, 8192),
            .sb_nlen = alloc(a, usize, 8192),
            .sb_ty = alloc(a, i64, 8192),
            .sb_mut = alloc(a, i64, 8192),
            .sb_count = 0,
            .sc_start = alloc(a, usize, 256),
            .sc_count = 0,
            .in_test = false,
            .loop_depth = 0,
            .ret_ty = SY_VOID,
        };
    }

    fn deinit(self: Ck, a: Allocator) void {
        free(a, self.fn_noff);
        free(a, self.fn_nlen);
        free(a, self.fn_node);
        free(a, self.co_noff);
        free(a, self.co_nlen);
        free(a, self.co_kind);
        free(a, self.co_val);
        free(a, self.co_ty);
        free(a, self.sb_noff);
        free(a, self.sb_nlen);
        free(a, self.sb_ty);
        free(a, self.sb_mut);
        free(a, self.sc_start);
    }

    fn failed(self: *Ck) bool {
        return self.dcode != 0;
    }

    /// Record the FIRST diagnostic; later calls are no-ops (the mirror
    /// short-circuits — see the module comment).
    fn fail(self: *Ck, code: i64, pos: usize) void {
        if (self.dcode != 0) { return; }
        self.dcode = code;
        self.dpos = pos;
    }

    fn cname(self: *Ck, n: i32) []u8 {
        var u: usize = @as(usize, n);
        return self.src[self.nodes[u].xoff .. self.nodes[u].xoff + self.nodes[u].xlen];
    }

    /// `resolve_type` over the subset: the detector guarantees a scalar
    /// spelling, so this cannot fail on admitted input (defensive i64).
    fn resolve_ty(self: *Ck, tnode: i32) i64 {
        var t: i64 = sy_from_name(self.cname(tnode));
        if (t == SY_NONE) { return SY_I64; }
        return t;
    }

    // -- scopes ----------------------------------------------------------------

    fn push_scope(self: *Ck) void {
        if (self.sc_count < self.sc_start.len) {
            self.sc_start[self.sc_count] = self.sb_count;
        }
        self.sc_count += 1;
    }

    fn pop_scope(self: *Ck) void {
        self.sc_count -= 1;
        if (self.sc_count < self.sc_start.len) {
            self.sb_count = self.sc_start[self.sc_count];
        }
    }

    fn define(self: *Ck, noff: usize, nlen: usize, ty: i64, m: i64) void {
        if (self.sb_count >= self.sb_noff.len) { return; }
        self.sb_noff[self.sb_count] = noff;
        self.sb_nlen[self.sb_count] = nlen;
        self.sb_ty[self.sb_count] = ty;
        self.sb_mut[self.sb_count] = m;
        self.sb_count += 1;
    }

    /// `Checker::lookup`: innermost scopes first (newest binding wins),
    /// then the top-level consts (as immutable `M_CONST` bindings).
    fn lookup(self: *Ck, name: []u8) LkOut {
        var i: usize = self.sb_count;
        while (i > 0) : (i -= 1) {
            var j: usize = i - 1;
            if (str_eq(self.src[self.sb_noff[j] .. self.sb_noff[j] + self.sb_nlen[j]], name)) {
                return LkOut{ .found = true, .ty = self.sb_ty[j], .m = self.sb_mut[j] };
            }
        }
        var k: usize = 0;
        while (k < self.co_count) : (k += 1) {
            if (str_eq(self.src[self.co_noff[k] .. self.co_noff[k] + self.co_nlen[k]], name)) {
                return LkOut{ .found = true, .ty = self.co_ty[k], .m = M_CONST };
            }
        }
        return LkOut{ .found = false, .ty = SY_NONE, .m = M_CONST };
    }

    fn fn_of(self: *Ck, name: []u8) i32 {
        var i: usize = 0;
        while (i < self.fn_count) : (i += 1) {
            if (str_eq(self.src[self.fn_noff[i] .. self.fn_noff[i] + self.fn_nlen[i]], name)) {
                return self.fn_node[i];
            }
        }
        return 0 - 1;
    }

    // -- the const-eval mirror (const_eval.rs) ---------------------------------
    //
    // Arithmetic wraps as i64 exactly like the Rust `wrapping_*` calls: the
    // kardashev operators lower to C int64 arithmetic, which wraps on every
    // supported target for every value the corpus reaches (the i64::MIN
    // corners are UB-free in Rust and unexercised here). A shift amount is
    // masked `& 63` — `(b as u32).wrapping_shl/shr`'s combined effect.

    fn ev(self: *Ck, n: i32) EvOut {
        var none: EvOut = EvOut{ .kind = 0, .val = 0 };
        if (self.failed() or n < 0) { return none; }
        var u: usize = @as(usize, n);
        var k: u8 = self.nodes[u].kind;
        if (k == ND_INT) {
            return EvOut{ .kind = 1, .val = self.nodes[u].val };
        }
        if (k == ND_BOOL) {
            return EvOut{ .kind = 2, .val = self.nodes[u].val };
        }
        if (k == ND_IDENT) {
            var name: []u8 = self.cname(n);
            var i: usize = 0;
            while (i < self.co_count) : (i += 1) {
                if (str_eq(self.src[self.co_noff[i] .. self.co_noff[i] + self.co_nlen[i]], name)) {
                    if (self.co_kind[i] != 0) {
                        return EvOut{ .kind = self.co_kind[i], .val = self.co_val[i] };
                    }
                }
            }
            self.fail(131, self.nodes[u].off);
            return none;
        }
        if (k == ND_COMPTIME) {
            return self.ev(self.nodes[u].a);
        }
        if (k == ND_UNARY) {
            var v: EvOut = self.ev(self.nodes[u].a);
            if (v.kind == 0) { return none; }
            var op: i64 = self.nodes[u].val;
            if (op == UOP_NEG) {
                if (v.kind == 1) { return EvOut{ .kind = 1, .val = 0 - v.val }; }
                self.fail(132, self.nodes[u].off);
                return none;
            }
            if (op == UOP_NOT) {
                if (v.kind == 2) {
                    if (v.val == 0) { return EvOut{ .kind = 2, .val = 1 }; }
                    return EvOut{ .kind = 2, .val = 0 };
                }
                self.fail(132, self.nodes[u].off);
                return none;
            }
            // UOP_BNOT
            if (v.kind == 1) { return EvOut{ .kind = 1, .val = ~v.val }; }
            self.fail(132, self.nodes[u].off);
            return none;
        }
        if (k == ND_BIN) {
            var l: EvOut = self.ev(self.nodes[u].a);
            if (l.kind == 0) { return none; }
            var r: EvOut = self.ev(self.nodes[u].b);
            if (r.kind == 0) { return none; }
            return self.ev_bin(self.nodes[u].val, l, r, self.nodes[u].off);
        }
        // A call — the only other subset shape — is not a constant; the
        // else arm also defensively covers anything out-of-subset.
        self.fail(130, self.nodes[u].off);
        return none;
    }

    fn ev_bin(self: *Ck, op: i64, l: EvOut, r: EvOut, pos: usize) EvOut {
        var none: EvOut = EvOut{ .kind = 0, .val = 0 };
        if (op == OPC_ADD or op == OPC_SUB or op == OPC_MUL or op == OPC_DIV or op == OPC_REM) {
            if (l.kind != 1 or r.kind != 1) {
                self.fail(132, pos);
                return none;
            }
            if (op == OPC_ADD) { return EvOut{ .kind = 1, .val = l.val + r.val }; }
            if (op == OPC_SUB) { return EvOut{ .kind = 1, .val = l.val - r.val }; }
            if (op == OPC_MUL) { return EvOut{ .kind = 1, .val = l.val * r.val }; }
            if (r.val == 0) {
                // Division / remainder by zero (E0132).
                self.fail(132, pos);
                return none;
            }
            if (op == OPC_DIV) { return EvOut{ .kind = 1, .val = l.val / r.val }; }
            return EvOut{ .kind = 1, .val = l.val % r.val };
        }
        if (op == OPC_EQ or op == OPC_NE) {
            if (l.kind != r.kind) {
                self.fail(132, pos);
                return none;
            }
            var eq: bool = l.val == r.val;
            if (op == OPC_NE) { eq = !eq; }
            if (eq) { return EvOut{ .kind = 2, .val = 1 }; }
            return EvOut{ .kind = 2, .val = 0 };
        }
        if (op == OPC_LT or op == OPC_LE or op == OPC_GT or op == OPC_GE) {
            // Int-int compares directly; bool-bool compares as 0/1 ints
            // (`ConstVal::Bool(a) => a as i64`).
            if (l.kind != r.kind) {
                self.fail(132, pos);
                return none;
            }
            var res: bool = false;
            if (op == OPC_LT) { res = l.val < r.val; }
            if (op == OPC_LE) { res = l.val <= r.val; }
            if (op == OPC_GT) { res = l.val > r.val; }
            if (op == OPC_GE) { res = l.val >= r.val; }
            if (res) { return EvOut{ .kind = 2, .val = 1 }; }
            return EvOut{ .kind = 2, .val = 0 };
        }
        if (op == OPC_AND or op == OPC_OR) {
            if (l.kind != 2 or r.kind != 2) {
                self.fail(132, pos);
                return none;
            }
            var out: bool = false;
            if (op == OPC_AND) { out = (l.val != 0) and (r.val != 0); }
            if (op == OPC_OR) { out = (l.val != 0) or (r.val != 0); }
            if (out) { return EvOut{ .kind = 2, .val = 1 }; }
            return EvOut{ .kind = 2, .val = 0 };
        }
        // Bitwise / shifts.
        if (l.kind != 1 or r.kind != 1) {
            self.fail(132, pos);
            return none;
        }
        if (op == OPC_BAND) { return EvOut{ .kind = 1, .val = l.val & r.val }; }
        if (op == OPC_BOR) { return EvOut{ .kind = 1, .val = l.val | r.val }; }
        if (op == OPC_BXOR) { return EvOut{ .kind = 1, .val = l.val ^ r.val }; }
        var amt: i64 = r.val & 63;
        if (op == OPC_SHL) { return EvOut{ .kind = 1, .val = l.val << amt }; }
        return EvOut{ .kind = 1, .val = l.val >> amt };
    }

    // -- expressions -----------------------------------------------------------

    /// `is_flex_int_literal`: a bare integer literal, or unary `-` over one
    /// (recursively) — adopts the expected integer type at its use site.
    fn is_flex(self: *Ck, n: i32) bool {
        if (n < 0) { return false; }
        var u: usize = @as(usize, n);
        if (self.nodes[u].kind == ND_INT) { return true; }
        if (self.nodes[u].kind == ND_UNARY and self.nodes[u].val == UOP_NEG) {
            return self.is_flex(self.nodes[u].a);
        }
        return false;
    }

    /// `check_expr` over the subset; `expected` is a type code or SY_NONE.
    /// SY_NONE out means the expression failed (the diagnostic is set).
    fn ck_expr(self: *Ck, n: i32, expected: i64) i64 {
        if (self.failed() or n < 0) { return SY_NONE; }
        var u: usize = @as(usize, n);
        var k: u8 = self.nodes[u].kind;
        if (k == ND_INT) {
            if (expected != SY_NONE and sy_is_int(expected)) { return expected; }
            return SY_I64;
        }
        if (k == ND_BOOL) { return SY_BOOL; }
        if (k == ND_IDENT) {
            var lk: LkOut = self.lookup(self.cname(n));
            if (lk.found) { return lk.ty; }
            self.fail(100, self.nodes[u].off);
            return SY_NONE;
        }
        if (k == ND_UNARY) {
            return self.ck_unary(n, expected);
        }
        if (k == ND_BIN) {
            return self.ck_binary(n, expected);
        }
        if (k == ND_CALL) {
            return self.ck_call(n);
        }
        if (k == ND_COMPTIME) {
            // The folded value's kind decides the type, with the integer
            // polymorphism applied to int results.
            var v: EvOut = self.ev(self.nodes[u].a);
            if (v.kind == 0) { return SY_NONE; }
            if (v.kind == 1) {
                if (expected != SY_NONE and sy_is_int(expected)) { return expected; }
                return SY_I64;
            }
            return SY_BOOL;
        }
        return SY_NONE;
    }

    fn ck_unary(self: *Ck, n: i32, expected: i64) i64 {
        var u: usize = @as(usize, n);
        var op: i64 = self.nodes[u].val;
        if (op == UOP_NEG) {
            var t: i64 = self.ck_expr(self.nodes[u].a, expected);
            if (t == SY_NONE) { return SY_NONE; }
            if (sy_is_int(t) and sy_is_signed(t)) { return t; }
            self.fail(110, self.nodes[u].off);
            return SY_NONE;
        }
        if (op == UOP_NOT) {
            var t2: i64 = self.ck_expr(self.nodes[u].a, SY_BOOL);
            if (t2 == SY_NONE) { return SY_NONE; }
            if (t2 == SY_BOOL) { return SY_BOOL; }
            self.fail(110, self.nodes[u].off);
            return SY_NONE;
        }
        // UOP_BNOT: any integer; a flexible literal adopts an integer
        // expectation (`expected.filter(is_int)`).
        var exp3: i64 = SY_NONE;
        if (expected != SY_NONE and sy_is_int(expected)) { exp3 = expected; }
        var t3: i64 = self.ck_expr(self.nodes[u].a, exp3);
        if (t3 == SY_NONE) { return SY_NONE; }
        if (sy_is_int(t3)) { return t3; }
        self.fail(110, self.nodes[u].off);
        return SY_NONE;
    }

    /// `check_int_operands`: both sides share a type; a flexible literal
    /// adopts `expected`, else the OTHER operand's concrete type (checked
    /// first — the anchoring order is observable through which side's
    /// diagnostic comes first), else i64.
    fn ck_operands(self: *Ck, l: i32, r: i32, expected: i64) TyPair {
        if (expected != SY_NONE) {
            var lt0: i64 = self.ck_expr(l, expected);
            var rt0: i64 = self.ck_expr(r, expected);
            return TyPair{ .lt = lt0, .rt = rt0 };
        }
        if (!self.is_flex(l)) {
            var lt1: i64 = self.ck_expr(l, SY_NONE);
            var anchor1: i64 = SY_NONE;
            if (lt1 != SY_NONE and sy_is_int(lt1)) { anchor1 = lt1; }
            var rt1: i64 = self.ck_expr(r, anchor1);
            return TyPair{ .lt = lt1, .rt = rt1 };
        }
        if (!self.is_flex(r)) {
            var rt2: i64 = self.ck_expr(r, SY_NONE);
            var anchor2: i64 = SY_NONE;
            if (rt2 != SY_NONE and sy_is_int(rt2)) { anchor2 = rt2; }
            var lt2: i64 = self.ck_expr(l, anchor2);
            return TyPair{ .lt = lt2, .rt = rt2 };
        }
        var lt3: i64 = self.ck_expr(l, SY_I64);
        var rt3: i64 = self.ck_expr(r, SY_I64);
        return TyPair{ .lt = lt3, .rt = rt3 };
    }

    fn ck_binary(self: *Ck, n: i32, expected: i64) i64 {
        var u: usize = @as(usize, n);
        var op: i64 = self.nodes[u].val;
        var l: i32 = self.nodes[u].a;
        var r: i32 = self.nodes[u].b;
        var lu: usize = @as(usize, l);
        var ru: usize = @as(usize, r);
        if (op == OPC_ADD or op == OPC_SUB or op == OPC_MUL or op == OPC_DIV or op == OPC_REM) {
            var exp: i64 = SY_NONE;
            if (expected != SY_NONE and sy_is_int(expected)) { exp = expected; }
            var p: TyPair = self.ck_operands(l, r, exp);
            if (p.lt == SY_NONE or p.rt == SY_NONE) { return SY_NONE; }
            // No f64 in the subset: the operand rule reduces to is_int.
            if (!sy_is_int(p.lt)) {
                self.fail(110, self.nodes[lu].off);
                return SY_NONE;
            }
            if (!sy_is_int(p.rt)) {
                self.fail(110, self.nodes[ru].off);
                return SY_NONE;
            }
            if (p.lt != p.rt) {
                self.fail(110, self.nodes[u].off);
                return SY_NONE;
            }
            return p.lt;
        }
        if (op == OPC_EQ or op == OPC_NE or op == OPC_LT or op == OPC_LE or op == OPC_GT or op == OPC_GE) {
            var p2: TyPair = self.ck_operands(l, r, SY_NONE);
            if (p2.lt == SY_NONE or p2.rt == SY_NONE) { return SY_NONE; }
            // The struct / aggregate arms are unreachable in the scalar
            // subset; the same-type rule is the whole check.
            if (p2.lt != p2.rt) {
                self.fail(110, self.nodes[u].off);
                return SY_NONE;
            }
            return SY_BOOL;
        }
        if (op == OPC_AND or op == OPC_OR) {
            var lt: i64 = self.ck_expr(l, SY_BOOL);
            var rt: i64 = self.ck_expr(r, SY_BOOL);
            if (lt == SY_NONE or rt == SY_NONE) { return SY_NONE; }
            // Flag-accumulation order: the lhs's diagnostic precedes the
            // rhs's; with the first-diag short-circuit, checking lhs first
            // is the whole mirror.
            var ok: bool = true;
            if (lt != SY_BOOL) {
                self.fail(110, self.nodes[lu].off);
                ok = false;
            }
            if (rt != SY_BOOL) {
                self.fail(110, self.nodes[ru].off);
                ok = false;
            }
            if (ok) { return SY_BOOL; }
            return SY_NONE;
        }
        // Bitwise / shifts: same integer type both sides.
        var exp4: i64 = SY_NONE;
        if (expected != SY_NONE and sy_is_int(expected)) { exp4 = expected; }
        var p3: TyPair = self.ck_operands(l, r, exp4);
        if (p3.lt == SY_NONE or p3.rt == SY_NONE) { return SY_NONE; }
        if (!sy_is_int(p3.lt)) {
            self.fail(110, self.nodes[lu].off);
            return SY_NONE;
        }
        if (!sy_is_int(p3.rt)) {
            self.fail(110, self.nodes[ru].off);
            return SY_NONE;
        }
        if (p3.lt != p3.rt) {
            self.fail(110, self.nodes[u].off);
            return SY_NONE;
        }
        return p3.lt;
    }

    fn ck_call(self: *Ck, n: i32) i64 {
        var u: usize = @as(usize, n);
        var callee: []u8 = self.cname(n);
        if (str_eq(callee, "print")) {
            var cnt: i64 = self.arg_count(n);
            if (cnt != 1) {
                self.fail(110, self.nodes[u].off);
                return SY_VOID;
            }
            var t: i64 = self.ck_expr(self.nodes[u].a, SY_NONE);
            if (t != SY_NONE) {
                // `print` accepts an integer (f64 and strings are out of
                // the scalar subset, so is_int is the whole rule here —
                // bool and void reject exactly as in Rust).
                if (!sy_is_int(t)) {
                    self.fail(110, self.nodes[@as(usize, self.nodes[u].a)].off);
                }
            }
            return SY_VOID;
        }
        if (str_eq(callee, "expect")) {
            if (!self.in_test) {
                self.fail(140, self.nodes[u].off);
                return SY_VOID;
            }
            var cnt2: i64 = self.arg_count(n);
            if (cnt2 != 1) {
                self.fail(110, self.nodes[u].off);
                return SY_VOID;
            }
            var t2: i64 = self.ck_expr(self.nodes[u].a, SY_BOOL);
            if (t2 != SY_NONE) {
                if (t2 != SY_BOOL) {
                    self.fail(110, self.nodes[@as(usize, self.nodes[u].a)].off);
                }
            }
            return SY_VOID;
        }
        var fnode: i32 = self.fn_of(callee);
        if (fnode < 0) {
            self.fail(100, self.nodes[u].off);
            return SY_NONE;
        }
        var fu: usize = @as(usize, fnode);
        // Arity against the declared parameter list.
        var pcount: i64 = 0;
        var p: i32 = self.nodes[fu].a;
        while (p >= 0) {
            pcount += 1;
            p = self.nodes[@as(usize, p)].next;
        }
        var acount: i64 = self.arg_count(n);
        var rett: i64 = self.resolve_ty(self.nodes[fu].b);
        if (acount != pcount) {
            self.fail(110, self.nodes[u].off);
            return rett;
        }
        // Each argument against its parameter type, in order.
        var arg: i32 = self.nodes[u].a;
        var pp: i32 = self.nodes[fu].a;
        while (arg >= 0 and pp >= 0) {
            var pt: i64 = self.resolve_ty(self.nodes[@as(usize, pp)].a);
            var at: i64 = self.ck_expr(arg, pt);
            if (at != SY_NONE and at != pt) {
                self.fail(110, self.nodes[@as(usize, arg)].off);
            }
            if (self.failed()) { return rett; }
            arg = self.nodes[@as(usize, arg)].next;
            pp = self.nodes[@as(usize, pp)].next;
        }
        return rett;
    }

    fn arg_count(self: *Ck, n: i32) i64 {
        var cnt: i64 = 0;
        var arg: i32 = self.nodes[@as(usize, n)].a;
        while (arg >= 0) {
            cnt += 1;
            arg = self.nodes[@as(usize, arg)].next;
        }
        return cnt;
    }

    // -- statements ------------------------------------------------------------

    fn ck_cond(self: *Ck, n: i32) void {
        var t: i64 = self.ck_expr(n, SY_BOOL);
        if (t != SY_NONE and t != SY_BOOL) {
            self.fail(110, self.nodes[@as(usize, n)].off);
        }
    }

    fn ck_block(self: *Ck, n: i32) void {
        if (self.failed() or n < 0) { return; }
        self.push_scope();
        var s: i32 = self.nodes[@as(usize, n)].a;
        while (s >= 0) {
            self.ck_stmt(s);
            if (self.failed()) { break; }
            s = self.nodes[@as(usize, s)].next;
        }
        self.pop_scope();
    }

    /// `check_compound_assign_arith`: the rhs is evaluated FIRST (its own
    /// diagnostics surface first), with the place type as the expectation
    /// only when it is an integer.
    fn ck_compound(self: *Ck, place_ty: i64, rhs: i32, stmt_off: usize) void {
        var exp: i64 = SY_NONE;
        if (sy_is_int(place_ty)) { exp = place_ty; }
        var rt: i64 = self.ck_expr(rhs, exp);
        if (self.failed()) { return; }
        if (!sy_is_int(place_ty)) {
            self.fail(110, stmt_off);
            return;
        }
        if (rt == SY_NONE) { return; }
        if (!sy_is_int(rt)) {
            self.fail(110, self.nodes[@as(usize, rhs)].off);
            return;
        }
        if (rt != place_ty) {
            self.fail(110, stmt_off);
        }
    }

    fn ck_stmt(self: *Ck, n: i32) void {
        if (self.failed() or n < 0) { return; }
        var u: usize = @as(usize, n);
        var k: u8 = self.nodes[u].kind;
        if (k == ND_LET) {
            var bind_ty: i64 = SY_I64;
            if (self.nodes[u].a >= 0) {
                var dt: i64 = self.resolve_ty(self.nodes[u].a);
                var vt: i64 = self.ck_expr(self.nodes[u].b, dt);
                if (vt != SY_NONE and dt != vt) {
                    self.fail(110, self.nodes[@as(usize, self.nodes[u].b)].off);
                }
                bind_ty = dt;
            } else {
                // Inferred (v0.121): no expected type — a literal defaults
                // to i64. (The bare-null/error/enum inference errors are
                // out-of-subset shapes.)
                var vt2: i64 = self.ck_expr(self.nodes[u].b, SY_NONE);
                if (vt2 != SY_NONE) { bind_ty = vt2; }
            }
            if (self.failed()) { return; }
            var m: i64 = M_VAR;
            if ((self.nodes[u].flags & F_CONST) != 0) { m = M_CONST; }
            self.define(self.nodes[u].xoff, self.nodes[u].xlen, bind_ty, m);
            return;
        }
        if (k == ND_ASSIGN) {
            var lk: LkOut = self.lookup(self.cname(n));
            if (!lk.found) {
                self.fail(100, self.nodes[u].off);
                return;
            }
            if (lk.m != M_VAR) {
                self.fail(110, self.nodes[u].off);
                return;
            }
            if (self.nodes[u].val >= 0) {
                self.ck_compound(lk.ty, self.nodes[u].a, self.nodes[u].off);
                return;
            }
            var vt3: i64 = self.ck_expr(self.nodes[u].a, lk.ty);
            if (vt3 != SY_NONE and vt3 != lk.ty) {
                self.fail(110, self.nodes[@as(usize, self.nodes[u].a)].off);
            }
            return;
        }
        if (k == ND_RETURN) {
            if (self.nodes[u].a >= 0) {
                if (self.ret_ty == SY_VOID) {
                    self.fail(110, self.nodes[u].off);
                    return;
                }
                var vt4: i64 = self.ck_expr(self.nodes[u].a, self.ret_ty);
                if (vt4 != SY_NONE and vt4 != self.ret_ty) {
                    self.fail(110, self.nodes[@as(usize, self.nodes[u].a)].off);
                }
            } else {
                if (self.ret_ty != SY_VOID) {
                    self.fail(110, self.nodes[u].off);
                }
            }
            return;
        }
        if (k == ND_IF) {
            self.ck_cond(self.nodes[u].a);
            if (self.failed()) { return; }
            self.ck_block(self.nodes[u].b);
            if (self.failed()) { return; }
            if (self.nodes[u].c >= 0) { self.ck_stmt(self.nodes[u].c); }
            return;
        }
        if (k == ND_WHILE) {
            self.ck_cond(self.nodes[u].a);
            if (self.failed()) { return; }
            // The continue-clause statement checks in the loop's OUTER
            // scope, before the loop depth increments.
            if (self.nodes[u].b >= 0) {
                self.ck_stmt(self.nodes[u].b);
                if (self.failed()) { return; }
            }
            self.loop_depth += 1;
            self.ck_block(self.nodes[u].c);
            self.loop_depth -= 1;
            return;
        }
        if (k == ND_BREAK or k == ND_CONTINUE) {
            if (self.loop_depth == 0) {
                self.fail(120, self.nodes[u].off);
            }
            return;
        }
        if (k == ND_DEFER) {
            self.ck_stmt(self.nodes[u].a);
            return;
        }
        if (k == ND_BLOCK) {
            self.ck_block(n);
            return;
        }
        // An expression statement (the only remaining subset shape); the
        // result type is discarded, exactly like `Stmt::Expr`.
        self.ck_expr(n, SY_NONE);
    }

    // -- items -----------------------------------------------------------------

    fn ck_func(self: *Ck, fnode: i32) void {
        var u: usize = @as(usize, fnode);
        self.ret_ty = self.resolve_ty(self.nodes[u].b);
        self.in_test = false;
        self.loop_depth = 0;
        self.push_scope();
        var p: i32 = self.nodes[u].a;
        while (p >= 0) {
            var pu: usize = @as(usize, p);
            var pt: i64 = self.resolve_ty(self.nodes[pu].a);
            self.define(self.nodes[pu].xoff, self.nodes[pu].xlen, pt, M_PARAM);
            p = self.nodes[pu].next;
        }
        self.ck_block(self.nodes[u].c);
        self.pop_scope();
    }

    fn ck_test(self: *Ck, tnode: i32) void {
        var u: usize = @as(usize, tnode);
        self.ret_ty = SY_VOID;
        self.in_test = true;
        self.loop_depth = 0;
        // `check_test` pushes no parameter scope; the body block owns its
        // own (SPEC §3 / sema).
        self.ck_block(self.nodes[u].a);
        self.in_test = false;
    }
};

/// Whether `name` collides with a builtin (`E0101`, sema pass 1).
fn ss_is_builtin_name(name: []u8) bool {
    return str_eq(name, "print") or str_eq(name, "expect") or str_eq(name, "c_allocator") or str_eq(name, "alloc") or str_eq(name, "free");
}

/// The stage-27 checker over an ALREADY-DETECTED in-subset module: replay
/// the Rust pass order and report the first diagnostic, or OK (code 0).
pub fn ss_verdict(a: Allocator, src: []u8, nodes: []Node, root: i32) SsOut {
    var ck: Ck = Ck.init(a, src, nodes);

    // Pass 1 — function items in source order: the builtin-redefinition
    // check, then the signature registration (used by calls and by the
    // Pass-2 `const X = f();` E0311 rule).
    var cur: i32 = root;
    while (cur >= 0) {
        var u: usize = @as(usize, cur);
        if (nodes[u].kind == ND_FN) {
            if (ss_is_builtin_name(src[nodes[u].xoff .. nodes[u].xoff + nodes[u].xlen])) {
                ck.fail(101, nodes[u].off);
                var out1: SsOut = SsOut{ .code = ck.dcode, .pos = ck.dpos };
                ck.deinit(a);
                return out1;
            }
            if (ck.fn_count < ck.fn_noff.len) {
                ck.fn_noff[ck.fn_count] = nodes[u].xoff;
                ck.fn_nlen[ck.fn_count] = nodes[u].xlen;
                ck.fn_node[ck.fn_count] = cur;
                ck.fn_count += 1;
            }
        }
        cur = nodes[u].next;
    }

    // Pass 2 — top-level consts in source order: the E0311 call rule, the
    // const-eval fold, the annotation check, then the record (so a later
    // const — and every body — resolves earlier ones by name).
    cur = root;
    while (cur >= 0) {
        var u2: usize = @as(usize, cur);
        if (nodes[u2].kind == ND_CONST) {
            var v: i32 = nodes[u2].b;
            var vu: usize = @as(usize, v);
            if (nodes[vu].kind == ND_CALL) {
                if (ck.fn_of(ck.cname(v)) >= 0) {
                    // `const X = f();` where `f` is a declared fn: "not a
                    // type-constructor" (E0311). An unknown callee falls
                    // through to const-eval's E0130 instead.
                    ck.fail(311, nodes[vu].off);
                }
            }
            if (!ck.failed()) {
                var have_dt: bool = nodes[u2].a >= 0;
                var dt: i64 = SY_NONE;
                if (have_dt) { dt = ck.resolve_ty(nodes[u2].a); }
                var evd: EvOut = ck.ev(v);
                if (evd.kind != 0) {
                    if (have_dt) {
                        var ok: bool = false;
                        if (evd.kind == 1) { ok = sy_is_int(dt); }
                        if (evd.kind == 2) { ok = dt == SY_BOOL; }
                        if (!ok) {
                            ck.fail(110, nodes[vu].off);
                        }
                    }
                    if (!ck.failed() and ck.co_count < ck.co_noff.len) {
                        var ty: i64 = dt;
                        if (!have_dt) {
                            ty = SY_I64;
                            if (evd.kind == 2) { ty = SY_BOOL; }
                        }
                        ck.co_noff[ck.co_count] = nodes[u2].xoff;
                        ck.co_nlen[ck.co_count] = nodes[u2].xlen;
                        ck.co_kind[ck.co_count] = evd.kind;
                        ck.co_val[ck.co_count] = evd.val;
                        ck.co_ty[ck.co_count] = ty;
                        ck.co_count += 1;
                    }
                }
            }
            if (ck.failed()) {
                var out2: SsOut = SsOut{ .code = ck.dcode, .pos = ck.dpos };
                ck.deinit(a);
                return out2;
            }
        }
        cur = nodes[u2].next;
    }

    // Pass 3 — fn and test bodies in source order.
    cur = root;
    while (cur >= 0) {
        var u3: usize = @as(usize, cur);
        if (nodes[u3].kind == ND_FN) {
            ck.ck_func(cur);
        }
        if (nodes[u3].kind == ND_TEST) {
            ck.ck_test(cur);
        }
        if (ck.failed()) {
            var out3: SsOut = SsOut{ .code = ck.dcode, .pos = ck.dpos };
            ck.deinit(a);
            return out3;
        }
        cur = nodes[u3].next;
    }

    var out: SsOut = SsOut{ .code = 0, .pos = 0 };
    ck.deinit(a);
    return out;
}
