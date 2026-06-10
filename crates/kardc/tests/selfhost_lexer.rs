//! Self-host stage 1 (v0.159): differential test of `selfhost/lexer.ks` — the
//! kardashev lexer written in kardashev — against the Rust reference lexer.
//!
//! `selfhost/lexdump.ks` is compiled ONCE (full file-based pipeline + `-O0`
//! cc build) and then executed on every corpus file; its stdout — one
//! `<KINDNAME> <off> <len>` line per token, `EOF`-terminated — must be
//! byte-identical to [`rust_dump`], which renders `kardc::lexer::lex`'s
//! output in the same canonical format (the KINDNAME table is documented in
//! `selfhost/lexer.ks`).
//!
//! Error contract (both sides, byte-identical): for a lexically erroneous
//! input the WHOLE dump is exactly one line, `ERROR <code> <pos>` with code
//! 1 = E0001 / 2 = E0002 and pos = the first diagnostic's span start. The
//! Rust `lex` collects all errors but pushes them in strict scan order and
//! discards its token vector on `Err`, so the first diagnostic is the one
//! artifact both implementations can render identically — the `.ks` lexer
//! stops at its first error, which is the same error.
//!
//! Corpus: every `.ks` under `tests/spec` (INCLUDING `_`-prefixed import
//! fixtures — they are real sources), `tests/std`, `tests/selfhost`,
//! `examples`, `selfhost`, plus the bundled `crates/kardc/src/std.ks`.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

use kardc::backend::{BuildOptions, OptLevel};
use kardc::emit_c::EmitMode;
use kardc::token::{Kw, TokenKind};

static COUNTER: AtomicUsize = AtomicUsize::new(0);

/// A process-unique temp path (the e2e/std-suite helper).
fn temp_path(tag: &str) -> PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!("kardc_selfhost_{}_{}_{}", tag, std::process::id(), n))
}

/// The repository root (this file lives in `crates/kardc/tests/`).
fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root should canonicalize")
}

/// The canonical KINDNAME spelling of a keyword — `KW_<UPPERCASE>`; one arm
/// per `Kw` variant so a new keyword fails compilation here until mapped.
fn kw_name(kw: Kw) -> &'static str {
    match kw {
        Kw::Pub => "KW_PUB",
        Kw::Fn => "KW_FN",
        Kw::Const => "KW_CONST",
        Kw::Var => "KW_VAR",
        Kw::Return => "KW_RETURN",
        Kw::If => "KW_IF",
        Kw::Else => "KW_ELSE",
        Kw::While => "KW_WHILE",
        Kw::Break => "KW_BREAK",
        Kw::Continue => "KW_CONTINUE",
        Kw::Defer => "KW_DEFER",
        Kw::Comptime => "KW_COMPTIME",
        Kw::Test => "KW_TEST",
        Kw::True => "KW_TRUE",
        Kw::False => "KW_FALSE",
        Kw::And => "KW_AND",
        Kw::Or => "KW_OR",
        Kw::Struct => "KW_STRUCT",
        Kw::Orelse => "KW_ORELSE",
        Kw::Null => "KW_NULL",
        Kw::Try => "KW_TRY",
        Kw::Catch => "KW_CATCH",
        Kw::Error => "KW_ERROR",
        Kw::Enum => "KW_ENUM",
        Kw::Switch => "KW_SWITCH",
        Kw::Union => "KW_UNION",
        Kw::Errdefer => "KW_ERRDEFER",
        Kw::For => "KW_FOR",
        Kw::Unreachable => "KW_UNREACHABLE",
    }
}

/// The canonical KINDNAME spelling of a token kind (the table documented in
/// `selfhost/lexer.ks`). Exhaustive: a new `TokenKind` fails compilation here
/// until the `.ks` side and this table both learn it.
fn kind_name(kind: &TokenKind) -> &'static str {
    match kind {
        TokenKind::Ident(_) => "IDENT",
        TokenKind::Int(_) => "INT",
        TokenKind::Float(_) => "FLOAT",
        TokenKind::Str(_) => "STR",
        TokenKind::Keyword(kw) => kw_name(*kw),
        TokenKind::LParen => "LPAREN",
        TokenKind::RParen => "RPAREN",
        TokenKind::LBrace => "LBRACE",
        TokenKind::RBrace => "RBRACE",
        TokenKind::LBracket => "LBRACKET",
        TokenKind::RBracket => "RBRACKET",
        TokenKind::Comma => "COMMA",
        TokenKind::Semicolon => "SEMICOLON",
        TokenKind::Colon => "COLON",
        TokenKind::Dot => "DOT",
        TokenKind::Eq => "EQ",
        TokenKind::PlusEq => "PLUSEQ",
        TokenKind::MinusEq => "MINUSEQ",
        TokenKind::StarEq => "STAREQ",
        TokenKind::SlashEq => "SLASHEQ",
        TokenKind::PercentEq => "PERCENTEQ",
        TokenKind::EqEq => "EQEQ",
        TokenKind::BangEq => "BANGEQ",
        TokenKind::Lt => "LT",
        TokenKind::Le => "LE",
        TokenKind::Gt => "GT",
        TokenKind::Ge => "GE",
        TokenKind::Plus => "PLUS",
        TokenKind::Minus => "MINUS",
        TokenKind::Star => "STAR",
        TokenKind::Slash => "SLASH",
        TokenKind::Percent => "PERCENT",
        TokenKind::Bang => "BANG",
        TokenKind::Question => "QUESTION",
        TokenKind::FatArrow => "FATARROW",
        TokenKind::Amp => "AMP",
        TokenKind::DotDot => "DOTDOT",
        TokenKind::Pipe => "PIPE",
        TokenKind::At => "AT",
        TokenKind::Caret => "CARET",
        TokenKind::Tilde => "TILDE",
        TokenKind::Shl => "SHL",
        TokenKind::Shr => "SHR",
        TokenKind::Eof => "EOF",
    }
}

