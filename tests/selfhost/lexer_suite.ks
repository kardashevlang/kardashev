// lexer_suite.ks — in-language tests for the self-hosted lexer (v0.159).
//
// Run: kard test tests/selfhost/lexer_suite.ks (driven from
// `crates/kardc/tests/selfhost_lexer.rs` so it is part of `cargo test`).
// Pins the SPEC §1 rules the differential corpus exercises statistically:
// the full token tables, maximal munch, exact spans, string escapes and the
// two error codes — each against a hand-laid-out source.

@import("../../selfhost/lexer.ks");
@import("std");

// The `idx`-th token of `src` (0-based; re-lexes from the start — TK_ERROR is
// sticky, so indexing past an error keeps returning it).
fn sl_tok(src: []u8, idx: usize) Token {
    var lx: Lexer = Lexer.init(src);
    var t: Token = lx.next();
    var i: usize = 0;
    while (i < idx) : (i += 1) {
        t = lx.next();
    }
    return t;
}

// Kind of the `idx`-th token of `src`.
fn sl_kind(src: []u8, idx: usize) u8 {
    var t: Token = sl_tok(src, idx);
    return t.kind;
}

test "empty source is just a zero-width EOF" {
    var t: Token = sl_tok("", 0);
    expect(t.kind == TK_EOF);
    expect(t.off == 0);
    expect(t.len == 0);
    // EOF repeats.
    var u: Token = sl_tok("", 5);
    expect(u.kind == TK_EOF);
}

test "every operator and punct, in table order" {
    var s: []u8 = "( ) { } [ ] , ; : . .. = == => != ! < <= << > >= >> + += - -= * *= / /= % %= ? & | @ ^ ~";
    expect(sl_kind(s, 0) == TK_LPAREN);
    expect(sl_kind(s, 1) == TK_RPAREN);
    expect(sl_kind(s, 2) == TK_LBRACE);
    expect(sl_kind(s, 3) == TK_RBRACE);
    expect(sl_kind(s, 4) == TK_LBRACKET);
    expect(sl_kind(s, 5) == TK_RBRACKET);
    expect(sl_kind(s, 6) == TK_COMMA);
    expect(sl_kind(s, 7) == TK_SEMICOLON);
    expect(sl_kind(s, 8) == TK_COLON);
    expect(sl_kind(s, 9) == TK_DOT);
    expect(sl_kind(s, 10) == TK_DOTDOT);
    expect(sl_kind(s, 11) == TK_EQ);
    expect(sl_kind(s, 12) == TK_EQEQ);
    expect(sl_kind(s, 13) == TK_FATARROW);
    expect(sl_kind(s, 14) == TK_BANGEQ);
    expect(sl_kind(s, 15) == TK_BANG);
    expect(sl_kind(s, 16) == TK_LT);
    expect(sl_kind(s, 17) == TK_LE);
    expect(sl_kind(s, 18) == TK_SHL);
    expect(sl_kind(s, 19) == TK_GT);
    expect(sl_kind(s, 20) == TK_GE);
    expect(sl_kind(s, 21) == TK_SHR);
    expect(sl_kind(s, 22) == TK_PLUS);
    expect(sl_kind(s, 23) == TK_PLUSEQ);
    expect(sl_kind(s, 24) == TK_MINUS);
    expect(sl_kind(s, 25) == TK_MINUSEQ);
    expect(sl_kind(s, 26) == TK_STAR);
    expect(sl_kind(s, 27) == TK_STAREQ);
    expect(sl_kind(s, 28) == TK_SLASH);
    expect(sl_kind(s, 29) == TK_SLASHEQ);
    expect(sl_kind(s, 30) == TK_PERCENT);
    expect(sl_kind(s, 31) == TK_PERCENTEQ);
    expect(sl_kind(s, 32) == TK_QUESTION);
    expect(sl_kind(s, 33) == TK_AMP);
    expect(sl_kind(s, 34) == TK_PIPE);
    expect(sl_kind(s, 35) == TK_AT);
    expect(sl_kind(s, 36) == TK_CARET);
    expect(sl_kind(s, 37) == TK_TILDE);
    expect(sl_kind(s, 38) == TK_EOF);
}

