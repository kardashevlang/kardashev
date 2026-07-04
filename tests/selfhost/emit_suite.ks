// emit_suite.ks — in-language tests for the self-hosted subset C emitter
// (v0.161 scalars + v0.162 strings + v0.163 heap buffers + v0.164
// generalized `[]T` slices and `@as` casts + v0.165 slicing views + v0.166
// test blocks / EmitMode::Test + v0.167 `@import` resolution + v0.168 fixed
// arrays `[N]T` and `for` loops + v0.169 plain data structs).
//
// Run: kard test tests/selfhost/emit_suite.ks (driven from
// `crates/kardc/tests/selfhost_emit.rs` so it is part of `cargo test`).
// Pins the pieces the differential corpus exercises statistically: the type
// and operator spelling tables, the subset detector's verdicts and first-hit
// positions, the const-fold rules (including the failure fallbacks the
// differential can only reach through sema-invalid inputs), dead-function
// elimination, the `defer` lowering shapes, and full byte-for-byte C output
// for hand-laid programs.

@import("../../selfhost/lexer.ks");
@import("../../selfhost/ast.ks");
@import("../../selfhost/parser.ks");
@import("../../selfhost/modres.ks");
@import("../../selfhost/emit.ks");
@import("std");

// Lex + parse `src` with the self-hosted toolchain (suite sources must be
// lexically and syntactically valid unless a test says otherwise).
fn eh_parse(a: Allocator, src: []u8) Parser {
    var toks: ArrayList(Token) = ArrayList(Token).init(a);
    var lx: Lexer = Lexer.init(src);
    var t: Token = lx.next();
    while (t.kind != TK_EOF and t.kind != TK_ERROR) {
        toks.push(a, t);
        t = lx.next();
    }
    toks.push(a, Token{ .kind = TK_EOF, .off = src.len, .len = 0 });
    var p: Parser = Parser.init(a, src, toks.items[0..toks.count]);
    var items: i32 = p.parse_module(a) catch 0 - 1;
    if (items < 0) {
        p.root = 0 - 1;
    }
    return p;
}

/// The subset verdict for `src`.
fn eh_detect(a: Allocator, src: []u8) Det {
    var p: Parser = eh_parse(a, src);
    return es_detect(src, p.nodes, p.root);
}

/// The emitted C for `src` (which must parse and be in the subset).
fn eh_emit(a: Allocator, src: []u8) []u8 {
    var p: Parser = eh_parse(a, src);
    return es_emit_program(a, src, p.nodes, p.root);
}

/// The `EmitMode::Test` (harness) C for `src` (v0.166).
fn eh_emit_test(a: Allocator, src: []u8) []u8 {
    var p: Parser = eh_parse(a, src);
    return es_emit_test(a, src, p.nodes, p.root);
}

/// Naive substring search (the suite's only string tool beyond `str_eq`).
fn eh_find(hay: []u8, needle: []u8) bool {
    if (needle.len > hay.len) { return false; }
    var i: usize = 0;
    while (i + needle.len <= hay.len) : (i += 1) {
        var j: usize = 0;
        var all: bool = true;
        while (j < needle.len) : (j += 1) {
            if (hay[i + j] != needle[j]) {
                all = false;
                break;
            }
        }
        if (all) { return true; }
    }
    return false;
}

/// The byte position of the first occurrence of `needle` in `hay`, or -1.
fn eh_pos(hay: []u8, needle: []u8) i64 {
    if (needle.len == 0 or needle.len > hay.len) { return 0 - 1; }
    var i: usize = 0;
    while (i + needle.len <= hay.len) : (i += 1) {
        var j: usize = 0;
        var all: bool = true;
        while (j < needle.len) : (j += 1) {
            if (hay[i + j] != needle[j]) {
                all = false;
                break;
            }
        }
        if (all) { return @as(i64, i); }
    }
    return 0 - 1;
}

/// Count non-overlapping occurrences of `needle` in `hay`.
fn eh_count(hay: []u8, needle: []u8) i64 {
    if (needle.len == 0 or needle.len > hay.len) { return 0; }
    var n: i64 = 0;
    var i: usize = 0;
    while (i + needle.len <= hay.len) {
        var j: usize = 0;
        var all: bool = true;
        while (j < needle.len) : (j += 1) {
            if (hay[i + j] != needle[j]) {
                all = false;
                break;
            }
        }
        if (all) {
            n += 1;
            i += needle.len;
        } else {
            i += 1;
        }
    }
    return n;
}

/// Append the fixed 10-line prelude + the section blank to `sb` — the exact
/// bytes `Em.emit_prelude` produces.
fn eh_prelude(a: Allocator, sb: *StrBuilder) void {
    sb.append(a, "#include <stdint.h>\n");
    sb.append(a, "#include <stdbool.h>\n");
    sb.append(a, "#include <stdio.h>\n");
    sb.append(a, "#include <stdlib.h>\n");
    sb.append(a, "#include <string.h>\n");
    sb.append(a, "#include <time.h>\n");
    sb.append(a, "typedef struct { int _unused; } kd_allocator;\n");
    sb.append(a, "static void kd_print(long long v) { printf(\"%lld\\n\", v); }\n");
    sb.append(a, "static void kd_print_f64(double x) { printf(\"%g\\n\", x); }\n");
    sb.append(a, "_Noreturn void kd_unreachable(void) { fputs(\"reached unreachable code\\n\", stderr); exit(101); }\n");
    sb.append(a, "\n");
}

// --- spelling tables --------------------------------------------------------------

test "type codes: from_name maps the seven subset spellings" {
    expect(et_from_name("i32") == ET_I32);
    expect(et_from_name("i64") == ET_I64);
    expect(et_from_name("bool") == ET_BOOL);
    expect(et_from_name("void") == ET_VOID);
    expect(et_from_name("u8") == ET_U8);
    expect(et_from_name("usize") == ET_USIZE);
    expect(et_from_name("Allocator") == ET_ALLOC);
    expect(et_from_name("f64") == ET_NONE);
    expect(et_from_name("Self") == ET_NONE);
    expect(et_from_name("") == ET_NONE);
}

test "type codes: C spellings, is_int and the u8 promotion class" {
    expect(str_eq(et_c_name(ET_I32), "int32_t"));
    expect(str_eq(et_c_name(ET_I64), "int64_t"));
    expect(str_eq(et_c_name(ET_BOOL), "bool"));
    expect(str_eq(et_c_name(ET_VOID), "void"));
    expect(str_eq(et_c_name(ET_U8), "uint8_t"));
    expect(str_eq(et_c_name(ET_USIZE), "uintptr_t"));
    expect(str_eq(et_c_name(ET_SLICE_U8), "kd_slice_uint8_t"));
    expect(str_eq(et_c_name(ET_ALLOC), "kd_allocator"));
    // The defensive fallback spelling mirrors the Rust `cty` fallback.
    expect(str_eq(et_c_name(ET_NONE), "int64_t"));
    expect(et_is_int(ET_I32));
    expect(et_is_int(ET_I64));
    expect(et_is_int(ET_U8));
    expect(et_is_int(ET_USIZE));
    expect(!et_is_int(ET_BOOL));
    expect(!et_is_int(ET_VOID));
    expect(!et_is_int(ET_SLICE_U8));
    expect(!et_is_int(ET_ALLOC));
    // Only `u8` is sub-32-bit here, so only it truncates back (§28.2).
    expect(et_promotes_in_c(ET_U8));
    expect(!et_promotes_in_c(ET_I32));
    expect(!et_promotes_in_c(ET_USIZE));
}

test "operator spellings: c_op and bool-result classification" {
    expect(str_eq(es_c_op(OPC_ADD), "+"));
    expect(str_eq(es_c_op(OPC_SUB), "-"));
    expect(str_eq(es_c_op(OPC_MUL), "*"));
    expect(str_eq(es_c_op(OPC_DIV), "/"));
    expect(str_eq(es_c_op(OPC_REM), "%"));
    expect(str_eq(es_c_op(OPC_EQ), "=="));
    expect(str_eq(es_c_op(OPC_NE), "!="));
    expect(str_eq(es_c_op(OPC_LT), "<"));
    expect(str_eq(es_c_op(OPC_LE), "<="));
    expect(str_eq(es_c_op(OPC_GT), ">"));
    expect(str_eq(es_c_op(OPC_GE), ">="));
    expect(str_eq(es_c_op(OPC_AND), "&&"));
    expect(str_eq(es_c_op(OPC_OR), "||"));
    expect(str_eq(es_c_op(OPC_BAND), "&"));
    expect(str_eq(es_c_op(OPC_BOR), "|"));
    expect(str_eq(es_c_op(OPC_BXOR), "^"));
    expect(str_eq(es_c_op(OPC_SHL), "<<"));
    expect(str_eq(es_c_op(OPC_SHR), ">>"));
    expect(es_is_bool_result(OPC_EQ));
    expect(es_is_bool_result(OPC_GE));
    expect(es_is_bool_result(OPC_AND));
    expect(es_is_bool_result(OPC_OR));
    expect(!es_is_bool_result(OPC_ADD));
    expect(!es_is_bool_result(OPC_BAND));
    expect(!es_is_bool_result(OPC_SHL));
}