/// The reference dump: lex `src` with the Rust lexer and render the exact
/// byte format `selfhost/lexdump.ks` prints (see the module docs).
fn rust_dump(src: &str) -> String {
    match kardc::lexer::lex(src) {
        Ok(tokens) => {
            let mut out = String::new();
            for t in &tokens {
                out.push_str(kind_name(&t.kind));
                out.push(' ');
                out.push_str(&t.span.start.to_string());
                out.push(' ');
                out.push_str(&(t.span.end - t.span.start).to_string());
                out.push('\n');
            }
            out
        }
        Err(diags) => {
            let d = &diags[0];
            let code = match d.code {
                "E0001" => 1,
                "E0002" => 2,
                other => panic!("unexpected lexer diagnostic code {other}"),
            };
            format!("ERROR {} {}\n", code, d.span.start)
        }
    }
}

/// Compile `selfhost/lexdump.ks` (program mode, `-O0`) to a temp executable.
fn build_lexdump() -> PathBuf {
    let src = repo_root().join("selfhost/lexdump.ks");
    let c = kardc::compile_program(&src, EmitMode::Program).unwrap_or_else(|diags| {
        let text = std::fs::read_to_string(&src).unwrap_or_default();
        panic!(
            "selfhost/lexdump.ks failed to compile:\n{}",
            kardc::diag::render_all(&diags, &src.display().to_string(), &text)
        )
    });
    let exe = temp_path("lexdump");
    let opts = BuildOptions {
        opt: OptLevel::O0,
        ..BuildOptions::default()
    };
    kardc::backend::cc_build(&c, &exe, &opts).expect("cc should build lexdump");
    exe
}

/// Recursively collect every `.ks` file under `dir` (fixtures included).
fn collect_ks(dir: &Path, out: &mut Vec<PathBuf>) {
    let entries = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("cannot read corpus dir {}: {e}", dir.display()));
    for entry in entries {
        let path = entry.expect("dir entry").path();
        if path.is_dir() {
            collect_ks(&path, out);
        } else if path.extension().is_some_and(|x| x == "ks") {
            out.push(path);
        }
    }
}