test "every keyword, in Kw order" {
    var s: []u8 = "pub fn const var return if else while break continue defer comptime test true false and or struct orelse null try catch error enum switch union errdefer for unreachable";
    expect(sl_kind(s, 0) == TK_KW_PUB);
    expect(sl_kind(s, 1) == TK_KW_FN);
    expect(sl_kind(s, 2) == TK_KW_CONST);
    expect(sl_kind(s, 3) == TK_KW_VAR);
    expect(sl_kind(s, 4) == TK_KW_RETURN);
    expect(sl_kind(s, 5) == TK_KW_IF);
    expect(sl_kind(s, 6) == TK_KW_ELSE);
    expect(sl_kind(s, 7) == TK_KW_WHILE);
    expect(sl_kind(s, 8) == TK_KW_BREAK);
    expect(sl_kind(s, 9) == TK_KW_CONTINUE);
    expect(sl_kind(s, 10) == TK_KW_DEFER);
    expect(sl_kind(s, 11) == TK_KW_COMPTIME);
    expect(sl_kind(s, 12) == TK_KW_TEST);
    expect(sl_kind(s, 13) == TK_KW_TRUE);
    expect(sl_kind(s, 14) == TK_KW_FALSE);
    expect(sl_kind(s, 15) == TK_KW_AND);
    expect(sl_kind(s, 16) == TK_KW_OR);
    expect(sl_kind(s, 17) == TK_KW_STRUCT);
    expect(sl_kind(s, 18) == TK_KW_ORELSE);
    expect(sl_kind(s, 19) == TK_KW_NULL);
    expect(sl_kind(s, 20) == TK_KW_TRY);
    expect(sl_kind(s, 21) == TK_KW_CATCH);
    expect(sl_kind(s, 22) == TK_KW_ERROR);
    expect(sl_kind(s, 23) == TK_KW_ENUM);
    expect(sl_kind(s, 24) == TK_KW_SWITCH);
    expect(sl_kind(s, 25) == TK_KW_UNION);
    expect(sl_kind(s, 26) == TK_KW_ERRDEFER);
    expect(sl_kind(s, 27) == TK_KW_FOR);
    expect(sl_kind(s, 28) == TK_KW_UNREACHABLE);
    expect(sl_kind(s, 29) == TK_EOF);
}

test "keyword-vs-ident boundaries" {
    // A keyword with anything appended/prepended is an ordinary identifier;
    // type names are identifiers too (SPEC §1).
    var s: []u8 = "fnx _fn returns for_ pubx fn0 i32 usize bool";
    var i: usize = 0;
    while (i < 9) : (i += 1) {
        expect(sl_kind(s, i) == TK_IDENT);
    }
    expect(sl_kind("fn", 0) == TK_KW_FN);
    expect(sl_kind("unreachable", 0) == TK_KW_UNREACHABLE);
    expect(sl_kind("unreachables", 0) == TK_IDENT);
}

test "maximal munch: compounds win over prefixes" {
    expect(sl_kind("===", 0) == TK_EQEQ);
    expect(sl_kind("===", 1) == TK_EQ);
    expect(sl_kind("!=!", 0) == TK_BANGEQ);
    expect(sl_kind("!=!", 1) == TK_BANG);
    expect(sl_kind("<=<", 0) == TK_LE);
    expect(sl_kind("<=<", 1) == TK_LT);
    expect(sl_kind("<<<", 0) == TK_SHL);
    expect(sl_kind("<<<", 1) == TK_LT);
    expect(sl_kind(">>>", 0) == TK_SHR);
    expect(sl_kind(">>>", 1) == TK_GT);
    expect(sl_kind(">=>", 0) == TK_GE);
    expect(sl_kind(">=>", 1) == TK_GT);
    expect(sl_kind("=>=", 0) == TK_FATARROW);
    expect(sl_kind("=>=", 1) == TK_EQ);
    expect(sl_kind("+=+", 0) == TK_PLUSEQ);
    expect(sl_kind("+=+", 1) == TK_PLUS);
    expect(sl_kind("-=-", 0) == TK_MINUSEQ);
    expect(sl_kind("-=-", 1) == TK_MINUS);
    expect(sl_kind("*=*", 0) == TK_STAREQ);
    expect(sl_kind("*=*", 1) == TK_STAR);
    expect(sl_kind("/=/", 0) == TK_SLASHEQ);
    expect(sl_kind("/=/", 1) == TK_SLASH);
    expect(sl_kind("%=%", 0) == TK_PERCENTEQ);
    expect(sl_kind("%=%", 1) == TK_PERCENT);
    // a--b: no `--` token — two MINUS.
    expect(sl_kind("a--b", 0) == TK_IDENT);
    expect(sl_kind("a--b", 1) == TK_MINUS);
    expect(sl_kind("a--b", 2) == TK_MINUS);
    expect(sl_kind("a--b", 3) == TK_IDENT);
    // dots: `..` vs `.` (and `...` = `..` then `.`).
    expect(sl_kind("...", 0) == TK_DOTDOT);
    expect(sl_kind("...", 1) == TK_DOT);
}