// --- the subset detector ------------------------------------------------------------

test "detect: a full-feature subset program is in" {
    var a: Allocator = c_allocator();
    var d: Det = eh_detect(a, "const K: i64 = comptime (2 + 3);\nfn f(x: i32, b: bool) i32 {\n    defer print(1);\n    var i: i32 = 0;\n    while (i < x) : (i = i + 1) {\n        if (b and (i != 2)) { print(i); } else { continue; }\n        if (i > 5) { break; }\n    }\n    { var y = K; print(y); }\n    return x;\n}\npub fn main() void { print(f(3, true)); }\n");
    expect(!d.found);
}

test "detect: a module without fn main is nomain at 0" {
    var a: Allocator = c_allocator();
    var d: Det = eh_detect(a, "fn helper() void {}\n");
    expect(d.found);
    expect(str_eq(d.word, "nomain"));
    expect(d.pos == 0);
    var d2: Det = eh_detect(a, "");
    expect(d2.found);
    expect(str_eq(d2.word, "nomain"));
}

test "detect: float literal, first hit with position" {
    var a: Allocator = c_allocator();
    //                          0         1         2
    //                          0123456789012345678901234567
    var d: Det = eh_detect(a, "fn main() void { print(1.5); }");
    expect(d.found);
    expect(str_eq(d.word, "float"));
    expect(d.pos == 23);
}

test "detect: strings, []u8, .len and s[i] are in the subset (v0.162)" {
    var a: Allocator = c_allocator();
    var d: Det = eh_detect(a, "fn grab(s: []u8) []u8 { return s; }\npub fn main() void {\n    var s: []u8 = \"x\";\n    var c: u8 = s[0];\n    var n: usize = s.len;\n    print(grab(s));\n    print(c);\n    print(n);\n}\n");
    expect(!d.found);
}

test "detect: composite type forms and non-subset type names" {
    var a: Allocator = c_allocator();
    // `[]T` ranges over the five scalar elements (v0.164); anything else
    // is out.
    var d: Det = eh_detect(a, "fn main() void { var s: []f64 = q(); }");
    expect(d.found);
    expect(str_eq(d.word, "type-name"));
    var d1: Det = eh_detect(a, "fn main() void { var s: []i32 = q(); var t: []usize = q(); var w: []bool = q(); }");
    expect(!d1.found);
    var d2: Det = eh_detect(a, "fn main() void { var p: *i32 = q(); }");
    expect(d2.found);
    expect(str_eq(d2.word, "type-form"));
    var d3: Det = eh_detect(a, "fn main() f64 { return q(); }");
    expect(d3.found);
    expect(str_eq(d3.word, "type-name"));
    var d4: Det = eh_detect(a, "fn main() ?i32 { return q(); }");
    expect(d4.found);
    expect(str_eq(d4.word, "type-form"));
}

test "detect: field access is in (v0.169), method calls stay out" {
    var a: Allocator = c_allocator();
    // Any field NAME is a subset shape now — `s.ptr` on a slice is
    // sema-invalid (E0165 territory), not a skip.
    var d: Det = eh_detect(a, "pub fn main() void { var s: []u8 = \"x\"; print(s.ptr); }");
    expect(!d.found);
    var d2: Det = eh_detect(a, "pub fn main() void { print(a.b.len); }");
    expect(!d2.found);
    // A method CALL keeps its verdict.
    var d3: Det = eh_detect(a, "pub fn main() void { p.dist(1); }");
    expect(d3.found);
    expect(str_eq(d3.word, "method-call"));
}

test "detect: out-of-subset statements" {
    var a: Allocator = c_allocator();
    // (unlabeled `for` joined the subset in v0.168; the labeled form
    // keeps the verdict, like the labeled while below)
    var d: Det = eh_detect(a, "fn main() void { lab: for (xs) |x| { break :lab; } }");
    expect(d.found);
    expect(str_eq(d.word, "label"));
    expect(d.pos == 17);
    var d2: Det = eh_detect(a, "fn main() void { switch (x) { else => {} } }");
    expect(d2.found);
    expect(str_eq(d2.word, "switch"));
    expect(d2.pos == 17);
    var d3: Det = eh_detect(a, "fn main() void { errdefer print(1); }");
    expect(d3.found);
    expect(str_eq(d3.word, "errdefer"));
    var d4: Det = eh_detect(a, "fn main() void { lab: while (true) { break :lab; } }");
    expect(d4.found);
    expect(str_eq(d4.word, "label"));
    // (`s.f = 1;` joined the subset in v0.169 — field places are in; a
    // place rooted in a CALL is still out)
    var d5: Det = eh_detect(a, "fn main() void { f().x = 1; }");
    expect(d5.found);
    expect(str_eq(d5.word, "place-assign"));
}

test "detect: out-of-subset items and parameters" {
    var a: Allocator = c_allocator();
    // `test` blocks are subset items since v0.166 — their bodies are
    // walked like any block (a float inside still skips).
    var d: Det = eh_detect(a, "pub fn main() void {}\ntest \"t\" { expect(true); }");
    expect(!d.found);
    var d0: Det = eh_detect(a, "pub fn main() void {}\ntest \"t\" { print(1.5); }");
    expect(d0.found);
    expect(str_eq(d0.word, "float"));
    var d2: Det = eh_detect(a, "fn id(comptime T: type, x: i64) i64 { return x; }\npub fn main() void { print(id(i64, 1)); }");
    expect(d2.found);
    expect(str_eq(d2.word, "generic-param"));
    expect(d2.pos == 6);
    var d3: Det = eh_detect(a, "pub fn main() void {}\n@import(\"other.ks\");");
    expect(d3.found);
    expect(str_eq(d3.word, "import"));
    // A plain data-struct declaration is a subset item (v0.169)...
    var d4: Det = eh_detect(a, "pub fn main() void {}\nconst S = struct { x: i32 };");
    expect(!d4.found);
    // ...a method inside one is the finding; a non-subset FIELD type too.
    var d5: Det = eh_detect(a, "pub fn main() void {}\nconst S = struct { x: i32, fn m(self: S) void {} };");
    expect(d5.found);
    expect(str_eq(d5.word, "method"));
    var d6: Det = eh_detect(a, "pub fn main() void {}\nconst S = struct { x: f64 };");
    expect(d6.found);
    expect(str_eq(d6.word, "type-name"));
}

test "detect: the nomain gate is Program-mode only (v0.166)" {
    var a: Allocator = c_allocator();
    var s: []u8 = "fn helper() i64 { return 1; }\ntest \"t\" { expect(helper() == 1); }";
    var p: Parser = eh_parse(a, s);
    var dp: Det = es_detect_mode(s, p.nodes, p.root, true);
    expect(dp.found);
    expect(str_eq(dp.word, "nomain"));
    var dt: Det = es_detect_mode(s, p.nodes, p.root, false);
    expect(!dt.found);
}

test "detect: allocator builtins and deep expressions" {
    var a: Allocator = c_allocator();
    // The full allocator round-trip is IN the subset (v0.163).
    var d: Det = eh_detect(a, "pub fn main() void {\n    var al: Allocator = c_allocator();\n    var s: []u8 = alloc(al, u8, 3);\n    s[0] = 1;\n    s[1] += 2;\n    free(al, s);\n}");
    expect(!d.found);
    // A mis-shaped `alloc` is out: wrong arity or a non-`u8` element.
    //                            0         1         2         3
    //                            0123456789012345678901234567890123456
    var d2: Det = eh_detect(a, "fn main() void { var s = alloc(q, u8); }");
    expect(d2.found);
    expect(str_eq(d2.word, "builtin-call"));
    expect(d2.pos == 25);
    var d3: Det = eh_detect(a, "fn main() void { var s = alloc(q, f64, 3); }");
    expect(d3.found);
    expect(str_eq(d3.word, "builtin-call"));
    // The walk reaches into defer bodies, continue-clauses and nested calls.
    var d4: Det = eh_detect(a, "fn main() void { defer { print(g(1.25)); } }");
    expect(d4.found);
    expect(str_eq(d4.word, "float"));
    var d5: Det = eh_detect(a, "fn main() void { var o = q(); if (o) |v| { } }");
    expect(d5.found);
    expect(str_eq(d5.word, "capture"));
}

