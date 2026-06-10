// lexer.ks — self-host stage 1 (v0.159): the kardashev lexer, written in
// kardashev.
//
// A rule-for-rule replica of `crates/kardc/src/lexer.rs` (the Rust reference
// implementation of SPEC §1), differentially tested against it byte-for-byte
// by `crates/kardc/tests/selfhost_lexer.rs`. Tokens carry SPANS into the
// source (`off`/`len` byte offsets) instead of decoded payloads — for a lexer
// whose output is compared positionally, the span IS the payload (the text of
// an IDENT/INT/FLOAT/STR is exactly `src[off .. off+len]`).
//
// Differences from the Rust reference, by design (documented contract):
// - The Rust `lex` COLLECTS every error and returns them all; this lexer
//   stops at the FIRST lexical error: `next` returns a sticky TK_ERROR token
//   whose `off` is the error position and whose `len` encodes the error code
//   (1 = E0001 unexpected character / string error, 2 = E0002 integer literal
//   out of range). Because the Rust scan pushes diagnostics in strict
//   left-to-right order, its FIRST diagnostic always coincides with this
//   lexer's TK_ERROR — which is the artifact the differential test compares.
// - String escapes are validated but not decoded (spans only). The four legal
//   escapes are SPEC §1's `\n \t \\ \"`; any other `\x` is E0001 at the
//   backslash. An unterminated string is E0001 at the opening quote.
//
// Canonical KINDNAME table (shared with the Rust differential driver; the
// dump line for a token is `<KINDNAME> <off> <len>`):
//
//   ERROR EOF IDENT INT FLOAT STR
//   KW_PUB KW_FN KW_CONST KW_VAR KW_RETURN KW_IF KW_ELSE KW_WHILE KW_BREAK
//   KW_CONTINUE KW_DEFER KW_COMPTIME KW_TEST KW_TRUE KW_FALSE KW_AND KW_OR
//   KW_STRUCT KW_ORELSE KW_NULL KW_TRY KW_CATCH KW_ERROR KW_ENUM KW_SWITCH
//   KW_UNION KW_ERRDEFER KW_FOR KW_UNREACHABLE
//   LPAREN RPAREN LBRACE RBRACE LBRACKET RBRACKET COMMA SEMICOLON COLON DOT
//   EQ PLUSEQ MINUSEQ STAREQ SLASHEQ PERCENTEQ EQEQ BANGEQ LT LE GT GE PLUS
//   MINUS STAR SLASH PERCENT BANG QUESTION FATARROW AMP DOTDOT PIPE AT CARET
//   TILDE SHL SHR

@import("std");

// --- token kinds (one u8 constant per Rust TokenKind variant) ---------------

/// A lexical error (sticky; `off` = error position, `len` = code: 1 or 2).
pub const TK_ERROR: u8 = 0;
/// End of input — always the final token, zero-width at `src.len`.
pub const TK_EOF: u8 = 1;
/// An identifier `[A-Za-z_][A-Za-z0-9_]*` that is not a keyword (SPEC §1).
pub const TK_IDENT: u8 = 2;
/// A decimal integer literal `[0-9]+` that fits in i64 (SPEC §1).
pub const TK_INT: u8 = 3;
/// A float literal `digits.digits` (v0.144).
pub const TK_FLOAT: u8 = 4;
/// A string literal `"…"` with escapes `\n \t \\ \"` (span includes quotes).
pub const TK_STR: u8 = 5;

