// emit_suite.ks — in-language tests for the self-hosted subset C emitter
// (v0.161 scalars + v0.162 strings + v0.163 heap buffers + v0.164
// generalized `[]T` slices and `@as` casts + v0.165 slicing views + v0.166
// test blocks / EmitMode::Test + v0.167 `@import` resolution + v0.168 fixed
// arrays `[N]T` and `for` loops + v0.169 plain data structs + v0.170
// struct methods and associated functions + v0.171 enums + v0.172
// switch with contextual enum literals + v0.173 optionals ?T + v0.174
// error unions !T + v0.175 pointers *T + v0.176 labeled loops + v0.177
// f64).
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

test "type codes: from_name maps the eight subset spellings" {
    expect(et_from_name("i32") == ET_I32);
    expect(et_from_name("i64") == ET_I64);
    expect(et_from_name("bool") == ET_BOOL);
    expect(et_from_name("void") == ET_VOID);
    expect(et_from_name("u8") == ET_U8);
    expect(et_from_name("usize") == ET_USIZE);
    expect(et_from_name("Allocator") == ET_ALLOC);
    expect(et_from_name("f64") == ET_F64);
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

test "detect: float literals are in (v0.177), unknown names still skip" {
    var a: Allocator = c_allocator();
    var d: Det = eh_detect(a, "fn main() void { print(1.5); }");
    expect(!d.found);
    // The FIRST unsupported construct still reports with its position.
    //                           0         1         2
    //                           012345678901234567890123456
    var d2: Det = eh_detect(a, "fn main() void { var x: Foo = q(); print(2.5); }");
    expect(d2.found);
    expect(str_eq(d2.word, "type-name"));
    expect(d2.pos == 24);
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
    var d: Det = eh_detect(a, "fn main() void { var s: []Foo = q(); }");
    expect(d.found);
    expect(str_eq(d.word, "type-name"));
    var d1: Det = eh_detect(a, "fn main() void { var s: []i32 = q(); var t: []usize = q(); var w: []bool = q(); }");
    expect(!d1.found);
    // (`*i32` joined in v0.175 — a comptime-sized array keeps the form
    // verdict, and a non-subset pointee is a type-name)
    var d2: Det = eh_detect(a, "fn main() void { var p: *i32 = q(); }");
    expect(!d2.found);
    var d2b: Det = eh_detect(a, "fn f(xs: [n]i64) void {}\nfn main() void { }");
    expect(d2b.found);
    expect(str_eq(d2b.word, "type-form"));
    var d2c: Det = eh_detect(a, "fn main() void { var p: *Foo = q(); }");
    expect(d2c.found);
    expect(str_eq(d2c.word, "type-name"));
    var d3: Det = eh_detect(a, "fn main() Foo { return q(); }");
    expect(d3.found);
    expect(str_eq(d3.word, "type-name"));
    // (`?i32` joined in v0.173 and `!i32` in v0.174 — non-subset inner
    // names are type-names; pointer forms keep the type-form verdict)
    var d4: Det = eh_detect(a, "fn main() !i32 { return q(); }");
    expect(!d4.found);
    var d5: Det = eh_detect(a, "fn main() ?Foo { return q(); }");
    expect(d5.found);
    expect(str_eq(d5.word, "type-name"));
    var d5b: Det = eh_detect(a, "fn main() !Foo { return q(); }");
    expect(d5b.found);
    expect(str_eq(d5b.word, "type-name"));
    var d6: Det = eh_detect(a, "fn main() ?i32 { return q(); }");
    expect(!d6.found);
}

test "detect: field access (v0.169) and method calls (v0.170) are in" {
    var a: Allocator = c_allocator();
    // Any field NAME is a subset shape now — `s.ptr` on a slice is
    // sema-invalid (E0165 territory), not a skip.
    var d: Det = eh_detect(a, "pub fn main() void { var s: []u8 = \"x\"; print(s.ptr); }");
    expect(!d.found);
    var d2: Det = eh_detect(a, "pub fn main() void { print(a.b.len); }");
    expect(!d2.found);
    // A method call walks its receiver and args (v0.170) — an unknown
    // receiver is sema's E0170, not a skip...
    var d3: Det = eh_detect(a, "pub fn main() void { p.dist(1); }");
    expect(!d3.found);
    // ...and an out-of-subset construct INSIDE one still surfaces.
    var d4: Det = eh_detect(a, "pub fn main() void { p.dist(g(i64, 1)); }");
    expect(!d4.found);
    var d4b: Det = eh_detect(a, "pub fn main() void { p.dist(x.*.q()); }");
    expect(!d4b.found);
}

test "detect: out-of-subset statements" {
    var a: Allocator = c_allocator();
    // (labeled loops joined the subset in v0.176 — a tagged-union item
    // keeps a verdict)
    var d: Det = eh_detect(a, "const U = union(enum) { a: i64 };\nfn main() void { }");
    expect(d.found);
    expect(str_eq(d.word, "union"));
    expect(d.pos == 0);
    // (`switch` v0.172, `try` v0.174, `&x` v0.175; a top-level generic
    // fn + its call joined in v0.178 — but a NON-subset type argument
    // keeps a verdict at the argument)
    var d2: Det = eh_detect(a, "fn id(comptime T: type, x: i64) i64 { return x; }\nfn main() void { print(id(i64, 1)); }");
    expect(!d2.found);
    var d2b: Det = eh_detect(a, "fn id(comptime T: type, x: i64) i64 { return x; }\nfn main() void { print(id(u16, 1)); }");
    expect(d2b.found);
    expect(str_eq(d2b.word, "type-name"));
    // (`errdefer` joined in v0.174, floats in v0.177; the body still
    // walks — an unknown TYPE inside surfaces)
    var d3: Det = eh_detect(a, "fn main() void { errdefer { var t: Foo = 1; } }");
    expect(d3.found);
    expect(str_eq(d3.word, "type-name"));
    // (labeled while joined in v0.176; an unknown break target is
    // sema's E0301, not a skip)
    var d4: Det = eh_detect(a, "fn main() void { lab: while (true) { break :lab; } }");
    expect(!d4.found);
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
    expect(!d0.found);
    // A top-level generic fn is a subset item since v0.178: type params
    // bind, value params take a bare subset-int annotation; a NON-int
    // value annotation keeps a verdict at the annotation.
    var d2: Det = eh_detect(a, "fn id(comptime T: type, x: i64) i64 { return x; }\npub fn main() void { print(id(i64, 1)); }");
    expect(!d2.found);
    var d2v: Det = eh_detect(a, "fn rep(comptime n: usize, x: i64) i64 { return x + @as(i64, n); }\npub fn main() void { print(rep(3, 4)); }");
    expect(!d2v.found);
    var d2w: Det = eh_detect(a, "fn rep(comptime b: bool, x: i64) i64 { return x; }\npub fn main() void { print(rep(true, 4)); }");
    expect(d2w.found);
    expect(str_eq(d2w.word, "type-name"));
    expect(d2w.pos == 19);
    var d3: Det = eh_detect(a, "pub fn main() void {}\n@import(\"other.ks\");");
    expect(d3.found);
    expect(str_eq(d3.word, "import"));
    // A plain data-struct declaration is a subset item (v0.169)...
    var d4: Det = eh_detect(a, "pub fn main() void {}\nconst S = struct { x: i32 };");
    expect(!d4.found);
    // ...a VALUE-receiver method inside one is admitted (v0.170), a
    // POINTER receiver stays out, and a non-subset FIELD type skips.
    var d5: Det = eh_detect(a, "pub fn main() void {}\nconst S = struct { x: i32, fn m(self: S) void {} };");
    expect(!d5.found);
    // (pointer receivers joined in v0.175 — a generic method parameter
    // keeps a verdict)
    var d5b: Det = eh_detect(a, "pub fn main() void {}\nconst S = struct { x: i32, fn m(comptime T: type) void {} };");
    expect(d5b.found);
    expect(str_eq(d5b.word, "generic-param"));
    var d6: Det = eh_detect(a, "pub fn main() void {}\nconst S = struct { x: Foo };");
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
    var d3: Det = eh_detect(a, "fn main() void { var s = alloc(q, Foo, 3); }");
    expect(d3.found);
    expect(str_eq(d3.word, "builtin-call"));
    // The walk reaches into defer bodies, continue-clauses and nested calls.
    var d4: Det = eh_detect(a, "fn main() void { defer { var z: Foo = g(); } }");
    expect(d4.found);
    expect(str_eq(d4.word, "type-name"));
    // (`if (o) |v|` joined the subset in v0.173 — a switch payload
    // capture keeps the verdict)
    var d5: Det = eh_detect(a, "fn main() void { var o = q(); if (o) |v| { } }");
    expect(!d5.found);
    var d6: Det = eh_detect(a, "fn main() void { var o = q(); switch (o) { .A => |v| { }, else => { } } }");
    expect(d6.found);
    expect(str_eq(d6.word, "capture"));
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
    // (deref places joined in v0.175 — sema's E0230/E0100 territory; a
    // call-rooted place still stays out)
    var d4: Det = eh_detect(a, "pub fn main() void { p.* = 1; }");
    expect(!d4.found);
    var d5: Det = eh_detect(a, "pub fn main() void { g()[0] = 1; }");
    expect(d5.found);
    expect(str_eq(d5.word, "place-assign"));
    // Out-of-subset constructs inside an admissible write still surface.
    var d6: Det = eh_detect(a, "pub fn main() void { var s: []u8 = \"ab\"; s[0] = w.*.f(); }");
    expect(!d6.found);
    var d7: Det = eh_detect(a, "pub fn main() void { var s: []u8 = \"ab\"; s[0] = @bad(1); }");
    expect(d7.found);
    expect(str_eq(d7.word, "builtin"));
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
    var d2: Det = eh_detect(a, "pub fn main() void { var s: []u8 = \"abcd\"; print(s[1..@bad(1)]); }");
    expect(d2.found);
    expect(str_eq(d2.word, "builtin"));
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
    // (f64 elements joined in v0.177 — an unknown element still skips)
    var d3: Det = eh_detect(a, "pub fn main() void { var xs: [2]f64 = [2]f64{ 1.5, 2.5 }; }");
    expect(!d3.found);
    var d3b: Det = eh_detect(a, "pub fn main() void { var xs: [2]Foo = [2]Foo{ 1, 2 }; }");
    expect(d3b.found);
    expect(str_eq(d3b.word, "type-name"));
    expect(d3b.pos == 29);
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

test "methods: kd_<Struct>_<method> naming, assoc/value/explicit-self (v0.170)" {
    var a: Allocator = c_allocator();
    var c: []u8 = eh_emit(a, "const Counter = struct {\n    n: i32,\n\n    fn get(self: Counter) i32 {\n        return self.n;\n    }\n\n    fn plus(self: Counter, k: i32) i32 {\n        return self.n + self.get() + k;\n    }\n\n    fn make(n: i32) Counter {\n        return Counter{ .n = n };\n    }\n};\n\npub fn main() void {\n    var c: Counter = Counter.make(4);\n    print(c.get());\n    print(Counter.plus(c, 5));\n    print(Counter.make(1).get());\n}");
    // Declarations: `self` is an ordinary by-value parameter.
    expect(eh_find(c, "int32_t kd_Counter_get(kd_struct_Counter kd_self);\n"));
    expect(eh_find(c, "int32_t kd_Counter_plus(kd_struct_Counter kd_self, int32_t kd_k);\n"));
    expect(eh_find(c, "kd_struct_Counter kd_Counter_make(int32_t kd_n);\n"));
    // Assoc call: args as-is; value call: receiver prepended; the
    // explicit-self form `Counter.plus(c, 5)` matches the value form.
    expect(eh_find(c, "    kd_struct_Counter kd_c = kd_Counter_make(4);\n"));
    expect(eh_find(c, "kd_print((long long)(kd_Counter_get(kd_c)));\n"));
    expect(eh_find(c, "kd_print((long long)(kd_Counter_plus(kd_c, 5)));\n"));
    // A call-result receiver chains.
    expect(eh_find(c, "kd_print((long long)(kd_Counter_get(kd_Counter_make(1))));\n"));
    // A method body calls a sibling through `self` (value form).
    expect(eh_find(c, "    return ((((kd_self).kd_n + kd_Counter_get(kd_self)) + kd_k));\n"));
    // Definitions: free fns first, then struct fns in declaration order.
    expect(eh_find(c, "int32_t kd_Counter_get(kd_struct_Counter kd_self) {\n"));
}

test "methods: name-level liveness — dead names dropped, all structs marked" {
    var a: Allocator = c_allocator();
    var c: []u8 = eh_emit(a, "const A = struct {\n    x: i64,\n    fn ping(self: A) i64 { return self.x; }\n    fn dead(self: A) i64 { return 0; }\n};\nconst B = struct {\n    y: i64,\n    fn ping(self: B) i64 { return self.y * 2; }\n};\npub fn main() void {\n    var v: A = A{ .x = 3 };\n    print(v.ping());\n}");
    // `ping` is live NAME-LEVEL: BOTH structs' ping emit, though only A's
    // receiver appears; `dead` emits nowhere.
    expect(eh_find(c, "int64_t kd_A_ping(kd_struct_A kd_self);\n"));
    expect(eh_find(c, "int64_t kd_B_ping(kd_struct_B kd_self);\n"));
    expect(!eh_find(c, "kd_A_dead"));
    // Test mode with NO tests: every function AND method lives.
    var c2: []u8 = eh_emit_test(a, "const A = struct {\n    x: i64,\n    fn solo(self: A) i64 { return self.x; }\n};\nfn lone() void {}\npub fn main() void { print(1); }");
    expect(eh_find(c2, "kd_A_solo"));
    expect(eh_find(c2, "kd_lone"));
}

test "enums: typedef with resolved values, seeds before structs (v0.171)" {
    var a: Allocator = c_allocator();
    var c: []u8 = eh_emit(a, "const Color = enum { Red, Green, Blue };\nconst Status = enum { Ok = 200, NotFound = 404, Teapot };\nconst P = struct { x: i64 };\npub fn main() void {\n    var c: Color = Color.Red;\n    var s: Status = Status.Teapot;\n    var p: P = P{ .x = 1 };\n    if (c == Color.Red) { print(@intFromEnum(s)); }\n    print(p.x);\n}");
    // Every enumerator carries its RESOLVED value; auto-increment
    // continues from an explicit one (Teapot = 405).
    expect(eh_find(c, "typedef enum { kd_enum_Color_Red = 0, kd_enum_Color_Green = 1, kd_enum_Color_Blue = 2 } kd_enum_Color;\n"));
    expect(eh_find(c, "typedef enum { kd_enum_Status_Ok = 200, kd_enum_Status_NotFound = 404, kd_enum_Status_Teapot = 405 } kd_enum_Status;\n"));
    // Enum seeds precede struct typedefs in the dependency walk.
    expect(eh_find(c, " } kd_enum_Status;\ntypedef struct { int64_t kd_x; } kd_struct_P;\n"));
    // Qualified literals lower to the C enumerator; equality is plain ==.
    expect(eh_find(c, "    kd_enum_Color kd_c = kd_enum_Color_Red;\n"));
    expect(eh_find(c, "    if ((kd_c == kd_enum_Color_Red)) {\n"));
}

test "enums: conversions and composite positions" {
    var a: Allocator = c_allocator();
    var c: []u8 = eh_emit(a, "const Dir = enum { N, E, S, W };\nfn spin(d: Dir) Dir {\n    return @enumFromInt(Dir, @intFromEnum(d) + 1);\n}\npub fn main() void {\n    var ds: [2]Dir = [2]Dir{ Dir.E, Dir.W };\n    print(@intFromEnum(ds[1]));\n    var v: []Dir = ds[0..2];\n    print(@intFromEnum(v[0]));\n    print(@as(i32, @intFromEnum(spin(Dir.N))));\n}");
    // @intFromEnum → an int64_t cast; @enumFromInt → an enum-type cast.
    expect(eh_find(c, "((int64_t)(kd_arr_enum_Dir_2_get(kd_ds, 1)))"));
    expect(eh_find(c, "    return (((kd_enum_Dir)((((int64_t)(kd_d)) + 1))));\n"));
    // Arrays and slices of enums mangle enum_<Name>.
    expect(eh_find(c, "typedef struct { kd_enum_Dir data[2]; } kd_arr_enum_Dir_2;\n"));
    expect(eh_find(c, "typedef struct { kd_enum_Dir *ptr; uintptr_t len; } kd_slice_enum_Dir;\n"));
    expect(eh_find(c, "kd_enum_Dir kd_spin(kd_enum_Dir kd_d);\n"));
}

test "switch: enum exhaustive lowering, case chains, divergence (v0.172)" {
    var a: Allocator = c_allocator();
    var c: []u8 = eh_emit(a, "const Op = enum { Add, Sub };\nfn apply(op: Op, x: i64) i64 {\n    switch (op) {\n        .Add => { return x + 1; },\n        .Sub => { return x - 1; },\n    }\n}\npub fn main() void {\n    print(apply(.Add, 41));\n}");
    // Bare `.V` labels take the scrutinee's enum; the LAST case of an arm
    // opens the brace; every arm closes `} break;` (no fallthrough).
    expect(eh_find(c, "    switch (kd_op) {\n        case kd_enum_Op_Add: {\n            return ((kd_x + 1));\n        } break;\n        case kd_enum_Op_Sub: {\n            return ((kd_x - 1));\n        } break;\n    }\n"));
    // An exhaustive enum switch with all-diverging arms DIVERGES: no code
    // follows it inside kd_apply (and no fall-through flush).
    expect(eh_find(c, "        } break;\n    }\n}\n"));
    // A contextual `.V` argument takes the parameter's enum.
    expect(eh_find(c, "kd_print((long long)(kd_apply(kd_enum_Op_Add, 41)));\n"));
}

test "switch: integer labels share arms, GNU ranges, else default" {
    var a: Allocator = c_allocator();
    var c: []u8 = eh_emit(a, "pub fn main() void {\n    var n: i64 = 3;\n    switch (n) {\n        0, 1 => { print(10); },\n        2 .. 5 => { print(20); },\n        else => { print(30); },\n    }\n}");
    // Multi-label arm: bare `case 0:` then `case 1: {`; a range spells
    // the GNU `case 2 ... 5:`; `else` is `default:`.
    expect(eh_find(c, "        case 0:\n        case 1: {\n            kd_print((long long)(10));\n        } break;\n        case 2 ... 5: {\n            kd_print((long long)(20));\n        } break;\n        default: {\n            kd_print((long long)(30));\n        } break;\n"));
    // An integer switch (with else) does NOT diverge on its own: the
    // implicit void return follows.
    expect(eh_count(c, "} break;") == 3);
}

test "coercion: contextual .V at let/assign/return/args/array elems" {
    var a: Allocator = c_allocator();
    var c: []u8 = eh_emit(a, "const C = enum { R, G, B };\nfn pick(c: C, d: C) C {\n    var out: C = c;\n    out = d;\n    out = .R;\n    return .G;\n}\npub fn main() void {\n    var cs: [2]C = [2]C{ .G, C.B };\n    print(@intFromEnum(pick(.B, cs[0])));\n}");
    expect(eh_find(c, "    kd_enum_C kd_out = kd_c;\n"));
    expect(eh_find(c, "    kd_out = kd_enum_C_R;\n"));
    expect(eh_find(c, "    return (kd_enum_C_G);\n"));
    // Array-literal elements coerce; call arguments coerce by position.
    expect(eh_find(c, "((kd_arr_enum_C_2){ .data = { kd_enum_C_G, kd_enum_C_B } })"));
    expect(eh_find(c, "kd_pick(kd_enum_C_B, kd_arr_enum_C_2_get(kd_cs, 0))"));
}

test "optionals: typedef + helpers, widenings, orelse/unwrap (v0.173)" {
    var a: Allocator = c_allocator();
    var c: []u8 = eh_emit(a, "fn find(n: i64) ?i64 {\n    if (n > 0) { return n * 2; }\n    return null;\n}\npub fn main() void {\n    var x: ?i64 = find(5);\n    print(find(0) orelse 99);\n    var y: ?i64 = null;\n    y = 7;\n    print(y.?);\n    print(x orelse 0);\n}");
    // The typedef block: struct + _orelse + _unwrap, exact bytes.
    expect(eh_find(c, "typedef struct { bool has; int64_t val; } kd_opt_int64_t;\n"));
    expect(eh_find(c, "static inline int64_t kd_opt_int64_t_orelse(kd_opt_int64_t o, int64_t d) { return o.has ? o.val : d; }\n"));
    expect(eh_find(c, "static inline int64_t kd_opt_int64_t_unwrap(kd_opt_int64_t o) { if (!o.has) { fputs(\"panic: unwrapped a null optional\\n\", stderr); exit(101); } return o.val; }\n"));
    // Widenings: a `T` return wraps; `null` is the empty optional (at
    // return, let and assignment positions).
    expect(eh_find(c, "        return (((kd_opt_int64_t){ .has = true, .val = (kd_n * 2) }));\n"));
    expect(eh_find(c, "    return (((kd_opt_int64_t){ .has = false }));\n"));
    expect(eh_find(c, "    kd_opt_int64_t kd_y = ((kd_opt_int64_t){ .has = false });\n"));
    expect(eh_find(c, "    kd_y = ((kd_opt_int64_t){ .has = true, .val = 7 });\n"));
    // orelse / unwrap lower through the inline helpers.
    expect(eh_find(c, "kd_print((long long)(kd_opt_int64_t_orelse(kd_find(0), 99)));\n"));
    expect(eh_find(c, "kd_print((long long)(kd_opt_int64_t_unwrap(kd_y)));\n"));
}

test "optionals: if-capture hoists once, binds payload, __kd_if counter" {
    var a: Allocator = c_allocator();
    var c: []u8 = eh_emit(a, "fn side(n: i64) ?i64 {\n    return n;\n}\npub fn main() void {\n    if (side(41)) |v| {\n        print(v);\n    } else {\n        print(0 - 1);\n    }\n    if (side(1)) |w| {\n        print(w);\n    }\n}");
    // The capture block: hoisted __kd_if temp, .has test, payload bind.
    expect(eh_find(c, "    {\n        kd_opt_int64_t __kd_if0 = kd_side(41);\n        if (__kd_if0.has) {\n            int64_t kd_v = __kd_if0.val;\n            kd_print((long long)(kd_v));\n        } else {\n            {\n                kd_print((long long)((0 - 1)));\n            }\n        }\n    }\n"));
    // The counter advances per capture within one function.
    expect(eh_find(c, "__kd_if1 = kd_side(1);"));
}

test "errunions: typedefs, code space, try/errdefer flushes (v0.174)" {
    var a: Allocator = c_allocator();
    var c: []u8 = eh_emit(a, "fn may(n: i64) !i64 {\n    if (n < 0) { return error.Neg; }\n    return n * 2;\n}\nfn chain(n: i64) !i64 {\n    defer print(700);\n    errdefer print(800);\n    var v: i64 = try may(n);\n    return v + 1;\n}\npub fn main() void {\n    print(chain(5) catch 0 - 1);\n    print(chain(0 - 2) catch |e| @as(i64, e) * 10);\n}");
    // The typedef + eager-catch helper, exact bytes.
    expect(eh_find(c, "typedef struct { int32_t err; int64_t val; } kd_err_int64_t;\n"));
    expect(eh_find(c, "static inline int64_t kd_err_int64_t_catch(kd_err_int64_t e, int64_t d) { return e.err == 0 ? e.val : d; }\n"));
    // error.Neg is the FIRST error name → code 1; the failure value
    // carries it at the return coercion site.
    expect(eh_find(c, "        return (((kd_err_int64_t){ .err = 1 }));\n"));
    // try: hoist, error-path flush runs the ERRDEFER (800) then the
    // defer (700), the propagation re-wraps; success unwraps .val; the
    // ordinary return flushes only the defer.
    expect(eh_find(c, "    kd_err_int64_t __kd_try0 = kd_may(kd_n);\n    if (__kd_try0.err != 0) {\n        kd_print((long long)(800));\n        kd_print((long long)(700));\n        return (kd_err_int64_t){ .err = __kd_try0.err };\n    }\n    int64_t kd_v = __kd_try0.val;\n"));
    expect(eh_find(c, "    kd_err_int64_t __kd_ret = (((kd_err_int64_t){ .err = 0, .val = (kd_v + 1) }));\n    kd_print((long long)(700));\n    return __kd_ret;\n"));
    // The eager catch lowers through the helper; the capturing catch
    // hoists __kd_eu/__kd_catch and binds the i32 code lazily.
    expect(eh_find(c, "kd_err_int64_t_catch(kd_chain(5), (0 - 1))"));
    expect(eh_find(c, "    kd_err_int64_t __kd_eu0 = kd_chain((0 - 2));\n    int64_t __kd_catch0;\n    if (__kd_eu0.err != 0) {\n        int32_t kd_e = __kd_eu0.err;\n        __kd_catch0 = (((int64_t)(kd_e)) * 10);\n    } else {\n        __kd_catch0 = __kd_eu0.val;\n    }\n"));
}

test "errunions: !void — no helper, lazy catch, fallthrough success" {
    var a: Allocator = c_allocator();
    var c: []u8 = eh_emit(a, "fn step(n: i64) !void {\n    if (n == 0) { return error.Zero; }\n    print(n);\n}\nfn run(n: i64) !void {\n    try step(n);\n}\npub fn main() void {\n    run(1) catch print(0 - 1);\n    run(0) catch |e| print(100 + e);\n}");
    // The payload-less typedef; NO _catch helper for !void.
    expect(eh_find(c, "typedef struct { int32_t err; } kd_err_void;\n"));
    expect(!eh_find(c, "kd_err_void_catch"));
    // The fallthrough success return lands at COLUMN 0 (the Rust quirk).
    expect(eh_find(c, "\nreturn ((kd_err_void){ .err = 0 });\n}\n"));
    // try over !void discards a void payload.
    expect(eh_find(c, "    (void)(((void)0));\n"));
    // Both catch forms over !void hoist and run the handler lazily.
    expect(eh_find(c, "    kd_err_void __kd_eu0 = kd_run(1);\n    if (__kd_eu0.err != 0) {\n        kd_print((long long)((0 - 1)));\n    }\n"));
    expect(eh_find(c, "        int32_t kd_e = __kd_eu1.err;\n        kd_print((long long)((100 + kd_e)));\n"));
}

test "pointers: addrof/deref, auto-ref matrix, field auto-deref (v0.175)" {
    var a: Allocator = c_allocator();
    var c: []u8 = eh_emit(a, "const Counter = struct {\n    n: i64,\n\n    fn bump(self: *Counter, k: i64) void {\n        self.n += k;\n    }\n};\npub fn main() void {\n    var x: i64 = 41;\n    var p: *i64 = &x;\n    p.* += 1;\n    print(p.*);\n    var c: Counter = Counter{ .n = 1 };\n    c.bump(4);\n    var pc: *Counter = &c;\n    pc.bump(10);\n    print(pc.n);\n    var cs: [2]Counter = [2]Counter{ Counter{ .n = 0 }, Counter{ .n = 5 } };\n    cs[1].bump(7);\n    print(cs[1].n);\n}");
    // `*T` spells structurally; `&x` parenthesizes; `p.*` reads/writes
    // re-spell the deref (compound: both sides).
    expect(eh_find(c, "    int64_t* kd_p = (&(kd_x));\n"));
    expect(eh_find(c, "    *(kd_p) = *(kd_p) + (1);\n"));
    expect(eh_find(c, "kd_print((long long)((*(kd_p))));\n"));
    // A pointer receiver is an ordinary `T*` first parameter; the value
    // receiver auto-refs `(&(c))`, a pointer receiver passes through,
    // and an ELEMENT receiver refs its bounds-checked `_at` pointer.
    expect(eh_find(c, "void kd_Counter_bump(kd_struct_Counter* kd_self, int64_t kd_k);\n"));
    expect(eh_find(c, "    kd_Counter_bump((&(kd_c)), 4);\n"));
    expect(eh_find(c, "    kd_Counter_bump(kd_pc, 10);\n"));
    expect(eh_find(c, "    kd_Counter_bump((kd_arr_struct_Counter_2_at(&(kd_cs), 1)), 7);\n"));
    // Field access through `*Struct` auto-derefs — reads and the
    // compound write inside the method body alike.
    expect(eh_find(c, "kd_print((long long)((*(kd_pc)).kd_n));\n"));
    expect(eh_find(c, "    ((*(kd_self)).kd_n) = ((*(kd_self)).kd_n) + (kd_k);\n"));
}

test "labeled loops: goto lowering, targeted flushes, clause rule (v0.176)" {
    var a: Allocator = c_allocator();
    var c: []u8 = eh_emit(a, "pub fn main() void {\n    var xs: [2]i64 = [2]i64{ 1, 2 };\n    b: for (xs) |y| {\n        defer print(9);\n        if (y == 1) { continue :b; }\n        print(y);\n    }\n    outer: while (true) {\n        var i: i64 = 0;\n        while (i < 3) : (i += 1) {\n            if (i == 2) { break :outer; }\n        }\n    }\n}");
    // `continue :b` flushes the loop's defers THEN gotos its cont-label;
    // the label precedes the index increment, which runs for the
    // fall-through path too (clause-after-label ordering).
    expect(eh_find(c, "            if ((kd_y == 1)) {\n                kd_print((long long)(9));\n                goto __kd_cont_b;\n            }\n"));
    expect(eh_find(c, "            __kd_cont_b:;\n            __kd_fi0 += 1;\n"));
    // The for's break-label sits past the outer block close; the while's
    // past its own close. `break :outer` from the inner while gotos it.
    expect(eh_find(c, "    }\n    __kd_brk_b:;\n"));
    expect(eh_find(c, "                goto __kd_brk_outer;\n"));
    expect(eh_find(c, "    __kd_brk_outer:;\n"));
    // The labeled while carries its cont-label before the re-test even
    // with no continue-clause written.
    expect(eh_find(c, "        __kd_cont_outer:;\n    }\n"));
}

test "f64: shortest-roundtrip literals, print route, casts (v0.177)" {
    var a: Allocator = c_allocator();
    var c: []u8 = eh_emit(a, "pub fn main() void {\n    var x: f64 = 3.140;\n    var y: f64 = 100.0;\n    var z: f64 = 0.30000000000000004;\n    var w: f64 = 9007199254740993.0;\n    print(x);\n    print(@as(i64, y));\n    print(@as(f64, 7));\n    print(z + w);\n}");
    // Literals canonicalize to the shortest round-trip (`3.140` → `3.14`;
    // the 2^53+1 literal rounds to ...992.0; the classic 0.3+ε keeps all
    // 17 digits); `print(f64)` routes through kd_print_f64; casts spell
    // `double`.
    expect(eh_find(c, "    double kd_x = 3.14;\n"));
    expect(eh_find(c, "    double kd_y = 100.0;\n"));
    expect(eh_find(c, "    double kd_z = 0.30000000000000004;\n"));
    expect(eh_find(c, "    double kd_w = 9007199254740992.0;\n"));
    expect(eh_find(c, "    kd_print_f64(kd_x);\n"));
    expect(eh_find(c, "kd_print((long long)(((int64_t)(kd_y))));\n"));
    expect(eh_find(c, "((double)(7))"));
    expect(eh_find(c, "    kd_print_f64((kd_z + kd_w));\n"));
}

test "generics: instances, value params, negative mangle (v0.178)" {
    var a: Allocator = c_allocator();
    var c: []u8 = eh_emit(a, "fn imax(comptime T: type, x: T, y: T) T {\n    if (x > y) { return x; }\n    return y;\n}\nfn addk(comptime k: i64, x: i64) i64 {\n    return x + k;\n}\npub fn main() void {\n    print(imax(i64, 3, 9));\n    print(imax(i32, @as(i32, 4), @as(i32, 2)));\n    print(addk(-3, 10));\n    print(imax(i64, 1, 2));\n}");
    // One specialised C function per distinct instantiation, in
    // discovery order, forward-declared right after the plain fns; the
    // repeat i64 call dedups; a negative value arg mangles `m<digits>`
    // (`-` is not a C identifier character); calls pass ONLY the
    // runtime args.
    expect(eh_find(c, "int64_t kd_imax__int64_t(int64_t kd_x, int64_t kd_y);\nint32_t kd_imax__int32_t(int32_t kd_x, int32_t kd_y);\nint64_t kd_addk__m3(int64_t kd_x);\n"));
    expect(eh_find(c, "kd_print((long long)(kd_imax__int64_t(3, 9)));\n"));
    expect(eh_find(c, "kd_print((long long)(kd_addk__m3(10)));\n"));
    // The instance body: the value param reference emits the bound
    // literal, never a C variable.
    expect(eh_find(c, "int64_t kd_addk__m3(int64_t kd_x) {\n    return ((kd_x + -3));\n}\n"));
    expect(!eh_find(c, "kd_k"));
}

test "generics: [n]T value-size params + comptime-arg const env (v0.178)" {
    var a: Allocator = c_allocator();
    var c: []u8 = eh_emit(a, "const BASE = 3;\nfn total(comptime n: usize, xs: [n]i64) i64 {\n    var s: i64 = 0;\n    var i: usize = 0;\n    while (i < n) : (i = i + 1) { s = s + xs[i]; }\n    return s;\n}\nfn addn(comptime n: i64, x: i64) i64 {\n    return n + x;\n}\npub fn main() void {\n    var z: [4]i64 = [4]i64{ 1, 2, 3, 4 };\n    print(total(4, z));\n    print(addn(BASE * 2, 1));\n}");
    // `[n]T` resolves to the BOUND length (the `[4]i64` array typedef is
    // shared with the literal); the value argument const-evaluates over
    // the top-level consts (BASE * 2 = 6); the size use in the body is
    // the literal 4.
    expect(eh_find(c, "int64_t kd_total__4(kd_arr_int64_t_4 kd_xs);\n"));
    expect(eh_find(c, "int64_t kd_addn__6(int64_t kd_x);\n"));
    expect(eh_find(c, "    while ((kd_i < 4)) {\n"));
    expect(eh_find(c, "kd_print((long long)(kd_total__4(kd_z)));\n"));
}

test "generics: liveness sources + test-discovered instances (v0.178)" {
    var a: Allocator = c_allocator();
    // A zero-instantiation generic's body still contributes its called
    // names (§43.1): `kept` stays; the generic itself is never emitted
    // under its plain name.
    var c: []u8 = eh_emit(a, "fn kept() i64 { return 41; }\nfn unused_gen(comptime T: type, x: T) T {\n    return @as(T, kept()) + x;\n}\npub fn main() void {\n    print(1);\n}");
    expect(eh_find(c, "int64_t kd_kept(void);\n"));
    expect(!eh_find(c, "kd_unused_gen"));
    // An instance discovered in a TEST body is recorded in sema's single
    // table and therefore emitted in Program mode too (§43.1: every
    // recorded instantiation, liveness notwithstanding).
    var c2: []u8 = eh_emit(a, "fn id(comptime T: type, x: T) T { return x; }\npub fn main() void { print(2); }\ntest \"t\" { expect(id(i64, 1) == 1); }");
    expect(eh_find(c2, "int64_t kd_id__int64_t(int64_t kd_x);\n"));
    expect(eh_find(c2, "int64_t kd_id__int64_t(int64_t kd_x) {\n    return (kd_x);\n}\n"));
}

test "generic structs: instances, Self, applications (v0.179)" {
    var a: Allocator = c_allocator();
    var c: []u8 = eh_emit(a, "fn Box(comptime T: type) type {\n    return struct {\n        val: T,\n        fn init(v: T) Self { return Self{ .val = v }; }\n        fn get(self: Self) T { return self.val; }\n        fn set(self: *Self, v: T) void { self.val = v; }\n    };\n}\nconst IntBox = Box(i64);\npub fn main() void {\n    var b: IntBox = IntBox.init(41);\n    print(b.get());\n    b.set(7);\n    var c2: Box(i32) = Box(i32).init(@as(i32, 5));\n    print(c2.get());\n}");
    // One monomorphised struct per (ctor, args) tuple — the alias and the
    // direct application share `Box__int64_t`; methods emit per instance
    // as `kd_<Instance>_<method>` with `Self` resolved to the instance; a
    // `*Self` receiver takes the auto-ref matrix.
    expect(eh_find(c, "typedef struct { int64_t kd_val; } kd_struct_Box__int64_t;"));
    expect(eh_find(c, "typedef struct { int32_t kd_val; } kd_struct_Box__int32_t;"));
    expect(eh_find(c, "kd_struct_Box__int64_t kd_Box__int64_t_init(int64_t kd_v);\n"));
    expect(eh_find(c, "void kd_Box__int64_t_set(kd_struct_Box__int64_t* kd_self, int64_t kd_v);\n"));
    expect(eh_find(c, "    kd_struct_Box__int64_t kd_b = kd_Box__int64_t_init(41);\n"));
    expect(eh_find(c, "    kd_Box__int64_t_set((&(kd_b)), 7);\n"));
    expect(eh_find(c, "    kd_struct_Box__int32_t kd_c2 = kd_Box__int32_t_init(((int32_t)(5)));\n"));
    expect(eh_find(c, "int64_t kd_Box__int64_t_init(int64_t kd_v) {\n    return (((kd_struct_Box__int64_t){ .kd_val = (kd_v) }));\n}\n") == false);
    // The instance methods define AFTER the plain functions, under the
    // instance substitution: `Self{ … }` spells the instance compound
    // literal.
    expect(eh_find(c, "kd_struct_Box__int64_t kd_Box__int64_t_init(int64_t kd_v) {\n    return (((kd_struct_Box__int64_t){ .kd_val = kd_v }));\n}\n"));
}

test "generic structs: plain-struct Self + composition (v0.179)" {
    var a: Allocator = c_allocator();
    // `Self`/`@This()` in a PLAIN struct's methods (§32.2) resolve to the
    // enclosing struct in signatures and bodies.
    var c: []u8 = eh_emit(a, "const Point = struct {\n    x: i64,\n    fn mk(x: i64) Self { return Self{ .x = x }; }\n    fn bump(self: *@This()) void { self.x = self.x + 1; }\n};\npub fn main() void {\n    var p: Point = Point.mk(3);\n    p.bump();\n    print(p.x);\n}");
    expect(eh_find(c, "kd_struct_Point kd_Point_mk(int64_t kd_x);\n"));
    expect(eh_find(c, "void kd_Point_bump(kd_struct_Point* kd_self);\n"));
    expect(eh_find(c, "    kd_Point_bump((&(kd_p)));\n"));
    // Composition: a ctor field of ANOTHER instance type keeps windows
    // intact (two-phase field resolution) and the dependency walk orders
    // the inner typedef first.
    var c2: []u8 = eh_emit(a, "fn Slot(comptime T: type) type {\n    return struct { v: T, fn of(x: T) Self { return Self{ .v = x }; } };\n}\nfn Pair(comptime T: type) type {\n    return struct {\n        lo: Slot(T),\n        hi: Slot(T),\n        fn mk(x: T, y: T) Self { return Self{ .lo = Slot(T).of(x), .hi = Slot(T).of(y) }; }\n    };\n}\npub fn main() void {\n    var p: Pair(i64) = Pair(i64).mk(4, 5);\n    print(p.lo.v + p.hi.v);\n}");
    expect(eh_find(c2, "typedef struct { int64_t kd_v; } kd_struct_Slot__int64_t;"));
    expect(eh_find(c2, "typedef struct { kd_struct_Slot__int64_t kd_lo; kd_struct_Slot__int64_t kd_hi; } kd_struct_Pair__int64_t;"));
    expect(eh_find(c2, "kd_struct_Pair__int64_t kd_Pair__int64_t_mk(int64_t kd_x, int64_t kd_y);\n"));
}