test "detect: place chains in (v0.169), non-name roots out" {
    var a: Allocator = c_allocator();
    var d: Det = eh_detect(a, "pub fn main() void { var s: []u8 = \"ab\"; s[0] = 1; s[1] *= 2; }");
    expect(!d.found);
    // Chains THROUGH an index (the `_at` lowering) joined in v0.169 —
    // these are sema's E0165/E0220 territory now, not skips.
    var d2: Det = eh_detect(a, "pub fn main() void { var s: []u8 = \"ab\"; s[0].f = 1; }");
    expect(!d2.found);
    var d3: Det = eh_detect(a, "pub fn main() void { var s: []u8 = \"ab\"; s[0][1] = 1; }");
    expect(!d3.found);
    // A deref place (pointers) and a call-rooted place stay out.
    var d4: Det = eh_detect(a, "pub fn main() void { p.* = 1; }");
    expect(d4.found);
    expect(str_eq(d4.word, "place-assign"));
    var d5: Det = eh_detect(a, "pub fn main() void { g()[0] = 1; }");
    expect(d5.found);
    expect(str_eq(d5.word, "place-assign"));
    // Out-of-subset constructs inside an admissible write still surface.
    var d6: Det = eh_detect(a, "pub fn main() void { var s: []u8 = \"ab\"; s[0] = 1.5; }");
    expect(d6.found);
    expect(str_eq(d6.word, "float"));
}

// --- const folding -------------------------------------------------------------------

test "fold: comptime arithmetic, comparisons, shifts" {
    var a: Allocator = c_allocator();
    var c: []u8 = eh_emit(a, "pub fn main() void { print(comptime (3 * 4 + 1)); print(comptime (10 / 3)); print(comptime (10 % 3)); }");
    expect(eh_find(c, "kd_print((long long)(13));"));
    expect(eh_find(c, "kd_print((long long)(3));"));
    expect(eh_find(c, "kd_print((long long)(1));"));
    // Shift amounts mask to 0..63 (`5 << 64` folds to 5, not UB).
    var c2: []u8 = eh_emit(a, "pub fn main() void { print(comptime (5 << 64)); print(comptime (1 << 6)); }");
    expect(eh_find(c2, "kd_print((long long)(5));"));
    expect(eh_find(c2, "kd_print((long long)(64));"));
}

test "fold: a failing comptime falls back to expression lowering" {
    var a: Allocator = c_allocator();
    // Division by zero cannot fold; the Rust emitter falls back to the raw
    // expression (such a program is sema-rejected upstream, but emission is
    // total and this arm is pinned here).
    var c: []u8 = eh_emit(a, "pub fn main() void { print(comptime (1 / 0)); }");
    expect(eh_find(c, "kd_print((long long)((1 / 0)));"));
    // A call is not a compile-time constant either.
    var c2: []u8 = eh_emit(a, "fn g() i64 { return 1; }\npub fn main() void { print(comptime (g() + 1)); }");
    expect(eh_find(c2, "kd_print((long long)((kd_g() + 1)));"));
}

test "fold: top-level consts in source order, failures skipped" {
    var a: Allocator = c_allocator();
    var c: []u8 = eh_emit(a, "const A: i64 = comptime (2 + 3);\nconst B = A * 2;\nconst F = B > 9;\nconst BAD = later + 1;\nconst later: i64 = 1;\npub fn main() void { print(A + B); }");
    expect(eh_find(c, "static const int64_t kd_A = 5;\n"));
    // An unannotated integer const infers i64, a bool const infers bool.
    expect(eh_find(c, "static const int64_t kd_B = 10;\n"));
    expect(eh_find(c, "static const bool kd_F = true;\n"));
    // A forward reference cannot fold: the const is skipped, not emitted.
    expect(!eh_find(c, "kd_BAD"));
    expect(eh_find(c, "static const int64_t kd_later = 1;\n"));
    var c2: []u8 = eh_emit(a, "const M: i32 = 100;\npub fn main() void { print(M); }");
    expect(eh_find(c2, "static const int32_t kd_M = 100;\n"));
}

// --- inference ------------------------------------------------------------------------

test "inference: literal defaults, locals, and the const quirk" {
    var a: Allocator = c_allocator();
    var c: []u8 = eh_emit(a, "const K: i32 = 9;\nfn gi() i32 { return 3; }\npub fn main() void {\n    var x = 5;\n    var b = !true;\n    var m = -x;\n    var q = K;\n    var u = gi();\n    var w: i32 = 7;\n    var y = w + 1;\n    print(x); print(m); print(q); print(u); print(y);\n    if (b) { print(0); }\n}");
    // A bare integer literal infers i64.
    expect(eh_find(c, "int64_t kd_x = 5;"));
    // `!` yields bool; `-x` keeps x's type.
    expect(eh_find(c, "bool kd_b = (!true);"));
    expect(eh_find(c, "int64_t kd_m = (-kd_x);"));
    // The mirrored Rust quirk: a top-level const name is not a local, so the
    // initializer is un-inferable and falls back to i64 (NOT the const's i32).
    expect(eh_find(c, "int64_t kd_q = kd_K;"));
    // A call infers the collected return type; arithmetic keeps the lhs type.
    expect(eh_find(c, "int32_t kd_u = kd_gi();"));
    expect(eh_find(c, "int32_t kd_y = (kd_w + 1);"));
}

// --- liveness ---------------------------------------------------------------------------

test "liveness: unreachable functions are not declared or defined" {
    var a: Allocator = c_allocator();
    var c: []u8 = eh_emit(a, "fn used(x: i64) i64 { return x + 1; }\nfn dead(x: i64) i64 { return deep(x); }\nfn deep(x: i64) i64 { return x; }\nfn via_defer() void { print(7); }\npub fn main() void {\n    defer via_defer();\n    print(used(1));\n}");
    expect(eh_find(c, "int64_t kd_used(int64_t kd_x);"));
    expect(eh_find(c, "void kd_via_defer(void);"));
    expect(!eh_find(c, "kd_dead"));
    expect(!eh_find(c, "kd_deep"));
    // Mutual recursion stays live through the cycle.
    var c2: []u8 = eh_emit(a, "fn even(n: i64) bool { if (n == 0) { return true; } return odd(n - 1); }\nfn odd(n: i64) bool { if (n == 0) { return false; } return even(n - 1); }\npub fn main() void { if (even(4)) { print(1); } }");
    expect(eh_find(c2, "bool kd_even(int64_t kd_n);"));
    expect(eh_find(c2, "bool kd_odd(int64_t kd_n);"));
}

// --- defer lowering ------------------------------------------------------------------------

test "defer: LIFO at fall-through and the __kd_ret hoist on return" {
    var a: Allocator = c_allocator();
    var c: []u8 = eh_emit(a, "fn f() i32 {\n    defer print(1);\n    defer print(2);\n    return 7;\n}\npub fn main() void { print(f()); }");
    // A non-void return with active defers evaluates into a temp first,
    // then flushes LIFO, then returns the temp.
    expect(eh_find(c, "    int32_t __kd_ret = (7);\n    kd_print((long long)(2));\n    kd_print((long long)(1));\n    return __kd_ret;\n"));
}

test "defer: while continue-clause runs after defers on both loop edges" {
    var a: Allocator = c_allocator();
    var c: []u8 = eh_emit(a, "pub fn main() void {\n    var i: i64 = 0;\n    while (i < 4) : (i = i + 1) {\n        defer print(9);\n        if (i == 1) { continue; }\n        print(i);\n    }\n}");
    // The continue-clause is duplicated: once before `continue;` (after the
    // loop scope's defers) and once at the body's fall-through end.
    expect(eh_count(c, "kd_i = (kd_i + 1);") == 2);
    expect(eh_find(c, "            kd_print((long long)(9));\n            kd_i = (kd_i + 1);\n            continue;\n"));
}

// --- whole-program byte equality ---------------------------------------------------------

test "emit: minimal void-main program, full C bytes" {
    var a: Allocator = c_allocator();
    var c: []u8 = eh_emit(a, "pub fn main() void { print(1); }");
    var sb: StrBuilder = StrBuilder.init(a);
    eh_prelude(a, &sb);
    sb.append(a, "void kd_main(void);\n");
    sb.append(a, "\n");
    sb.append(a, "void kd_main(void) {\n");
    sb.append(a, "    kd_print((long long)(1));\n");
    sb.append(a, "}\n");
    sb.append(a, "\n");
    sb.append(a, "int main(int argc, char **argv){ (void)argc;(void)argv; kd_main(); return 0; }\n");
    var want: []u8 = sb.build(a);
    expect(str_eq(c, want));
    free(a, want);
    sb.deinit(a);
}