/// Keyword `pub`.
pub const TK_KW_PUB: u8 = 6;
/// Keyword `fn`.
pub const TK_KW_FN: u8 = 7;
/// Keyword `const`.
pub const TK_KW_CONST: u8 = 8;
/// Keyword `var`.
pub const TK_KW_VAR: u8 = 9;
/// Keyword `return`.
pub const TK_KW_RETURN: u8 = 10;
/// Keyword `if`.
pub const TK_KW_IF: u8 = 11;
/// Keyword `else`.
pub const TK_KW_ELSE: u8 = 12;
/// Keyword `while`.
pub const TK_KW_WHILE: u8 = 13;
/// Keyword `break`.
pub const TK_KW_BREAK: u8 = 14;
/// Keyword `continue`.
pub const TK_KW_CONTINUE: u8 = 15;
/// Keyword `defer`.
pub const TK_KW_DEFER: u8 = 16;
/// Keyword `comptime`.
pub const TK_KW_COMPTIME: u8 = 17;
/// Keyword `test`.
pub const TK_KW_TEST: u8 = 18;
/// Keyword `true`.
pub const TK_KW_TRUE: u8 = 19;
/// Keyword `false`.
pub const TK_KW_FALSE: u8 = 20;
/// Keyword `and`.
pub const TK_KW_AND: u8 = 21;
/// Keyword `or`.
pub const TK_KW_OR: u8 = 22;
/// Keyword `struct`.
pub const TK_KW_STRUCT: u8 = 23;
/// Keyword `orelse`.
pub const TK_KW_ORELSE: u8 = 24;
/// Keyword `null`.
pub const TK_KW_NULL: u8 = 25;
/// Keyword `try`.
pub const TK_KW_TRY: u8 = 26;
/// Keyword `catch`.
pub const TK_KW_CATCH: u8 = 27;
/// Keyword `error`.
pub const TK_KW_ERROR: u8 = 28;
/// Keyword `enum`.
pub const TK_KW_ENUM: u8 = 29;
/// Keyword `switch`.
pub const TK_KW_SWITCH: u8 = 30;
/// Keyword `union`.
pub const TK_KW_UNION: u8 = 31;
/// Keyword `errdefer`.
pub const TK_KW_ERRDEFER: u8 = 32;
/// Keyword `for`.
pub const TK_KW_FOR: u8 = 33;
/// Keyword `unreachable`.
pub const TK_KW_UNREACHABLE: u8 = 34;

/// `(`
pub const TK_LPAREN: u8 = 35;
/// `)`
pub const TK_RPAREN: u8 = 36;
/// `{`
pub const TK_LBRACE: u8 = 37;
/// `}`
pub const TK_RBRACE: u8 = 38;
/// `[`
pub const TK_LBRACKET: u8 = 39;
/// `]`
pub const TK_RBRACKET: u8 = 40;
/// `,`
pub const TK_COMMA: u8 = 41;
/// `;`
pub const TK_SEMICOLON: u8 = 42;
/// `:`
pub const TK_COLON: u8 = 43;
/// `.`
pub const TK_DOT: u8 = 44;
/// `=`
pub const TK_EQ: u8 = 45;
/// `+=`
pub const TK_PLUSEQ: u8 = 46;
/// `-=`
pub const TK_MINUSEQ: u8 = 47;
/// `*=`
pub const TK_STAREQ: u8 = 48;
/// `/=`
pub const TK_SLASHEQ: u8 = 49;
/// `%=`
pub const TK_PERCENTEQ: u8 = 50;
/// `==`
pub const TK_EQEQ: u8 = 51;
/// `!=`
pub const TK_BANGEQ: u8 = 52;
/// `<`
pub const TK_LT: u8 = 53;
/// `<=`
pub const TK_LE: u8 = 54;
/// `>`
pub const TK_GT: u8 = 55;
/// `>=`
pub const TK_GE: u8 = 56;
/// `+`
pub const TK_PLUS: u8 = 57;
/// `-`
pub const TK_MINUS: u8 = 58;
/// `*`
pub const TK_STAR: u8 = 59;
/// `/`
pub const TK_SLASH: u8 = 60;
/// `%`
pub const TK_PERCENT: u8 = 61;
/// `!`
pub const TK_BANG: u8 = 62;
/// `?`
pub const TK_QUESTION: u8 = 63;
/// `=>` (switch arms)
pub const TK_FATARROW: u8 = 64;
/// `&` (address-of / bitwise and)
pub const TK_AMP: u8 = 65;
/// `..` (slice ranges)
pub const TK_DOTDOT: u8 = 66;
/// `|` (captures / bitwise or)
pub const TK_PIPE: u8 = 67;
/// `@` (builtins)
pub const TK_AT: u8 = 68;
/// `^` (bitwise xor)
pub const TK_CARET: u8 = 69;
/// `~` (bitwise not)
pub const TK_TILDE: u8 = 70;
/// `<<` (left shift)
pub const TK_SHL: u8 = 71;
/// `>>` (right shift)
pub const TK_SHR: u8 = 72;

