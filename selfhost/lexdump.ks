// lexdump.ks — driver for the self-hosted lexer (v0.159).
//
//   kard run selfhost/lexdump.ks -- <file.ks>
//
// Reads the file named by the first program argument, lexes it with
// `selfhost/lexer.ks`, and dumps the token stream — ONE LINE PER TOKEN:
//
//   <KINDNAME> <off> <len>        e.g.  KW_FN 0 2 / IDENT 3 4 / ... / EOF 12 0
//
// For a lexically erroneous input the WHOLE dump is exactly one line:
//
//   ERROR <code> <pos>            code: 1 = E0001, 2 = E0002
//
// (the Rust reference `lex` discards its token vector when it returns
// diagnostics, so the first diagnostic is the only artifact both sides can
// render identically — the differential driver in
// `crates/kardc/tests/selfhost_lexer.rs` produces these exact bytes from the
// Rust lexer's output). The exit code is always 0: the dump CAPTURES the
// error; the comparison is on stdout.

@import("lexer.ks");
@import("std");

/// Print one dump line `<name> <x> <y>` (a token's `name off len`, or
/// `ERROR code pos`).
fn ld_line(a: Allocator, name: []u8, x: usize, y: usize) void {
    var sb: StrBuilder = StrBuilder.init(a);
    sb.append(a, name);
    sb.append_byte(a, 32);              // ' '
    sb.append_i64(a, @as(i64, x));
    sb.append_byte(a, 32);              // ' '
    sb.append_i64(a, @as(i64, y));
    var line: []u8 = sb.build(a);
    print(line);
    free(a, line);
    sb.deinit(a);
}

pub fn main() i32 {
    var a: Allocator = c_allocator();
    var path: []u8 = @arg(a, 1);
    var src: []u8 = @readFile(a, path);

    // Pass 1: scan to the end. If the source has a lexical error the dump is
    // exactly one ERROR line (see header) — so nothing may be printed yet.
    var lx: Lexer = Lexer.init(src);
    var t: Token = lx.next();
    while (t.kind != TK_EOF and t.kind != TK_ERROR) {
        t = lx.next();
    }
    if (t.kind == TK_ERROR) {
        // TK_ERROR encodes the code in `len` and the position in `off`.
        ld_line(a, "ERROR", t.len, t.off);
        return 0;
    }

    // Pass 2: clean source — re-lex and print every token, EOF included.
    var lx2: Lexer = Lexer.init(src);
    var u: Token = lx2.next();
    while (u.kind != TK_EOF) {
        ld_line(a, tk_name(u.kind), u.off, u.len);
        u = lx2.next();
    }
    ld_line(a, "EOF", u.off, u.len);
    return 0;
}
