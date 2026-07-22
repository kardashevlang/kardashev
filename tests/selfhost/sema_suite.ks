// sema_suite.ks — in-language tests for the self-hosted stage-27 sema
// mirror (v0.186, `selfhost/sema.ks`): the scalar-core subset detector's
// verdicts + first-hit positions, and the checker's first-diagnostic
// codes + byte positions, over hand-laid single-file sources.
//
// Run: kard test tests/selfhost/sema_suite.ks (driven from
// `crates/kardc/tests/selfhost_sema.rs` so it is part of `cargo test`).
// The differential corpus compares these verdicts against the REAL Rust
// `sema::check` statistically; this suite pins the corners by hand.

@import("../../selfhost/lexer.ks");
@import("../../selfhost/ast.ks");
@import("../../selfhost/parser.ks");
@import("../../selfhost/sema.ks");
@import("std");

// Lex + parse `src` with the self-hosted toolchain (suite sources must be
// lexically and syntactically valid).
fn sh_parse(a: Allocator, src: []u8) Parser {
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

/// The detector verdict for `src`.
fn sh_detect(a: Allocator, src: []u8) SsDet {
    var p: Parser = sh_parse(a, src);
    return ss_detect(src, p.nodes, p.root);
}

/// The checker verdict for an in-subset `src` (code 0 = OK).
fn sh_check(a: Allocator, src: []u8) SsOut {
    var p: Parser = sh_parse(a, src);
    return ss_verdict(a, src, p.nodes, p.root);
}

test "detector: the scalar core is in-subset" {
    var a: Allocator = c_allocator();
    var d: SsDet = sh_detect(a, "const N: i64 = 2 + 3;\nfn f(x: u16) u16 { return x; }\npub fn main() void {\n    var i: i64 = 0;\n    while (i < N) : (i += 1) {\n        defer print(i);\n        if (i == 1) { continue; }\n    }\n}\ntest \"t\" { expect(true); }");
    expect(!d.found);
}

test "detector: first-hit words and positions" {
    var a: Allocator = c_allocator();
    // A float literal is an out-of-subset EXPRESSION at its own byte.
    var d1: SsDet = sh_detect(a, "pub fn main() void { var x: f64 = 1.5; }");
    expect(d1.found);
    // The f64 ANNOTATION hits first, as a type-name, at byte 28.
    expect(str_eq(d1.word, "type-name"));
    expect(d1.pos == 28);
    // A composite type form.
    var d2: SsDet = sh_detect(a, "pub fn main() void { var s: []u8 = \"x\"; }");
    expect(str_eq(d2.word, "type-form"));
    expect(d2.pos == 28);
    // An out-of-subset statement form.
    var d3: SsDet = sh_detect(a, "pub fn main() void { switch (1) { else => { print(0); } } }");
    expect(str_eq(d3.word, "stmt"));
    expect(d3.pos == 21);
    // The allocator builtins are out (`call`).
    var d4: SsDet = sh_detect(a, "pub fn main() void { free(c_allocator(), c_allocator()); }");
    expect(str_eq(d4.word, "call"));
    expect(d4.pos == 21);
    // A labeled loop.
    var d5: SsDet = sh_detect(a, "pub fn main() void { l: while (true) { break :l; } }");
    expect(str_eq(d5.word, "label"));
    // A comptime parameter.
    var d6: SsDet = sh_detect(a, "fn g(comptime T: type, x: i64) i64 { return x; }\npub fn main() void { print(g(i64, 1)); }");
    expect(str_eq(d6.word, "generic-param"));
    expect(d6.pos == 5);
}

test "checker: a clean scalar program is OK" {
    var a: Allocator = c_allocator();
    var v: SsOut = sh_check(a, "const K: i64 = 40 + 2;\nfn add(x: i64, y: i64) i64 { return x + y; }\npub fn main() void {\n    var t: i64 = add(K, 8);\n    print(t);\n}");
    expect(v.code == 0);
}

test "checker: pass order — builtin redefinition wins over a later const error" {
    var a: Allocator = c_allocator();
    // The const error (E0131) is EARLIER in the source, but pass 1 (fns)
    // runs before pass 2 (consts): E0101 at the fn item.
    var v: SsOut = sh_check(a, "const X: i64 = MISSING;\nfn print(x: i64) void {}\npub fn main() void {}");
    expect(v.code == 101);
    expect(v.pos == 24);
}

test "checker: const diagnostics — E0311 / E0130 / E0131 / E0132 / E0110" {
    var a: Allocator = c_allocator();
    // A call to a DECLARED fn in a const initializer is E0311 at the call.
    var v1: SsOut = sh_check(a, "fn f() i64 { return 1; }\nconst X = f();\npub fn main() void { print(X); }");
    expect(v1.code == 311);
    expect(v1.pos == 35);
    // A call to an UNKNOWN callee falls through to const-eval's E0130.
    var v2: SsOut = sh_check(a, "const X = g();\npub fn main() void { print(X); }");
    expect(v2.code == 130);
    expect(v2.pos == 10);
    // A forward const reference is E0131 at the identifier.
    var v3: SsOut = sh_check(a, "const A: i64 = B;\nconst B: i64 = 2;\npub fn main() void {}");
    expect(v3.code == 131);
    expect(v3.pos == 15);
    // A const type error is E0132 at the operator node.
    var v4: SsOut = sh_check(a, "const X = 1 + true;\npub fn main() void {}");
    expect(v4.code == 132);
    // An annotated const whose folded kind mismatches is E0110 at the value.
    var v5: SsOut = sh_check(a, "const B: bool = 3;\npub fn main() void {}");
    expect(v5.code == 110);
    expect(v5.pos == 16);
}

test "checker: body spans — operand vs operator vs statement" {
    var a: Allocator = c_allocator();
    // An unknown name in a call argument: E0100 at the identifier.
    var v1: SsOut = sh_check(a, "pub fn main() void { print(zzz); }");
    expect(v1.code == 100);
    expect(v1.pos == 27);
    // Assigning to a const: E0110 at the STATEMENT.
    var v2: SsOut = sh_check(a, "pub fn main() void { const c: i64 = 1; c = 2; }");
    expect(v2.code == 110);
    expect(v2.pos == 39);
    // A same-type mismatch: E0110 at the OPERATOR node (its span start is
    // the lhs's start).
    var v3: SsOut = sh_check(a, "pub fn main() void { var a: u8 = 1; var b: i64 = 2; print(a + b); }");
    expect(v3.code == 110);
    expect(v3.pos == 58);
    // `and` reports the lhs operand first.
    var v4: SsOut = sh_check(a, "pub fn main() void { if (1 and true) { print(1); } }");
    expect(v4.code == 110);
    expect(v4.pos == 25);
}

test "checker: literal anchoring order — a flexible lhs checks the rhs first" {
    var a: Allocator = c_allocator();
    // `5 < missing`: the lhs is a flexible literal, so the CONCRETE rhs
    // anchors — and is checked first: E0100 at `missing`.
    var v: SsOut = sh_check(a, "pub fn main() void { if (5 < missing) { print(1); } }");
    expect(v.code == 100);
    expect(v.pos == 29);
}

test "checker: loop / test gates — E0120 and E0140" {
    var a: Allocator = c_allocator();
    var v1: SsOut = sh_check(a, "pub fn main() void { break; }");
    expect(v1.code == 120);
    expect(v1.pos == 21);
    var v2: SsOut = sh_check(a, "pub fn main() void { expect(true); }");
    expect(v2.code == 140);
    expect(v2.pos == 21);
    // Inside a test block, `expect` is fine and a bad argument is E0110
    // at the argument.
    var v3: SsOut = sh_check(a, "test \"t\" { expect(7); }");
    expect(v3.code == 110);
    expect(v3.pos == 18);
}

test "checker: scope death and shadowing" {
    var a: Allocator = c_allocator();
    // A block-scoped name dies with its block: E0100 at the later use.
    var v1: SsOut = sh_check(a, "pub fn main() void { { var t: i64 = 5; print(t); } print(t); }");
    expect(v1.code == 100);
    expect(v1.pos == 57);
    // Shadowing a const with a var makes the name assignable.
    var v2: SsOut = sh_check(a, "const K: i64 = 7;\npub fn main() void { var K: i64 = 9; K = 1; print(K); }");
    expect(v2.code == 0);
}

test "checker: calls — arity at the call, argument type at the argument" {
    var a: Allocator = c_allocator();
    var v1: SsOut = sh_check(a, "fn f(a: i64) i64 { return a; }\npub fn main() void { print(f(1, 2)); }");
    expect(v1.code == 110);
    expect(v1.pos == 58);
    var v2: SsOut = sh_check(a, "fn f(a: bool) void {}\npub fn main() void { f(5); }");
    expect(v2.code == 110);
    expect(v2.pos == 45);
    var v3: SsOut = sh_check(a, "pub fn main() void { nothere(); }");
    expect(v3.code == 100);
    expect(v3.pos == 21);
}