// --- token ------------------------------------------------------------------

/// One token: a kind plus its span into the source. `src[off .. off+len]` is
/// the token's exact text. For TK_EOF the span is zero-width at `src.len`.
/// For TK_ERROR, `off` is the error position and `len` ENCODES the error code
/// (1 = E0001, 2 = E0002) — it is not a length.
pub const Token = struct {
    kind: u8,
    off: usize,
    len: usize,
};

// --- classification helpers (mirror lexer.rs free fns) -----------------------

// First-character rule for identifiers: a letter or underscore (SPEC §1).
fn lx_is_ident_start(b: u8) bool {
    if (b == 95) {                          // '_'
        return true;
    }
    if (b >= 97 and b <= 122) {             // 'a'..'z'
        return true;
    }
    if (b >= 65 and b <= 90) {              // 'A'..'Z'
        return true;
    }
    return false;
}

// Continuation rule for identifiers: a letter, digit or underscore.
fn lx_is_ident_continue(b: u8) bool {
    if (lx_is_ident_start(b)) {
        return true;
    }
    return lx_is_digit(b);
}

// ASCII decimal digit?
fn lx_is_digit(b: u8) bool {
    if (b >= 48 and b <= 57) {              // '0'..'9'
        return true;
    }
    return false;
}

// Byte length of the UTF-8 code point whose leading byte is `b` — used only
// to step over (and position) multi-byte input exactly like the Rust
// `utf8_len` (a stray continuation byte advances one to make progress).
fn lx_utf8_len(b: u8) usize {
    if (b < 128) {
        return 1;
    }
    if ((b >> 5) == 6) {                    // 0b110xxxxx
        return 2;
    }
    if ((b >> 4) == 14) {                   // 0b1110xxxx
        return 3;
    }
    if ((b >> 3) == 30) {                   // 0b11110xxx
        return 4;
    }
    return 1;
}

// Does the digit string `w` fit in i64? Replicates Rust's
// `str::parse::<i64>()` acceptance for `[0-9]+` (leading zeros allowed,
// 9223372036854775807 is the last accepted value). Overflow-safe NEGATIVE
// accumulation — the std `parse_i64` technique (|i64 min| > i64 max).
fn lx_int_fits(w: []u8) bool {
    var min: i64 = (0 - 9223372036854775807) - 1;
    var acc: i64 = 0;
    var i: usize = 0;
    while (i < w.len) : (i += 1) {
        var d: i64 = @as(i64, w[i]) - 48;
        if (acc < (min + d) / 10) {
            return false;
        }
        acc = acc * 10 - d;
    }
    if (acc == min) {                       // exactly 2^63: one past i64 max
        return false;
    }
    return true;
}

