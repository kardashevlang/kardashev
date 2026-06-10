// parser.ks — self-host stage 2 (v0.160): the kardashev parser, written in
// kardashev.
//
// A decision-for-decision replica of `crates/kardc/src/parser.rs` (the Rust
// reference implementation of SPEC §2 and the per-feature grammar sections),
// consuming the v0.159 self-hosted lexer (`selfhost/lexer.ks`) and producing
// the arena AST of `selfhost/ast.ks`. It is differentially tested against
// the Rust parser byte-for-byte by `crates/kardc/tests/selfhost_parser.rs`
// via the canonical dump rendered by `selfhost/astdump.ks`.
//
// Faithfulness contract:
// - Same lookahead decisions: `peek2`/`peek3` clamped to the trailing EOF
//   token, the labeled-loop three-token lookahead, the `const` item dispatch
//   on the token after `=`, the assignment-operator statement dispatch.
// - Same precedence ladder: `orelse`/`catch` under ten `parse_binary` levels
//   (`or` < `and` < `|` < `^` < `&` < `==`/`!=` < relational < shifts <
//   additive < multiplicative), then unary, `comptime`, postfix, primary.
// - Same span-merge rules: every node's `off/len` equals the Rust node's
//   `Span` (notably: parenthesised expressions do NOT extend spans).
// - First error wins, then STOP: the Rust parser recovers (`sync_item` /
//   `sync_stmt`) and collects several diagnostics, but it pushes them in
//   strict source order, so its FIRST diagnostic — the only artifact the
//   differential compares — always coincides with this parser's first
//   recorded error. The one non-fatal diagnostic is E0201 (`pub test`):
//   both parsers record it and KEEP parsing, exactly like the reference.
//   Error codes are numeric: 200 (E0200 "expected …" and the shape
//   constraints that reuse it) and 201 (E0201).
//
// Memory: nodes live in a growable arena owned by the Parser; nothing else
// is allocated (all strings are spans into the borrowed source).

@import("lexer.ks");
@import("ast.ks");
@import("std");

// --- free helpers -------------------------------------------------------------

/// Decode the digits `src[off .. off+len]` as an i64. The lexer only emits
/// TK_INT for literals that fit (E0002 otherwise), so plain accumulation is
/// overflow-safe; leading zeros decode like Rust's `str::parse::<i64>()`.
fn ps_int_value(src: []u8, off: usize, len: usize) i64 {
    var acc: i64 = 0;
    var i: usize = 0;
    while (i < len) : (i += 1) {
        acc = acc * 10 + (@as(i64, src[off + i]) - 48);
    }
    return acc;
}

/// Classify an assignment-operator token (parser.rs `assign_op_kind`):
/// -2 = not an assignment operator, -1 = plain `=`, else the OPC_* code of a
/// compound `+= -= *= /= %=`.
fn ps_assign_op(k: u8) i64 {
    if (k == TK_EQ) { return 0 - 1; }
    if (k == TK_PLUSEQ) { return OPC_ADD; }
    if (k == TK_MINUSEQ) { return OPC_SUB; }
    if (k == TK_STAREQ) { return OPC_MUL; }
    if (k == TK_SLASHEQ) { return OPC_DIV; }
    if (k == TK_PERCENTEQ) { return OPC_REM; }
    return 0 - 2;
}

// --- the parser -----------------------------------------------------------------

/// The number of binary-operator precedence levels (parser.rs `BIN_LEVELS`):
/// level 0 is the loosest (`or`), level 9 the tightest (`* / %`).
pub const PS_BIN_LEVELS: i64 = 10;