test "emit: integer main wires the exit code, params and consts" {
    var a: Allocator = c_allocator();
    var c: []u8 = eh_emit(a, "const N: i64 = 2;\nfn add(x: i32, y: i32) i32 { return x + y; }\npub fn main() i32 { print(N); return add(1, 2); }");
    var sb: StrBuilder = StrBuilder.init(a);
    eh_prelude(a, &sb);
    sb.append(a, "static const int64_t kd_N = 2;\n");
    sb.append(a, "\n");
    sb.append(a, "int32_t kd_add(int32_t kd_x, int32_t kd_y);\n");
    sb.append(a, "int32_t kd_main(void);\n");
    sb.append(a, "\n");
    sb.append(a, "int32_t kd_add(int32_t kd_x, int32_t kd_y) {\n");
    sb.append(a, "    return ((kd_x + kd_y));\n");
    sb.append(a, "}\n");
    sb.append(a, "\n");
    sb.append(a, "int32_t kd_main(void) {\n");
    sb.append(a, "    kd_print((long long)(kd_N));\n");
    sb.append(a, "    return (kd_add(1, 2));\n");
    sb.append(a, "}\n");
    sb.append(a, "\n");
    sb.append(a, "int main(int argc, char **argv){ (void)argc;(void)argv; return (int) kd_main(); }\n");
    var want: []u8 = sb.build(a);
    expect(str_eq(c, want));
    free(a, want);
    sb.deinit(a);
}

// --- strings (v0.162) ------------------------------------------------------------------

test "strings: escape decode + re-encode through emission" {
    var a: Allocator = c_allocator();
    var c: []u8 = eh_emit(a, "pub fn main() void { print(\"a\\nb\\tc\"); print(\"q\\\"w\\\\e\"); print(\"\"); }");
    // `\n`/`\t` decode to bytes and re-encode readably; `.len` is the
    // DECODED byte count (5, not the 7 source characters).
    expect(eh_find(c, "((kd_slice_uint8_t){ .ptr = (uint8_t *)\"a\\nb\\tc\", .len = 5 })"));
    expect(eh_find(c, "((kd_slice_uint8_t){ .ptr = (uint8_t *)\"q\\\"w\\\\e\", .len = 5 })"));
    expect(eh_find(c, "((kd_slice_uint8_t){ .ptr = (uint8_t *)\"\", .len = 0 })"));
}

test "strings: c_string_literal hex escapes and the hex-digit split" {
    var a: Allocator = c_allocator();
    // Bytes that cannot appear readably: 0x07 then 'f' must split the C
    // literal so the escape cannot absorb the digit.
    var sb: StrBuilder = StrBuilder.init(a);
    sb.append_byte(a, 97);
    sb.append_byte(a, 7);
    sb.append(a, "fb");
    var bytes: []u8 = sb.build(a);
    sb.deinit(a);
    var lit: []u8 = es_c_string_literal(a, bytes);
    expect(str_eq(lit, "\"a\\x07\" \"fb\""));
    // A non-hex-digit follower needs no split; a high byte hex-escapes.
    var sb2: StrBuilder = StrBuilder.init(a);
    sb2.append_byte(a, 1);
    sb2.append_byte(a, 122);
    sb2.append_byte(a, 195);
    sb2.append_byte(a, 169);
    var bytes2: []u8 = sb2.build(a);
    sb2.deinit(a);
    var lit2: []u8 = es_c_string_literal(a, bytes2);
    expect(str_eq(lit2, "\"\\x01z\\xc3\\xa9\""));
    // Carriage return stays readable; consecutive escapes never split.
    var sb3: StrBuilder = StrBuilder.init(a);
    sb3.append_byte(a, 13);
    sb3.append_byte(a, 2);
    sb3.append_byte(a, 3);
    var bytes3: []u8 = sb3.build(a);
    sb3.deinit(a);
    var lit3: []u8 = es_c_string_literal(a, bytes3);
    expect(str_eq(lit3, "\"\\r\\x02\\x03\""));
}

test "strings: decode handles all four legal escapes" {
    var a: Allocator = c_allocator();
    // Source text: "x\n\t\\\"y" (12 bytes with quotes) decodes to 6 bytes.
    var srct: []u8 = "\"x\\n\\t\\\\\\\"y\"";
    var bytes: []u8 = es_decode_str(a, srct, 0, srct.len);
    expect(bytes.len == 6);
    expect(bytes[0] == 120);
    expect(bytes[1] == 10);
    expect(bytes[2] == 9);
    expect(bytes[3] == 92);
    expect(bytes[4] == 34);
    expect(bytes[5] == 121);
}

test "strings: slice typedef gating mirrors sema interning" {
    var a: Allocator = c_allocator();
    // No string literal, no []u8 type → no typedef block.
    var c: []u8 = eh_emit(a, "pub fn main() void { print(1); }");
    expect(!eh_find(c, "kd_slice_uint8_t"));
    // A string in a §43.1-DEAD function still interns (sema checks the
    // whole module; typedefs ignore liveness).
    var c2: []u8 = eh_emit(a, "fn dead() void { print(\"never\"); }\npub fn main() void { print(1); }");
    expect(eh_find(c2, "typedef struct { uint8_t *ptr; uintptr_t len; } kd_slice_uint8_t;\n"));
    expect(eh_find(c2, "kd_slice_uint8_t_get"));
    expect(eh_find(c2, "kd_slice_uint8_t_at"));
    expect(eh_find(c2, "kd_slice_uint8_t_alloc"));
    expect(!eh_find(c2, "kd_dead"));
    // A []u8 type annotation alone (no literal) also interns.
    var c3: []u8 = eh_emit(a, "fn id(s: []u8) []u8 { return s; }\npub fn main() void { print(1); }");
    expect(eh_find(c3, "typedef struct { uint8_t *ptr; uintptr_t len; } kd_slice_uint8_t;\n"));
}

test "strings: print hoists into __kd_strN, counter resets per function" {
    var a: Allocator = c_allocator();
    var c: []u8 = eh_emit(a, "fn f() void { print(\"a\"); print(\"b\"); }\npub fn main() void { f(); print(\"c\"); }");
    // Two hoists in f → __kd_str0 and __kd_str1; main resets → __kd_str0.
    expect(eh_find(c, "{ kd_slice_uint8_t __kd_str0 = (((kd_slice_uint8_t){ .ptr = (uint8_t *)\"a\", .len = 1 })); fwrite(__kd_str0.ptr, 1, __kd_str0.len, stdout); fputc('\\n', stdout); };"));
    expect(eh_find(c, "__kd_str1 = (((kd_slice_uint8_t){ .ptr = (uint8_t *)\"b\", .len = 1 }))"));
    expect(eh_find(c, "__kd_str0 = (((kd_slice_uint8_t){ .ptr = (uint8_t *)\"c\", .len = 1 }))"));
    expect(!eh_find(c, "__kd_str2"));
}

test "strings: .len, s[i] and u8 lowering with truncate-back" {
    var a: Allocator = c_allocator();
    var c: []u8 = eh_emit(a, "pub fn main() void {\n    var s: []u8 = \"kz\";\n    var n = s.len;\n    var c: u8 = s[0];\n    print(n);\n    print(s[s.len - 1]);\n    print(~c);\n    print(c << 1);\n    var d = c - 32;\n    print(d);\n}");
    // `.len` infers usize; the read index goes through the `_get` helper.
    expect(eh_find(c, "uintptr_t kd_n = (kd_s).len;"));
    expect(eh_find(c, "uint8_t kd_c = kd_slice_uint8_t_get(kd_s, 0);"));
    expect(eh_find(c, "kd_print((long long)(kd_slice_uint8_t_get(kd_s, ((kd_s).len - 1))));"));
    // §28.2: `~`/`<<` over a u8 operand truncate back through a cast.
    expect(eh_find(c, "kd_print((long long)(((uint8_t)(~kd_c))));"));
    expect(eh_find(c, "kd_print((long long)(((uint8_t)(kd_c << 1))));"));
    // Ordinary u8 arithmetic keeps the operand type but needs no cast.
    expect(eh_find(c, "uint8_t kd_d = (kd_c - 32);"));
}

