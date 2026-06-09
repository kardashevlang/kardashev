//! Lexer: source text → tokens.
//!
//! Turns kardashev source (`&str`) into a `Vec<Token>` carrying byte-offset
//! [`Span`]s, per SPEC §1. Whitespace and `//` line comments are skipped.
//! Identifiers are `[A-Za-z_][A-Za-z0-9_]*`; a spelling that matches a keyword
//! lexes as [`TokenKind::Keyword`], otherwise [`TokenKind::Ident`]. Type names
//! (`i32`, `bool`, ...) are *not* keywords — they stay identifiers and are
//! resolved in sema.
//!
//! Every lexical error is collected rather than aborting at the first one, so a
//! single `kard` invocation reports them all. Two error codes originate here:
//! `E0001` (unexpected character) and `E0002` (integer literal out of range).

use crate::diag::Diagnostic;
use crate::span::Span;
use crate::token::{Kw, Token, TokenKind};

/// Lex `src` into a token stream terminated by a single zero-width `Eof` token.
///
/// Returns `Ok(tokens)` when the source is lexically clean. If any character is
/// unrecognized (`E0001`) or an integer literal overflows `i64` (`E0002`),
/// lexing continues so every error is reported, and `Err(diags)` is returned.
pub fn lex(src: &str) -> Result<Vec<Token>, Vec<Diagnostic>> {
    let bytes = src.as_bytes();
    let len = bytes.len();
    let mut pos = 0usize;
    let mut tokens: Vec<Token> = Vec::new();
    let mut diags: Vec<Diagnostic> = Vec::new();

    while pos < len {
        let b = bytes[pos];

        // --- Whitespace -----------------------------------------------------
        if b == b' ' || b == b'\t' || b == b'\r' || b == b'\n' {
            pos += 1;
            continue;
        }

        // --- Line comment `// ... <eol>` -----------------------------------
        if b == b'/' && pos + 1 < len && bytes[pos + 1] == b'/' {
            pos += 2;
            while pos < len && bytes[pos] != b'\n' {
                pos += 1;
            }
            continue;
        }

        // --- Identifier or keyword -----------------------------------------
        if is_ident_start(b) {
            let start = pos;
            pos += 1;
            while pos < len && is_ident_continue(bytes[pos]) {
                pos += 1;
            }
            let span = Span::new(start, pos);
            let text = &src[start..pos];
            let kind = match Kw::from_str(text) {
                Some(kw) => TokenKind::Keyword(kw),
                None => TokenKind::Ident(text.to_string()),
            };
            tokens.push(Token::new(kind, span));
            continue;
        }

        // --- Integer literal `[0-9]+` --------------------------------------
        if b.is_ascii_digit() {
            let start = pos;
            pos += 1;
            while pos < len && bytes[pos].is_ascii_digit() {
                pos += 1;
            }
            // A `.` immediately followed by a digit makes a float literal
            // `3.14` (v0.144). A `.` followed by `.` is the slice-range `..`
            // (`0..5`), and a `.` followed by a non-digit is field access on the
            // (rare) integer — both leave the integer literal intact.
            let is_float = pos + 1 < len && bytes[pos] == b'.' && bytes[pos + 1].is_ascii_digit();
            if is_float {
                pos += 1; // consume `.`
                while pos < len && bytes[pos].is_ascii_digit() {
                    pos += 1;
                }
                let span = Span::new(start, pos);
                let text = &src[start..pos];
                match text.parse::<f64>() {
                    Ok(v) => tokens.push(Token::new(TokenKind::Float(v), span)),
                    Err(_) => {
                        diags.push(Diagnostic::error(
                            span,
                            "E0002",
                            format!("float literal `{}` is malformed", text),
                        ));
                        tokens.push(Token::new(TokenKind::Float(0.0), span));
                    }
                }
                continue;
            }
            let span = Span::new(start, pos);
            let text = &src[start..pos];
            match text.parse::<i64>() {
                Ok(v) => tokens.push(Token::new(TokenKind::Int(v), span)),
                Err(_) => {
                    diags.push(Diagnostic::error(
                        span,
                        "E0002",
                        format!("integer literal `{}` out of range for i64", text),
                    ));
                    // Keep a placeholder token so the (discarded-on-error)
                    // stream stays structurally sane during recovery.
                    tokens.push(Token::new(TokenKind::Int(0), span));
                }
            }
            continue;
        }

        // --- String literal `"..."` ----------------------------------------
        if b == b'"' {
            let start = pos;
            pos += 1; // consume the opening quote
            let mut value = String::new();
            let mut terminated = false;
            while pos < len {
                let c = bytes[pos];
                if c == b'"' {
                    pos += 1; // consume the closing quote
                    terminated = true;
                    break;
                }
                if c == b'\\' {
                    if pos + 1 < len {
                        let e = bytes[pos + 1];
                        match e {
                            b'n' => value.push('\n'),
                            b't' => value.push('\t'),
                            b'\\' => value.push('\\'),
                            b'"' => value.push('"'),
                            _ => {
                                diags.push(Diagnostic::error(
                                    Span::new(pos, pos + 2),
                                    "E0001",
                                    "unknown escape sequence in string literal",
                                ));
                                // Best-effort recovery: keep the escaped byte.
                                value.push(e as char);
                            }
                        }
                        pos += 2;
                        continue;
                    } else {
                        // A trailing backslash at EOF — the string is unterminated.
                        pos += 1;
                        break;
                    }
                }
                // Ordinary content — copy the whole UTF-8 char verbatim.
                let ch_len = utf8_len(c);
                let end = (pos + ch_len).min(len);
                value.push_str(&src[pos..end]);
                pos = end;
            }
            let span = Span::new(start, pos);
            if !terminated {
                diags.push(Diagnostic::error(
                    span,
                    "E0001",
                    "unterminated string literal",
                ));
            }
            tokens.push(Token::new(TokenKind::Str(value), span));
            continue;
        }

        // --- Operators & punctuation ---------------------------------------
        // Two-char operators (`== != <= >=`) win over their one-char prefixes.
        let next = if pos + 1 < len { Some(bytes[pos + 1]) } else { None };
        let (kind, width) = match b {
            b'(' => (TokenKind::LParen, 1),
            b')' => (TokenKind::RParen, 1),
            b'{' => (TokenKind::LBrace, 1),
            b'}' => (TokenKind::RBrace, 1),
            b'[' => (TokenKind::LBracket, 1),
            b']' => (TokenKind::RBracket, 1),
            b',' => (TokenKind::Comma, 1),
            b';' => (TokenKind::Semicolon, 1),
            b':' => (TokenKind::Colon, 1),
            b'.' => {
                if next == Some(b'.') {
                    (TokenKind::DotDot, 2)
                } else {
                    (TokenKind::Dot, 1)
                }
            }
            b'&' => (TokenKind::Amp, 1),
            b'|' => (TokenKind::Pipe, 1),
            b'@' => (TokenKind::At, 1),
            // `+ - * / %` and their compound-assignment forms `+= -= *= /= %=`
            // (v0.131): a trailing `=` makes the two-char compound token.
            b'+' if next == Some(b'=') => (TokenKind::PlusEq, 2),
            b'+' => (TokenKind::Plus, 1),
            b'-' if next == Some(b'=') => (TokenKind::MinusEq, 2),
            b'-' => (TokenKind::Minus, 1),
            b'*' if next == Some(b'=') => (TokenKind::StarEq, 2),
            b'*' => (TokenKind::Star, 1),
            b'/' if next == Some(b'=') => (TokenKind::SlashEq, 2),
            b'/' => (TokenKind::Slash, 1),
            b'%' if next == Some(b'=') => (TokenKind::PercentEq, 2),
            b'%' => (TokenKind::Percent, 1),
            b'?' => (TokenKind::Question, 1),
            b'=' => {
                if next == Some(b'=') {
                    (TokenKind::EqEq, 2)
                } else if next == Some(b'>') {
                    (TokenKind::FatArrow, 2)
                } else {
                    (TokenKind::Eq, 1)
                }
            }
            b'!' => {
                if next == Some(b'=') {
                    (TokenKind::BangEq, 2)
                } else {
                    (TokenKind::Bang, 1)
                }
            }
            b'<' => {
                if next == Some(b'<') {
                    (TokenKind::Shl, 2)
                } else if next == Some(b'=') {
                    (TokenKind::Le, 2)
                } else {
                    (TokenKind::Lt, 1)
                }
            }
            b'>' => {
                if next == Some(b'>') {
                    (TokenKind::Shr, 2)
                } else if next == Some(b'=') {
                    (TokenKind::Ge, 2)
                } else {
                    (TokenKind::Gt, 1)
                }
            }
            b'^' => (TokenKind::Caret, 1),
            b'~' => (TokenKind::Tilde, 1),
            _ => {
                // Unrecognized byte — the span covers the whole UTF-8 char so a
                // multibyte symbol is caret-underlined correctly. Then recover.
                let ch_len = utf8_len(b);
                let end = (pos + ch_len).min(len);
                diags.push(Diagnostic::error(
                    Span::new(pos, end),
                    "E0001",
                    "unexpected character",
                ));
                pos = end;
                continue;
            }
        };
        tokens.push(Token::new(kind, Span::new(pos, pos + width)));
        pos += width;
    }

    // The stream always ends with a single zero-width `Eof` token.
    tokens.push(Token::new(TokenKind::Eof, Span::new(len, len)));

    if diags.is_empty() {
        Ok(tokens)
    } else {
        Err(diags)
    }
}