// Keyword lookup: map an identifier spelling to its TK_KW_* kind, or
// TK_IDENT if it is not a keyword. Dispatch on length first, then `str_eq`
// against the literal spelling — every arm mirrors `Kw::from_str` in
// `token.rs` (29 keywords; type names like `i32` are NOT keywords, SPEC §1).
fn lx_kw_kind(w: []u8) u8 {
    if (w.len == 2) {
        if (str_eq(w, "fn")) { return TK_KW_FN; }
        if (str_eq(w, "if")) { return TK_KW_IF; }
        if (str_eq(w, "or")) { return TK_KW_OR; }
    }
    if (w.len == 3) {
        if (str_eq(w, "pub")) { return TK_KW_PUB; }
        if (str_eq(w, "var")) { return TK_KW_VAR; }
        if (str_eq(w, "and")) { return TK_KW_AND; }
        if (str_eq(w, "try")) { return TK_KW_TRY; }
        if (str_eq(w, "for")) { return TK_KW_FOR; }
    }
    if (w.len == 4) {
        if (str_eq(w, "else")) { return TK_KW_ELSE; }
        if (str_eq(w, "test")) { return TK_KW_TEST; }
        if (str_eq(w, "true")) { return TK_KW_TRUE; }
        if (str_eq(w, "null")) { return TK_KW_NULL; }
        if (str_eq(w, "enum")) { return TK_KW_ENUM; }
    }
    if (w.len == 5) {
        if (str_eq(w, "const")) { return TK_KW_CONST; }
        if (str_eq(w, "while")) { return TK_KW_WHILE; }
        if (str_eq(w, "break")) { return TK_KW_BREAK; }
        if (str_eq(w, "defer")) { return TK_KW_DEFER; }
        if (str_eq(w, "false")) { return TK_KW_FALSE; }
        if (str_eq(w, "catch")) { return TK_KW_CATCH; }
        if (str_eq(w, "error")) { return TK_KW_ERROR; }
        if (str_eq(w, "union")) { return TK_KW_UNION; }
    }
    if (w.len == 6) {
        if (str_eq(w, "return")) { return TK_KW_RETURN; }
        if (str_eq(w, "struct")) { return TK_KW_STRUCT; }
        if (str_eq(w, "orelse")) { return TK_KW_ORELSE; }
        if (str_eq(w, "switch")) { return TK_KW_SWITCH; }
    }
    if (w.len == 8) {
        if (str_eq(w, "continue")) { return TK_KW_CONTINUE; }
        if (str_eq(w, "comptime")) { return TK_KW_COMPTIME; }
        if (str_eq(w, "errdefer")) { return TK_KW_ERRDEFER; }
    }
    if (w.len == 11) {
        if (str_eq(w, "unreachable")) { return TK_KW_UNREACHABLE; }
    }
    return TK_IDENT;
}

// --- the lexer ----------------------------------------------------------------