test "strings: whole-program byte equality with the typedef section" {
    var a: Allocator = c_allocator();
    var c: []u8 = eh_emit(a, "pub fn main() void { print(\"hi\"); }");
    var sb: StrBuilder = StrBuilder.init(a);
    eh_prelude(a, &sb);
    sb.append(a, "typedef struct { uint8_t *ptr; uintptr_t len; } kd_slice_uint8_t;\n");
    sb.append(a, "static inline uint8_t kd_slice_uint8_t_get(kd_slice_uint8_t s, int64_t i) { if (i < 0 || (uint64_t)i >= s.len) { fputs(\"panic: slice index out of bounds\\n\", stderr); exit(101); } return s.ptr[i]; }\n");
    sb.append(a, "static inline uint8_t *kd_slice_uint8_t_at(kd_slice_uint8_t s, int64_t i) { if (i < 0 || (uint64_t)i >= s.len) { fputs(\"panic: slice index out of bounds\\n\", stderr); exit(101); } return s.ptr + i; }\n");
    sb.append(a, "static inline kd_slice_uint8_t kd_slice_uint8_t_alloc(uintptr_t n) { kd_slice_uint8_t s; s.ptr = malloc(n * sizeof(uint8_t)); if (!s.ptr && n != 0) { fputs(\"panic: out of memory\\n\", stderr); exit(101); } s.len = n; return s; }\n");
    sb.append(a, "\n");
    sb.append(a, "void kd_main(void);\n");
    sb.append(a, "\n");
    sb.append(a, "void kd_main(void) {\n");
    sb.append(a, "    { kd_slice_uint8_t __kd_str0 = (((kd_slice_uint8_t){ .ptr = (uint8_t *)\"hi\", .len = 2 })); fwrite(__kd_str0.ptr, 1, __kd_str0.len, stdout); fputc('\\n', stdout); };\n");
    sb.append(a, "}\n");
    sb.append(a, "\n");
    sb.append(a, "int main(int argc, char **argv){ (void)argc;(void)argv; kd_main(); return 0; }\n");
    var want: []u8 = sb.build(a);
    expect(str_eq(c, want));
    free(a, want);
    sb.deinit(a);
}

// --- generalized []T slices + @as (v0.164) -------------------------------------------------

test "slices: the type-code family and its C spellings" {
    expect(et_slice_of(ET_U8) == ET_SLICE_U8);
    expect(et_is_slice(ET_SLICE_U8));
    expect(!et_is_slice(ET_U8));
    expect(!et_is_slice(ET_NONE));
    expect(et_slice_elem(et_slice_of(ET_I64)) == ET_I64);
    expect(str_eq(et_slice_c_name(et_slice_of(ET_I32)), "kd_slice_int32_t"));
    expect(str_eq(et_slice_c_name(et_slice_of(ET_I64)), "kd_slice_int64_t"));
    expect(str_eq(et_slice_c_name(et_slice_of(ET_BOOL)), "kd_slice_bool"));
    expect(str_eq(et_slice_c_name(et_slice_of(ET_U8)), "kd_slice_uint8_t"));
    expect(str_eq(et_slice_c_name(et_slice_of(ET_USIZE)), "kd_slice_uintptr_t"));
    expect(str_eq(et_c_name(et_slice_of(ET_I64)), "kd_slice_int64_t"));
    expect(et_is_slice_elem(ET_I32));
    expect(et_is_slice_elem(ET_BOOL));
    expect(!et_is_slice_elem(ET_VOID));
    expect(!et_is_slice_elem(ET_ALLOC));
    expect(!et_is_slice_elem(ET_NONE));
}

test "slices: typedef blocks follow sema's first-intern order" {
    var a: Allocator = c_allocator();
    // Signatures intern before bodies: g's `[]i64` param beats f's body
    // string even though f comes first.
    var c: []u8 = eh_emit(a, "fn f() void { print(\"x\"); }\nfn g(v: []i64) usize { return v.len; }\npub fn main() void { f(); }");
    var i64pos: i64 = eh_pos(c, "} kd_slice_int64_t;");
    var u8pos: i64 = eh_pos(c, "} kd_slice_uint8_t;");
    expect(i64pos >= 0);
    expect(u8pos >= 0);
    expect(i64pos < u8pos);
    // Params left-to-right, then the return type.
    var c2: []u8 = eh_emit(a, "fn h(x: []i32) []i64 { return alloc(c_allocator(), i64, x.len); }\npub fn main() void { }");
    expect(eh_pos(c2, "} kd_slice_int32_t;") < eh_pos(c2, "} kd_slice_int64_t;"));
    // The while continue-clause is checked BEFORE the body...
    var c3: []u8 = eh_emit(a, "fn cnt(x: usize) usize { return x; }\npub fn main() void {\n    var al: Allocator = c_allocator();\n    var i: usize = 0;\n    while (i < 3) : (i += cnt(\"ab\".len)) {\n        var v: []i64 = alloc(al, i64, 1);\n        free(al, v);\n    }\n}");
    expect(eh_pos(c3, "} kd_slice_uint8_t;") < eh_pos(c3, "} kd_slice_int64_t;"));
    // ...and alloc interns its element AFTER the count argument.
    var c4: []u8 = eh_emit(a, "pub fn main() void {\n    var al: Allocator = c_allocator();\n    var q = alloc(al, i64, \"x\".len);\n    free(al, q);\n}");
    expect(eh_pos(c4, "} kd_slice_uint8_t;") < eh_pos(c4, "} kd_slice_int64_t;"));
    // A Let annotation interns BEFORE its initializer's strings.
    var c5: []u8 = eh_emit(a, "pub fn main() void {\n    var al: Allocator = c_allocator();\n    var q: []i64 = alloc(al, i64, \"abc\".len);\n    free(al, q);\n}");
    expect(eh_pos(c5, "} kd_slice_int64_t;") < eh_pos(c5, "} kd_slice_uint8_t;"));
}

test "slices: generalized lowering — typed helpers, writes, inference" {
    var a: Allocator = c_allocator();
    var c: []u8 = eh_emit(a, "pub fn main() void {\n    var al: Allocator = c_allocator();\n    var v: []i64 = alloc(al, i64, 3);\n    v[0] = 7;\n    v[1] += v[0];\n    print(v[1]);\n    print(v.len);\n    var w = alloc(al, bool, 1);\n    w[0] = true;\n    free(al, w);\n    free(al, v);\n}");
    expect(eh_find(c, "kd_slice_int64_t kd_v = kd_slice_int64_t_alloc((uintptr_t)(3));"));
    expect(eh_find(c, "(kd_v).ptr[__kd_idx0] = (7);"));
    expect(eh_find(c, "(kd_v).ptr[__kd_idx1] = (kd_v).ptr[__kd_idx1] + (kd_slice_int64_t_get(kd_v, 0));"));
    expect(eh_find(c, "kd_print((long long)(kd_slice_int64_t_get(kd_v, 1)));"));
    // `var w = alloc(al, bool, 1);` infers `[]bool`.
    expect(eh_find(c, "kd_slice_bool kd_w = kd_slice_bool_alloc((uintptr_t)(1));"));
    expect(eh_find(c, "(kd_w).ptr[__kd_idx2] = (true);"));
}

test "casts: @as lowering and its result type" {
    var a: Allocator = c_allocator();
    var c: []u8 = eh_emit(a, "pub fn main() void {\n    var i: usize = 200;\n    var b: u8 = @as(u8, i);\n    var w = @as(i64, b) * 3;\n    print(w);\n    print(@as(i32, w));\n}");
    expect(eh_find(c, "uint8_t kd_b = ((uint8_t)(kd_i));"));
    // `@as(i64, b) * 3` infers i64 through the cast's target type.
    expect(eh_find(c, "int64_t kd_w = (((int64_t)(kd_b)) * 3);"));
    expect(eh_find(c, "kd_print((long long)(((int32_t)(kd_w))));"));
}

// --- import resolution (v0.167) ---------------------------------------------------------------

test "modres: lexical path normalization mirrors the canonical keys" {
    var a: Allocator = c_allocator();
    expect(str_eq(mr_normalize(a, "a/b/c.ks"), "a/b/c.ks"));
    expect(str_eq(mr_normalize(a, "a//b/./c.ks"), "a/b/c.ks"));
    expect(str_eq(mr_normalize(a, "a/b/../c.ks"), "a/c.ks"));
    expect(str_eq(mr_normalize(a, "tests/selfhost/../../selfhost/lexer.ks"), "selfhost/lexer.ks"));
    // `..` with nothing to pop is kept, so relative paths may escape.
    expect(str_eq(mr_normalize(a, "../x.ks"), "../x.ks"));
    expect(str_eq(mr_normalize(a, "a/../../x.ks"), "../x.ks"));
    // A leading `/` (absolute) is preserved.
    expect(str_eq(mr_normalize(a, "/r/a/../b.ks"), "/r/b.ks"));
    expect(str_eq(mr_normalize(a, "./x.ks"), "x.ks"));
}