/// Run the lexdump binary on `input` and diff its stdout against
/// [`rust_dump`]. `Ok(lines)` is the number of token lines compared.
fn diff_one(exe: &Path, input: &Path, src: &str) -> Result<usize, String> {
    let expected = rust_dump(src);
    let out = Command::new(exe)
        .arg(input)
        .output()
        .unwrap_or_else(|e| panic!("failed to run lexdump on {}: {e}", input.display()));
    if out.status.code() != Some(0) {
        return Err(format!(
            "{}: lexdump exited {:?}\n--- stderr ---\n{}",
            input.display(),
            out.status.code(),
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    let got = String::from_utf8_lossy(&out.stdout);
    if got != expected {
        // Name the first divergent line so a corpus-wide failure is readable.
        let g: Vec<&str> = got.lines().collect();
        let e: Vec<&str> = expected.lines().collect();
        let mut i = 0;
        while i < g.len() && i < e.len() && g[i] == e[i] {
            i += 1;
        }
        return Err(format!(
            "{}: dump mismatch at line {} — rust `{}` vs selfhost `{}` ({} vs {} lines)",
            input.display(),
            i + 1,
            e.get(i).unwrap_or(&"<eof>"),
            g.get(i).unwrap_or(&"<eof>"),
            e.len(),
            g.len()
        ));
    }
    Ok(expected.lines().count())
}

/// (b) The full-repository differential corpus: every real `.ks` source in
/// the repo, byte-for-byte. ~650 files; one shared `-O0` lexdump build, one
/// subprocess execution per file (each a few ms) keeps this inside a normal
/// `cargo test` budget, so the corpus is NOT capped.
#[test]
fn selfhost_lexer_differential_corpus() {
    let root = repo_root();
    let exe = build_lexdump();

    let mut corpus: Vec<PathBuf> = Vec::new();
    collect_ks(&root.join("tests/spec"), &mut corpus);
    collect_ks(&root.join("tests/std"), &mut corpus);
    collect_ks(&root.join("tests/selfhost"), &mut corpus);
    collect_ks(&root.join("examples"), &mut corpus);
    collect_ks(&root.join("selfhost"), &mut corpus);
    corpus.push(root.join("crates/kardc/src/std.ks"));
    corpus.sort();
    corpus.dedup();
    assert!(
        corpus.len() >= 300,
        "differential corpus shrank to {} files — expected the full tree (650+)",
        corpus.len()
    );

    let mut failures: Vec<String> = Vec::new();
    let mut lines_total = 0usize;
    for file in &corpus {
        let src = match std::fs::read_to_string(file) {
            Ok(s) => s,
            Err(e) => {
                failures.push(format!("{}: unreadable corpus file: {e}", file.display()));
                continue;
            }
        };
        match diff_one(&exe, file, &src) {
            Ok(lines) => lines_total += lines,
            Err(msg) => failures.push(msg),
        }
    }
    let _ = std::fs::remove_file(&exe);

    assert!(
        failures.is_empty(),
        "{} of {} corpus files mismatched the Rust lexer:\n{}",
        failures.len(),
        corpus.len(),
        failures.join("\n")
    );
    println!(
        "selfhost lexer differential: {} files, {} token lines byte-identical",
        corpus.len(),
        lines_total
    );
}

/// (c) Targeted error / edge inputs (written to temp files): both lexers must
/// agree on the single `ERROR <code> <pos>` line — and on the tricky clean
/// inputs' token streams.
#[test]
fn selfhost_lexer_differential_error_inputs() {
    let exe = build_lexdump();
    let cases: &[(&str, &str)] = &[
        ("unterminated_string", "\"unterminated"),
        ("bad_escape", "\"bad \\q escape\""),
        ("bad_escape_after_tokens", "fn x # \"a\\q\""),
        ("trailing_backslash", "\"trail\\"),
        ("overflow_literal", "9223372036854775808"),
        ("overflow_huge", "x 99999999999999999999999"),
        ("max_i64_ok", "9223372036854775807"),
        ("leading_zeros_ok", "000009223372036854775807"),
        ("stray_hash", "# x $"),
        ("stray_dollar", "x $"),
        ("stray_backslash", "\\"),
        ("multibyte_stray", "é"),
        ("multibyte_in_string_ok", "\"héllo wörld\""),
        ("empty", ""),
        ("only_comment", "// nothing else\n"),
        ("munch_mix", "1..2 1.5 a--b <=< >>= === !=! x_1 _x fnx fn"),
        ("whitespace_mix", "\tx\r\n  y"),
    ];
    let mut failures: Vec<String> = Vec::new();
    for (tag, src) in cases {
        let input = temp_path(&format!("err_{tag}"));
        std::fs::write(&input, src).expect("write temp error input");
        if let Err(msg) = diff_one(&exe, &input, src) {
            failures.push(format!("[{tag}] {msg}"));
        }
        let _ = std::fs::remove_file(&input);
    }
    let _ = std::fs::remove_file(&exe);
    assert!(
        failures.is_empty(),
        "{} targeted inputs mismatched:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// (4) The in-language suite: `tests/selfhost/lexer_suite.ks` must compile in
/// test mode and report every test passing (exit code 0 = failure count).
#[test]
fn selfhost_lexer_suite_passes() {
    let suite = repo_root().join("tests/selfhost/lexer_suite.ks");
    let c = kardc::compile_program(&suite, EmitMode::Test).unwrap_or_else(|diags| {
        let text = std::fs::read_to_string(&suite).unwrap_or_default();
        panic!(
            "lexer_suite.ks failed to compile:\n{}",
            kardc::diag::render_all(&diags, &suite.display().to_string(), &text)
        )
    });
    let exe = temp_path("suite");
    let opts = BuildOptions {
        opt: OptLevel::O0,
        ..BuildOptions::default()
    };
    kardc::backend::cc_build(&c, &exe, &opts).expect("cc should build the suite harness");
    let output = Command::new(&exe).output().expect("should run the harness");
    let _ = std::fs::remove_file(&exe);
    assert_eq!(
        output.status.code(),
        Some(0),
        "lexer_suite.ks had failing tests:\n--- stderr ---\n{}\n--- stdout ---\n{}",
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout)
    );
}
