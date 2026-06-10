// parser_suite.ks — in-language tests for the self-hosted parser (v0.160).
//
// Run: kard test tests/selfhost/parser_suite.ks (driven from
// `crates/kardc/tests/selfhost_parser.rs` so it is part of `cargo test`).
// Pins the SPEC §2 (+ per-feature) rules the differential corpus exercises
// statistically: item forms, the full precedence ladder, exact spans on
// hand-laid sources (including the parens-do-not-extend rule), every type
// form (applications, named error unions, `@This()`), switch arms with
// multi-labels / ranges / captures, loop labels, and the first-error
// positions of the parser's own shape constraints.

@import("../../selfhost/lexer.ks");
@import("../../selfhost/ast.ks");
@import("../../selfhost/parser.ks");
@import("std");

// Lex + parse `src` with the self-hosted toolchain; the returned Parser
// carries the arena (`nodes`), the item chain (`root`) and the first-error
// state (`failed`/`ecode`/`epos`). Suite sources must be lexically valid
// (a lex error would surface as a parse failure at the truncation point).
fn ps_parse(a: Allocator, src: []u8) Parser {
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
        // hard parse error: root stays -1; the error state is in p.
        p.root = 0 - 1;
    }
    return p;
}

// --- tiny arena accessors (read-only; Parser passed by value) ----------------

fn nk(p: Parser, n: i32) u8 {
    return p.nodes[@as(usize, n)].kind;
}

fn na(p: Parser, n: i32) i32 {
    return p.nodes[@as(usize, n)].a;
}

fn nb(p: Parser, n: i32) i32 {
    return p.nodes[@as(usize, n)].b;
}

fn nc(p: Parser, n: i32) i32 {
    return p.nodes[@as(usize, n)].c;
}

fn nnext(p: Parser, n: i32) i32 {
    return p.nodes[@as(usize, n)].next;
}

fn noff(p: Parser, n: i32) usize {
    return p.nodes[@as(usize, n)].off;
}

fn nlen(p: Parser, n: i32) usize {
    return p.nodes[@as(usize, n)].len;
}

fn nval(p: Parser, n: i32) i64 {
    return p.nodes[@as(usize, n)].val;
}

fn nval2(p: Parser, n: i32) i64 {
    return p.nodes[@as(usize, n)].val2;
}

fn nflags(p: Parser, n: i32) i64 {
    return p.nodes[@as(usize, n)].flags;
}

fn has_flag(p: Parser, n: i32, f: i64) bool {
    return (nflags(p, n) & f) != 0;
}

// The primary name text of node `n` (its `x` span).
fn xtext(p: Parser, n: i32) []u8 {
    var u: usize = @as(usize, n);
    return p.src[p.nodes[u].xoff..p.nodes[u].xoff + p.nodes[u].xlen];
}

// The secondary name text of node `n` (its `y` span).
fn ytext(p: Parser, n: i32) []u8 {
    var u: usize = @as(usize, n);
    return p.src[p.nodes[u].yoff..p.nodes[u].yoff + p.nodes[u].ylen];
}

// The tertiary name text of node `n` (its `z` span).
fn ztext(p: Parser, n: i32) []u8 {
    var u: usize = @as(usize, n);
    return p.src[p.nodes[u].zoff..p.nodes[u].zoff + p.nodes[u].zlen];
}

// The `idx`-th node of the sibling chain starting at `head` (0-based).
fn nth(p: Parser, head: i32, idx: i32) i32 {
    var cur: i32 = head;
    var i: i32 = 0;
    while (i < idx) : (i += 1) {
        cur = nnext(p, cur);
    }
    return cur;
}

// The length of the sibling chain starting at `head`.
fn chain_len(p: Parser, head: i32) i32 {
    var cur: i32 = head;
    var count: i32 = 0;
    while (cur >= 0) {
        count += 1;
        cur = nnext(p, cur);
    }
    return count;
}

// The first statement of the body of the FIRST item (must be a function —
// callers assert the kinds they walk; `expect` itself is test-block-only).
fn first_stmt(p: Parser) i32 {
    return na(p, nc(p, p.root));
}