/// The kardashev lexer (SPEC §1): byte scanner over `src` producing one
/// [`Token`] per `next()` call, terminated by TK_EOF. After the first lexical
/// error every subsequent `next()` returns the same TK_ERROR token (sticky).
pub const Lexer = struct {
    src: []u8,
    pos: usize,
    failed: bool,
    err_code: u8,
    err_pos: usize,

    /// A lexer positioned at the start of `src`.
    fn init(src: []u8) Self {
        return Self{ .src = src, .pos = 0, .failed = false, .err_code = 0, .err_pos = 0 };
    }

    // Record the first error and produce its sticky TK_ERROR token
    // (`off` = position, `len` = code: 1 = E0001, 2 = E0002).
    fn fail(self: *Self, code: u8, at: usize) Token {
        self.failed = true;
        self.err_code = code;
        self.err_pos = at;
        return Token{ .kind = TK_ERROR, .off = at, .len = @as(usize, code) };
    }

    // Produce a `w`-byte operator/punct token at the cursor and advance.
    fn make(self: *Self, kind: u8, w: usize) Token {
        var t: Token = Token{ .kind = kind, .off = self.pos, .len = w };
        self.pos += w;
        return t;
    }

    /// The next token. Whitespace and `//` line comments are skipped;
    /// two-char operators win over their one-char prefixes (maximal munch);
    /// the stream ends with a zero-width TK_EOF at `src.len` (repeated calls
    /// keep returning it). Mirrors `lexer.rs::lex` arm for arm.
    fn next(self: *Self) Token {
        if (self.failed) {
            return Token{ .kind = TK_ERROR, .off = self.err_pos, .len = @as(usize, self.err_code) };
        }
        var src: []u8 = self.src;
        var n: usize = src.len;
        while (self.pos < n) {
            var b: u8 = src[self.pos];

            // --- whitespace: space, tab, CR, LF --------------------------
            if (b == 32 or b == 9 or b == 13 or b == 10) {
                self.pos += 1;
                continue;
            }

            // --- line comment `// ... <eol>` ------------------------------
            if (b == 47 and self.pos + 1 < n) {
                if (src[self.pos + 1] == 47) {
                    self.pos += 2;
                    while (self.pos < n and src[self.pos] != 10) {
                        self.pos += 1;
                    }
                    continue;
                }
            }

            // --- identifier or keyword ------------------------------------
            if (lx_is_ident_start(b)) {
                var start: usize = self.pos;
                self.pos += 1;
                while (self.pos < n and lx_is_ident_continue(src[self.pos])) {
                    self.pos += 1;
                }
                var w: []u8 = src[start..self.pos];
                return Token{ .kind = lx_kw_kind(w), .off = start, .len = self.pos - start };
            }

            // --- integer / float literal ----------------------------------
            if (lx_is_digit(b)) {
                var start: usize = self.pos;
                self.pos += 1;
                while (self.pos < n and lx_is_digit(src[self.pos])) {
                    self.pos += 1;
                }
                // A `.` immediately followed by a digit makes a float
                // literal `3.14` (v0.144); `..` stays a range and `.x` stays
                // field access — exactly the Rust `is_float` test.
                var isf: bool = false;
                if (self.pos + 1 < n) {
                    if (src[self.pos] == 46 and lx_is_digit(src[self.pos + 1])) {
                        isf = true;
                    }
                }
                if (isf) {
                    self.pos += 1;          // consume '.'
                    while (self.pos < n and lx_is_digit(src[self.pos])) {
                        self.pos += 1;
                    }
                    // `digits.digits` always parses as f64 in the Rust
                    // reference (huge values round to inf) — never an error.
                    return Token{ .kind = TK_FLOAT, .off = start, .len = self.pos - start };
                }
                if (lx_int_fits(src[start..self.pos])) {
                    return Token{ .kind = TK_INT, .off = start, .len = self.pos - start };
                }
                return self.fail(2, start); // E0002: out of range for i64
            }

            // --- string literal "..." -------------------------------------
            if (b == 34) {
                var start: usize = self.pos;
                self.pos += 1;              // consume the opening quote
                var terminated: bool = false;
                while (self.pos < n) {
                    var c: u8 = src[self.pos];
                    if (c == 34) {
                        self.pos += 1;      // consume the closing quote
                        terminated = true;
                        break;
                    }
                    if (c == 92) {          // backslash
                        if (self.pos + 1 < n) {
                            var e: u8 = src[self.pos + 1];
                            // The four legal escapes: \n \t \\ \"
                            if (e != 110 and e != 116 and e != 92 and e != 34) {
                                // E0001: unknown escape (Rust span starts at
                                // the backslash).
                                return self.fail(1, self.pos);
                            }
                            self.pos += 2;
                            continue;
                        }
                        // A trailing backslash at EOF — unterminated.
                        self.pos += 1;
                        break;
                    }
                    // Ordinary content — step over the whole UTF-8 char.
                    self.pos += lx_utf8_len(c);
                    if (self.pos > n) {
                        self.pos = n;
                    }
                }
                if (!terminated) {
                    // E0001: unterminated string (Rust span starts at the
                    // opening quote).
                    return self.fail(1, start);
                }
                return Token{ .kind = TK_STR, .off = start, .len = self.pos - start };
            }

            // --- operators & punctuation ----------------------------------
            // `two` is the lookahead byte (0 when at the last byte — NUL
            // never begins any two-char operator tail we test, so the
            // sentinel is unambiguous).
            var two: u8 = 0;
            if (self.pos + 1 < n) {
                two = src[self.pos + 1];
            }
            if (b == 40) { return self.make(TK_LPAREN, 1); }            // (
            if (b == 41) { return self.make(TK_RPAREN, 1); }            // )
            if (b == 123) { return self.make(TK_LBRACE, 1); }           // {
            if (b == 125) { return self.make(TK_RBRACE, 1); }           // }
            if (b == 91) { return self.make(TK_LBRACKET, 1); }          // [
            if (b == 93) { return self.make(TK_RBRACKET, 1); }          // ]
            if (b == 44) { return self.make(TK_COMMA, 1); }             // ,
            if (b == 59) { return self.make(TK_SEMICOLON, 1); }         // ;
            if (b == 58) { return self.make(TK_COLON, 1); }             // :
            if (b == 46) {                                              // .
                if (two == 46) { return self.make(TK_DOTDOT, 2); }      // ..
                return self.make(TK_DOT, 1);
            }
            if (b == 38) { return self.make(TK_AMP, 1); }               // &
            if (b == 124) { return self.make(TK_PIPE, 1); }             // |
            if (b == 64) { return self.make(TK_AT, 1); }                // @
            if (b == 43) {                                              // +
                if (two == 61) { return self.make(TK_PLUSEQ, 2); }      // +=
                return self.make(TK_PLUS, 1);
            }
            if (b == 45) {                                              // -
                if (two == 61) { return self.make(TK_MINUSEQ, 2); }     // -=
                return self.make(TK_MINUS, 1);
            }
            if (b == 42) {                                              // *
                if (two == 61) { return self.make(TK_STAREQ, 2); }      // *=
                return self.make(TK_STAR, 1);
            }
            if (b == 47) {                                              // / (`//` already taken as a comment above)
                if (two == 61) { return self.make(TK_SLASHEQ, 2); }     // /=
                return self.make(TK_SLASH, 1);
            }
            if (b == 37) {                                              // %
                if (two == 61) { return self.make(TK_PERCENTEQ, 2); }   // %=
                return self.make(TK_PERCENT, 1);
            }
            if (b == 63) { return self.make(TK_QUESTION, 1); }          // ?
            if (b == 61) {                                              // =
                if (two == 61) { return self.make(TK_EQEQ, 2); }        // ==
                if (two == 62) { return self.make(TK_FATARROW, 2); }    // =>
                return self.make(TK_EQ, 1);
            }
            if (b == 33) {                                              // !
                if (two == 61) { return self.make(TK_BANGEQ, 2); }      // !=
                return self.make(TK_BANG, 1);
            }
            if (b == 60) {                                              // <
                if (two == 60) { return self.make(TK_SHL, 2); }         // <<
                if (two == 61) { return self.make(TK_LE, 2); }          // <=
                return self.make(TK_LT, 1);
            }
            if (b == 62) {                                              // >
                if (two == 62) { return self.make(TK_SHR, 2); }         // >>
                if (two == 61) { return self.make(TK_GE, 2); }          // >=
                return self.make(TK_GT, 1);
            }
            if (b == 94) { return self.make(TK_CARET, 1); }             // ^
            if (b == 126) { return self.make(TK_TILDE, 1); }            // ~
            // E0001: unexpected character (the Rust span covers the whole
            // UTF-8 char but STARTS at `pos` — the position we report).
            return self.fail(1, self.pos);
        }
        // The stream always ends with a single zero-width EOF token.
        return Token{ .kind = TK_EOF, .off = n, .len = 0 };
    }
};