/// The kardashev parser: a recursive-descent walk over a pre-lexed token
/// buffer (`toks`, terminated by a TK_EOF token), building nodes into the
/// `nodes` arena. After `parse_module`, `failed`/`ecode`/`epos` carry the
/// first diagnostic (if any) — see the module header for the error contract.
pub const Parser = struct {
    /// The source text (borrowed; all name spans index into it).
    src: []u8,
    /// The token buffer, ending with TK_EOF. Never empty.
    toks: []Token,
    /// Cursor into `toks` (never advanced past the final EOF).
    pos: usize,
    /// The node arena; `count` entries are live.
    nodes: []Node,
    count: usize,
    /// First-diagnostic state: code 200/201 at byte `epos`.
    failed: bool,
    ecode: i64,
    epos: usize,
    /// Span of the most recently `expect`-consumed token.
    last_off: usize,
    last_len: usize,
    /// Outputs of `parse_struct_body` (consumed immediately by its callers).
    sb_fields: i32,
    sb_methods: i32,
    /// The item-chain head after a successful `parse_module` (-1 before, and
    /// for an empty module).
    root: i32,

    /// A parser over `src`/`toks` with a fresh arena.
    fn init(a: Allocator, src: []u8, toks: []Token) Self {
        return Self{
            .src = src,
            .toks = toks,
            .pos = 0,
            .nodes = alloc(a, Node, 256),
            .count = 0,
            .failed = false,
            .ecode = 0,
            .epos = 0,
            .last_off = 0,
            .last_len = 0,
            .sb_fields = 0 - 1,
            .sb_methods = 0 - 1,
            .root = 0 - 1,
        };
    }

    // ---- arena -----------------------------------------------------------

    /// Append a fresh node of `kind` spanning `[off, end)` (doubling growth);
    /// links start at -1, spans/values/flags at 0. Returns its index.
    fn push_node(self: *Self, a: Allocator, kind: u8, off: usize, end: usize) i32 {
        if (self.count == self.nodes.len) {
            var grown: []Node = alloc(a, Node, self.nodes.len * 2);
            var i: usize = 0;
            while (i < self.count) : (i += 1) {
                grown[i] = self.nodes[i];
            }
            free(a, self.nodes);
            self.nodes = grown;
        }
        self.nodes[self.count] = Node{
            .kind = kind,
            .a = 0 - 1,
            .b = 0 - 1,
            .c = 0 - 1,
            .next = 0 - 1,
            .off = off,
            .len = end - off,
            .xoff = 0,
            .xlen = 0,
            .yoff = 0,
            .ylen = 0,
            .zoff = 0,
            .zlen = 0,
            .val = 0,
            .val2 = 0,
            .flags = 0,
        };
        self.count += 1;
        return @as(i32, self.count - 1);
    }

    /// Span start of node `n`.
    fn nstart(self: *Self, n: i32) usize {
        return self.nodes[@as(usize, n)].off;
    }

    /// Span end (exclusive) of node `n`.
    fn nend(self: *Self, n: i32) usize {
        var u: usize = @as(usize, n);
        return self.nodes[u].off + self.nodes[u].len;
    }

    /// Set the child links of node `n` (-1 = absent).
    fn set_abc(self: *Self, n: i32, av: i32, bv: i32, cv: i32) void {
        var u: usize = @as(usize, n);
        self.nodes[u].a = av;
        self.nodes[u].b = bv;
        self.nodes[u].c = cv;
    }

    /// Set the primary name span of node `n`.
    fn set_x(self: *Self, n: i32, off: usize, len: usize) void {
        var u: usize = @as(usize, n);
        self.nodes[u].xoff = off;
        self.nodes[u].xlen = len;
    }

    /// Set the secondary name span of node `n`.
    fn set_y(self: *Self, n: i32, off: usize, len: usize) void {
        var u: usize = @as(usize, n);
        self.nodes[u].yoff = off;
        self.nodes[u].ylen = len;
    }

    /// Set the tertiary name span of node `n`.
    fn set_z(self: *Self, n: i32, off: usize, len: usize) void {
        var u: usize = @as(usize, n);
        self.nodes[u].zoff = off;
        self.nodes[u].zlen = len;
    }

    /// Or `f` into node `n`'s flags.
    fn add_flag(self: *Self, n: i32, f: i64) void {
        var u: usize = @as(usize, n);
        self.nodes[u].flags = self.nodes[u].flags | f;
    }

    /// Set the integer payload of node `n`.
    fn set_val(self: *Self, n: i32, v: i64) void {
        self.nodes[@as(usize, n)].val = v;
    }

    /// Widen node `n`'s span to cover `[off, end)` merged with its own
    /// (parser.rs `Span::merge`: min start, max end).
    fn widen(self: *Self, n: i32, off: usize, end: usize) void {
        var u: usize = @as(usize, n);
        var s: usize = self.nodes[u].off;
        var e: usize = s + self.nodes[u].len;
        if (off < s) { s = off; }
        if (end > e) { e = end; }
        self.nodes[u].off = s;
        self.nodes[u].len = e - s;
    }

    // ---- cursor helpers ----------------------------------------------------

    fn peek_kind(self: *Self) u8 {
        return self.toks[self.pos].kind;
    }

    fn peek_off(self: *Self) usize {
        return self.toks[self.pos].off;
    }

    fn peek_len(self: *Self) usize {
        return self.toks[self.pos].len;
    }

    /// The kind one token ahead (clamped to the trailing EOF).
    fn peek2_kind(self: *Self) u8 {
        var i: usize = self.pos + 1;
        if (i >= self.toks.len) { i = self.toks.len - 1; }
        return self.toks[i].kind;
    }

    /// The kind two tokens ahead (clamped to the trailing EOF).
    fn peek3_kind(self: *Self) u8 {
        var i: usize = self.pos + 2;
        if (i >= self.toks.len) { i = self.toks.len - 1; }
        return self.toks[i].kind;
    }

    /// Advance one token, never past the final EOF.
    fn bump(self: *Self) void {
        if (self.pos + 1 < self.toks.len) {
            self.pos += 1;
        }
    }

    fn at_eof(self: *Self) bool {
        return self.peek_kind() == TK_EOF;
    }

    fn at(self: *Self, k: u8) bool {
        return self.peek_kind() == k;
    }

    fn eat(self: *Self, k: u8) bool {
        if (self.at(k)) {
            self.bump();
            return true;
        }
        return false;
    }

    /// Record the FIRST diagnostic (later ones keep the first — the Rust
    /// parser pushes diagnostics in source order, so its `diags[0]` is this).
    fn note(self: *Self, code: i64, p: usize) void {
        if (!self.failed) {
            self.failed = true;
            self.ecode = code;
            self.epos = p;
        }
    }

    /// Consume a token of kind `k`, recording its span in `last_off/last_len`,
    /// or record E0200 at the current token (parser.rs `expect_punct`).
    fn expect(self: *Self, k: u8) !void {
        if (!self.at(k)) {
            self.note(200, self.peek_off());
            return error.Parse;
        }
        self.last_off = self.peek_off();
        self.last_len = self.peek_len();
        self.bump();
    }

    /// Consume an identifier (span in `last_off/last_len`) or record E0200.
    fn expect_ident(self: *Self) !void {
        if (!self.at(TK_IDENT)) {
            self.note(200, self.peek_off());
            return error.Parse;
        }
        self.last_off = self.peek_off();
        self.last_len = self.peek_len();
        self.bump();
    }

    /// The current token's text.
    fn peek_text(self: *Self) []u8 {
        var o: usize = self.peek_off();
        return self.src[o..o + self.peek_len()];
    }

    // ---- items --------------------------------------------------------------

    /// Parse the whole token stream into a chain of items; returns the head
    /// (-1 for an empty module). Unlike the Rust parser there is no recovery:
    /// the first hard error stops the walk (header contract).
    fn parse_module(self: *Self, a: Allocator) !i32 {
        var ch: Chain = Chain.init();
        while (!self.at_eof()) {
            var it: i32 = try self.parse_item(a);
            ch.add(self.nodes, it);
        }
        self.root = ch.head;
        return ch.head;
    }

    fn parse_item(self: *Self, a: Allocator) !i32 {
        var start: usize = self.peek_off();
        // `@import("path");` — dispatched before the optional `pub`.
        if (self.at(TK_AT)) {
            return try self.parse_import(a, start);
        }
        var is_pub: bool = self.eat(TK_KW_PUB);
        var k: u8 = self.peek_kind();
        if (k == TK_KW_FN) {
            return try self.parse_func_decl(a, is_pub, start);
        }
        if (k == TK_KW_CONST) {
            return try self.parse_const(a, is_pub, start);
        }
        if (k == TK_KW_TEST) {
            if (is_pub) {
                // E0201: `test` blocks cannot be `pub`. NON-fatal — recorded
                // at the `test` keyword, then parsing continues (parser.rs).
                self.note(201, self.peek_off());
            }
            return try self.parse_test(a, start);
        }
        self.note(200, self.peek_off());
        return error.Parse;
    }

    /// `@import("path");` — the cursor is on `@` (parser.rs `parse_import`).
    fn parse_import(self: *Self, a: Allocator, start: usize) !i32 {
        self.bump(); // `@`
        var is_import: bool = false;
        if (self.at(TK_IDENT)) {
            if (str_eq(self.peek_text(), "import")) {
                is_import = true;
            }
        }
        if (!is_import) {
            self.note(200, self.peek_off());
            return error.Parse;
        }
        self.bump(); // `import`
        try self.expect(TK_LPAREN);
        try self.expect(TK_STR);
        var path_off: usize = self.last_off;
        var path_len: usize = self.last_len;
        try self.expect(TK_RPAREN);
        try self.expect(TK_SEMICOLON);
        var n: i32 = self.push_node(a, ND_IMPORT, start, self.last_off + self.last_len);
        self.set_x(n, path_off, path_len);
        return n;
    }

    /// A function definition with `pub` consumed and the cursor on `fn` —
    /// shared by top-level functions, struct methods, and anonymous
    /// struct-type methods (parser.rs `parse_func_decl`).
    fn parse_func_decl(self: *Self, a: Allocator, is_pub: bool, start: usize) !i32 {
        self.bump(); // `fn`
        try self.expect_ident();
        var name_off: usize = self.last_off;
        var name_len: usize = self.last_len;
        try self.expect(TK_LPAREN);
        var params: i32 = try self.parse_params(a);
        try self.expect(TK_RPAREN);
        var ret: i32 = try self.parse_type(a);
        var body: i32 = try self.parse_block(a);
        var n: i32 = self.push_node(a, ND_FN, start, self.nend(body));
        self.set_abc(n, params, ret, body);
        self.set_x(n, name_off, name_len);
        if (is_pub) {
            self.add_flag(n, F_PUB);
        }
        return n;
    }

    /// The parameter list, cursor just after `(`; stops at (without
    /// consuming) the `)`. Returns the PARAM chain head (-1 when empty).
    /// Trailing comma allowed (parser.rs `parse_params`).
    fn parse_params(self: *Self, a: Allocator) !i32 {
        var ch: Chain = Chain.init();
        if (self.at(TK_RPAREN)) {
            return ch.head;
        }
        while (true) {
            var is_ct: bool = false;
            var start: usize = 0;
            if (self.at(TK_KW_COMPTIME)) {
                is_ct = true;
                start = self.peek_off();
                self.bump(); // `comptime`
            }
            try self.expect_ident();
            var name_off: usize = self.last_off;
            var name_len: usize = self.last_len;
            if (!is_ct) {
                start = name_off;
            }
            try self.expect(TK_COLON);
            var ty: i32 = try self.parse_type(a);
            var p: i32 = self.push_node(a, ND_PARAM, start, self.nend(ty));
            self.set_abc(p, ty, 0 - 1, 0 - 1);
            self.set_x(p, name_off, name_len);
            if (is_ct) {
                self.add_flag(p, F_COMPTIME);
            }
            ch.add(self.nodes, p);
            if (self.eat(TK_COMMA)) {
                if (self.at(TK_RPAREN)) {
                    break; // trailing comma
                }
            } else {
                break;
            }
        }
        return ch.head;
    }

    // ---- types ----------------------------------------------------------------

    /// A type *base*: a name (or `@This()`) optionally followed by a
    /// type-constructor application `Name(A, B, …)` (parser.rs
    /// `parse_type_name` + `parse_type_base`). Returns an ND_TYPE node with
    /// only the base name / F_THIS / F_APP+args set; the prefix forms are
    /// layered on by `parse_type`.
    fn parse_type_base(self: *Self, a: Allocator) !i32 {
        var is_this: bool = self.at(TK_AT);
        var name_off: usize = 0;
        var name_len: usize = 0;
        var start: usize = 0;
        var end: usize = 0;
        if (is_this) {
            // `@This()` — the only `@`-builtin valid in type position; it
            // desugars to the NAME `Self` (F_THIS) spanning the whole form.
            start = self.peek_off();
            self.bump(); // `@`
            var ok_this: bool = false;
            if (self.at(TK_IDENT)) {
                if (str_eq(self.peek_text(), "This")) {
                    ok_this = true;
                }
            }
            if (!ok_this) {
                self.note(200, self.peek_off());
                return error.Parse;
            }
            self.bump(); // `This`
            try self.expect(TK_LPAREN);
            try self.expect(TK_RPAREN);
            end = self.last_off + self.last_len;
        } else {
            try self.expect_ident();
            name_off = self.last_off;
            name_len = self.last_len;
            start = name_off;
            end = name_off + name_len;
        }
        var has_args: bool = false;
        var args: i32 = 0 - 1;
        if (!is_this and self.at(TK_LPAREN)) {
            // A `(` after a plain base name in type position is unambiguous:
            // a generic application. Arguments are themselves bases (a bare
            // name or a nested application) — NO trailing comma (an arg parse
            // after a trailing `,` fails at `)`, exactly like the reference).
            has_args = true;
            self.bump(); // `(`
            var ch: Chain = Chain.init();
            if (!self.at(TK_RPAREN)) {
                while (true) {
                    var arg: i32 = try self.parse_type_base(a);
                    ch.add(self.nodes, arg);
                    if (!self.eat(TK_COMMA)) {
                        break;
                    }
                }
            }
            args = ch.head;
            try self.expect(TK_RPAREN);
            end = self.last_off + self.last_len;
        }
        var t: i32 = self.push_node(a, ND_TYPE, start, end);
        self.set_x(t, name_off, name_len);
        if (is_this) {
            self.add_flag(t, F_THIS);
        }
        if (has_args) {
            self.add_flag(t, F_APP);
            self.nodes[@as(usize, t)].a = args;
        }
        return t;
    }

    /// A full type reference (parser.rs `parse_type`): one of the leading
    /// forms `*T`, `[]T`, `[N]T`, `?T`, `!T`, the named error union `Set!T`,
    /// or a bare base — each prefix wrapping a `parse_type_base`.
    fn parse_type(self: *Self, a: Allocator) !i32 {
        if (self.at(TK_STAR)) {
            var star_off: usize = self.peek_off();
            self.bump(); // `*`
            var t: i32 = try self.parse_type_base(a);
            self.add_flag(t, F_PTR);
            self.widen(t, star_off, star_off);
            return t;
        }
        if (self.at(TK_LBRACKET)) {
            var lb_off: usize = self.peek_off();
            self.bump(); // `[`
            if (self.at(TK_RBRACKET)) {
                self.bump(); // `]`
                var t: i32 = try self.parse_type_base(a);
                self.add_flag(t, F_SLICE);
                self.widen(t, lb_off, lb_off);
                return t;
            }
            // The array size: an integer literal `[3]T` or a comptime
            // value-parameter name `[n]T` (parser.rs `parse_array_size`).
            var is_lit: bool = false;
            var lit: i64 = 0;
            var p_off: usize = 0;
            var p_len: usize = 0;
            if (self.at(TK_INT)) {
                is_lit = true;
                lit = ps_int_value(self.src, self.peek_off(), self.peek_len());
                self.bump();
            } else if (self.at(TK_IDENT)) {
                p_off = self.peek_off();
                p_len = self.peek_len();
                self.bump();
            } else {
                self.note(200, self.peek_off());
                return error.Parse;
            }
            try self.expect(TK_RBRACKET);
            var t2: i32 = try self.parse_type_base(a);
            if (is_lit) {
                self.add_flag(t2, F_ARRLIT);
                self.set_val(t2, lit);
            } else {
                self.add_flag(t2, F_ARRPARAM);
                self.set_y(t2, p_off, p_len);
            }
            self.widen(t2, lb_off, lb_off);
            return t2;
        }
        var has_opt: bool = false;
        var opt_off: usize = 0;
        if (self.at(TK_QUESTION)) {
            has_opt = true;
            opt_off = self.peek_off();
            self.bump();
        }
        // Only a `!` prefix when there was no `?` — never both on one type.
        var has_err: bool = false;
        var err_off: usize = 0;
        if (!has_opt and self.at(TK_BANG)) {
            has_err = true;
            err_off = self.peek_off();
            self.bump();
        }
        var base: i32 = try self.parse_type_base(a);
        // `Set!T` — a NAMED error union: base name then `!`, only when no
        // prefix was consumed and the base is not an application.
        if (!has_opt and !has_err) {
            if ((self.nodes[@as(usize, base)].flags & F_APP) == 0 and self.at(TK_BANG)) {
                self.bump(); // `!`
                var set_off: usize = self.nodes[@as(usize, base)].xoff;
                var set_len: usize = self.nodes[@as(usize, base)].xlen;
                var set_this: bool = (self.nodes[@as(usize, base)].flags & F_THIS) != 0;
                var set_start: usize = self.nstart(base);
                var pay: i32 = try self.parse_type_base(a);
                self.add_flag(pay, F_ERR);
                self.add_flag(pay, F_ERRSET);
                self.set_y(pay, set_off, set_len);
                if (set_this) {
                    self.add_flag(pay, F_ESETTHIS);
                }
                self.widen(pay, set_start, set_start);
                return pay;
            }
        }
        if (has_opt) {
            self.add_flag(base, F_OPT);
            self.widen(base, opt_off, opt_off);
        }
        if (has_err) {
            self.add_flag(base, F_ERR);
            self.widen(base, err_off, err_off);
        }
        return base;
    }

    // ---- const-introduced items -------------------------------------------------

    /// `const IDENT …` — a typed/inferred value const, or (via the token
    /// after `=`) a struct / enum / union / error-set declaration
    /// (parser.rs `parse_const`).
    fn parse_const(self: *Self, a: Allocator, is_pub: bool, start: usize) !i32 {
        self.bump(); // `const`
        try self.expect_ident();
        var name_off: usize = self.last_off;
        var name_len: usize = self.last_len;
        if (self.at(TK_EQ)) {
            var k2: u8 = self.peek2_kind();
            if (k2 == TK_KW_STRUCT) {
                return try self.parse_struct_decl(a, is_pub, name_off, name_len, start);
            }
            if (k2 == TK_KW_ENUM) {
                return try self.parse_enum_decl(a, is_pub, name_off, name_len, start);
            }
            if (k2 == TK_KW_UNION) {
                return try self.parse_union_decl(a, is_pub, name_off, name_len, start);
            }
            // `= error {` is a named error-set declaration; `= error .` is an
            // error-literal value const (falls through to the value path).
            if (k2 == TK_KW_ERROR and self.peek3_kind() == TK_LBRACE) {
                return try self.parse_error_set_decl(a, is_pub, name_off, name_len, start);
            }
            // Inferred value const `const IDENT = expr ;` (no annotation).
            self.bump(); // `=`
            var v: i32 = try self.parse_expr(a);
            try self.expect(TK_SEMICOLON);
            var n: i32 = self.push_node(a, ND_CONST, start, self.last_off + self.last_len);
            self.set_abc(n, 0 - 1, v, 0 - 1);
            self.set_x(n, name_off, name_len);
            if (is_pub) {
                self.add_flag(n, F_PUB);
            }
            return n;
        }
        // Annotated value const `const IDENT : type = expr ;`.
        try self.expect(TK_COLON);
        var ty: i32 = try self.parse_type(a);
        try self.expect(TK_EQ);
        var v2: i32 = try self.parse_expr(a);
        try self.expect(TK_SEMICOLON);
        var n2: i32 = self.push_node(a, ND_CONST, start, self.last_off + self.last_len);
        self.set_abc(n2, ty, v2, 0 - 1);
        self.set_x(n2, name_off, name_len);
        if (is_pub) {
            self.add_flag(n2, F_PUB);
        }
        return n2;
    }

    /// The fields-then-methods body of a `struct { … }`, cursor just after
    /// `{`; stops at (without consuming) the `}`. Outputs via
    /// `sb_fields`/`sb_methods` — consumed immediately by both callers
    /// (parser.rs `parse_struct_body`).
    fn parse_struct_body(self: *Self, a: Allocator) !void {
        var fields: Chain = Chain.init();
        while (!self.at(TK_RBRACE) and !self.at(TK_KW_FN) and !self.at(TK_KW_PUB)) {
            try self.expect_ident();
            var f_off: usize = self.last_off;
            var f_len: usize = self.last_len;
            try self.expect(TK_COLON);
            var ty: i32 = try self.parse_type(a);
            var f: i32 = self.push_node(a, ND_SFIELD, f_off, self.nend(ty));
            self.set_abc(f, ty, 0 - 1, 0 - 1);
            self.set_x(f, f_off, f_len);
            fields.add(self.nodes, f);
            if (!self.eat(TK_COMMA)) {
                break; // no separator → the field list is done
            }
        }
        var methods: Chain = Chain.init();
        while (!self.at(TK_RBRACE)) {
            var m_start: usize = self.peek_off();
            var m_pub: bool = self.eat(TK_KW_PUB);
            if (!self.at(TK_KW_FN)) {
                self.note(200, self.peek_off());
                return error.Parse;
            }
            var m: i32 = try self.parse_func_decl(a, m_pub, m_start);
            methods.add(self.nodes, m);
        }
        self.sb_fields = fields.head;
        self.sb_methods = methods.head;
    }

    /// `= struct { … } ;` with `const IDENT` consumed, cursor on the `=`.
    fn parse_struct_decl(self: *Self, a: Allocator, is_pub: bool, name_off: usize, name_len: usize, start: usize) !i32 {
        self.bump(); // `=`
        self.bump(); // `struct` (dispatch already verified it)
        try self.expect(TK_LBRACE);
        try self.parse_struct_body(a);
        var fields: i32 = self.sb_fields;
        var methods: i32 = self.sb_methods;
        try self.expect(TK_RBRACE);
        try self.expect(TK_SEMICOLON);
        var n: i32 = self.push_node(a, ND_STRUCT, start, self.last_off + self.last_len);
        self.set_abc(n, fields, methods, 0 - 1);
        self.set_x(n, name_off, name_len);
        if (is_pub) {
            self.add_flag(n, F_PUB);
        }
        return n;
    }

    /// `= enum { A, B = 2, … } ;` with `const IDENT` consumed.
    fn parse_enum_decl(self: *Self, a: Allocator, is_pub: bool, name_off: usize, name_len: usize, start: usize) !i32 {
        self.bump(); // `=`
        self.bump(); // `enum`
        try self.expect(TK_LBRACE);
        var ch: Chain = Chain.init();
        while (!self.at(TK_RBRACE)) {
            try self.expect_ident();
            var v_off: usize = self.last_off;
            var v_len: usize = self.last_len;
            var v_end: usize = v_off + v_len;
            var v: i32 = 0 - 1;
            if (self.eat(TK_EQ)) {
                // Optional explicit integer value `A = N` (range/negativity
                // is a sema concern; the literal must be an INT token).
                if (!self.at(TK_INT)) {
                    self.note(200, self.peek_off());
                    return error.Parse;
                }
                var value: i64 = ps_int_value(self.src, self.peek_off(), self.peek_len());
                v_end = self.peek_off() + self.peek_len();
                self.bump();
                v = self.push_node(a, ND_VARIANT, v_off, v_end);
                self.add_flag(v, F_VAL);
                self.set_val(v, value);
            } else {
                v = self.push_node(a, ND_VARIANT, v_off, v_end);
            }
            self.set_x(v, v_off, v_len);
            ch.add(self.nodes, v);
            if (!self.eat(TK_COMMA)) {
                break; // no separator → the variant list is done
            }
        }
        try self.expect(TK_RBRACE);
        try self.expect(TK_SEMICOLON);
        var n: i32 = self.push_node(a, ND_ENUM, start, self.last_off + self.last_len);
        self.set_abc(n, ch.head, 0 - 1, 0 - 1);
        self.set_x(n, name_off, name_len);
        if (is_pub) {
            self.add_flag(n, F_PUB);
        }
        return n;
    }

    /// `= union ( enum ) { v: T, … } ;` with `const IDENT` consumed.
    fn parse_union_decl(self: *Self, a: Allocator, is_pub: bool, name_off: usize, name_len: usize, start: usize) !i32 {
        self.bump(); // `=`
        self.bump(); // `union`
        try self.expect(TK_LPAREN);
        if (!self.eat(TK_KW_ENUM)) {
            self.note(200, self.peek_off());
            return error.Parse;
        }
        try self.expect(TK_RPAREN);
        try self.expect(TK_LBRACE);
        var ch: Chain = Chain.init();
        while (!self.at(TK_RBRACE)) {
            try self.expect_ident();
            var v_off: usize = self.last_off;
            var v_len: usize = self.last_len;
            try self.expect(TK_COLON);
            var payload: i32 = try self.parse_type(a);
            var v: i32 = self.push_node(a, ND_UVAR, v_off, self.nend(payload));
            self.set_abc(v, payload, 0 - 1, 0 - 1);
            self.set_x(v, v_off, v_len);
            ch.add(self.nodes, v);
            if (!self.eat(TK_COMMA)) {
                break; // no separator → the variant list is done
            }
        }
        try self.expect(TK_RBRACE);
        try self.expect(TK_SEMICOLON);
        var n: i32 = self.push_node(a, ND_UNION, start, self.last_off + self.last_len);
        self.set_abc(n, ch.head, 0 - 1, 0 - 1);
        self.set_x(n, name_off, name_len);
        if (is_pub) {
            self.add_flag(n, F_PUB);
        }
        return n;
    }

    /// `= error { A, B, … } ;` with `const IDENT` consumed.
    fn parse_error_set_decl(self: *Self, a: Allocator, is_pub: bool, name_off: usize, name_len: usize, start: usize) !i32 {
        self.bump(); // `=`
        self.bump(); // `error`
        try self.expect(TK_LBRACE);
        var ch: Chain = Chain.init();
        while (!self.at(TK_RBRACE)) {
            try self.expect_ident();
            var m: i32 = self.push_node(a, ND_MEMBER, self.last_off, self.last_off + self.last_len);
            self.set_x(m, self.last_off, self.last_len);
            ch.add(self.nodes, m);
            if (!self.eat(TK_COMMA)) {
                break; // no separator → the member list is done
            }
        }
        try self.expect(TK_RBRACE);
        try self.expect(TK_SEMICOLON);
        var n: i32 = self.push_node(a, ND_ERRSET, start, self.last_off + self.last_len);
        self.set_abc(n, ch.head, 0 - 1, 0 - 1);
        self.set_x(n, name_off, name_len);
        if (is_pub) {
            self.add_flag(n, F_PUB);
        }
        return n;
    }

    /// `test "name" { … }` with the cursor on `test`.
    fn parse_test(self: *Self, a: Allocator, start: usize) !i32 {
        self.bump(); // `test`
        try self.expect(TK_STR);
        var s_off: usize = self.last_off;
        var s_len: usize = self.last_len;
        var body: i32 = try self.parse_block(a);
        var n: i32 = self.push_node(a, ND_TEST, start, self.nend(body));
        self.set_abc(n, body, 0 - 1, 0 - 1);
        self.set_x(n, s_off, s_len);
        return n;
    }

    // ---- blocks & statements -------------------------------------------------

    fn parse_block(self: *Self, a: Allocator) !i32 {
        try self.expect(TK_LBRACE);
        var lb: usize = self.last_off;
        var ch: Chain = Chain.init();
        while (!self.at_eof() and !self.at(TK_RBRACE)) {
            var s: i32 = try self.parse_stmt(a);
            ch.add(self.nodes, s);
        }
        try self.expect(TK_RBRACE);
        var n: i32 = self.push_node(a, ND_BLOCK, lb, self.last_off + self.last_len);
        self.set_abc(n, ch.head, 0 - 1, 0 - 1);
        return n;
    }

    fn parse_stmt(self: *Self, a: Allocator) !i32 {
        var k: u8 = self.peek_kind();
        if (k == TK_KW_VAR or k == TK_KW_CONST) {
            return try self.parse_let(a);
        }
        if (k == TK_KW_RETURN) {
            return try self.parse_return(a);
        }
        if (k == TK_KW_IF) {
            return try self.parse_if(a);
        }
        if (k == TK_KW_WHILE) {
            return try self.parse_while(a, false, 0, 0);
        }
        if (k == TK_KW_FOR) {
            return try self.parse_for(a, false, 0, 0);
        }
        if (k == TK_KW_BREAK) {
            return try self.parse_break_continue(a, ND_BREAK);
        }
        if (k == TK_KW_CONTINUE) {
            return try self.parse_break_continue(a, ND_CONTINUE);
        }
        if (k == TK_KW_DEFER) {
            return try self.parse_defer(a, ND_DEFER);
        }
        if (k == TK_KW_ERRDEFER) {
            return try self.parse_defer(a, ND_ERRDEFER);
        }
        if (k == TK_KW_SWITCH) {
            return try self.parse_switch(a);
        }
        if (k == TK_LBRACE) {
            return try self.parse_block(a);
        }
        if (k == TK_IDENT) {
            // A labeled loop `name: while/for` — three-token lookahead.
            if (self.peek2_kind() == TK_COLON) {
                var k3: u8 = self.peek3_kind();
                if (k3 == TK_KW_WHILE or k3 == TK_KW_FOR) {
                    return try self.parse_labeled_loop(a);
                }
            }
            // A simple-name target followed by an assignment operator.
            if (ps_assign_op(self.peek2_kind()) != 0 - 2) {
                return try self.parse_assign(a);
            }
        }
        return try self.parse_expr_stmt(a);
    }

    fn parse_let(self: *Self, a: Allocator) !i32 {
        var start: usize = self.peek_off();
        var is_const: bool = self.at(TK_KW_CONST);
        self.bump(); // `var` | `const`
        try self.expect_ident();
        var name_off: usize = self.last_off;
        var name_len: usize = self.last_len;
        var ty: i32 = 0 - 1;
        if (self.eat(TK_COLON)) {
            var t: i32 = try self.parse_type(a);
            ty = t;
        }
        try self.expect(TK_EQ);
        var v: i32 = try self.parse_expr(a);
        try self.expect(TK_SEMICOLON);
        var n: i32 = self.push_node(a, ND_LET, start, self.last_off + self.last_len);
        self.set_abc(n, ty, v, 0 - 1);
        self.set_x(n, name_off, name_len);
        if (is_const) {
            self.add_flag(n, F_CONST);
        }
        return n;
    }

    fn parse_assign(self: *Self, a: Allocator) !i32 {
        try self.expect_ident();
        var name_off: usize = self.last_off;
        var name_len: usize = self.last_len;
        var op: i64 = ps_assign_op(self.peek_kind());
        if (op == 0 - 2) {
            self.note(200, self.peek_off());
            return error.Parse;
        }
        self.bump(); // the assignment operator
        var v: i32 = try self.parse_expr(a);
        try self.expect(TK_SEMICOLON);
        var n: i32 = self.push_node(a, ND_ASSIGN, name_off, self.last_off + self.last_len);
        self.set_abc(n, v, 0 - 1, 0 - 1);
        self.set_x(n, name_off, name_len);
        self.set_val(n, op);
        return n;
    }

    fn parse_return(self: *Self, a: Allocator) !i32 {
        var start: usize = self.peek_off();
        self.bump(); // `return`
        if (self.at(TK_SEMICOLON)) {
            var semi_end: usize = self.peek_off() + self.peek_len();
            self.bump();
            return self.push_node(a, ND_RETURN, start, semi_end);
        }
        var v: i32 = try self.parse_expr(a);
        try self.expect(TK_SEMICOLON);
        var n: i32 = self.push_node(a, ND_RETURN, start, self.last_off + self.last_len);
        self.set_abc(n, v, 0 - 1, 0 - 1);
        return n;
    }

    /// `if "(" cond ")" ("|" IDENT "|")? block ("else" (if | block))?`
    /// (parser.rs `parse_if`).
    fn parse_if(self: *Self, a: Allocator) !i32 {
        var start: usize = self.peek_off();
        self.bump(); // `if`
        try self.expect(TK_LPAREN);
        var cond: i32 = try self.parse_expr(a);
        try self.expect(TK_RPAREN);
        var has_cap: bool = false;
        var cap_off: usize = 0;
        var cap_len: usize = 0;
        if (self.at(TK_PIPE)) {
            self.bump(); // `|`
            try self.expect_ident();
            cap_off = self.last_off;
            cap_len = self.last_len;
            try self.expect(TK_PIPE);
            has_cap = true;
        }
        var then: i32 = try self.parse_block(a);
        var end: usize = self.nend(then);
        var els: i32 = 0 - 1;
        if (self.eat(TK_KW_ELSE)) {
            if (self.at(TK_KW_IF)) {
                var e1: i32 = try self.parse_if(a);
                els = e1;
            } else {
                var e2: i32 = try self.parse_block(a);
                els = e2;
            }
            end = self.nend(els);
        }
        var n: i32 = self.push_node(a, ND_IF, start, end);
        self.set_abc(n, cond, then, els);
        if (has_cap) {
            self.add_flag(n, F_CAP);
            self.set_x(n, cap_off, cap_len);
        }
        return n;
    }

    /// `while "(" cond ")" (":" "(" cont ")")? block` — `has_label` is true
    /// when reached via `parse_labeled_loop` and `l_off/l_len` is the label
    /// name span (the loop's span then starts at the label).
    fn parse_while(self: *Self, a: Allocator, has_label: bool, l_off: usize, l_len: usize) !i32 {
        var start: usize = self.peek_off();
        if (has_label) {
            start = l_off;
        }
        self.bump(); // `while`
        try self.expect(TK_LPAREN);
        var cond: i32 = try self.parse_expr(a);
        try self.expect(TK_RPAREN);
        var cont: i32 = 0 - 1;
        if (self.at(TK_COLON)) {
            self.bump(); // `:`
            try self.expect(TK_LPAREN);
            var c: i32 = try self.parse_loop_cont(a);
            cont = c;
            try self.expect(TK_RPAREN);
        }
        var body: i32 = try self.parse_block(a);
        var n: i32 = self.push_node(a, ND_WHILE, start, self.nend(body));
        self.set_abc(n, cond, cont, body);
        if (has_label) {
            self.add_flag(n, F_LABEL);
            self.set_x(n, l_off, l_len);
        }
        return n;
    }

    /// `name: while (…)` / `name: for (…)` — the cursor is on the label and
    /// the lookahead already confirmed the `:` + loop keyword.
    fn parse_labeled_loop(self: *Self, a: Allocator) !i32 {
        try self.expect_ident();
        var l_off: usize = self.last_off;
        var l_len: usize = self.last_len;
        try self.expect(TK_COLON);
        if (self.at(TK_KW_WHILE)) {
            return try self.parse_while(a, true, l_off, l_len);
        }
        if (self.at(TK_KW_FOR)) {
            return try self.parse_for(a, true, l_off, l_len);
        }
        self.note(200, self.peek_off());
        return error.Parse;
    }

    /// A `while` continue-clause: an assignment `IDENT op= expr` or a bare
    /// expression — no trailing `;` (parser.rs `parse_loop_cont`).
    fn parse_loop_cont(self: *Self, a: Allocator) !i32 {
        if (self.at(TK_IDENT) and ps_assign_op(self.peek2_kind()) != 0 - 2) {
            try self.expect_ident();
            var name_off: usize = self.last_off;
            var name_len: usize = self.last_len;
            var op: i64 = ps_assign_op(self.peek_kind());
            self.bump(); // the assignment operator
            var v: i32 = try self.parse_expr(a);
            var n: i32 = self.push_node(a, ND_ASSIGN, name_off, self.nend(v));
            self.set_abc(n, v, 0 - 1, 0 - 1);
            self.set_x(n, name_off, name_len);
            self.set_val(n, op);
            return n;
        }
        return try self.parse_expr(a);
    }

    /// `for "(" iter ("," 0 "..")? ")" "|" elem ("," index)? "|" block`
    /// (parser.rs `parse_for`), including the index-must-start-at-0 and
    /// capture-arity shape constraints (both E0200).
    fn parse_for(self: *Self, a: Allocator, has_label: bool, l_off: usize, l_len: usize) !i32 {
        var start: usize = self.peek_off();
        if (has_label) {
            start = l_off;
        }
        self.bump(); // `for`
        try self.expect(TK_LPAREN);
        var iter: i32 = try self.parse_expr(a);
        var index_form: bool = false;
        if (self.eat(TK_COMMA)) {
            if (!self.at(TK_INT)) {
                self.note(200, self.peek_off());
                return error.Parse;
            }
            var lo: i64 = ps_int_value(self.src, self.peek_off(), self.peek_len());
            if (lo != 0) {
                // "for index range must start at 0" — E0200 at the literal.
                self.note(200, self.peek_off());
                return error.Parse;
            }
            self.bump(); // the `0`
            try self.expect(TK_DOTDOT);
            index_form = true;
        }
        try self.expect(TK_RPAREN);
        try self.expect(TK_PIPE);
        var pipe_off: usize = self.last_off;
        try self.expect_ident();
        var elem_off: usize = self.last_off;
        var elem_len: usize = self.last_len;
        var has_second: bool = false;
        var idx_off: usize = 0;
        var idx_len: usize = 0;
        if (self.eat(TK_COMMA)) {
            try self.expect_ident();
            idx_off = self.last_off;
            idx_len = self.last_len;
            has_second = true;
        }
        try self.expect(TK_PIPE);
        // The capture arity must match the index form: `, 0..` needs two
        // captures, the plain form exactly one (both E0200 at the `|…|`).
        if (index_form and !has_second) {
            self.note(200, pipe_off);
            return error.Parse;
        }
        if (!index_form and has_second) {
            self.note(200, pipe_off);
            return error.Parse;
        }
        var body: i32 = try self.parse_block(a);
        var n: i32 = self.push_node(a, ND_FOR, start, self.nend(body));
        self.set_abc(n, iter, body, 0 - 1);
        self.set_x(n, elem_off, elem_len);
        if (index_form) {
            self.add_flag(n, F_IDX);
            self.set_y(n, idx_off, idx_len);
        }
        if (has_label) {
            self.add_flag(n, F_LABEL);
            self.set_z(n, l_off, l_len);
        }
        return n;
    }

    /// `break;` / `continue;` with an optional `: label` target — `kind` is
    /// ND_BREAK or ND_CONTINUE (parser.rs `parse_break`/`parse_continue`).
    fn parse_break_continue(self: *Self, a: Allocator, kind: u8) !i32 {
        var start: usize = self.peek_off();
        self.bump(); // `break` | `continue`
        var has_target: bool = false;
        var t_off: usize = 0;
        var t_len: usize = 0;
        if (self.eat(TK_COLON)) {
            try self.expect_ident();
            t_off = self.last_off;
            t_len = self.last_len;
            has_target = true;
        }
        try self.expect(TK_SEMICOLON);
        var n: i32 = self.push_node(a, kind, start, self.last_off + self.last_len);
        if (has_target) {
            self.add_flag(n, F_LABEL);
            self.set_x(n, t_off, t_len);
        }
        return n;
    }

    /// `defer stmt;` / `errdefer stmt;` — `kind` is ND_DEFER or ND_ERRDEFER.
    fn parse_defer(self: *Self, a: Allocator, kind: u8) !i32 {
        var start: usize = self.peek_off();
        self.bump(); // `defer` | `errdefer`
        var inner: i32 = try self.parse_stmt(a);
        var n: i32 = self.push_node(a, kind, start, self.nend(inner));
        self.set_abc(n, inner, 0 - 1, 0 - 1);
        return n;
    }

    /// One switch-arm label item (parser.rs `parse_switch_label`): a full
    /// expression, EXCEPT that an integer literal followed by `..` becomes an
    /// inclusive range. Returns either the label's expression node or an
    /// ND_RANGE node (the caller sorts them into the two arm chains).
    fn parse_switch_label(self: *Self, a: Allocator) !i32 {
        var e: i32 = try self.parse_expr(a);
        if (self.nodes[@as(usize, e)].kind == ND_INT and self.at(TK_DOTDOT)) {
            self.bump(); // `..`
            if (!self.at(TK_INT)) {
                self.note(200, self.peek_off());
                return error.Parse;
            }
            var hi: i64 = ps_int_value(self.src, self.peek_off(), self.peek_len());
            self.bump();
            var r: i32 = self.push_node(a, ND_RANGE, 0, 0);
            self.set_val(r, self.nodes[@as(usize, e)].val);
            self.nodes[@as(usize, r)].val2 = hi;
            return r;
        }
        return e;
    }

    /// `switch "(" expr ")" "{" arm* "}"` with `labels => |cap|? block` arms
    /// and an optional `else => block` default (a later `else` overwrites —
    /// duplicate-default is a sema concern). Lenient `,` between arms.
    fn parse_switch(self: *Self, a: Allocator) !i32 {
        var start: usize = self.peek_off();
        self.bump(); // `switch`
        try self.expect(TK_LPAREN);
        var scrutinee: i32 = try self.parse_expr(a);
        try self.expect(TK_RPAREN);
        try self.expect(TK_LBRACE);
        var arms: Chain = Chain.init();
        var default: i32 = 0 - 1;
        while (!self.at_eof() and !self.at(TK_RBRACE)) {
            if (self.at(TK_KW_ELSE)) {
                self.bump(); // `else`
                try self.expect(TK_FATARROW);
                var dfl: i32 = try self.parse_block(a);
                default = dfl;
            } else {
                var arm_start: usize = self.peek_off();
                var labels: Chain = Chain.init();
                var ranges: Chain = Chain.init();
                var first: i32 = try self.parse_switch_label(a);
                if (self.nodes[@as(usize, first)].kind == ND_RANGE) {
                    ranges.add(self.nodes, first);
                } else {
                    labels.add(self.nodes, first);
                }
                while (self.eat(TK_COMMA)) {
                    // Tolerate a trailing `,` before the `=>`.
                    if (self.at(TK_FATARROW)) {
                        break;
                    }
                    var l: i32 = try self.parse_switch_label(a);
                    if (self.nodes[@as(usize, l)].kind == ND_RANGE) {
                        ranges.add(self.nodes, l);
                    } else {
                        labels.add(self.nodes, l);
                    }
                }
                try self.expect(TK_FATARROW);
                var has_cap: bool = false;
                var cap_off: usize = 0;
                var cap_len: usize = 0;
                if (self.at(TK_PIPE)) {
                    self.bump(); // `|`
                    try self.expect_ident();
                    cap_off = self.last_off;
                    cap_len = self.last_len;
                    try self.expect(TK_PIPE);
                    has_cap = true;
                }
                var body: i32 = try self.parse_block(a);
                var arm: i32 = self.push_node(a, ND_ARM, arm_start, self.nend(body));
                self.set_abc(arm, labels.head, ranges.head, body);
                if (has_cap) {
                    self.add_flag(arm, F_CAP);
                    self.set_x(arm, cap_off, cap_len);
                }
                arms.add(self.nodes, arm);
            }
            // Arms separate with `,`; trailing comma after a block optional.
            if (self.at(TK_COMMA)) {
                self.bump();
            }
        }
        try self.expect(TK_RBRACE);
        var n: i32 = self.push_node(a, ND_SWITCH, start, self.last_off + self.last_len);
        self.set_abc(n, scrutinee, arms.head, default);
        return n;
    }

    /// An expression statement, or a place assignment when the parsed
    /// expression is a field/index/deref chain followed by an assignment
    /// operator (parser.rs `parse_expr_stmt`).
    fn parse_expr_stmt(self: *Self, a: Allocator) !i32 {
        var e: i32 = try self.parse_expr(a);
        var k: u8 = self.nodes[@as(usize, e)].kind;
        var op: i64 = 0 - 2;
        if (k == ND_FIELD or k == ND_INDEX or k == ND_DEREF) {
            op = ps_assign_op(self.peek_kind());
        }
        if (op != 0 - 2) {
            self.bump(); // the assignment operator
            var v: i32 = try self.parse_expr(a);
            try self.expect(TK_SEMICOLON);
            var n: i32 = self.push_node(a, ND_PASSIGN, self.nstart(e), self.last_off + self.last_len);
            self.set_abc(n, e, v, 0 - 1);
            self.set_val(n, op);
            return n;
        }
        try self.expect(TK_SEMICOLON);
        return e;
    }

    // ---- expressions (precedence climbing) -------------------------------------

    fn parse_expr(self: *Self, a: Allocator) !i32 {
        return try self.parse_orelse(a);
    }

    /// The lowest level: left-associative `orelse` / `catch [|e|]` over full
    /// `or`-expressions (parser.rs `parse_orelse`).
    fn parse_orelse(self: *Self, a: Allocator) !i32 {
        var lhs: i32 = try self.parse_binary(a, 0);
        while (true) {
            if (self.at(TK_KW_ORELSE)) {
                self.bump();
                var rhs: i32 = try self.parse_binary(a, 0);
                var n: i32 = self.push_node(a, ND_ORELSE, self.nstart(lhs), self.nend(rhs));
                self.set_abc(n, lhs, rhs, 0 - 1);
                lhs = n;
            } else if (self.at(TK_KW_CATCH)) {
                self.bump();
                var has_cap: bool = false;
                var cap_off: usize = 0;
                var cap_len: usize = 0;
                if (self.at(TK_PIPE)) {
                    self.bump(); // `|`
                    try self.expect_ident();
                    cap_off = self.last_off;
                    cap_len = self.last_len;
                    try self.expect(TK_PIPE);
                    has_cap = true;
                }
                var dflt: i32 = try self.parse_binary(a, 0);
                var n2: i32 = self.push_node(a, ND_CATCH, self.nstart(lhs), self.nend(dflt));
                self.set_abc(n2, lhs, dflt, 0 - 1);
                if (has_cap) {
                    self.add_flag(n2, F_CAP);
                    self.set_x(n2, cap_off, cap_len);
                }
                lhs = n2;
            } else {
                break;
            }
        }
        return lhs;
    }

    /// The infix operator code at the current token for precedence `level`,
    /// or -1 (parser.rs `binop_at`; rows loosest 0 → tightest 9).
    fn binop_at(self: *Self, level: i64) i64 {
        var k: u8 = self.peek_kind();
        if (level == 0) {
            if (k == TK_KW_OR) { return OPC_OR; }
            return 0 - 1;
        }
        if (level == 1) {
            if (k == TK_KW_AND) { return OPC_AND; }
            return 0 - 1;
        }
        if (level == 2) {
            if (k == TK_PIPE) { return OPC_BOR; }
            return 0 - 1;
        }
        if (level == 3) {
            if (k == TK_CARET) { return OPC_BXOR; }
            return 0 - 1;
        }
        if (level == 4) {
            if (k == TK_AMP) { return OPC_BAND; }
            return 0 - 1;
        }
        if (level == 5) {
            if (k == TK_EQEQ) { return OPC_EQ; }
            if (k == TK_BANGEQ) { return OPC_NE; }
            return 0 - 1;
        }
        if (level == 6) {
            if (k == TK_LT) { return OPC_LT; }
            if (k == TK_LE) { return OPC_LE; }
            if (k == TK_GT) { return OPC_GT; }
            if (k == TK_GE) { return OPC_GE; }
            return 0 - 1;
        }
        if (level == 7) {
            if (k == TK_SHL) { return OPC_SHL; }
            if (k == TK_SHR) { return OPC_SHR; }
            return 0 - 1;
        }
        if (level == 8) {
            if (k == TK_PLUS) { return OPC_ADD; }
            if (k == TK_MINUS) { return OPC_SUB; }
            return 0 - 1;
        }
        if (level == 9) {
            if (k == TK_STAR) { return OPC_MUL; }
            if (k == TK_SLASH) { return OPC_DIV; }
            if (k == TK_PERCENT) { return OPC_REM; }
            return 0 - 1;
        }
        return 0 - 1;
    }

    /// One left-associative infix level; the tightest level's operands are
    /// unary expressions (parser.rs `parse_binary`).
    fn parse_binary(self: *Self, a: Allocator, level: i64) !i32 {
        var lhs: i32 = 0 - 1;
        if (level + 1 == PS_BIN_LEVELS) {
            var l0: i32 = try self.parse_unary(a);
            lhs = l0;
        } else {
            var l1: i32 = try self.parse_binary(a, level + 1);
            lhs = l1;
        }
        while (true) {
            var op: i64 = self.binop_at(level);
            if (op < 0) {
                break;
            }
            self.bump();
            var rhs: i32 = 0 - 1;
            if (level + 1 == PS_BIN_LEVELS) {
                var r0: i32 = try self.parse_unary(a);
                rhs = r0;
            } else {
                var r1: i32 = try self.parse_binary(a, level + 1);
                rhs = r1;
            }
            var n: i32 = self.push_node(a, ND_BIN, self.nstart(lhs), self.nend(rhs));
            self.set_abc(n, lhs, rhs, 0 - 1);
            self.set_val(n, op);
            lhs = n;
        }
        return lhs;
    }

    /// The unary level: prefix `try`, `&place`, `- ! ~` (parser.rs
    /// `parse_unary`), each over a unary operand.
    fn parse_unary(self: *Self, a: Allocator) !i32 {
        if (self.at(TK_KW_TRY)) {
            var start: usize = self.peek_off();
            self.bump(); // `try`
            var inner: i32 = try self.parse_unary(a);
            var n: i32 = self.push_node(a, ND_TRY, start, self.nend(inner));
            self.set_abc(n, inner, 0 - 1, 0 - 1);
            return n;
        }
        if (self.at(TK_AMP)) {
            var start2: usize = self.peek_off();
            self.bump(); // `&`
            var inner2: i32 = try self.parse_unary(a);
            var n2: i32 = self.push_node(a, ND_ADDROF, start2, self.nend(inner2));
            self.set_abc(n2, inner2, 0 - 1, 0 - 1);
            return n2;
        }
        var k: u8 = self.peek_kind();
        var op: i64 = 0 - 1;
        if (k == TK_MINUS) { op = UOP_NEG; }
        if (k == TK_BANG) { op = UOP_NOT; }
        if (k == TK_TILDE) { op = UOP_BNOT; }
        if (op >= 0) {
            var start3: usize = self.peek_off();
            self.bump();
            var inner3: i32 = try self.parse_unary(a);
            var n3: i32 = self.push_node(a, ND_UNARY, start3, self.nend(inner3));
            self.set_abc(n3, inner3, 0 - 1, 0 - 1);
            self.set_val(n3, op);
            return n3;
        }
        return try self.parse_comptime(a);
    }

    /// The `comptime expr` prefix — its operand is a POSTFIX expression
    /// (parser.rs `parse_comptime`).
    fn parse_comptime(self: *Self, a: Allocator) !i32 {
        if (self.at(TK_KW_COMPTIME)) {
            var start: usize = self.peek_off();
            self.bump();
            var inner: i32 = try self.parse_postfix(a);
            var n: i32 = self.push_node(a, ND_COMPTIME, start, self.nend(inner));
            self.set_abc(n, inner, 0 - 1, 0 - 1);
            return n;
        }
        return try self.parse_postfix(a);
    }

    /// Postfix chains: `.field`, `.method(args)`, `.*`, `.?`, `[i]`,
    /// `[lo..hi]` — left-associative (parser.rs `parse_postfix`).
    fn parse_postfix(self: *Self, a: Allocator) !i32 {
        var e: i32 = try self.parse_primary(a);
        while (true) {
            if (self.at(TK_DOT)) {
                self.bump(); // `.`
                if (self.at(TK_STAR)) {
                    var star_end: usize = self.peek_off() + self.peek_len();
                    self.bump(); // `*`
                    var d: i32 = self.push_node(a, ND_DEREF, self.nstart(e), star_end);
                    self.set_abc(d, e, 0 - 1, 0 - 1);
                    e = d;
                } else if (self.at(TK_QUESTION)) {
                    var q_end: usize = self.peek_off() + self.peek_len();
                    self.bump(); // `?`
                    var uq: i32 = self.push_node(a, ND_UNWRAP, self.nstart(e), q_end);
                    self.set_abc(uq, e, 0 - 1, 0 - 1);
                    e = uq;
                } else {
                    try self.expect_ident();
                    var name_off: usize = self.last_off;
                    var name_len: usize = self.last_len;
                    if (self.at(TK_LPAREN)) {
                        self.bump(); // `(`
                        var args: i32 = try self.parse_args(a);
                        try self.expect(TK_RPAREN);
                        var mc: i32 = self.push_node(a, ND_MCALL, self.nstart(e), self.last_off + self.last_len);
                        self.set_abc(mc, e, args, 0 - 1);
                        self.set_x(mc, name_off, name_len);
                        e = mc;
                    } else {
                        var fl: i32 = self.push_node(a, ND_FIELD, self.nstart(e), name_off + name_len);
                        self.set_abc(fl, e, 0 - 1, 0 - 1);
                        self.set_x(fl, name_off, name_len);
                        e = fl;
                    }
                }
            } else if (self.at(TK_LBRACKET)) {
                self.bump(); // `[`
                var lo: i32 = try self.parse_expr(a);
                if (self.at(TK_DOTDOT)) {
                    self.bump(); // `..`
                    var hi: i32 = try self.parse_expr(a);
                    try self.expect(TK_RBRACKET);
                    var sl: i32 = self.push_node(a, ND_SLICEX, self.nstart(e), self.last_off + self.last_len);
                    self.set_abc(sl, e, lo, hi);
                    e = sl;
                } else {
                    try self.expect(TK_RBRACKET);
                    var ix: i32 = self.push_node(a, ND_INDEX, self.nstart(e), self.last_off + self.last_len);
                    self.set_abc(ix, e, lo, 0 - 1);
                    e = ix;
                }
            } else {
                break;
            }
        }
        return e;
    }

    fn parse_primary(self: *Self, a: Allocator) !i32 {
        var k: u8 = self.peek_kind();
        var t_off: usize = self.peek_off();
        var t_len: usize = self.peek_len();
        var t_end: usize = t_off + t_len;
        if (k == TK_INT) {
            var v: i64 = ps_int_value(self.src, t_off, t_len);
            self.bump();
            var n: i32 = self.push_node(a, ND_INT, t_off, t_end);
            self.set_val(n, v);
            return n;
        }
        if (k == TK_FLOAT) {
            self.bump();
            return self.push_node(a, ND_FLOAT, t_off, t_end);
        }
        if (k == TK_STR) {
            self.bump();
            return self.push_node(a, ND_STR, t_off, t_end);
        }
        if (k == TK_KW_TRUE or k == TK_KW_FALSE) {
            self.bump();
            var b: i32 = self.push_node(a, ND_BOOL, t_off, t_end);
            if (k == TK_KW_TRUE) {
                self.set_val(b, 1);
            }
            return b;
        }
        if (k == TK_KW_NULL) {
            self.bump();
            return self.push_node(a, ND_NULL, t_off, t_end);
        }
        if (k == TK_KW_ERROR) {
            // `error.Name` — an error value from the global error set.
            self.bump(); // `error`
            try self.expect(TK_DOT);
            try self.expect_ident();
            var n2: i32 = self.push_node(a, ND_ERRLIT, t_off, self.last_off + self.last_len);
            self.set_x(n2, self.last_off, self.last_len);
            return n2;
        }
        if (k == TK_KW_STRUCT) {
            // An anonymous `struct { … }` TYPE VALUE (expression position
            // only — named struct items are dispatched in `parse_const`).
            self.bump(); // `struct`
            try self.expect(TK_LBRACE);
            try self.parse_struct_body(a);
            var fields: i32 = self.sb_fields;
            var methods: i32 = self.sb_methods;
            try self.expect(TK_RBRACE);
            var st: i32 = self.push_node(a, ND_STRUCTTYPE, t_off, self.last_off + self.last_len);
            self.set_abc(st, fields, methods, 0 - 1);
            return st;
        }
        if (k == TK_KW_UNREACHABLE) {
            self.bump();
            return self.push_node(a, ND_UNREACHABLE, t_off, t_end);
        }
        if (k == TK_DOT) {
            // A leading `.Variant` — an unqualified enum literal.
            self.bump(); // `.`
            try self.expect_ident();
            var el: i32 = self.push_node(a, ND_ENUMLIT, t_off, self.last_off + self.last_len);
            self.set_x(el, self.last_off, self.last_len);
            return el;
        }
        if (k == TK_IDENT) {
            self.bump();
            if (self.at(TK_LPAREN)) {
                self.bump(); // `(`
                var args: i32 = try self.parse_args(a);
                try self.expect(TK_RPAREN);
                var c: i32 = self.push_node(a, ND_CALL, t_off, self.last_off + self.last_len);
                self.set_abc(c, args, 0 - 1, 0 - 1);
                self.set_x(c, t_off, t_len);
                return c;
            }
            if (self.at(TK_LBRACE)) {
                // Struct literal `Name{ .f = e, … }`.
                self.bump(); // `{`
                var inits: i32 = try self.parse_field_inits(a);
                try self.expect(TK_RBRACE);
                var sl: i32 = self.push_node(a, ND_SLIT, t_off, self.last_off + self.last_len);
                self.set_abc(sl, inits, 0 - 1, 0 - 1);
                self.set_x(sl, t_off, t_len);
                return sl;
            }
            var id: i32 = self.push_node(a, ND_IDENT, t_off, t_end);
            self.set_x(id, t_off, t_len);
            return id;
        }
        if (k == TK_LPAREN) {
            // Parenthesised expression: the INNER node, span NOT extended.
            self.bump(); // `(`
            var inner: i32 = try self.parse_expr(a);
            try self.expect(TK_RPAREN);
            return inner;
        }
        if (k == TK_LBRACKET) {
            // An array literal `[N]T{ e0, e1, … }` — the type parse consumes
            // `[`, the size, `]`, and the element type name.
            var elem: i32 = try self.parse_type(a);
            try self.expect(TK_LBRACE);
            var elems: i32 = try self.parse_array_elems(a);
            try self.expect(TK_RBRACE);
            var al: i32 = self.push_node(a, ND_ALIT, t_off, self.last_off + self.last_len);
            self.set_abc(al, elem, elems, 0 - 1);
            return al;
        }
        if (k == TK_AT) {
            // `@name(args)` — an expression builtin (unknown names are a
            // sema concern; `@import`/`@This()` never reach here).
            self.bump(); // `@`
            try self.expect_ident();
            var b_off: usize = self.last_off;
            var b_len: usize = self.last_len;
            try self.expect(TK_LPAREN);
            var args2: i32 = try self.parse_args(a);
            try self.expect(TK_RPAREN);
            var bn: i32 = self.push_node(a, ND_BUILTIN, t_off, self.last_off + self.last_len);
            self.set_abc(bn, args2, 0 - 1, 0 - 1);
            self.set_x(bn, b_off, b_len);
            return bn;
        }
        self.note(200, self.peek_off());
        return error.Parse;
    }

    /// The element expressions of an array literal, cursor just after `{`;
    /// stops at the `}`. Empty list and trailing comma supported.
    fn parse_array_elems(self: *Self, a: Allocator) !i32 {
        var ch: Chain = Chain.init();
        if (self.at(TK_RBRACE)) {
            return ch.head;
        }
        while (true) {
            var e: i32 = try self.parse_expr(a);
            ch.add(self.nodes, e);
            if (self.eat(TK_COMMA)) {
                if (self.at(TK_RBRACE)) {
                    break; // trailing comma
                }
            } else {
                break;
            }
        }
        return ch.head;
    }

    /// Call arguments, cursor just after `(`; stops at the `)`. Empty list
    /// and trailing comma supported.
    fn parse_args(self: *Self, a: Allocator) !i32 {
        var ch: Chain = Chain.init();
        if (self.at(TK_RPAREN)) {
            return ch.head;
        }
        while (true) {
            var e: i32 = try self.parse_expr(a);
            ch.add(self.nodes, e);
            if (self.eat(TK_COMMA)) {
                if (self.at(TK_RPAREN)) {
                    break; // trailing comma
                }
            } else {
                break;
            }
        }
        return ch.head;
    }

    /// The `.f = e` initializers of a struct literal, cursor just after `{`;
    /// stops at the `}`. Empty list and trailing comma supported.
    fn parse_field_inits(self: *Self, a: Allocator) !i32 {
        var ch: Chain = Chain.init();
        if (self.at(TK_RBRACE)) {
            return ch.head;
        }
        while (true) {
            try self.expect(TK_DOT);
            var dot_off: usize = self.last_off;
            try self.expect_ident();
            var name_off: usize = self.last_off;
            var name_len: usize = self.last_len;
            try self.expect(TK_EQ);
            var v: i32 = try self.parse_expr(a);
            var f: i32 = self.push_node(a, ND_FINIT, dot_off, self.nend(v));
            self.set_abc(f, v, 0 - 1, 0 - 1);
            self.set_x(f, name_off, name_len);
            ch.add(self.nodes, f);
            if (self.eat(TK_COMMA)) {
                if (self.at(TK_RBRACE)) {
                    break; // trailing comma
                }
            } else {
                break;
            }
        }
        return ch.head;
    }
};
