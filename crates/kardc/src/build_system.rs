//! The in-language build system (SPEC §7).
//!
//! Zig-philosophy: the build is described in the project's own `build.ks`,
//! read by the same `kard` binary. As of v0.122 a `build.ks` describes a
//! **build graph** of one or more named executable targets. Two surface forms
//! parse to the same [`BuildSpec`]:
//!
//! ```text
//! // Single-target sugar (legacy): the top-level `name`/`root` *are* the one
//! // target.
//! build {
//!     name = "hello";
//!     root = "src/main.ks";
//! }
//!
//! // Multi-target: one `exe "NAME" { root = ".."; }` block per target. A
//! // top-level `name = ".."` may still appear (the project name) but is
//! // ignored for target selection once any `exe` block is present.
//! build {
//!     exe "app"  { root = "src/main.ks"; }
//!     exe "tool" { root = "src/tool.ks"; }
//! }
//! ```
//!
//! The full imperative build graph (a kardashev program with a
//! `build(*Builder)` entry point — steps, dependencies, install artifacts)
//! remains a later roadmap item.
//!
//! `parse_build_kd` is a tiny self-contained recursive parser — it does not
//! reuse the language lexer/parser, because `build.ks` is a fixed declarative
//! shape rather than a program. It is tolerant of surrounding whitespace and
//! `//` line comments (SPEC §1/§7). Any malformed or missing field, a missing
//! root (with no `exe` block), or a duplicate target name yields a single error
//! code, `E0300`.

use crate::diag::Diagnostic;
use crate::span::Span;

/// One build target: an executable with a name and a root source file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Target {
    /// The output executable name.
    pub name: String,
    /// The root source file, relative to the project root.
    pub root: String,
}

/// The resolved build specification — a build graph of one or more targets
/// (v0.122). Supports the legacy single-target sugar `build { name=..; root=..; }`
/// and the multi-target `build { exe "name" { root=..; } ... }` form.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BuildSpec {
    pub targets: Vec<Target>,
}

impl BuildSpec {
    /// The target named `name`, or — when `name` is `None` and there is exactly
    /// one target — that sole target.
    pub fn select(&self, name: Option<&str>) -> Option<&Target> {
        match name {
            Some(n) => self.targets.iter().find(|t| t.name == n),
            None if self.targets.len() == 1 => self.targets.first(),
            None => None,
        }
    }
}

/// The stable error code for every `build.ks` parse failure.
const E_BUILD: &str = "E0300";