// --- KINDNAME table -----------------------------------------------------------

/// The canonical dump spelling for `kind` — the table in this file's header.
/// `selfhost/lexdump.ks` prints `<tk_name(kind)> <off> <len>` per token and
/// the Rust differential driver produces the identical spelling from
/// `TokenKind`, so the two dumps are byte-comparable.
pub fn tk_name(kind: u8) []u8 {
    if (kind == TK_ERROR) { return "ERROR"; }
    if (kind == TK_EOF) { return "EOF"; }
    if (kind == TK_IDENT) { return "IDENT"; }
    if (kind == TK_INT) { return "INT"; }
    if (kind == TK_FLOAT) { return "FLOAT"; }
    if (kind == TK_STR) { return "STR"; }
    if (kind == TK_KW_PUB) { return "KW_PUB"; }
    if (kind == TK_KW_FN) { return "KW_FN"; }
    if (kind == TK_KW_CONST) { return "KW_CONST"; }
    if (kind == TK_KW_VAR) { return "KW_VAR"; }
    if (kind == TK_KW_RETURN) { return "KW_RETURN"; }
    if (kind == TK_KW_IF) { return "KW_IF"; }
    if (kind == TK_KW_ELSE) { return "KW_ELSE"; }
    if (kind == TK_KW_WHILE) { return "KW_WHILE"; }
    if (kind == TK_KW_BREAK) { return "KW_BREAK"; }
    if (kind == TK_KW_CONTINUE) { return "KW_CONTINUE"; }
    if (kind == TK_KW_DEFER) { return "KW_DEFER"; }
    if (kind == TK_KW_COMPTIME) { return "KW_COMPTIME"; }
    if (kind == TK_KW_TEST) { return "KW_TEST"; }
    if (kind == TK_KW_TRUE) { return "KW_TRUE"; }
    if (kind == TK_KW_FALSE) { return "KW_FALSE"; }
    if (kind == TK_KW_AND) { return "KW_AND"; }
    if (kind == TK_KW_OR) { return "KW_OR"; }
    if (kind == TK_KW_STRUCT) { return "KW_STRUCT"; }
    if (kind == TK_KW_ORELSE) { return "KW_ORELSE"; }
    if (kind == TK_KW_NULL) { return "KW_NULL"; }
    if (kind == TK_KW_TRY) { return "KW_TRY"; }
    if (kind == TK_KW_CATCH) { return "KW_CATCH"; }
    if (kind == TK_KW_ERROR) { return "KW_ERROR"; }
    if (kind == TK_KW_ENUM) { return "KW_ENUM"; }
    if (kind == TK_KW_SWITCH) { return "KW_SWITCH"; }
    if (kind == TK_KW_UNION) { return "KW_UNION"; }
    if (kind == TK_KW_ERRDEFER) { return "KW_ERRDEFER"; }
    if (kind == TK_KW_FOR) { return "KW_FOR"; }
    if (kind == TK_KW_UNREACHABLE) { return "KW_UNREACHABLE"; }
    if (kind == TK_LPAREN) { return "LPAREN"; }
    if (kind == TK_RPAREN) { return "RPAREN"; }
    if (kind == TK_LBRACE) { return "LBRACE"; }
    if (kind == TK_RBRACE) { return "RBRACE"; }
    if (kind == TK_LBRACKET) { return "LBRACKET"; }
    if (kind == TK_RBRACKET) { return "RBRACKET"; }
    if (kind == TK_COMMA) { return "COMMA"; }
    if (kind == TK_SEMICOLON) { return "SEMICOLON"; }
    if (kind == TK_COLON) { return "COLON"; }
    if (kind == TK_DOT) { return "DOT"; }
    if (kind == TK_EQ) { return "EQ"; }
    if (kind == TK_PLUSEQ) { return "PLUSEQ"; }
    if (kind == TK_MINUSEQ) { return "MINUSEQ"; }
    if (kind == TK_STAREQ) { return "STAREQ"; }
    if (kind == TK_SLASHEQ) { return "SLASHEQ"; }
    if (kind == TK_PERCENTEQ) { return "PERCENTEQ"; }
    if (kind == TK_EQEQ) { return "EQEQ"; }
    if (kind == TK_BANGEQ) { return "BANGEQ"; }
    if (kind == TK_LT) { return "LT"; }
    if (kind == TK_LE) { return "LE"; }
    if (kind == TK_GT) { return "GT"; }
    if (kind == TK_GE) { return "GE"; }
    if (kind == TK_PLUS) { return "PLUS"; }
    if (kind == TK_MINUS) { return "MINUS"; }
    if (kind == TK_STAR) { return "STAR"; }
    if (kind == TK_SLASH) { return "SLASH"; }
    if (kind == TK_PERCENT) { return "PERCENT"; }
    if (kind == TK_BANG) { return "BANG"; }
    if (kind == TK_QUESTION) { return "QUESTION"; }
    if (kind == TK_FATARROW) { return "FATARROW"; }
    if (kind == TK_AMP) { return "AMP"; }
    if (kind == TK_DOTDOT) { return "DOTDOT"; }
    if (kind == TK_PIPE) { return "PIPE"; }
    if (kind == TK_AT) { return "AT"; }
    if (kind == TK_CARET) { return "CARET"; }
    if (kind == TK_TILDE) { return "TILDE"; }
    if (kind == TK_SHL) { return "SHL"; }
    if (kind == TK_SHR) { return "SHR"; }
    return "UNKNOWN";
}