test "modres: dir_of and basename split on the last separator" {
    var a: Allocator = c_allocator();
    expect(str_eq(mr_dir_of(a, "a/b/c.ks"), "a/b/"));
    expect(str_eq(mr_dir_of(a, "c.ks"), ""));
    expect(str_eq(mr_dir_of(a, "/c.ks"), "/"));
    expect(str_eq(mr_basename("a/b/std"), "std"));
    expect(str_eq(mr_basename("std.ks"), "std.ks"));
    expect(str_eq(mr_basename("a/x.ks"), "x.ks"));
}

// --- test blocks + EmitMode::Test (v0.166) ---------------------------------------------------

test "harness: c_escape keeps names readable, no hex escapes" {
    var a: Allocator = c_allocator();
    var sb: StrBuilder = StrBuilder.init(a);
    sb.append(a, "a\"b\\c");
    sb.append_byte(a, 10);
    sb.append_byte(a, 9);
    sb.append_byte(a, 13);
    sb.append_byte(a, 7);
    var raw: []u8 = sb.build(a);
    sb.deinit(a);
    var esc: []u8 = es_c_escape(a, raw);
    // \ " \n \t \r escaped; the raw 0x07 byte passes through VERBATIM
    // (unlike c_string_literal's \x07).
    var want: StrBuilder = StrBuilder.init(a);
    want.append(a, "a\\\"b\\\\c\\n\\t\\r");
    want.append_byte(a, 7);
    var w: []u8 = want.build(a);
    want.deinit(a);
    expect(str_eq(esc, w));
}

test "harness: expect lowering, tables, and the driver main" {
    var a: Allocator = c_allocator();
    var c: []u8 = eh_emit_test(a, "fn add(x: i64, y: i64) i64 { return x + y; }\ntest \"adds\" {\n    expect(add(1, 2) == 3);\n}\ntest \"quote \\\" name\" {\n    defer print(9);\n    expect(true);\n}");
    // `expect(c)` → the failure-return block, flushing active defers.
    expect(eh_find(c, "static int kd_test_0(void) {\n    if (!((kd_add(1, 2) == 3))) {\n        return 1;\n    }\n    return 0;\n}\n"));
    // The second test's expect flushes its registered defer FIRST.
    expect(eh_find(c, "    if (!(true)) {\n        kd_print((long long)(9));\n        return 1;\n    }\n    kd_print((long long)(9));\n    return 0;\n}\n"));
    // The name table c_escapes the DECODED name; the fn table indexes.
    expect(eh_find(c, "static const char *kd_test_names[] = { \"adds\", \"quote \\\" name\" };\n"));
    expect(eh_find(c, "static int (*kd_test_fns[])(void) = { kd_test_0, kd_test_1 };\n"));
    // The v0.150 driver main: filter/bench parsing, the run loop, the tally.
    expect(eh_find(c, "int main(int argc, char **argv) {\n    const char *filter = 0; int bench = 0;\n"));
    expect(eh_find(c, "if (strcmp(argv[ai], \"--bench\") == 0) { bench = 1; }"));
    expect(eh_find(c, "if (filter && !strstr(kd_test_names[ti], filter)) { continue; }"));
    expect(eh_find(c, "fprintf(stderr, \"%s: %s\\n\", rc == 0 ? \"ok\" : \"FAIL\", kd_test_names[ti]);"));
    expect(eh_find(c, "fprintf(stderr, \"%d/%d tests passed%s\\n\", ran - failures, ran, filter ? \" (filtered)\" : \"\");"));
    // No user main is wired in Test mode.
    expect(!eh_find(c, "kd_main()"));
}

test "harness: no tests means the trivial driver and EVERY function live" {
    var a: Allocator = c_allocator();
    var c: []u8 = eh_emit_test(a, "fn unreferenced(x: i64) i64 { return x; }\nfn also_dead() void { print(1); }\npub fn main() void { print(2); }");
    // The no-root fallback (`LiveFns::all_of`): every fn declared+defined,
    // including ones Program mode would drop.
    expect(eh_find(c, "int64_t kd_unreferenced(int64_t kd_x);"));
    expect(eh_find(c, "void kd_also_dead(void);"));
    expect(eh_find(c, "void kd_main(void);"));
    // No tables; `int total = 0;`; no run loop; the tally still prints.
    expect(!eh_find(c, "kd_test_names"));
    expect(eh_find(c, "int total = 0;\n    int failures = 0; int ran = 0;\n    fprintf(stderr,"));
    // Program mode over the same source drops the dead functions.
    var cp: []u8 = eh_emit(a, "fn unreferenced(x: i64) i64 { return x; }\nfn also_dead() void { print(1); }\npub fn main() void { print(2); }");
    expect(!eh_find(cp, "kd_unreferenced"));
    expect(!eh_find(cp, "kd_also_dead"));
}

test "harness: test-body liveness roots and the str_count quirk" {
    var a: Allocator = c_allocator();
    var c: []u8 = eh_emit_test(a, "fn used_by_test(x: i64) i64 { return x; }\nfn never_called() void { print(0); }\nfn stringer() void { print(\"a\"); print(\"b\"); }\ntest \"one\" {\n    print(\"t1\");\n    expect(used_by_test(1) == 1);\n}\ntest \"two\" {\n    print(\"t2\");\n    stringer();\n}");
    // Liveness roots at the test bodies: `never_called` is dropped.
    expect(eh_find(c, "int64_t kd_used_by_test(int64_t kd_x);"));
    expect(eh_find(c, "void kd_stringer(void);"));
    expect(!eh_find(c, "kd_never_called"));
    // The Rust emit_test_fn does NOT reset str_counter (mirrored quirk):
    // stringer's fn body uses __kd_str0/__kd_str1 (its own reset), test
    // "one" CONTINUES from the last emitted fn (__kd_str2), test "two"
    // continues again (__kd_str3).
    expect(eh_find(c, "void kd_stringer(void) {\n    { kd_slice_uint8_t __kd_str0"));
    expect(eh_find(c, "static int kd_test_0(void) {\n    { kd_slice_uint8_t __kd_str2"));
    expect(eh_find(c, "static int kd_test_1(void) {\n    { kd_slice_uint8_t __kd_str3"));
}

// --- the slicing view (v0.165) --------------------------------------------------------------

test "slicing: detector admits base/lo/hi, walks them for skips" {
    var a: Allocator = c_allocator();
    var d: Det = eh_detect(a, "pub fn main() void { var s: []u8 = \"abcd\"; print(s[1..3]); }");
    expect(!d.found);
    // Out-of-subset constructs inside the operands still surface.
    var d2: Det = eh_detect(a, "pub fn main() void { var s: []u8 = \"abcd\"; print(s[1..g(1.5)]); }");
    expect(d2.found);
    expect(str_eq(d2.word, "float"));
}

test "slicing: the bounds-checked view lowering, re-spliced operands" {
    var a: Allocator = c_allocator();
    var c: []u8 = eh_emit(a, "pub fn main() void {\n    var s: []u8 = \"abcdef\";\n    var t: []u8 = s[1..4];\n    print(t);\n    print(t.len);\n}");
    // The exact ternary: lo/hi/base re-spliced, `_Noreturn` failing branch,
    // `{ptr, len}` success branch.
    expect(eh_find(c, "kd_slice_uint8_t kd_t = (( (1) < 0 || (4) < (1) || (4) > ((kd_s).len) ) ? (fputs(\"panic: slice bounds out of range\\n\", stderr), exit(101), (kd_slice_uint8_t){0}) : (kd_slice_uint8_t){ .ptr = (kd_s).ptr + (1), .len = (4) - (1) });"));
    // The view's type is the base's slice type: `var t` infers `[]u8` and a
    // non-u8 base keeps its own helper family.
    var c2: []u8 = eh_emit(a, "pub fn main() void {\n    var al: Allocator = c_allocator();\n    var q: []i64 = alloc(al, i64, 4);\n    var v = q[1..3];\n    print(v[0]);\n    free(al, q);\n}");
    expect(eh_find(c2, "kd_slice_int64_t kd_v = (( (1) < 0 ||"));
    expect(eh_find(c2, "(kd_slice_int64_t){ .ptr = (kd_q).ptr + (1), .len = (3) - (1) });"));
    expect(eh_find(c2, "kd_print((long long)(kd_slice_int64_t_get(kd_v, 0)));"));
}

// --- index writes + allocator builtins (v0.163) -------------------------------------------