// The value expression of the FIRST item (must be a const declaration).
fn const_value(p: Parser) i32 {
    return nb(p, p.root);
}

// --- items ---------------------------------------------------------------------

test "items: every top-level form parses to its kind, in order" {
    var a: Allocator = c_allocator();
    var src: []u8 = "@import(\"m.ks\");\npub const N = 1;\nfn f() void {}\nconst P = struct {};\nconst E = enum { A };\nconst U = union(enum) { a: i32 };\nconst S = error{ X };\ntest \"t\" {}\n";
    var p: Parser = ps_parse(a, src);
    expect(!p.failed);
    expect(chain_len(p, p.root) == 8);
    expect(nk(p, nth(p, p.root, 0)) == ND_IMPORT);
    expect(nk(p, nth(p, p.root, 1)) == ND_CONST);
    expect(nk(p, nth(p, p.root, 2)) == ND_FN);
    expect(nk(p, nth(p, p.root, 3)) == ND_STRUCT);
    expect(nk(p, nth(p, p.root, 4)) == ND_ENUM);
    expect(nk(p, nth(p, p.root, 5)) == ND_UNION);
    expect(nk(p, nth(p, p.root, 6)) == ND_ERRSET);
    expect(nk(p, nth(p, p.root, 7)) == ND_TEST);
    // `pub` lands on the const and only there.
    expect(has_flag(p, nth(p, p.root, 1), F_PUB));
    expect(!has_flag(p, nth(p, p.root, 2), F_PUB));
    expect(str_eq(xtext(p, nth(p, p.root, 1)), "N"));
}

test "items: fn params carry names, comptime flags and types" {
    var a: Allocator = c_allocator();
    var p: Parser = ps_parse(a, "fn f(a: i32, comptime T: type) i64 { return 0; }");
    expect(!p.failed);
    var f: i32 = p.root;
    expect(str_eq(xtext(p, f), "f"));
    var params: i32 = na(p, f);
    expect(chain_len(p, params) == 2);
    var p0: i32 = nth(p, params, 0);
    var p1: i32 = nth(p, params, 1);
    expect(str_eq(xtext(p, p0), "a"));
    expect(!has_flag(p, p0, F_COMPTIME));
    expect(str_eq(xtext(p, na(p, p0)), "i32"));
    expect(str_eq(xtext(p, p1), "T"));
    expect(has_flag(p, p1, F_COMPTIME));
    expect(str_eq(xtext(p, na(p, p1)), "type"));
    // the return type
    expect(str_eq(xtext(p, nb(p, f)), "i64"));
}

test "items: enum explicit values, union payloads, error-set members" {
    var a: Allocator = c_allocator();
    var src: []u8 = "const E = enum { A, B = 5, C };\nconst U = union(enum) { i: i32, s: []u8 };\nconst S = error{ X, Y, Z };";
    var p: Parser = ps_parse(a, src);
    expect(!p.failed);
    var e: i32 = nth(p, p.root, 0);
    var vs: i32 = na(p, e);
    expect(chain_len(p, vs) == 3);
    expect(!has_flag(p, nth(p, vs, 0), F_VAL));
    expect(has_flag(p, nth(p, vs, 1), F_VAL));
    expect(nval(p, nth(p, vs, 1)) == 5);
    expect(str_eq(xtext(p, nth(p, vs, 1)), "B"));
    expect(!has_flag(p, nth(p, vs, 2), F_VAL));
    var un: i32 = nth(p, p.root, 1);
    var uv: i32 = na(p, un);
    expect(chain_len(p, uv) == 2);
    expect(str_eq(xtext(p, nth(p, uv, 0)), "i"));
    expect(str_eq(xtext(p, nth(p, uv, 1)), "s"));
    expect(has_flag(p, na(p, nth(p, uv, 1)), F_SLICE));
    var s: i32 = nth(p, p.root, 2);
    var ms: i32 = na(p, s);
    expect(chain_len(p, ms) == 3);
    expect(str_eq(xtext(p, nth(p, ms, 0)), "X"));
    expect(str_eq(xtext(p, nth(p, ms, 1)), "Y"));
    expect(str_eq(xtext(p, nth(p, ms, 2)), "Z"));
}

