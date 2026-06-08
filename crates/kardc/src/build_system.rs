//! The in-language build system.
//!
//! Zig-philosophy: the build is described in the project's own `build.ks`,
//! read by the same `kard` binary. v1 supports a minimal declarative form:
//!
//! ```text
//! build {
//!     name = "hello";
//!     root = "src/main.ks";
//! }
//! ```
//!
//! The full imperative build graph (steps, dependencies, install artifacts)
//! is a later roadmap item.
//!
//! `parse_build_kd` is a tiny self-contained recursive parser — it does not
//! reuse the language lexer/parser, because `build.ks` is a fixed declarative
//! shape rather than a program. It is tolerant of surrounding whitespace and
//! `//` line comments (SPEC §1/§7). Any malformed or missing field yields a
//! single error code, `E0300`.

use crate::diag::Diagnostic;
use crate::span::Span;

/// The resolved build specification.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BuildSpec {
    /// The output executable name.
    pub name: String,
    /// The root source file, relative to the project root.
    pub root: String,
}

/// The stable error code for every `build.ks` parse failure.
const E_BUILD: &str = "E0300";

/// Parse the v1 minimal `build.ks` form, extracting `name` and `root`.
///
/// Tolerates leading/trailing whitespace and `//` comments anywhere between
/// tokens. Unknown fields (still of the `IDENT = "..." ;` shape) are ignored so
/// the format can grow forward-compatibly, but both `name` and `root` are
/// required. Any structural problem or a missing required field produces an
/// `E0300` diagnostic.
pub fn parse_build_kd(src: &str) -> Result<BuildSpec, Vec<Diagnostic>> {
    let mut s = Scanner::new(src);

    // Header: the `build` keyword.
    let header = match s.read_ident() {
        Some((id, span)) if id == "build" => span,
        Some((id, span)) => {
            return Err(vec![Diagnostic::error(
                span,
                E_BUILD,
                format!("expected a `build` block, found `{}`", id),
            )]);
        }
        None => {
            return Err(vec![Diagnostic::error(
                s.eof_span(),
                E_BUILD,
                "expected a `build` block",
            )]);
        }
    };

    if let Err(d) = s.expect('{') {
        return Err(vec![d]);
    }

    let mut name: Option<String> = None;
    let mut root: Option<String> = None;

    loop {
        s.skip_trivia();
        match s.peek() {
            Some('}') => {
                s.bump();
                break;
            }
            None => {
                return Err(vec![Diagnostic::error(
                    s.eof_span(),
                    E_BUILD,
                    "unterminated `build` block: expected a field or `}`",
                )]);
            }
            _ => {}
        }

        // field := IDENT "=" STRING ";"
        let (key, _key_span) = match s.read_ident() {
            Some(kv) => kv,
            None => {
                let bad = s.bad_token_span();
                return Err(vec![Diagnostic::error(
                    bad,
                    E_BUILD,
                    "expected a field name or `}`",
                )]);
            }
        };
        if let Err(d) = s.expect('=') {
            return Err(vec![d]);
        }
        let value = match s.read_string() {
            Ok(v) => v,
            Err(d) => return Err(vec![d]),
        };
        if let Err(d) = s.expect(';') {
            return Err(vec![d]);
        }

        match key.as_str() {
            "name" => name = Some(value),
            "root" => root = Some(value),
            // Forward-compatible: ignore unknown well-formed fields.
            _ => {}
        }
    }

    // Both fields are mandatory in the v1 form.
    let mut missing = Vec::new();
    if name.is_none() {
        missing.push(Diagnostic::error(
            header,
            E_BUILD,
            "`build` block is missing the required `name` field",
        ));
    }
    if root.is_none() {
        missing.push(Diagnostic::error(
            header,
            E_BUILD,
            "`build` block is missing the required `root` field",
        ));
    }
    if !missing.is_empty() {
        return Err(missing);
    }

    Ok(BuildSpec {
        name: name.unwrap(),
        root: root.unwrap(),
    })
}