/// Parse a `build.ks` into a [`BuildSpec`] (SPEC §7), accepting both the legacy
/// single-target sugar and the multi-target `exe` form.
///
/// Grammar (whitespace- and `//`-comment-tolerant everywhere between tokens):
///
/// ```text
/// build-file := "build" "{" item* "}"
/// item       := field | exe-block
/// field      := IDENT "=" STRING ";"
/// exe-block  := "exe" STRING "{" field* "}"
/// ```
///
/// * Each `exe "NAME" { root = "..."; }` block contributes one [`Target`]
///   (its quoted name + its `root`); unknown well-formed fields inside the
///   block are ignored, but `root` is required per `exe`.
/// * When **no** `exe` block is present, the form is the legacy single-target
///   sugar: the top-level `name` and `root` *are* the one target, and both are
///   required.
/// * A top-level `name = ".."` (the project name) may coexist with `exe`
///   blocks; once any `exe` block is present it is ignored for targets.
///
/// Any structural problem, a missing required field, a build with neither a
/// top-level `root` nor any `exe` block, or a duplicate target name produces an
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

    // Top-level `name`/`root` (the legacy single-target sugar) plus any `exe`
    // target blocks. If at least one `exe` block is present, the targets come
    // from those blocks and the top-level fields are project-level metadata.
    let mut proj_name: Option<String> = None;
    let mut top_root: Option<String> = None;
    let mut targets: Vec<Target> = Vec::new();

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
                    "unterminated `build` block: expected a field, an `exe` target, or `}`",
                )]);
            }
            _ => {}
        }

        // item := field | exe-block, both starting with an identifier.
        let (key, key_span) = match s.read_ident() {
            Some(kv) => kv,
            None => {
                let bad = s.bad_token_span();
                return Err(vec![Diagnostic::error(
                    bad,
                    E_BUILD,
                    "expected a field name, an `exe` target, or `}`",
                )]);
            }
        };

        if key == "exe" {
            // exe-block := "exe" STRING "{" field* "}"
            let exe_name = match s.read_string() {
                Ok(v) => v,
                Err(d) => return Err(vec![d]),
            };
            if let Err(d) = s.expect('{') {
                return Err(vec![d]);
            }

            let mut exe_root: Option<String> = None;
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
                            format!(
                                "unterminated `exe \"{}\"` block: expected a field or `}}`",
                                exe_name
                            ),
                        )]);
                    }
                    _ => {}
                }

                let (fkey, _fspan) = match s.read_ident() {
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
                match fkey.as_str() {
                    "root" => exe_root = Some(value),
                    // Forward-compatible: ignore unknown well-formed fields.
                    _ => {}
                }
            }

            let exe_root = match exe_root {
                Some(r) => r,
                None => {
                    return Err(vec![Diagnostic::error(
                        key_span,
                        E_BUILD,
                        format!(
                            "`exe \"{}\"` target is missing the required `root` field",
                            exe_name
                        ),
                    )]);
                }
            };

            if targets.iter().any(|t| t.name == exe_name) {
                return Err(vec![Diagnostic::error(
                    key_span,
                    E_BUILD,
                    format!("duplicate target name `{}`", exe_name),
                )]);
            }
            targets.push(Target {
                name: exe_name,
                root: exe_root,
            });
        } else {
            // field := IDENT "=" STRING ";"
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
                "name" => proj_name = Some(value),
                "root" => top_root = Some(value),
                // Forward-compatible: ignore unknown well-formed fields.
                _ => {}
            }
        }
    }

    // Multi-target form: the `exe` blocks are the targets (duplicates already
    // rejected above); the top-level `name`/`root` are project metadata.
    if !targets.is_empty() {
        return Ok(BuildSpec { targets });
    }

    // Legacy single-target sugar: the top-level `name` and `root` are the one
    // target, and both are mandatory.
    let mut missing = Vec::new();
    if proj_name.is_none() {
        missing.push(Diagnostic::error(
            header,
            E_BUILD,
            "`build` block is missing the required `name` field",
        ));
    }
    if top_root.is_none() {
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
        targets: vec![Target {
            name: proj_name.unwrap(),
            root: top_root.unwrap(),
        }],
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

    // ---- legacy single-target sugar ---------------------------------------

    #[test]
    fn legacy_form_parses_to_one_target() {
        let src = "build {\n    name = \"hello\";\n    root = \"src/main.ks\";\n}\n";
        let spec = parse_build_kd(src).expect("should parse");
        assert_eq!(spec.targets.len(), 1);
        assert_eq!(spec.targets[0].name, "hello");
        assert_eq!(spec.targets[0].root, "src/main.ks");
    }

    #[test]
    fn tolerates_whitespace_and_comments() {
        let src = "  // a project\n\n  build  {  // open\n\
                   \t root = \"src/app.ks\" ;  // the entrypoint\n\
                   name=\"app\";\n} // done\n\n";
        let spec = parse_build_kd(src).expect("should parse");
        assert_eq!(spec.targets, vec![Target {
            name: "app".to_string(),
            root: "src/app.ks".to_string(),
        }]);
    }

    #[test]
    fn fields_may_appear_in_any_order_with_extras() {
        let src = "build { root = \"r.ks\"; version = \"0.1.0\"; name = \"n\"; }";
        let spec = parse_build_kd(src).expect("should parse");
        assert_eq!(spec.targets.len(), 1);
        assert_eq!(spec.targets[0].name, "n");
        assert_eq!(spec.targets[0].root, "r.ks");
    }

    #[test]
    fn decodes_string_escapes() {
        let src = "build { name = \"a\\tb\"; root = \"c\\\\d\"; }";
        let spec = parse_build_kd(src).expect("should parse");
        assert_eq!(spec.targets[0].name, "a\tb");
        assert_eq!(spec.targets[0].root, "c\\d");
    }

    // ---- multi-target `exe` form ------------------------------------------

    #[test]
    fn multi_target_parses_to_n_targets_in_order() {
        let src = "build {\n\
                   \x20   exe \"app\"  { root = \"src/main.ks\"; }\n\
                   \x20   exe \"tool\" { root = \"src/tool.ks\"; }\n\
                   \x20   exe \"bench\" { root = \"src/bench.ks\"; }\n\
                   }\n";
        let spec = parse_build_kd(src).expect("should parse");
        assert_eq!(
            spec.targets,
            vec![
                Target { name: "app".to_string(), root: "src/main.ks".to_string() },
                Target { name: "tool".to_string(), root: "src/tool.ks".to_string() },
                Target { name: "bench".to_string(), root: "src/bench.ks".to_string() },
            ]
        );
    }

    #[test]
    fn top_level_name_is_ignored_when_exe_blocks_are_present() {
        let src = "build {\n\
                   \x20   name = \"the-project\";\n\
                   \x20   exe \"only\" { root = \"src/main.ks\"; }\n\
                   }\n";
        let spec = parse_build_kd(src).expect("should parse");
        assert_eq!(spec.targets, vec![Target {
            name: "only".to_string(),
            root: "src/main.ks".to_string(),
        }]);
    }

    #[test]
    fn exe_block_ignores_unknown_fields_but_requires_root() {
        let ok = parse_build_kd(
            "build { exe \"a\" { kind = \"exe\"; root = \"r.ks\"; } }",
        )
        .expect("should parse");
        assert_eq!(ok.targets[0].root, "r.ks");

        let errs = parse_build_kd("build { exe \"a\" { kind = \"exe\"; } }")
            .expect_err("missing root");
        assert!(errs.iter().all(|d| d.code == E_BUILD));
        assert!(errs.iter().any(|d| d.message.contains("root")));
    }

    // ---- selection --------------------------------------------------------

    #[test]
    fn select_by_name_finds_the_target() {
        let src = "build { exe \"app\" { root = \"a.ks\"; } exe \"tool\" { root = \"t.ks\"; } }";
        let spec = parse_build_kd(src).expect("should parse");
        assert_eq!(spec.select(Some("tool")).unwrap().root, "t.ks");
        assert_eq!(spec.select(Some("app")).unwrap().name, "app");
        assert!(spec.select(Some("nope")).is_none());
    }

    #[test]
    fn select_none_returns_sole_target_or_none_when_multiple() {
        // Sole target (legacy form): unnamed selection resolves to it.
        let single = parse_build_kd("build { name = \"x\"; root = \"r.ks\"; }").unwrap();
        assert_eq!(single.select(None).unwrap().name, "x");

        // Sole target (single `exe`): same.
        let single_exe = parse_build_kd("build { exe \"x\" { root = \"r.ks\"; } }").unwrap();
        assert_eq!(single_exe.select(None).unwrap().name, "x");

        // Multiple targets: unnamed selection is ambiguous → None.
        let multi =
            parse_build_kd("build { exe \"a\" { root = \"a.ks\"; } exe \"b\" { root = \"b.ks\"; } }")
                .unwrap();
        assert!(multi.select(None).is_none());
    }

    // ---- errors -----------------------------------------------------------

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
        assert!(errs.iter().all(|d| d.code == E_BUILD));
        assert!(errs.iter().any(|d| d.message.contains("name")));
    }

    #[test]
    fn neither_root_nor_exe_block_is_an_error() {
        let errs = parse_build_kd("build { version = \"0.1.0\"; }").expect_err("should fail");
        assert!(errs.iter().all(|d| d.code == E_BUILD));
        assert!(errs.iter().any(|d| d.message.contains("root")));
    }

    #[test]
    fn duplicate_target_names_are_an_error() {
        let src = "build { exe \"dup\" { root = \"a.ks\"; } exe \"dup\" { root = \"b.ks\"; } }";
        let errs = parse_build_kd(src).expect_err("should fail");
        assert_eq!(errs[0].code, E_BUILD);
        assert!(errs[0].message.contains("duplicate"));
        assert!(errs[0].message.contains("dup"));
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

    #[test]
    fn unterminated_exe_block_is_an_error() {
        let errs = parse_build_kd("build { exe \"a\" { root = \"x\";").expect_err("should fail");
        assert_eq!(errs[0].code, E_BUILD);
    }
}