test "items: struct fields then methods, method pub flag" {
    var a: Allocator = c_allocator();
    var src: []u8 = "const P = struct { x: i32, y: i32, fn m(self: Self) i32 { return 0; } pub fn n(self: *Self) void {} };";
    var p: Parser = ps_parse(a, src);
    expect(!p.failed);
    var st: i32 = p.root;
    expect(nk(p, st) == ND_STRUCT);
    expect(chain_len(p, na(p, st)) == 2);          // fields
    var methods: i32 = nb(p, st);
    expect(chain_len(p, methods) == 2);
    expect(!has_flag(p, nth(p, methods, 0), F_PUB));
    expect(has_flag(p, nth(p, methods, 1), F_PUB));
    // the pointer-receiver param type is `*Self`
    var n2: i32 = nth(p, methods, 1);
    var recv_ty: i32 = na(p, nth(p, na(p, n2), 0));
    expect(has_flag(p, recv_ty, F_PTR));
    expect(str_eq(xtext(p, recv_ty), "Self"));
}

// --- precedence ------------------------------------------------------------------

test "precedence: mul binds tighter than add" {
    var a: Allocator = c_allocator();
    var p: Parser = ps_parse(a, "const X = 1 + 2 * 3;");
    expect(!p.failed);
    var add: i32 = const_value(p);
    expect(nk(p, add) == ND_BIN);
    expect(nval(p, add) == OPC_ADD);
    expect(nk(p, na(p, add)) == ND_INT);
    expect(nval(p, na(p, add)) == 1);
    var mul: i32 = nb(p, add);
    expect(nval(p, mul) == OPC_MUL);
    expect(nval(p, na(p, mul)) == 2);
    expect(nval(p, nb(p, mul)) == 3);
}

test "precedence: the full ladder or<and<bor<bxor<band<eq" {
    var a: Allocator = c_allocator();
    var p: Parser = ps_parse(a, "const X = a or b and c | d ^ e & f == g;");
    expect(!p.failed);
    var orn: i32 = const_value(p);
    expect(nval(p, orn) == OPC_OR);
    var andn: i32 = nb(p, orn);
    expect(nval(p, andn) == OPC_AND);
    var born: i32 = nb(p, andn);
    expect(nval(p, born) == OPC_BOR);
    var bxorn: i32 = nb(p, born);
    expect(nval(p, bxorn) == OPC_BXOR);
    var bandn: i32 = nb(p, bxorn);
    expect(nval(p, bandn) == OPC_BAND);
    var eqn: i32 = nb(p, bandn);
    expect(nval(p, eqn) == OPC_EQ);
    expect(str_eq(xtext(p, na(p, eqn)), "f"));
    expect(str_eq(xtext(p, nb(p, eqn)), "g"));
}

test "precedence: shift between relational and additive" {
    var a: Allocator = c_allocator();
    // 1 << 2 + 3 < 4  ⇒  (1 << (2 + 3)) < 4
    var p: Parser = ps_parse(a, "const X = 1 << 2 + 3 < 4;");
    expect(!p.failed);
    var lt: i32 = const_value(p);
    expect(nval(p, lt) == OPC_LT);
    var shl: i32 = na(p, lt);
    expect(nval(p, shl) == OPC_SHL);
    var add: i32 = nb(p, shl);
    expect(nval(p, add) == OPC_ADD);
    expect(nval(p, nb(p, lt)) == 4);
}

test "precedence: left associativity of sub" {
    var a: Allocator = c_allocator();
    // 10 - 4 - 3  ⇒  (10 - 4) - 3
    var p: Parser = ps_parse(a, "const X = 10 - 4 - 3;");
    expect(!p.failed);
    var outer: i32 = const_value(p);
    expect(nval(p, outer) == OPC_SUB);
    var inner: i32 = na(p, outer);
    expect(nk(p, inner) == ND_BIN);
    expect(nval(p, inner) == OPC_SUB);
    expect(nval(p, na(p, inner)) == 10);
    expect(nval(p, nb(p, inner)) == 4);
    expect(nval(p, nb(p, outer)) == 3);
}