/// First-character rule for identifiers: a letter or underscore.
fn is_ident_start(b: u8) -> bool {
    b == b'_' || b.is_ascii_alphabetic()
}

/// Continuation rule for identifiers: a letter, digit or underscore.
fn is_ident_continue(b: u8) -> bool {
    b == b'_' || b.is_ascii_alphanumeric()
}

/// Byte length of the UTF-8 code point whose leading byte is `b`. Used only to
/// step over (and span) unrecognized non-ASCII input without splitting a char.
fn utf8_len(b: u8) -> usize {
    if b < 0x80 {
        1
    } else if b >> 5 == 0b110 {
        2
    } else if b >> 4 == 0b1110 {
        3
    } else if b >> 3 == 0b11110 {
        4
    } else {
        // A stray continuation byte; advance one to make progress.
        1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Lex, asserting success, and return just the token kinds.
    fn kinds(src: &str) -> Vec<TokenKind> {
        lex(src)
            .expect("expected clean lex")
            .into_iter()
            .map(|t| t.kind)
            .collect()
    }

    #[test]
    fn empty_source_is_just_eof() {
        let toks = lex("").expect("clean");
        assert_eq!(toks.len(), 1);
        assert_eq!(toks[0].kind, TokenKind::Eof);
        // Zero-width span at the end of input.
        assert_eq!(toks[0].span, Span::new(0, 0));
    }

    #[test]
    fn every_token_kind() {
        // One occurrence of each literal, name and operator/punctuation kind.
        let src = "x 5 \"hi\" fn ( ) { } [ ] , ; : . = == != < <= > >= + - * / % !";
        assert_eq!(
            kinds(src),
            vec![
                TokenKind::Ident("x".to_string()),
                TokenKind::Int(5),
                TokenKind::Str("hi".to_string()),
                TokenKind::Keyword(Kw::Fn),
                TokenKind::LParen,
                TokenKind::RParen,
                TokenKind::LBrace,
                TokenKind::RBrace,
                TokenKind::LBracket,
                TokenKind::RBracket,
                TokenKind::Comma,
                TokenKind::Semicolon,
                TokenKind::Colon,
                TokenKind::Dot,
                TokenKind::Eq,
                TokenKind::EqEq,
                TokenKind::BangEq,
                TokenKind::Lt,
                TokenKind::Le,
                TokenKind::Gt,
                TokenKind::Ge,
                TokenKind::Plus,
                TokenKind::Minus,
                TokenKind::Star,
                TokenKind::Slash,
                TokenKind::Percent,
                TokenKind::Bang,
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn all_keywords_map() {
        let src =
            "pub fn const var return if else while break continue defer comptime test true false and or";
        let expected = [
            Kw::Pub,
            Kw::Fn,
            Kw::Const,
            Kw::Var,
            Kw::Return,
            Kw::If,
            Kw::Else,
            Kw::While,
            Kw::Break,
            Kw::Continue,
            Kw::Defer,
            Kw::Comptime,
            Kw::Test,
            Kw::True,
            Kw::False,
            Kw::And,
            Kw::Or,
        ];
        let got = kinds(src);
        for (i, kw) in expected.iter().enumerate() {
            assert_eq!(got[i], TokenKind::Keyword(*kw), "keyword #{}", i);
        }
        assert_eq!(got.last(), Some(&TokenKind::Eof));
    }

    #[test]
    fn type_names_are_idents_not_keywords() {
        // SPEC §1: `i32`, `bool`, ... are ordinary identifiers.
        assert_eq!(
            kinds("i32 bool void usize"),
            vec![
                TokenKind::Ident("i32".to_string()),
                TokenKind::Ident("bool".to_string()),
                TokenKind::Ident("void".to_string()),
                TokenKind::Ident("usize".to_string()),
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn line_comment_is_skipped() {
        let toks = lex("// a comment\nx").expect("clean");
        assert_eq!(
            toks.iter().map(|t| t.kind.clone()).collect::<Vec<_>>(),
            vec![TokenKind::Ident("x".to_string()), TokenKind::Eof]
        );
        // `// a comment\n` is 13 bytes, so `x` starts at byte 13.
        assert_eq!(toks[0].span, Span::new(13, 14));
    }

    #[test]
    fn comment_to_end_of_file_without_newline() {
        assert_eq!(kinds("x // trailing"), vec![TokenKind::Ident("x".to_string()), TokenKind::Eof]);
    }

    #[test]
    fn bang_versus_bangeq_split() {
        // `!=` is greedy; a lone `!` stays `Bang`.
        assert_eq!(
            kinds("! != !=!"),
            vec![
                TokenKind::Bang,
                TokenKind::BangEq,
                TokenKind::BangEq,
                TokenKind::Bang,
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn lt_versus_le_split() {
        assert_eq!(
            kinds("< <= <=<"),
            vec![
                TokenKind::Lt,
                TokenKind::Le,
                TokenKind::Le,
                TokenKind::Lt,
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn eq_versus_eqeq_split() {
        assert_eq!(
            kinds("= == ==="),
            vec![
                TokenKind::Eq,
                TokenKind::EqEq,
                TokenKind::EqEq,
                TokenKind::Eq,
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn ge_and_gt_split() {
        assert_eq!(
            kinds(">= >"),
            vec![TokenKind::Ge, TokenKind::Gt, TokenKind::Eof]
        );
    }

    #[test]
    fn string_escapes_decode() {
        // Source text: "a\n\t\\\""  →  a, newline, tab, backslash, quote.
        let src = "\"a\\n\\t\\\\\\\"\"";
        let toks = lex(src).expect("clean");
        assert_eq!(toks[0].kind, TokenKind::Str("a\n\t\\\"".to_string()));
        assert_eq!(toks[1].kind, TokenKind::Eof);
    }

    #[test]
    fn integer_literal_and_spans() {
        let toks = lex("abc 123").expect("clean");
        assert_eq!(toks[0].kind, TokenKind::Ident("abc".to_string()));
        assert_eq!(toks[0].span, Span::new(0, 3));
        assert_eq!(toks[1].kind, TokenKind::Int(123));
        assert_eq!(toks[1].span, Span::new(4, 7));
    }

    #[test]
    fn max_i64_lexes() {
        assert_eq!(
            kinds("9223372036854775807"),
            vec![TokenKind::Int(i64::MAX), TokenKind::Eof]
        );
    }

    #[test]
    fn full_small_program() {
        let src = "pub fn main() void {\n    var x: i64 = 1 + 2;\n    return x;\n}\n";
        let toks = lex(src).expect("clean");
        let kinds: Vec<TokenKind> = toks.iter().map(|t| t.kind.clone()).collect();
        assert_eq!(
            kinds,
            vec![
                TokenKind::Keyword(Kw::Pub),
                TokenKind::Keyword(Kw::Fn),
                TokenKind::Ident("main".to_string()),
                TokenKind::LParen,
                TokenKind::RParen,
                TokenKind::Ident("void".to_string()),
                TokenKind::LBrace,
                TokenKind::Keyword(Kw::Var),
                TokenKind::Ident("x".to_string()),
                TokenKind::Colon,
                TokenKind::Ident("i64".to_string()),
                TokenKind::Eq,
                TokenKind::Int(1),
                TokenKind::Plus,
                TokenKind::Int(2),
                TokenKind::Semicolon,
                TokenKind::Keyword(Kw::Return),
                TokenKind::Ident("x".to_string()),
                TokenKind::Semicolon,
                TokenKind::RBrace,
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn overflow_is_e0002() {
        let err = lex("99999999999999999999999").expect_err("overflow");
        assert_eq!(err.len(), 1);
        assert_eq!(err[0].code, "E0002");
        assert_eq!(err[0].span, Span::new(0, 23));
    }

    #[test]
    fn unknown_char_is_e0001_and_collected() {
        // Both `#` and `$` are unknown; `x` still lexes between them. (`@` is a
        // token since v0.126 and `^` since v0.132, so neither is used here.)
        let err = lex("# x $").expect_err("two unknown chars");
        assert_eq!(err.len(), 2);
        assert!(err.iter().all(|d| d.code == "E0001"));
        assert_eq!(err[0].span, Span::new(0, 1));
        assert_eq!(err[1].span, Span::new(4, 5));
    }

    #[test]
    fn unknown_multibyte_char_spans_full_codepoint() {
        // `é` is two bytes in UTF-8; the error span must cover both.
        let err = lex("é").expect_err("non-ascii");
        assert_eq!(err.len(), 1);
        assert_eq!(err[0].code, "E0001");
        assert_eq!(err[0].span, Span::new(0, 2));
    }

    #[test]
    fn unterminated_string_is_e0001() {
        let err = lex("\"oops").expect_err("unterminated");
        assert_eq!(err.len(), 1);
        assert_eq!(err[0].code, "E0001");
    }
}