/// A minimal cursor over `build.ks` source, tracking a byte offset for spans.
struct Scanner<'a> {
    src: &'a str,
    pos: usize,
}

impl<'a> Scanner<'a> {
    fn new(src: &'a str) -> Scanner<'a> {
        Scanner { src, pos: 0 }
    }

    /// The current character without advancing.
    fn peek(&self) -> Option<char> {
        self.src[self.pos..].chars().next()
    }

    /// Advance past the current character.
    fn bump(&mut self) {
        if let Some(c) = self.peek() {
            self.pos += c.len_utf8();
        }
    }

    /// A zero-width span at the end of input.
    fn eof_span(&self) -> Span {
        Span::new(self.src.len(), self.src.len())
    }

    /// A one-character-wide span at the current cursor (for "unexpected token").
    fn bad_token_span(&self) -> Span {
        match self.peek() {
            Some(c) => Span::new(self.pos, self.pos + c.len_utf8()),
            None => self.eof_span(),
        }
    }

    /// Skip whitespace and `//` line comments.
    fn skip_trivia(&mut self) {
        loop {
            match self.peek() {
                Some(c) if c.is_whitespace() => self.bump(),
                Some('/') if self.src[self.pos..].starts_with("//") => {
                    while let Some(c) = self.peek() {
                        if c == '\n' {
                            break;
                        }
                        self.bump();
                    }
                }
                _ => break,
            }
        }
    }

    /// Read an identifier (`[A-Za-z_][A-Za-z0-9_]*`) after skipping trivia.
    /// Returns the spelling and its span, or `None` if the cursor is not on an
    /// identifier start.
    fn read_ident(&mut self) -> Option<(String, Span)> {
        self.skip_trivia();
        let start = self.pos;
        match self.peek() {
            Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
            _ => return None,
        }
        let mut text = String::new();
        while let Some(c) = self.peek() {
            if c.is_ascii_alphanumeric() || c == '_' {
                text.push(c);
                self.bump();
            } else {
                break;
            }
        }
        Some((text, Span::new(start, self.pos)))
    }

    /// Expect the given punctuation character, consuming it. Skips leading
    /// trivia. On mismatch returns an `E0300` diagnostic.
    fn expect(&mut self, ch: char) -> Result<(), Diagnostic> {
        self.skip_trivia();
        match self.peek() {
            Some(c) if c == ch => {
                self.bump();
                Ok(())
            }
            Some(c) => Err(Diagnostic::error(
                Span::new(self.pos, self.pos + c.len_utf8()),
                E_BUILD,
                format!("expected `{}`, found `{}`", ch, c),
            )),
            None => Err(Diagnostic::error(
                self.eof_span(),
                E_BUILD,
                format!("expected `{}` but reached end of input", ch),
            )),
        }
    }

    /// Read a `"..."` string literal (escapes `\n \t \\ \"`, per SPEC §1) after
    /// skipping trivia. Returns the decoded contents.
    fn read_string(&mut self) -> Result<String, Diagnostic> {
        self.skip_trivia();
        let start = self.pos;
        match self.peek() {
            Some('"') => self.bump(),
            Some(c) => {
                return Err(Diagnostic::error(
                    Span::new(self.pos, self.pos + c.len_utf8()),
                    E_BUILD,
                    format!("expected a string literal, found `{}`", c),
                ));
            }
            None => {
                return Err(Diagnostic::error(
                    self.eof_span(),
                    E_BUILD,
                    "expected a string literal but reached end of input",
                ));
            }
        }

        let mut out = String::new();
        loop {
            match self.peek() {
                None => {
                    return Err(Diagnostic::error(
                        Span::new(start, self.pos),
                        E_BUILD,
                        "unterminated string literal",
                    ));
                }
                Some('"') => {
                    self.bump();
                    break;
                }
                Some('\\') => {
                    let esc_start = self.pos;
                    self.bump();
                    match self.peek() {
                        Some('n') => {
                            out.push('\n');
                            self.bump();
                        }
                        Some('t') => {
                            out.push('\t');
                            self.bump();
                        }
                        Some('\\') => {
                            out.push('\\');
                            self.bump();
                        }
                        Some('"') => {
                            out.push('"');
                            self.bump();
                        }
                        Some(c) => {
                            return Err(Diagnostic::error(
                                Span::new(esc_start, self.pos + c.len_utf8()),
                                E_BUILD,
                                format!("invalid escape `\\{}` in string literal", c),
                            ));
                        }
                        None => {
                            return Err(Diagnostic::error(
                                Span::new(esc_start, self.pos),
                                E_BUILD,
                                "unterminated escape in string literal",
                            ));
                        }
                    }
                }
                Some(c) => {
                    out.push(c);
                    self.bump();
                }
            }
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_canonical_form() {
        let src = "build {\n    name = \"hello\";\n    root = \"src/main.ks\";\n}\n";
        let spec = parse_build_kd(src).expect("should parse");
        assert_eq!(spec.name, "hello");
        assert_eq!(spec.root, "src/main.ks");
    }

    #[test]
    fn tolerates_whitespace_and_comments() {
        let src = "  // a project\n\n  build  {  // open\n\
                   \t root = \"src/app.ks\" ;  // the entrypoint\n\
                   name=\"app\";\n} // done\n\n";
        let spec = parse_build_kd(src).expect("should parse");
        assert_eq!(spec.name, "app");
        assert_eq!(spec.root, "src/app.ks");
    }

    #[test]
    fn fields_may_appear_in_any_order_with_extras() {
        let src = "build { root = \"r.ks\"; version = \"0.1.0\"; name = \"n\"; }";
        let spec = parse_build_kd(src).expect("should parse");
        assert_eq!(spec.name, "n");
        assert_eq!(spec.root, "r.ks");
    }

    #[test]
    fn decodes_string_escapes() {
        let src = "build { name = \"a\\tb\"; root = \"c\\\\d\"; }";
        let spec = parse_build_kd(src).expect("should parse");
        assert_eq!(spec.name, "a\tb");
        assert_eq!(spec.root, "c\\d");
    }

    #[test]
    fn missing_root_is_an_error() {
        let src = "build { name = \"hello\"; }";
        let errs = parse_build_kd(src).expect_err("should fail");
        assert!(errs.iter().all(|d| d.code == E_BUILD));
        assert!(errs.iter().any(|d| d.message.contains("root")));
    }

    #[test]
    fn missing_name_is_an_error() {
        let src = "build { root = \"src/main.ks\"; }";
        let errs = parse_build_kd(src).expect_err("should fail");
        assert!(errs.iter().any(|d| d.message.contains("name")));
    }

    #[test]
    fn unquoted_value_is_an_error() {
        let src = "build { name = hello; root = \"x\"; }";
        let errs = parse_build_kd(src).expect_err("should fail");
        assert_eq!(errs[0].code, E_BUILD);
    }

    #[test]
    fn missing_semicolon_is_an_error() {
        let src = "build { name = \"hello\" root = \"x\"; }";
        let errs = parse_build_kd(src).expect_err("should fail");
        assert_eq!(errs[0].code, E_BUILD);
    }

    #[test]
    fn wrong_header_keyword_is_an_error() {
        let src = "module { name = \"x\"; root = \"y\"; }";
        let errs = parse_build_kd(src).expect_err("should fail");
        assert_eq!(errs[0].code, E_BUILD);
        assert!(errs[0].message.contains("build"));
    }

    #[test]
    fn empty_input_is_an_error() {
        let errs = parse_build_kd("   \n  // nothing\n").expect_err("should fail");
        assert_eq!(errs[0].code, E_BUILD);
    }

    #[test]
    fn unterminated_block_is_an_error() {
        let errs = parse_build_kd("build { name = \"x\";").expect_err("should fail");
        assert_eq!(errs[0].code, E_BUILD);
    }
}