test "precedence: orelse/catch lowest, left-associative, catch capture" {
    var a: Allocator = c_allocator();
    var p: Parser = ps_parse(a, "const X = o orelse 1 catch 2;\nconst Y = e catch |c| 0;");
    expect(!p.failed);
    var cat: i32 = const_value(p);
    expect(nk(p, cat) == ND_CATCH);
    expect(!has_flag(p, cat, F_CAP));
    var ore: i32 = na(p, cat);
    expect(nk(p, ore) == ND_ORELSE);
    expect(str_eq(xtext(p, na(p, ore)), "o"));
    var y: i32 = nth(p, p.root, 1);
    var cap: i32 = nb(p, y);
    expect(nk(p, cap) == ND_CATCH);
    expect(has_flag(p, cap, F_CAP));
    expect(str_eq(xtext(p, cap), "c"));
}

test "precedence: unary/try/addrof and postfix chains" {
    var a: Allocator = c_allocator();
    var p: Parser = ps_parse(a, "fn f() !void { var x = try g(); var y = &q.b.?; var z = ~h.*.c; }");
    expect(!p.failed);
    var s0: i32 = first_stmt(p);
    var tr: i32 = nb(p, s0);
    expect(nk(p, tr) == ND_TRY);
    expect(nk(p, na(p, tr)) == ND_CALL);
    var s1: i32 = nnext(p, s0);
    var ad: i32 = nb(p, s1);
    expect(nk(p, ad) == ND_ADDROF);
    var uw: i32 = na(p, ad);
    expect(nk(p, uw) == ND_UNWRAP);
    expect(nk(p, na(p, uw)) == ND_FIELD);
    var s2: i32 = nnext(p, s1);
    var un: i32 = nb(p, s2);
    expect(nk(p, un) == ND_UNARY);
    expect(nval(p, un) == UOP_BNOT);
    var fld: i32 = na(p, un);
    expect(nk(p, fld) == ND_FIELD);
    expect(nk(p, na(p, fld)) == ND_DEREF);
}

// --- spans -------------------------------------------------------------------------

test "spans: exact offsets on a hand-laid source" {
    var a: Allocator = c_allocator();
    //                          0         1         2
    //                          0123456789012345678901234567 8
    var src: []u8 = "fn f() void { return 1 + 2; }";
    var p: Parser = ps_parse(a, src);
    expect(!p.failed);
    var f: i32 = p.root;
    expect(noff(p, f) == 0);
    expect(nlen(p, f) == 29);
    var ret_ty: i32 = nb(p, f);
    expect(noff(p, ret_ty) == 7);
    expect(nlen(p, ret_ty) == 4);
    var body: i32 = nc(p, f);
    expect(noff(p, body) == 12);
    expect(nlen(p, body) == 17);
    var ret: i32 = na(p, body);
    expect(noff(p, ret) == 14);
    expect(nlen(p, ret) == 13);                    // `return 1 + 2;`
    var add: i32 = na(p, ret);
    expect(noff(p, add) == 21);
    expect(nlen(p, add) == 5);                     // `1 + 2`
    expect(noff(p, nb(p, add)) == 25);
    expect(nlen(p, nb(p, add)) == 1);
}

test "spans: parentheses do not extend expression spans" {
    var a: Allocator = c_allocator();
    //                          0         1         2         3
    //                          0123456789012345678901234567890123456
    var src: []u8 = "fn g() void { var x = (1 + 2) * 3; }";
    var p: Parser = ps_parse(a, src);
    expect(!p.failed);
    var let: i32 = first_stmt(p);
    expect(nk(p, let) == ND_LET);
    expect(noff(p, let) == 14);
    expect(nlen(p, let) == 20);                    // `var x = (1 + 2) * 3;`
    var mul: i32 = nb(p, let);
    expect(nval(p, mul) == OPC_MUL);
    expect(noff(p, mul) == 23);                    // starts at `1`, NOT `(`
    expect(nlen(p, mul) == 10);                    // `1 + 2) * 3`
    var add: i32 = na(p, mul);
    expect(noff(p, add) == 23);
    expect(nlen(p, add) == 5);                     // `1 + 2`
}