test "heap: alloc, free and c_allocator lowering shapes" {
    var a: Allocator = c_allocator();
    var c: []u8 = eh_emit(a, "fn take(al: Allocator, n: usize) []u8 { return alloc(al, u8, n + 1); }\npub fn main() void {\n    var al: Allocator = c_allocator();\n    var s: []u8 = take(al, 3);\n    free(al, s);\n}");
    // `Allocator` spells `kd_allocator` in params and locals; the builtins
    // lower to the slice helper / a compound literal / a raw `free`.
    expect(eh_find(c, "kd_slice_uint8_t kd_take(kd_allocator kd_al, uintptr_t kd_n);"));
    expect(eh_find(c, "    return (kd_slice_uint8_t_alloc((uintptr_t)((kd_n + 1))));\n"));
    expect(eh_find(c, "    kd_allocator kd_al = ((kd_allocator){0});\n"));
    expect(eh_find(c, "    free((kd_s).ptr);\n"));
    // The allocator ARGUMENT of alloc/free is accepted but never emitted.
    expect(!eh_find(c, "kd_slice_uint8_t_alloc(kd_al"));
    // `alloc(a, u8, n)` alone interns `[]u8`: the typedef block appears.
    expect(eh_find(c, "typedef struct { uint8_t *ptr; uintptr_t len; } kd_slice_uint8_t;\n"));
    // `var s = c_allocator();` infers the Allocator type.
    var c2: []u8 = eh_emit(a, "pub fn main() void { var x = c_allocator(); free(x, alloc(x, u8, 1)); }");
    expect(eh_find(c2, "    kd_allocator kd_x = ((kd_allocator){0});\n"));
    // ...and `var s = alloc(a, u8, n);` infers `[]u8`.
    var c3: []u8 = eh_emit(a, "pub fn main() void {\n    var al: Allocator = c_allocator();\n    var s = alloc(al, u8, 2);\n    print(s.len);\n}");
    expect(eh_find(c3, "    kd_slice_uint8_t kd_s = kd_slice_uint8_t_alloc((uintptr_t)(2));\n"));
}

test "heap: index writes hoist __kd_idx, compound re-spells the slot" {
    var a: Allocator = c_allocator();
    var c: []u8 = eh_emit(a, "pub fn main() void {\n    var al: Allocator = c_allocator();\n    var s: []u8 = alloc(al, u8, 4);\n    s[0] = 65;\n    s[s.len - 1] += 2;\n    free(al, s);\n}");
    // Plain write: hoist, bounds-check against the runtime `.len`, store.
    expect(eh_find(c, "    { int64_t __kd_idx0 = (0); if (__kd_idx0 < 0 || (uint64_t)__kd_idx0 >= (kd_s).len) { fputs(\"panic: slice index out of bounds\\n\", stderr); exit(101); } (kd_s).ptr[__kd_idx0] = (65); }\n"));
    // Compound write: ONE index evaluation, the slot re-spelled both sides.
    expect(eh_find(c, "    { int64_t __kd_idx1 = (((kd_s).len - 1)); if (__kd_idx1 < 0 || (uint64_t)__kd_idx1 >= (kd_s).len) { fputs(\"panic: slice index out of bounds\\n\", stderr); exit(101); } (kd_s).ptr[__kd_idx1] = (kd_s).ptr[__kd_idx1] + (2); }\n"));
    // The counter resets per function.
    var c2: []u8 = eh_emit(a, "fn f(s: []u8) void { s[0] = 1; s[1] = 2; }\nfn g(s: []u8) void { s[2] = 3; }\npub fn main() void {\n    var al: Allocator = c_allocator();\n    var s: []u8 = alloc(al, u8, 4);\n    f(s);\n    g(s);\n    free(al, s);\n}");
    expect(eh_count(c2, "__kd_idx0") > 0);
    expect(eh_find(c2, "__kd_idx1"));
    expect(!eh_find(c2, "__kd_idx2"));
}

test "emit: if/else ladder, bare block, expression statement shapes" {
    var a: Allocator = c_allocator();
    var c: []u8 = eh_emit(a, "pub fn main() void {\n    var x: i64 = 3;\n    if (x == 1) {\n        print(10);\n    } else if (x == 2) {\n        print(20);\n    } else {\n        print(30);\n    }\n    {\n        var t: i64 = 5;\n        print(t);\n    }\n    x = x + 1;\n    x += 2;\n}");
    expect(eh_find(c, "    if ((kd_x == 1)) {\n        kd_print((long long)(10));\n    } else if ((kd_x == 2)) {\n        kd_print((long long)(20));\n    } else {\n        kd_print((long long)(30));\n    }\n"));
    expect(eh_find(c, "    {\n        int64_t kd_t = 5;\n        kd_print((long long)(kd_t));\n    }\n"));
    expect(eh_find(c, "    kd_x = (kd_x + 1);\n"));
    expect(eh_find(c, "    kd_x = kd_x + (2);\n"));
}

test "detect: arrays and for are in the subset (v0.168)" {
    var a: Allocator = c_allocator();
    // Fixed arrays, array literals, and both `for` capture forms are in.
    var d: Det = eh_detect(a, "pub fn main() void {\n    var xs: [3]i64 = [3]i64{ 1, 2, 3 };\n    for (xs) |x| { print(x); }\n    for (xs, 0..) |x, i| { print(x); print(i); }\n}");
    expect(!d.found);
    // A comptime-parameter size `[n]T` stays a type-form skip...
    var d2: Det = eh_detect(a, "fn f(xs: [n]i64) void {}\npub fn main() void {}");
    expect(d2.found);
    expect(str_eq(d2.word, "type-form"));
    expect(d2.pos == 9);
    // ...and a non-scalar element is a type-name skip (the LET annotation
    // walks before its initializer).
    var d3: Det = eh_detect(a, "pub fn main() void { var xs: [2]f64 = [2]f64{ 1.5, 2.5 }; }");
    expect(d3.found);
    expect(str_eq(d3.word, "type-name"));
    expect(d3.pos == 29);
}

test "arrays: typedefs before slices, storage max(len,1), get shape" {
    var a: Allocator = c_allocator();
    var c: []u8 = eh_emit(a, "pub fn main() void {\n    var xs: [3]i64 = [3]i64{ 1, 2, 3 };\n    var e: [0]u8 = [0]u8{};\n    print(xs[0]);\n    print(e.len);\n    var v: []i64 = xs[0..2];\n    print(v.len);\n}");
    // Array typedef + the bounds-checked value getter.
    expect(eh_find(c, "typedef struct { int64_t data[3]; } kd_arr_int64_t_3;\n"));
    expect(eh_find(c, "static inline int64_t kd_arr_int64_t_3_get(kd_arr_int64_t_3 a, int64_t i) { if (i < 0 || (uint64_t)i >= 3) { fputs(\"panic: array index out of bounds\\n\", stderr); exit(101); } return a.data[i]; }\n"));
    // A zero-length array keeps one storage slot but a 0 bound.
    expect(eh_find(c, "typedef struct { uint8_t data[1]; } kd_arr_uint8_t_0;\n"));
    expect(eh_find(c, "(uint64_t)i >= 0"));
    // Dependency order: every array typedef precedes the first slice's
    // (the last array's `_at` line is directly followed by it).
    expect(eh_find(c, "return (uint8_t *)a->data + i; }\ntypedef struct { int64_t *ptr; uintptr_t len; } kd_slice_int64_t;\n"));
}

test "arrays: literal, get read, .len constant, index writes, empty" {
    var a: Allocator = c_allocator();
    var c: []u8 = eh_emit(a, "pub fn main() void {\n    var xs: [3]i64 = [3]i64{ 1, 2, 3 };\n    xs[0] = 5;\n    xs[1] += 2;\n    print(xs[1]);\n    print(xs.len);\n    var e: [0]u8 = [0]u8{};\n    print(e.len);\n}");
    // The literal is a compound literal with const-folded elements.
    expect(eh_find(c, "    kd_arr_int64_t_3 kd_xs = ((kd_arr_int64_t_3){ .data = { 1, 2, 3 } });\n"));
    // Reads go through the checked getter; `.len` folds to the count.
    expect(eh_find(c, "kd_arr_int64_t_3_get(kd_xs, 1)"));
    expect(eh_find(c, "((uintptr_t)3)"));
    // Index writes hoist __kd_idx and bound against the CONSTANT length;
    // the compound form re-spells the `.data` slot on both sides.
    expect(eh_find(c, "    { int64_t __kd_idx0 = (0); if (__kd_idx0 < 0 || (uint64_t)__kd_idx0 >= 3) { fputs(\"panic: array index out of bounds\\n\", stderr); exit(101); } (kd_xs).data[__kd_idx0] = (5); }\n"));
    expect(eh_find(c, "    { int64_t __kd_idx1 = (1); if (__kd_idx1 < 0 || (uint64_t)__kd_idx1 >= 3) { fputs(\"panic: array index out of bounds\\n\", stderr); exit(101); } (kd_xs).data[__kd_idx1] = (kd_xs).data[__kd_idx1] + (2); }\n"));
    // The empty literal is the zero-initializer.
    expect(eh_find(c, "((kd_arr_uint8_t_0){0})"));
}