test "ints vs floats vs ranges" {
    // 1..3 is INT DOTDOT INT (a `.` followed by `.` keeps the int intact).
    expect(sl_kind("1..3", 0) == TK_INT);
    expect(sl_kind("1..3", 1) == TK_DOTDOT);
    expect(sl_kind("1..3", 2) == TK_INT);
    var r: Token = sl_tok("1..3", 1);
    expect(r.off == 1);
    expect(r.len == 2);
    // digits.digits is FLOAT.
    var f: Token = sl_tok("3.14", 0);
    expect(f.kind == TK_FLOAT);
    expect(f.off == 0);
    expect(f.len == 4);
    expect(sl_kind("0.0", 0) == TK_FLOAT);
    var g: Token = sl_tok("12.500", 0);
    expect(g.kind == TK_FLOAT);
    expect(g.len == 6);
    // `.` followed by a non-digit leaves the int intact.
    expect(sl_kind("1.x", 0) == TK_INT);
    expect(sl_kind("1.x", 1) == TK_DOT);
    expect(sl_kind("1.x", 2) == TK_IDENT);
    // A trailing `1.` at EOF: INT then DOT.
    expect(sl_kind("1.", 0) == TK_INT);
    expect(sl_kind("1.", 1) == TK_DOT);
    // Only ONE fraction: 1.2.3 is FLOAT DOT INT.
    expect(sl_kind("1.2.3", 0) == TK_FLOAT);
    expect(sl_kind("1.2.3", 1) == TK_DOT);
    expect(sl_kind("1.2.3", 2) == TK_INT);
}

test "int extremes: i64 max fits, one past is E0002" {
    var ok: Token = sl_tok("9223372036854775807", 0);
    expect(ok.kind == TK_INT);
    expect(ok.len == 19);
    // 2^63 itself overflows.
    var bad: Token = sl_tok("9223372036854775808", 0);
    expect(bad.kind == TK_ERROR);
    expect(bad.len == 2);                 // error code E0002
    expect(bad.off == 0);                 // at the literal start
    // Way past.
    var huge: Token = sl_tok("99999999999999999999999", 0);
    expect(huge.kind == TK_ERROR);
    expect(huge.len == 2);
    // Leading zeros are fine (Rust's i64 parse accepts them).
    expect(sl_kind("000009223372036854775807", 0) == TK_INT);
    expect(sl_kind("00000", 0) == TK_INT);
    // The error position is the literal start, after earlier tokens.
    var later: Token = sl_tok("ab 12345678901234567890", 1);
    expect(later.kind == TK_ERROR);
    expect(later.len == 2);
    expect(later.off == 3);
}

test "spans are exact on a hand-laid-out source" {
    var s: []u8 = "fn main() {}";
    var t0: Token = sl_tok(s, 0);
    expect(t0.kind == TK_KW_FN);
    expect(t0.off == 0);
    expect(t0.len == 2);
    var t1: Token = sl_tok(s, 1);
    expect(t1.kind == TK_IDENT);
    expect(t1.off == 3);
    expect(t1.len == 4);
    var t2: Token = sl_tok(s, 2);
    expect(t2.kind == TK_LPAREN);
    expect(t2.off == 7);
    expect(t2.len == 1);
    var t3: Token = sl_tok(s, 3);
    expect(t3.kind == TK_RPAREN);
    expect(t3.off == 8);
    var t4: Token = sl_tok(s, 4);
    expect(t4.kind == TK_LBRACE);
    expect(t4.off == 10);
    var t5: Token = sl_tok(s, 5);
    expect(t5.kind == TK_RBRACE);
    expect(t5.off == 11);
    var t6: Token = sl_tok(s, 6);
    expect(t6.kind == TK_EOF);
    expect(t6.off == 12);
    expect(t6.len == 0);
}

test "comments and whitespace are skipped" {
    // `// a comment\n` is 13 bytes; `x` starts at 13 (the lexer.rs pin).
    var t: Token = sl_tok("// a comment\nx", 0);
    expect(t.kind == TK_IDENT);
    expect(t.off == 13);
    expect(t.len == 1);
    // A trailing comment without a newline runs to EOF.
    expect(sl_kind("x // trailing", 1) == TK_EOF);
    var e: Token = sl_tok("x // trailing", 1);
    expect(e.off == 13);
    // A comment-only source is just EOF; `//` needs no third char.
    expect(sl_kind("//", 0) == TK_EOF);
    // Tabs/CR/LF are whitespace.
    var w: Token = sl_tok("\tx\n y", 1);
    expect(w.kind == TK_IDENT);
    expect(w.off == 4);
}