// --- types --------------------------------------------------------------------------

test "types: every prefix form sets its flag" {
    var a: Allocator = c_allocator();
    var p: Parser = ps_parse(a, "fn f(o: ?i32, e: !i64, q: *P, s: []u8, r: [3]i32, n2: [n]T) void {}");
    expect(!p.failed);
    var params: i32 = na(p, p.root);
    expect(chain_len(p, params) == 6);
    var t0: i32 = na(p, nth(p, params, 0));
    expect(has_flag(p, t0, F_OPT));
    expect(str_eq(xtext(p, t0), "i32"));
    var t1: i32 = na(p, nth(p, params, 1));
    expect(has_flag(p, t1, F_ERR));
    expect(!has_flag(p, t1, F_ERRSET));
    var t2: i32 = na(p, nth(p, params, 2));
    expect(has_flag(p, t2, F_PTR));
    var t3: i32 = na(p, nth(p, params, 3));
    expect(has_flag(p, t3, F_SLICE));
    var t4: i32 = na(p, nth(p, params, 4));
    expect(has_flag(p, t4, F_ARRLIT));
    expect(nval(p, t4) == 3);
    var t5: i32 = na(p, nth(p, params, 5));
    expect(has_flag(p, t5, F_ARRPARAM));
    expect(str_eq(ytext(p, t5), "n"));
}

test "types: ctor applications, nesting, and empty argument lists" {
    var a: Allocator = c_allocator();
    var p: Parser = ps_parse(a, "fn f(x: Map(K(i32), V())) void {}");
    expect(!p.failed);
    var t: i32 = na(p, nth(p, na(p, p.root), 0));
    expect(str_eq(xtext(p, t), "Map"));
    expect(has_flag(p, t, F_APP));
    var args: i32 = na(p, t);
    expect(chain_len(p, args) == 2);
    var k: i32 = nth(p, args, 0);
    expect(str_eq(xtext(p, k), "K"));
    expect(has_flag(p, k, F_APP));
    expect(chain_len(p, na(p, k)) == 1);
    expect(str_eq(xtext(p, na(p, k)), "i32"));
    var v: i32 = nth(p, args, 1);
    expect(str_eq(xtext(p, v), "V"));
    expect(has_flag(p, v, F_APP));                 // `V()` — applied, no args
    expect(na(p, v) < 0);
}

test "types: named error union Set!T and @This() in type position" {
    var a: Allocator = c_allocator();
    var src: []u8 = "fn g() E!i32 { return 1; }\nconst P = struct { x: i32, fn m(self: *Self) @This() { return self.*; } };";
    var p: Parser = ps_parse(a, src);
    expect(!p.failed);
    var ret: i32 = nb(p, nth(p, p.root, 0));
    expect(has_flag(p, ret, F_ERR));
    expect(has_flag(p, ret, F_ERRSET));
    expect(str_eq(ytext(p, ret), "E"));
    expect(str_eq(xtext(p, ret), "i32"));
    var st: i32 = nth(p, p.root, 1);
    var m: i32 = nth(p, nb(p, st), 0);
    var mret: i32 = nb(p, m);
    expect(has_flag(p, mret, F_THIS));             // `@This()` ⇒ the name `Self`
    expect(!has_flag(p, mret, F_APP));
}

// --- statements ------------------------------------------------------------------------