test "for: lowering — iter temp, fi counter, raw continue, index form" {
    var a: Allocator = c_allocator();
    var c: []u8 = eh_emit(a, "pub fn main() void {\n    var xs: [3]i64 = [3]i64{ 1, 2, 3 };\n    for (xs, 0..) |x, i| {\n        if (x == 2) { continue; }\n        print(i);\n    }\n    var al: Allocator = c_allocator();\n    var s: []u8 = alloc(al, u8, 2);\n    for (s) |b| { print(b); }\n    free(al, s);\n}");
    // The whole array-loop block, byte for byte: outer scope, snapshot
    // temp, uintptr counter, CONSTANT bound, elem + index bindings, and
    // `continue` stepping the counter first.
    expect(eh_find(c, "    {\n        kd_arr_int64_t_3 __kd_for0 = kd_xs;\n        uintptr_t __kd_fi0 = 0;\n        while (__kd_fi0 < 3) {\n            int64_t kd_x = __kd_for0.data[__kd_fi0];\n            uintptr_t kd_i = __kd_fi0;\n            if ((kd_x == 2)) {\n                __kd_fi0 += 1;\n                continue;\n            }\n            kd_print((long long)(kd_i));\n            __kd_fi0 += 1;\n        }\n    }\n"));
    // A slice loop bounds on the runtime `.len` and reads through `.ptr`.
    expect(eh_find(c, "        while (__kd_fi1 < __kd_for1.len) {\n            uint8_t kd_b = __kd_for1.ptr[__kd_fi1];\n"));
    // The counter resets per function (and per test fn, like str/idx).
    var c2: []u8 = eh_emit(a, "fn f(xs: [2]i64) void { for (xs) |x| { print(x); } }\nfn g(xs: [2]i64) void { for (xs) |x| { print(x); } }\npub fn main() void {\n    var xs: [2]i64 = [2]i64{ 1, 2 };\n    f(xs);\n    g(xs);\n}");
    expect(eh_count(c2, "__kd_for0 = ") == 2);
    expect(!eh_find(c2, "__kd_for1"));
}

test "harness: for_count resets per test fn too" {
    var a: Allocator = c_allocator();
    var c: []u8 = eh_emit_test(a, "test \"a\" {\n    var xs: [1]i64 = [1]i64{ 7 };\n    for (xs) |x| { print(x); }\n}\ntest \"b\" {\n    var ys: [1]i64 = [1]i64{ 8 };\n    for (ys) |y| { print(y); }\n}");
    expect(eh_count(c, "__kd_for0 = ") == 2);
    expect(!eh_find(c, "__kd_for1"));
}

test "structs: typedef shapes, dependency order, empty struct (v0.169)" {
    var a: Allocator = c_allocator();
    var c: []u8 = eh_emit(a, "const Buf = struct { data: [4]i32, n: i32 };\nconst Empty = struct {};\npub fn main() void {\n    var b: Buf = Buf{ .data = [4]i32{ 0, 0, 0, 0 }, .n = 0 };\n    var e: Empty = Empty{};\n    b.data[1] = 5;\n    print(b.data[1]);\n    var e2 = e;\n}");
    // A struct field list joins with single spaces; fields spell kd_<f>.
    expect(eh_find(c, "typedef struct { kd_arr_int32_t_4 kd_data; int32_t kd_n; } kd_struct_Buf;\n"));
    // The empty struct keeps one char slot (the allocator uses int).
    expect(eh_find(c, "typedef struct { char _unused; } kd_struct_Empty;\n"));
    // The dependency walk emits a struct's ARRAY FIELD dep before it: the
    // array block's `_at` line is directly followed by the struct typedef.
    expect(eh_find(c, "return (int32_t *)a->data + i; }\ntypedef struct { kd_arr_int32_t_4 kd_data; int32_t kd_n; } kd_struct_Buf;\n"));
    // Arrays OF structs and slices OF structs mangle struct_<Name>.
    var c2: []u8 = eh_emit(a, "const Cell = struct { v: i64 };\npub fn main() void {\n    var cs: [2]Cell = [2]Cell{ Cell{ .v = 1 }, Cell{ .v = 2 } };\n    var sl: []Cell = cs[0..1];\n    print(sl.len);\n    print(cs[1].v);\n}");
    expect(eh_find(c2, "typedef struct { kd_struct_Cell data[2]; } kd_arr_struct_Cell_2;\n"));
    expect(eh_find(c2, "typedef struct { kd_struct_Cell *ptr; uintptr_t len; } kd_slice_struct_Cell;\n"));
}

test "structs: literals, field reads/writes, aggregate copies" {
    var a: Allocator = c_allocator();
    var c: []u8 = eh_emit(a, "const Point = struct { x: i64, y: i64 };\npub fn main() void {\n    var p: Point = Point{ .x = 1, .y = 2 };\n    var q: Point = p;\n    q.x = 100;\n    q.y += 3;\n    print(p.x);\n    print(q.y);\n}");
    // The literal is a designated compound literal in SOURCE order.
    expect(eh_find(c, "    kd_struct_Point kd_p = ((kd_struct_Point){ .kd_x = 1, .kd_y = 2 });\n"));
    // An aggregate copy is a plain C assignment.
    expect(eh_find(c, "    kd_struct_Point kd_q = kd_p;\n"));
    // Plain field write; compound re-spells the place on both sides.
    expect(eh_find(c, "    ((kd_q).kd_x) = (100);\n"));
    expect(eh_find(c, "    ((kd_q).kd_y) = ((kd_q).kd_y) + (3);\n"));
    // Field reads parenthesize the base.
    expect(eh_find(c, "kd_print((long long)((kd_p).kd_x));\n"));
    // Nested field-chain write: parens nest per step.
    var c2: []u8 = eh_emit(a, "const In = struct { n: i64 };\nconst Out = struct { i: In };\npub fn main() void {\n    var o: Out = Out{ .i = In{ .n = 0 } };\n    o.i.n = 3;\n    print(o.i.n);\n}");
    expect(eh_find(c2, "    (((kd_o).kd_i).kd_n) = (3);\n"));
}

test "structs: place chains through indexes — _at lowering, shared counter" {
    var a: Allocator = c_allocator();
    var c: []u8 = eh_emit(a, "const Cell = struct { v: i64, w: i64 };\npub fn main() void {\n    var cs: [3]Cell = [3]Cell{ Cell{ .v = 1, .w = 0 }, Cell{ .v = 2, .w = 0 }, Cell{ .v = 3, .w = 0 } };\n    var i: i64 = 1;\n    cs[i].v += 10;\n    cs[0].w = cs[i].v;\n    var s: []u8 = \"xy\";\n    s[0] = 65;\n    print(cs[0].w);\n}");
    // Compound through an index hoists the place ADDRESS once (__kd_pl),
    // through the bounds-checked _at element pointer.
    expect(eh_find(c, "    { int64_t *__kd_pl0 = (&(kd_arr_struct_Cell_3_at(&(kd_cs), kd_i)->kd_v)); *__kd_pl0 = *__kd_pl0 + (10); }\n"));
    // A plain chain write spells `at(...)->kd_f`; the RHS reads by value
    // through `_get`.
    expect(eh_find(c, "    (kd_arr_struct_Cell_3_at(&(kd_cs), 0)->kd_w) = ((kd_arr_struct_Cell_3_get(kd_cs, kd_i)).kd_v);\n"));
    // __kd_pl shares the __kd_idx counter: the later direct slice write
    // numbers __kd_idx1.
    expect(eh_find(c, "__kd_idx1"));
    expect(!eh_find(c, "__kd_idx0"));
}

test "structs: an array view through an indexed element spells _at" {
    var a: Allocator = c_allocator();
    var c: []u8 = eh_emit(a, "const B = struct { buf: [3]i64 };\npub fn main() void {\n    var xs: [2]B = [2]B{ B{ .buf = [3]i64{ 1, 2, 3 } }, B{ .buf = [3]i64{ 4, 5, 6 } } };\n    var v: []i64 = xs[0].buf[0..3];\n    v[1] = 99;\n    print(xs[0].buf[1]);\n    print(v[2]);\n}");
    // The view's backing pointer reaches the REAL element storage through
    // `_at` (a `_get` copy would dangle).
    expect(eh_find(c, ".ptr = (kd_arr_struct_B_2_at(&(kd_xs), 0)->kd_buf).data + (0)"));
    // The rvalue read of the same chain uses `_get` + field access.
    expect(eh_find(c, "kd_print((long long)(kd_arr_int64_t_3_get((kd_arr_struct_B_2_get(kd_xs, 0)).kd_buf, 1)));\n"));
}