test "string literals: spans include the quotes" {
    var t: Token = sl_tok("\"hi\"", 0);
    expect(t.kind == TK_STR);
    expect(t.off == 0);
    expect(t.len == 4);
    // All four escapes in one literal: "a\n\t\\\"z" is 12 source bytes.
    var e: Token = sl_tok("\"a\\n\\t\\\\\\\"z\"", 0);
    expect(e.kind == TK_STR);
    expect(e.off == 0);
    expect(e.len == 12);
    var after: Token = sl_tok("\"a\\n\\t\\\\\\\"z\"", 1);
    expect(after.kind == TK_EOF);
    expect(after.off == 12);
    // Multibyte content counts in BYTES: "é" is 4 source bytes.
    var m: Token = sl_tok("\"é\"", 0);
    expect(m.kind == TK_STR);
    expect(m.len == 4);
}

test "string errors are E0001" {
    // Unterminated: error at the OPENING quote.
    var u: Token = sl_tok("\"oops", 0);
    expect(u.kind == TK_ERROR);
    expect(u.len == 1);                   // error code E0001
    expect(u.off == 0);
    // Unknown escape: error at the BACKSLASH.
    var b: Token = sl_tok("\"a\\q\"", 0);
    expect(b.kind == TK_ERROR);
    expect(b.len == 1);
    expect(b.off == 2);
    // A trailing backslash at EOF is unterminated (error at the quote).
    var tr: Token = sl_tok("\"ab\\", 0);
    expect(tr.kind == TK_ERROR);
    expect(tr.len == 1);
    expect(tr.off == 0);
}

test "unexpected characters are E0001 and errors are sticky" {
    var h: Token = sl_tok("#", 0);
    expect(h.kind == TK_ERROR);
    expect(h.len == 1);
    expect(h.off == 0);
    // After clean tokens, the error position is exact.
    var d: Token = sl_tok("ab #", 1);
    expect(d.kind == TK_ERROR);
    expect(d.off == 3);
    // Sticky: indexing past the error keeps returning the same token.
    var s2: Token = sl_tok("ab #", 4);
    expect(s2.kind == TK_ERROR);
    expect(s2.off == 3);
    expect(s2.len == 1);
    // `$` is also unknown (`@` and `^` are real tokens since v0.126/v0.132).
    expect(sl_kind("$", 0) == TK_ERROR);
}

test "a small program lexes like the lexer.rs pin" {
    var s: []u8 = "pub fn main() void {\n    var x: i64 = 1 + 2;\n    return x;\n}\n";
    expect(sl_kind(s, 0) == TK_KW_PUB);
    expect(sl_kind(s, 1) == TK_KW_FN);
    expect(sl_kind(s, 2) == TK_IDENT);    // main
    expect(sl_kind(s, 3) == TK_LPAREN);
    expect(sl_kind(s, 4) == TK_RPAREN);
    expect(sl_kind(s, 5) == TK_IDENT);    // void — a type name, not a keyword
    expect(sl_kind(s, 6) == TK_LBRACE);
    expect(sl_kind(s, 7) == TK_KW_VAR);
    expect(sl_kind(s, 8) == TK_IDENT);    // x
    expect(sl_kind(s, 9) == TK_COLON);
    expect(sl_kind(s, 10) == TK_IDENT);   // i64
    expect(sl_kind(s, 11) == TK_EQ);
    expect(sl_kind(s, 12) == TK_INT);
    expect(sl_kind(s, 13) == TK_PLUS);
    expect(sl_kind(s, 14) == TK_INT);
    expect(sl_kind(s, 15) == TK_SEMICOLON);
    expect(sl_kind(s, 16) == TK_KW_RETURN);
    expect(sl_kind(s, 17) == TK_IDENT);
    expect(sl_kind(s, 18) == TK_SEMICOLON);
    expect(sl_kind(s, 19) == TK_RBRACE);
    expect(sl_kind(s, 20) == TK_EOF);
}

test "tk_name spells the canonical table" {
    expect(str_eq(tk_name(TK_KW_FN), "KW_FN"));
    expect(str_eq(tk_name(TK_IDENT), "IDENT"));
    expect(str_eq(tk_name(TK_DOTDOT), "DOTDOT"));
    expect(str_eq(tk_name(TK_SHR), "SHR"));
    expect(str_eq(tk_name(TK_EOF), "EOF"));
    expect(str_eq(tk_name(TK_ERROR), "ERROR"));
}