test "stmts: let/assign/place-assign with compound operators" {
    var a: Allocator = c_allocator();
    var p: Parser = ps_parse(a, "fn f() void { var v: i32 = 0; v += 2; q.x[0] *= 3; r = 1; }");
    expect(!p.failed);
    var s0: i32 = first_stmt(p);
    expect(nk(p, s0) == ND_LET);
    expect(!has_flag(p, s0, F_CONST));
    expect(na(p, s0) >= 0);                        // the `: i32` annotation
    var s1: i32 = nnext(p, s0);
    expect(nk(p, s1) == ND_ASSIGN);
    expect(nval(p, s1) == OPC_ADD);
    var s2: i32 = nnext(p, s1);
    expect(nk(p, s2) == ND_PASSIGN);
    expect(nval(p, s2) == OPC_MUL);
    expect(nk(p, na(p, s2)) == ND_INDEX);
    var s3: i32 = nnext(p, s2);
    expect(nk(p, s3) == ND_ASSIGN);
    expect(nval(p, s3) < 0);                       // plain `=`
}

test "stmts: if/else-if chains and the optional capture" {
    var a: Allocator = c_allocator();
    var p: Parser = ps_parse(a, "fn f() void { if (o) |v| { } else if (c) { } else { } }");
    expect(!p.failed);
    var ifn: i32 = first_stmt(p);
    expect(nk(p, ifn) == ND_IF);
    expect(has_flag(p, ifn, F_CAP));
    expect(str_eq(xtext(p, ifn), "v"));
    var els: i32 = nc(p, ifn);
    expect(nk(p, els) == ND_IF);                   // the chained else-if
    expect(!has_flag(p, els, F_CAP));
    expect(nk(p, nc(p, els)) == ND_BLOCK);         // its final else block
}

test "stmts: while with continue-clause and label, break/continue targets" {
    var a: Allocator = c_allocator();
    var p: Parser = ps_parse(a, "fn f() void { outer: while (c) : (i += 1) { break :outer; continue; } }");
    expect(!p.failed);
    var w: i32 = first_stmt(p);
    expect(nk(p, w) == ND_WHILE);
    expect(has_flag(p, w, F_LABEL));
    expect(str_eq(xtext(p, w), "outer"));
    expect(noff(p, w) == 14);                      // the span starts at the label
    var cont: i32 = nb(p, w);
    expect(nk(p, cont) == ND_ASSIGN);
    expect(nval(p, cont) == OPC_ADD);
    var body: i32 = nc(p, w);
    var brk: i32 = na(p, body);
    expect(nk(p, brk) == ND_BREAK);
    expect(has_flag(p, brk, F_LABEL));
    expect(str_eq(xtext(p, brk), "outer"));
    var cnt: i32 = nnext(p, brk);
    expect(nk(p, cnt) == ND_CONTINUE);
    expect(!has_flag(p, cnt, F_LABEL));
}

test "stmts: for with index form, captures and label" {
    var a: Allocator = c_allocator();
    var p: Parser = ps_parse(a, "fn f() void { lp: for (xs, 0..) |x, i| { } for (ys) |y| { } }");
    expect(!p.failed);
    var f0: i32 = first_stmt(p);
    expect(nk(p, f0) == ND_FOR);
    expect(str_eq(xtext(p, f0), "x"));
    expect(has_flag(p, f0, F_IDX));
    expect(str_eq(ytext(p, f0), "i"));
    expect(has_flag(p, f0, F_LABEL));
    expect(str_eq(ztext(p, f0), "lp"));
    var f1: i32 = nnext(p, f0);
    expect(nk(p, f1) == ND_FOR);
    expect(!has_flag(p, f1, F_IDX));
    expect(!has_flag(p, f1, F_LABEL));
    expect(str_eq(xtext(p, f1), "y"));
}

test "stmts: defer/errdefer wrap one statement" {
    var a: Allocator = c_allocator();
    var p: Parser = ps_parse(a, "fn f() void { defer g(); errdefer { h(); } }");
    expect(!p.failed);
    var d: i32 = first_stmt(p);
    expect(nk(p, d) == ND_DEFER);
    expect(nk(p, na(p, d)) == ND_CALL);
    var ed: i32 = nnext(p, d);
    expect(nk(p, ed) == ND_ERRDEFER);
    expect(nk(p, na(p, ed)) == ND_BLOCK);
}

