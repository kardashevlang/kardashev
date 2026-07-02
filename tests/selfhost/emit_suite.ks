// emit_suite.ks — in-language tests for the self-hosted subset C emitter
// (v0.161).
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

test "type codes: from_name maps the four subset spellings" {
    expect(et_from_name("i32") == ET_I32);
    expect(et_from_name("i64") == ET_I64);
    expect(et_from_name("bool") == ET_BOOL);
    expect(et_from_name("void") == ET_VOID);
    expect(et_from_name("u8") == ET_NONE);
    expect(et_from_name("f64") == ET_NONE);
    expect(et_from_name("Self") == ET_NONE);
    expect(et_from_name("") == ET_NONE);
}

test "type codes: C spellings and is_int" {
    expect(str_eq(et_c_name(ET_I32), "int32_t"));
    expect(str_eq(et_c_name(ET_I64), "int64_t"));
    expect(str_eq(et_c_name(ET_BOOL), "bool"));
    expect(str_eq(et_c_name(ET_VOID), "void"));
    // The defensive fallback spelling mirrors the Rust `cty` fallback.
    expect(str_eq(et_c_name(ET_NONE), "int64_t"));
    expect(et_is_int(ET_I32));
    expect(et_is_int(ET_I64));
    expect(!et_is_int(ET_BOOL));
    expect(!et_is_int(ET_VOID));
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

test "detect: string literal position" {
    var a: Allocator = c_allocator();
    //                          0         1         2
    //                          01234567890123456789012345
    var d: Det = eh_detect(a, "fn main() void { var s = \"x\"; }");
    expect(d.found);
    expect(str_eq(d.word, "string"));
    expect(d.pos == 25);
}

test "detect: composite type forms and non-subset type names" {
    var a: Allocator = c_allocator();
    var d: Det = eh_detect(a, "fn main() void { var s: []u8 = q(); }");
    expect(d.found);
    expect(str_eq(d.word, "type-form"));
    var d2: Det = eh_detect(a, "fn main() void { var c: u8 = q(); }");
    expect(d2.found);
    expect(str_eq(d2.word, "type-name"));
    var d3: Det = eh_detect(a, "fn main() f64 { return q(); }");
    expect(d3.found);
    expect(str_eq(d3.word, "type-name"));
    var d4: Det = eh_detect(a, "fn main() ?i32 { return q(); }");
    expect(d4.found);
    expect(str_eq(d4.word, "type-form"));
}

test "detect: out-of-subset statements" {
    var a: Allocator = c_allocator();
    var d: Det = eh_detect(a, "fn main() void { for (xs) |x| { } }");
    expect(d.found);
    expect(str_eq(d.word, "for"));
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
    var d5: Det = eh_detect(a, "fn main() void { s.f = 1; }");
    expect(d5.found);
    expect(str_eq(d5.word, "place-assign"));
}

test "detect: out-of-subset items and parameters" {
    var a: Allocator = c_allocator();
    var d: Det = eh_detect(a, "pub fn main() void {}\ntest \"t\" { expect(true); }");
    expect(d.found);
    expect(str_eq(d.word, "test"));
    expect(d.pos == 22);
    var d2: Det = eh_detect(a, "fn id(comptime T: type, x: i64) i64 { return x; }\npub fn main() void { print(id(i64, 1)); }");
    expect(d2.found);
    expect(str_eq(d2.word, "generic-param"));
    expect(d2.pos == 6);
    var d3: Det = eh_detect(a, "pub fn main() void {}\n@import(\"other.ks\");");
    expect(d3.found);
    expect(str_eq(d3.word, "import"));
}

test "detect: allocator builtins and deep expressions" {
    var a: Allocator = c_allocator();
    var d: Det = eh_detect(a, "fn main() void { free(q, r); }");
    expect(d.found);
    expect(str_eq(d.word, "builtin-call"));
    expect(d.pos == 17);
    // The walk reaches into defer bodies, continue-clauses and nested calls.
    var d2: Det = eh_detect(a, "fn main() void { defer { print(g(1.25)); } }");
    expect(d2.found);
    expect(str_eq(d2.word, "float"));
    var d3: Det = eh_detect(a, "fn main() void { var o = q(); if (o) |v| { } }");
    expect(d3.found);
    expect(str_eq(d3.word, "capture"));
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

test "emit: if/else ladder, bare block, expression statement shapes" {
    var a: Allocator = c_allocator();
    var c: []u8 = eh_emit(a, "pub fn main() void {\n    var x: i64 = 3;\n    if (x == 1) {\n        print(10);\n    } else if (x == 2) {\n        print(20);\n    } else {\n        print(30);\n    }\n    {\n        var t: i64 = 5;\n        print(t);\n    }\n    x = x + 1;\n    x += 2;\n}");
    expect(eh_find(c, "    if ((kd_x == 1)) {\n        kd_print((long long)(10));\n    } else if ((kd_x == 2)) {\n        kd_print((long long)(20));\n    } else {\n        kd_print((long long)(30));\n    }\n"));
    expect(eh_find(c, "    {\n        int64_t kd_t = 5;\n        kd_print((long long)(kd_t));\n    }\n"));
    expect(eh_find(c, "    kd_x = (kd_x + 1);\n"));
    expect(eh_find(c, "    kd_x = kd_x + (2);\n"));
}