test "stmts: switch arms with multi-labels, ranges, captures and else" {
    var a: Allocator = c_allocator();
    var p: Parser = ps_parse(a, "fn f() void { switch (x) { 1, 2 => {}, 3..5, .A => |v| {}, else => {} } }");
    expect(!p.failed);
    var sw: i32 = first_stmt(p);
    expect(nk(p, sw) == ND_SWITCH);
    expect(nk(p, na(p, sw)) == ND_IDENT);          // scrutinee
    var arms: i32 = nb(p, sw);
    expect(chain_len(p, arms) == 2);
    var a0: i32 = nth(p, arms, 0);
    expect(chain_len(p, na(p, a0)) == 2);          // labels 1, 2
    expect(nb(p, a0) < 0);                         // no ranges
    expect(!has_flag(p, a0, F_CAP));
    var a1: i32 = nth(p, arms, 1);
    expect(chain_len(p, na(p, a1)) == 1);          // the `.A` label
    expect(nk(p, na(p, a1)) == ND_ENUMLIT);
    var r: i32 = nb(p, a1);
    expect(chain_len(p, r) == 1);                  // the 3..5 range
    expect(nval(p, r) == 3);
    expect(nval2(p, r) == 5);
    expect(has_flag(p, a1, F_CAP));
    expect(str_eq(xtext(p, a1), "v"));
    expect(nc(p, sw) >= 0);                        // the else default block
}

// --- expressions -------------------------------------------------------------------------

test "exprs: primaries — literals, null, unreachable, error/enum literals" {
    var a: Allocator = c_allocator();
    var p: Parser = ps_parse(a, "fn f() void { var s = \"hi\"; var fl = 1.5; var t = true; var n = null; var e = error.Oops; var v = .Variant; unreachable; }");
    expect(!p.failed);
    var s0: i32 = first_stmt(p);
    expect(nk(p, nb(p, s0)) == ND_STR);
    var s1: i32 = nnext(p, s0);
    expect(nk(p, nb(p, s1)) == ND_FLOAT);
    var s2: i32 = nnext(p, s1);
    expect(nk(p, nb(p, s2)) == ND_BOOL);
    expect(nval(p, nb(p, s2)) == 1);
    var s3: i32 = nnext(p, s2);
    expect(nk(p, nb(p, s3)) == ND_NULL);
    var s4: i32 = nnext(p, s3);
    expect(nk(p, nb(p, s4)) == ND_ERRLIT);
    expect(str_eq(xtext(p, nb(p, s4)), "Oops"));
    var s5: i32 = nnext(p, s4);
    expect(nk(p, nb(p, s5)) == ND_ENUMLIT);
    expect(str_eq(xtext(p, nb(p, s5)), "Variant"));
    var s6: i32 = nnext(p, s5);
    expect(nk(p, s6) == ND_UNREACHABLE);
}

test "exprs: calls, method calls, builtins, struct and array literals" {
    var a: Allocator = c_allocator();
    var p: Parser = ps_parse(a, "fn f() void { g(1, 2); o.m(3); var s = @sizeOf(i32); var q = P{ .x = 1, }; var r = [2]i32{ 4, 5 }; var sl = xs[1..3]; }");
    expect(!p.failed);
    var s0: i32 = first_stmt(p);
    expect(nk(p, s0) == ND_CALL);
    expect(str_eq(xtext(p, s0), "g"));
    expect(chain_len(p, na(p, s0)) == 2);
    var s1: i32 = nnext(p, s0);
    expect(nk(p, s1) == ND_MCALL);
    expect(str_eq(xtext(p, s1), "m"));
    expect(nk(p, na(p, s1)) == ND_IDENT);          // receiver `o`
    expect(chain_len(p, nb(p, s1)) == 1);
    var s2: i32 = nnext(p, s1);
    var bi: i32 = nb(p, s2);
    expect(nk(p, bi) == ND_BUILTIN);
    expect(str_eq(xtext(p, bi), "sizeOf"));
    var s3: i32 = nnext(p, s2);
    var sl3: i32 = nb(p, s3);
    expect(nk(p, sl3) == ND_SLIT);
    expect(str_eq(xtext(p, sl3), "P"));
    var fi: i32 = na(p, sl3);
    expect(nk(p, fi) == ND_FINIT);
    expect(str_eq(xtext(p, fi), "x"));
    var s4: i32 = nnext(p, s3);
    var al: i32 = nb(p, s4);
    expect(nk(p, al) == ND_ALIT);
    expect(nk(p, na(p, al)) == ND_TYPE);
    expect(chain_len(p, nb(p, al)) == 2);
    var s5: i32 = nnext(p, s4);
    var sx: i32 = nb(p, s5);
    expect(nk(p, sx) == ND_SLICEX);
    expect(nval(p, nb(p, sx)) == 1);
    expect(nval(p, nc(p, sx)) == 3);
}

test "exprs: anonymous struct type value with fields and methods" {
    var a: Allocator = c_allocator();
    var p: Parser = ps_parse(a, "fn F(comptime T: type) type { return struct { v: T, fn get(self: Self) T { return self.v; } }; }");
    expect(!p.failed);
    var ret: i32 = first_stmt(p);
    expect(nk(p, ret) == ND_RETURN);
    var st: i32 = na(p, ret);
    expect(nk(p, st) == ND_STRUCTTYPE);
    expect(chain_len(p, na(p, st)) == 1);          // one field
    expect(chain_len(p, nb(p, st)) == 1);          // one method
    expect(str_eq(xtext(p, nth(p, nb(p, st), 0)), "get"));
}

// --- errors --------------------------------------------------------------------------------

test "errors: missing semicolon is E0200 at the closing brace" {
    var a: Allocator = c_allocator();
    //                          0         1         2
    //                          012345678901234567890123
    var p: Parser = ps_parse(a, "fn f() void { return 1 }");
    expect(p.failed);
    expect(p.ecode == 200);
    expect(p.epos == 23);
}

test "errors: pub test is E0201 at `test`, and parsing continues" {
    var a: Allocator = c_allocator();
    var p: Parser = ps_parse(a, "pub test \"t\" {}");
    expect(p.failed);
    expect(p.ecode == 201);
    expect(p.epos == 4);
    // non-fatal: the module still carries the test item.
    expect(p.root >= 0);
    expect(nk(p, p.root) == ND_TEST);
}

test "errors: for-loop shape constraints report E0200 at the exact spot" {
    var a: Allocator = c_allocator();
    //                           0         1         2
    //                           0123456789012345678901234
    var p: Parser = ps_parse(a, "fn f() void { for (xs, 1..) |x, i| {} }");
    expect(p.failed);
    expect(p.ecode == 200);
    expect(p.epos == 23);                          // the non-zero `1`
    var p2: Parser = ps_parse(a, "fn f() void { for (xs) |x, i| {} }");
    expect(p2.failed);
    expect(p2.ecode == 200);
    expect(p2.epos == 23);                         // the opening `|`
}

test "errors: a stray token at top level and at expression start" {
    var a: Allocator = c_allocator();
    var p: Parser = ps_parse(a, "fn f() void {}\n+");
    expect(p.failed);
    expect(p.ecode == 200);
    expect(p.epos == 15);
    var p2: Parser = ps_parse(a, "fn f() void { var x = ; }");
    expect(p2.failed);
    expect(p2.ecode == 200);
    expect(p2.epos == 22);                         // the `;` where an expr begins
}

test "errors: first error wins across recovery-order differences" {
    var a: Allocator = c_allocator();
    // E0201 (pub test) comes FIRST in source order; the later hard error
    // must not displace it.
    var p: Parser = ps_parse(a, "pub test \"t\" {}\nfn f( void {}");
    expect(p.failed);
    expect(p.ecode == 201);
    expect(p.epos == 4);
    // …and a hard error BEFORE a pub test stays the first.
    var p2: Parser = ps_parse(a, "fn f( void {}\npub test \"t\" {}");
    expect(p2.failed);
    expect(p2.ecode == 200);
    // `void` is an identifier (type names are not keywords), so the param
    // name parses and the failure is the missing `:` at the `{`.
    expect(p2.epos == 11);
}
