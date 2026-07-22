//! Self-host stage 27 (v0.186): differential test of `selfhost/sema.ks` —
//! the sema mirror OPENS, over the SCALAR CORE of SINGLE-FILE modules (the
//! v0.111 procedural language at every integer width; see the subset table
//! in `selfhost/sema.ks`).
//!
//! `selfhost/semadump.ks` is compiled ONCE (full pipeline + `-O0` cc) and
//! executed on every corpus file; its single-line stdout must be
//! byte-identical to [`rust_expected`], which classifies the same file as:
//!
//! - **`ERROR <code> <pos>`** — fails to lex/parse/resolve (the cdump code
//!   mapping: 1/2 = E0001/E0002, 200/201 = E0200/E0201, 291–294
//!   structural; concatenated coordinates).
//! - **`SKIP <word> <pos>`** — parses but is outside the stage-27 SEMA
//!   subset, `<word>`/`<pos>` naming the FIRST out-of-subset construct in
//!   a fixed depth-first walk ([`ss_detect`], mirrored word-for-word by
//!   `selfhost/sema.ks`). `import` covers EVERY multi-file/std module —
//!   stage 27 is single-file (sema over a flattened module is a later
//!   stage) — so the flatten mirror stays modres.ks's proven v0.167
//!   territory.
//! - **`OK`** — in the subset and the REAL `sema::check` accepts it.
//! - **`DIAG <code> <pos>`** — in the subset and the REAL `sema::check`
//!   rejects it; `<code>`/`<pos>` are the FIRST diagnostic's numeric code
//!   and byte position. This is the stage's teeth: the reference is the
//!   production sema itself, not a hand mirror — so the kardashev checker
//!   must reproduce sema's pass order (builtin redefinition → const
//!   folding → bodies), its span choices (operand errors at the operand,
//!   mismatches at the operator/statement/value), its integer-literal
//!   polymorphism (including `check_int_operands`' anchoring ORDER — a
//!   flexible lhs anchors on the concrete rhs, which is then checked
//!   first), and the const-eval mirror's exact codes (E0130/E0131/E0132).
//!
//! Corpus: the same tree as the other differentials — every `.ks` under
//! `tests/spec`, `tests/std`, `tests/selfhost`, `examples`, `selfhost`,
//! plus the bundled `crates/kardc/src/std.ks`.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

use kardc::ast::{Expr, Func, Item, Module, Stmt, TypeExpr};
use kardc::backend::{BuildOptions, OptLevel};
use kardc::emit_c::EmitMode;

/// Floors on the compared-verdict counts: catch a detector regression that
/// silently reclassifies what used to be OK/DIAG-compared.
const MIN_OK_COMPARED: usize = 73;
const MIN_DIAG_COMPARED: usize = 30;

static COUNTER: AtomicUsize = AtomicUsize::new(0);

fn temp_path(tag: &str) -> PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!("kardc_selfsema_{}_{}_{}", tag, std::process::id(), n))
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root should canonicalize")
}

// ---- the import-resolution mirror (v0.167/v0.182 — the cdump copy) -----------

fn normalize_path(p: &str) -> String {
    let absolute = p.starts_with('/');
    let mut segs: Vec<&str> = Vec::new();
    for seg in p.split('/') {
        if seg.is_empty() || seg == "." {
            continue;
        }
        if seg == ".." && segs.last().is_some_and(|s| *s != "..") {
            segs.pop();
            continue;
        }
        segs.push(seg);
    }
    let joined = segs.join("/");
    if absolute {
        format!("/{joined}")
    } else {
        joined
    }
}

fn dir_of(p: &str) -> String {
    match p.rfind('/') {
        Some(i) => p[..=i].to_string(),
        None => String::new(),
    }
}

fn basename(p: &str) -> &str {
    match p.rfind('/') {
        Some(i) => &p[i + 1..],
        None => p,
    }
}

struct FlatFile {
    module: Module,
    base: usize,
}

enum Resolved {
    Line(String),
    Flat(Vec<FlatFile>),
}

struct Resolver {
    src_len: usize,
    states: std::collections::HashMap<String, bool>,
    out: Vec<FlatFile>,
    fail: Option<String>,
}

impl Resolver {
    fn resolve_file(&mut self, norm: &str, import_pos: usize, is_root: bool) {
        if self.fail.is_some() {
            return;
        }
        let mut content = std::fs::read_to_string(norm).unwrap_or_default();
        let base_name = basename(norm);
        let mut norm = norm;
        if (base_name == "std" || base_name == "std.ks") && content.is_empty() {
            if self.states.contains_key("<std>") {
                return;
            }
            norm = "<std>";
            content = include_str!("../src/std.ks").to_string();
        } else {
            if let Some(&on_stack) = self.states.get(norm) {
                if on_stack {
                    self.fail = Some(format!("ERROR 292 {}\n", import_pos));
                }
                return;
            }
            if content.is_empty() && !is_root {
                self.fail = Some(format!("ERROR 291 {}\n", import_pos));
                return;
            }
        }
        self.states.insert(norm.to_string(), true);

        let base = self.src_len;
        self.src_len += content.len();
        let module = match kardc::lexer::lex(&content) {
            Ok(tokens) => match kardc::parser::parse(&tokens) {
                Ok(m) => m,
                Err(diags) => {
                    let d = &diags[0];
                    self.fail = Some(if is_root {
                        let code = match d.code {
                            "E0200" => 200,
                            "E0201" => 201,
                            other => panic!("unexpected parser diagnostic code {other}"),
                        };
                        format!("ERROR {} {}\n", code, base + d.span.start)
                    } else {
                        "ERROR 294 0\n".to_string()
                    });
                    self.states.insert(norm.to_string(), false);
                    return;
                }
            },
            Err(diags) => {
                let d = &diags[0];
                self.fail = Some(if is_root {
                    let code = match d.code {
                        "E0001" => 1,
                        "E0002" => 2,
                        other => panic!("unexpected lexer diagnostic code {other}"),
                    };
                    format!("ERROR {} {}\n", code, base + d.span.start)
                } else {
                    "ERROR 294 0\n".to_string()
                });
                self.states.insert(norm.to_string(), false);
                return;
            }
        };

        let dir = dir_of(norm);
        for item in &module.items {
            if self.fail.is_some() {
                break;
            }
            if let Item::Import(imp) = item {
                let target = normalize_path(&format!("{}{}", dir, imp.path));
                self.resolve_file(&target, base + imp.span.start, false);
            }
        }
        if self.fail.is_none() {
            self.out.push(FlatFile { module, base });
        }
        self.states.insert(norm.to_string(), false);
    }
}

fn mirror_resolve(root: &Path) -> Resolved {
    let mut r = Resolver {
        src_len: 0,
        states: std::collections::HashMap::new(),
        out: Vec::new(),
        fail: None,
    };
    let norm = normalize_path(&root.display().to_string());
    r.resolve_file(&norm, 0, true);
    if let Some(line) = r.fail {
        return Resolved::Line(line);
    }
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    for ff in &r.out {
        for item in &ff.module.items {
            let named: Option<(&str, usize)> = match item {
                Item::Func(f) => Some((&f.name, f.span.start)),
                Item::Const(c) => Some((&c.name, c.span.start)),
                Item::Struct(s) => Some((&s.name, s.span.start)),
                Item::Enum(e) => Some((&e.name, e.span.start)),
                Item::Union(u) => Some((&u.name, u.span.start)),
                Item::ErrorSet(e) => Some((&e.name, e.span.start)),
                Item::Test(_) | Item::Import(_) => None,
            };
            if let Some((name, pos)) = named {
                if !seen.insert(name.to_string()) {
                    return Resolved::Line(format!("ERROR 293 {}\n", ff.base + pos));
                }
            }
        }
    }
    Resolved::Flat(r.out)
}

// ---- the stage-27 subset detector (the `ss_detect` twin) ---------------------

type Hit = (&'static str, usize);

/// The stage-27 scalar set — `sy_from_name` (NO f64; floats join a later
/// sema stage).
fn sema_scalar(name: &str) -> bool {
    matches!(
        name,
        "i32" | "i64" | "bool" | "void" | "u8" | "usize" | "i8" | "i16" | "u16" | "u32" | "u64"
    )
}

/// A type reference: bare scalar spellings only. Any composite FORM is
/// `type-form`; any other spelling — `f64`, `type`, a named type, `Self`
/// (written or the desugared `@This()`) — is `type-name`.
fn sd_type(t: &TypeExpr) -> Option<Hit> {
    if t.optional
        || t.error_union
        || t.pointer
        || t.slice
        || t.array_len.is_some()
        || t.error_set.is_some()
        || t.ctor_args.is_some()
    {
        return Some(("type-form", t.span.start));
    }
    if !sema_scalar(&t.name) {
        return Some(("type-name", t.span.start));
    }
    None
}

fn sd_expr(e: &Expr) -> Option<Hit> {
    match e {
        Expr::Int { .. } | Expr::Bool { .. } | Expr::Ident { .. } => None,
        Expr::Unary { expr, .. } | Expr::Comptime { expr, .. } => sd_expr(expr),
        Expr::Binary { lhs, rhs, .. } => sd_expr(lhs).or_else(|| sd_expr(rhs)),
        Expr::Call { callee, args, span } => {
            // The allocator builtins pull `Allocator`/slice types into
            // sema — out of the scalar stage.
            if matches!(callee.as_str(), "c_allocator" | "alloc" | "free") {
                return Some(("call", span.start));
            }
            args.iter().find_map(sd_expr)
        }
        other => Some(("expr", other.span().start)),
    }
}

fn sd_block(b: &kardc::ast::Block) -> Option<Hit> {
    b.stmts.iter().find_map(sd_stmt)
}

fn sd_stmt(s: &Stmt) -> Option<Hit> {
    match s {
        Stmt::Let { ty, value, .. } => ty
            .as_ref()
            .and_then(sd_type)
            .or_else(|| sd_expr(value)),
        Stmt::Assign { value, .. } => sd_expr(value),
        Stmt::Return { value, .. } => value.as_ref().and_then(sd_expr),
        Stmt::If {
            cond,
            capture,
            then,
            els,
            span,
        } => {
            if capture.is_some() {
                return Some(("capture", span.start));
            }
            sd_expr(cond)
                .or_else(|| sd_block(then))
                .or_else(|| els.as_deref().and_then(sd_stmt))
        }
        Stmt::While {
            cond,
            cont,
            body,
            label,
            span,
        } => {
            if label.is_some() {
                return Some(("label", span.start));
            }
            sd_expr(cond)
                .or_else(|| cont.as_deref().and_then(sd_stmt))
                .or_else(|| sd_block(body))
        }
        Stmt::Break { target, span } | Stmt::Continue { target, span } => {
            if target.is_some() {
                return Some(("label", span.start));
            }
            None
        }
        Stmt::Defer { stmt, .. } => sd_stmt(stmt),
        Stmt::Block(b) => sd_block(b),
        Stmt::FieldAssign { .. } | Stmt::For { .. } | Stmt::Switch { .. } | Stmt::ErrDefer { .. } => {
            Some(("stmt", s.span().start))
        }
        Stmt::Expr(e) => sd_expr(e),
    }
}

/// The flat walk: stage 27 is SINGLE-FILE, so an IMPORT PRE-PASS runs
/// first — the first `@import` item across the flattened files (append
/// order: sub-files precede their importer; the selfhost flattener erases
/// these items but records exactly this position, `MrOut.first_import`) —
/// then the subset walk proper, files in append order, items in source
/// order, the FIRST out-of-subset construct winning.
fn ss_detect(files: &[FlatFile]) -> Option<(String, usize)> {
    for ff in files {
        for item in &ff.module.items {
            if let Item::Import(imp) = item {
                return Some(("import".to_string(), ff.base + imp.span.start));
            }
        }
    }
    for ff in files {
        for item in &ff.module.items {
            let hit: Option<Hit> = match item {
                Item::Import(_) => None, // handled by the pre-pass
                Item::Struct(s) => Some(("item", s.span.start)),
                Item::Enum(e) => Some(("item", e.span.start)),
                Item::Union(u) => Some(("item", u.span.start)),
                Item::ErrorSet(es) => Some(("item", es.span.start)),
                Item::Func(f) => fn_hit(f),
                Item::Const(c) => c
                    .ty
                    .as_ref()
                    .and_then(sd_type)
                    .or_else(|| sd_expr(&c.value)),
                Item::Test(t) => sd_block(&t.body),
            };
            if let Some((word, pos)) = hit {
                return Some((word.to_string(), ff.base + pos));
            }
        }
    }
    None
}

fn fn_hit(f: &Func) -> Option<Hit> {
    for p in &f.params {
        if p.is_comptime {
            return Some(("generic-param", p.span.start));
        }
        if let Some(h) = sd_type(&p.ty) {
            return Some(h);
        }
    }
    sd_type(&f.ret).or_else(|| sd_block(&f.body))
}

// ---- the reference classifier -------------------------------------------------

/// Classify `path`: resolve (ERROR lines / std handling), detect
/// (SKIP lines), then run the REAL `sema::check` on the single-file module
/// for the OK / DIAG verdict.
fn rust_expected(path: &Path) -> String {
    let files = match mirror_resolve(path) {
        Resolved::Line(line) => return line,
        Resolved::Flat(files) => files,
    };
    if let Some((word, pos)) = ss_detect(&files) {
        return format!("SKIP {} {}\n", word, pos);
    }
    // No imports anywhere ⇒ exactly the root file, at base 0 — so the real
    // sema's spans ARE the driver's concatenated coordinates.
    assert_eq!(
        files.len(),
        1,
        "an import-free resolve must yield exactly the root ({})",
        path.display()
    );
    assert_eq!(files[0].base, 0);
    match kardc::sema::check(&files[0].module) {
        Ok(_) => "OK\n".to_string(),
        Err(diags) => {
            let d = &diags[0];
            let num: i64 = d.code.trim_start_matches('E').parse().unwrap_or_else(|_| {
                panic!("undecodable sema diagnostic code {}", d.code)
            });
            format!("DIAG {} {}\n", num, d.span.start)
        }
    }
}

// ---- harness -------------------------------------------------------------------

fn shared_semadump() -> &'static Path {
    static DUMP: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
    DUMP.get_or_init(build_semadump)
}

fn build_semadump() -> PathBuf {
    let src = repo_root().join("selfhost/semadump.ks");
    let c = kardc::compile_program(&src, EmitMode::Program).unwrap_or_else(|diags| {
        let text = std::fs::read_to_string(&src).unwrap_or_default();
        panic!(
            "selfhost/semadump.ks failed to compile:\n{}",
            kardc::diag::render_all(&diags, &src.display().to_string(), &text)
        )
    });
    let exe = temp_path("semadump");
    let opts = BuildOptions {
        opt: OptLevel::O0,
        ..BuildOptions::default()
    };
    kardc::backend::cc_build(&c, &exe, &opts).expect("cc should build semadump");
    exe
}

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

/// Run semadump on `input` (argv: file, the bundled std's source path —
/// the cdump convention) and return its stdout; the exit code is always 0.
fn run_driver(exe: &Path, input: &Path) -> Result<String, String> {
    let out = Command::new(exe)
        .arg(input)
        .arg(repo_root().join("crates/kardc/src/std.ks"))
        .output()
        .unwrap_or_else(|e| panic!("failed to run semadump on {}: {e}", input.display()));
    if out.status.code() != Some(0) {
        return Err(format!(
            "{}: semadump exited {:?}\n--- stderr ---\n{}",
            input.display(),
            out.status.code(),
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// (a) The full-repository differential corpus.
#[test]
fn selfhost_sema_differential_corpus() {
    let root = repo_root();
    let exe = shared_semadump();

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
        "differential corpus shrank to {} files — expected the full tree (700+)",
        corpus.len()
    );

    struct Tally {
        n_ok: usize,
        n_diag: usize,
        n_skip: usize,
        n_error: usize,
        failures: Vec<String>,
    }
    let tally = std::sync::Mutex::new(Tally {
        n_ok: 0,
        n_diag: 0,
        n_skip: 0,
        n_error: 0,
        failures: Vec::new(),
    });
    let next = AtomicUsize::new(0);
    let workers = std::thread::available_parallelism()
        .map(|n| n.get().min(8))
        .unwrap_or(4);
    std::thread::scope(|sc| {
        for _ in 0..workers {
            sc.spawn(|| loop {
                let i = next.fetch_add(1, Ordering::Relaxed);
                let Some(file) = corpus.get(i) else { break };
                let want = rust_expected(file);
                let got = match run_driver(exe, file) {
                    Ok(s) => s,
                    Err(msg) => {
                        tally.lock().unwrap().failures.push(msg);
                        continue;
                    }
                };
                let mut t = tally.lock().unwrap();
                if want.starts_with("OK") {
                    t.n_ok += 1;
                } else if want.starts_with("DIAG ") {
                    t.n_diag += 1;
                } else if want.starts_with("SKIP ") {
                    t.n_skip += 1;
                } else {
                    t.n_error += 1;
                }
                if got != want {
                    t.failures.push(format!(
                        "{}: sema verdict mismatch — rust `{}` vs selfhost `{}`",
                        file.display(),
                        want.trim_end(),
                        got.trim_end()
                    ));
                }
            });
        }
    });
    let Tally {
        n_ok,
        n_diag,
        n_skip,
        n_error,
        failures,
    } = tally.into_inner().unwrap();

    assert!(
        n_ok >= MIN_OK_COMPARED,
        "only {} corpus files were OK-compared (floor {MIN_OK_COMPARED}) — did the subset detector regress?",
        n_ok
    );
    assert!(
        n_diag >= MIN_DIAG_COMPARED,
        "only {} corpus files were DIAG-compared (floor {MIN_DIAG_COMPARED}) — did the subset detector regress?",
        n_diag
    );
    println!(
        "selfhost sema differential: {} files — {} OK-agreed, {} DIAG-agreed, {} SKIP-agreed, {} ERROR-agreed",
        corpus.len(),
        n_ok,
        n_diag,
        n_skip,
        n_error
    );
    assert!(
        failures.is_empty(),
        "{} corpus verdicts mismatched the Rust sema:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// (b) Targeted inputs: sema-specific edges the corpus under-exercises —
/// pass ordering, span choices, the literal-polymorphism anchoring order,
/// const-eval corners — plus SKIP-verdict positions. Every case must
/// produce the identical single-line verdict.
#[test]
fn selfhost_sema_differential_targeted_inputs() {
    let exe = shared_semadump();
    let cases: &[(&str, &str)] = &[
        // -- OK programs --------------------------------------------------------
        (
            "ok_scalar_core",
            "const N: i64 = 40 + 2;\nconst FLAG = N > 10;\nfn add(a: i64, b: i64) i64 { return a + b; }\nfn shout(n: u16) void {\n    var i: u16 = 0;\n    while (i < n) : (i += 1) {\n        if (i % 2 == 0) { print(i); } else { continue; }\n    }\n}\npub fn main() void {\n    defer print(999);\n    var x: i64 = add(N, 8);\n    {\n        const x2: i64 = x * 2;\n        print(x2);\n    }\n    shout(3);\n    if (FLAG and !(x == 0)) { print(1); }\n}\n",
        ),
        (
            "ok_shadowing_and_test_blocks",
            "const K: i64 = 7;\nfn f() i64 { return K; }\npub fn main() void {\n    var K: i64 = 9;\n    K = K + 1;\n    print(K);\n    print(f());\n}\ntest \"expect gates here\" {\n    expect(f() == 7);\n    var t: bool = true;\n    expect(t);\n}\n",
        ),
        (
            "ok_flex_literal_widths",
            "pub fn main() void {\n    var a: u8 = 200;\n    var b: u8 = a + 55;\n    var c: i16 = 0 - 300;\n    var d: u64 = 1 << 40;\n    print(b);\n    print(c);\n    print(d);\n    var e = 5;\n    print(e);\n}\n",
        ),
        (
            "ok_comptime_positions",
            "const BASE: i64 = 100;\npub fn main() void {\n    var x: i64 = comptime (BASE + 1);\n    if (comptime (BASE > 10)) { print(x); }\n    print(comptime (1 << 6));\n}\n",
        ),
        // -- pass-order + const diagnostics ------------------------------------
        (
            "diag_redefine_builtin_second_fn",
            "fn ok() void {}\nfn free(x: i64) i64 { return x; }\npub fn main() void { ok(); }\n",
        ),
        (
            "diag_const_call_declared_fn_e0311",
            "fn f() i64 { return 1; }\nconst X = f();\npub fn main() void { print(X); }\n",
        ),
        (
            "diag_const_call_unknown_e0130",
            "const X = g();\npub fn main() void { print(X); }\n",
        ),
        (
            "diag_const_forward_ref_e0131",
            "const A: i64 = B + 1;\nconst B: i64 = 2;\npub fn main() void { print(A); }\n",
        ),
        (
            "diag_const_type_error_e0132",
            "const X = 1 + true;\npub fn main() void { print(X); }\n",
        ),
        (
            "diag_const_div_zero_e0132",
            "const X = 7 / (3 - 3);\npub fn main() void { print(X); }\n",
        ),
        (
            "diag_const_annotation_mismatch_e0110",
            "const B: bool = 3;\npub fn main() void { print(0); }\n",
        ),
        // -- body diagnostics: spans + ordering ---------------------------------
        (
            "diag_unknown_name_in_call",
            "pub fn main() void { print(zzz); }\n",
        ),
        (
            "diag_assign_unknown_e0100",
            "pub fn main() void { ghost = 4; }\n",
        ),
        (
            "diag_assign_to_const_e0110",
            "pub fn main() void {\n    const c: i64 = 1;\n    c = 2;\n}\n",
        ),
        (
            "diag_operand_anchor_order_rhs_first",
            "pub fn main() void {\n    if (5 < missing) { print(1); }\n}\n",
        ),
        (
            "diag_and_reports_lhs_first",
            "pub fn main() void {\n    if (1 and true) { print(1); }\n}\n",
        ),
        (
            "diag_mismatch_at_operator_node",
            "pub fn main() void {\n    var a: u8 = 1;\n    var b: i64 = 2;\n    print(a + b);\n}\n",
        ),
        (
            "diag_return_value_from_void",
            "fn f() void { return 3; }\npub fn main() void { f(); }\n",
        ),
        (
            "diag_bare_return_from_valued",
            "fn f() i64 { return; }\npub fn main() void { print(f()); }\n",
        ),
        (
            "diag_break_outside_loop_e0120",
            "pub fn main() void { break; }\n",
        ),
        (
            "diag_expect_outside_test_e0140",
            "pub fn main() void { expect(true); }\n",
        ),
        (
            "diag_arity_at_call_site",
            "fn f(a: i64) i64 { return a; }\npub fn main() void { print(f(1, 2)); }\n",
        ),
        (
            "diag_arg_type_at_arg_site",
            "fn f(a: bool) void {}\npub fn main() void { f(5); }\n",
        ),
        (
            "diag_compound_mismatch_at_stmt",
            "pub fn main() void {\n    var x: u8 = 1;\n    var y: i64 = 2;\n    x += y;\n}\n",
        ),
        (
            "diag_condition_not_bool",
            "pub fn main() void {\n    while (1) { print(0); }\n}\n",
        ),
        (
            "diag_block_scope_death",
            "pub fn main() void {\n    {\n        var t: i64 = 5;\n        print(t);\n    }\n    print(t);\n}\n",
        ),
        // -- SKIP verdict positions ---------------------------------------------
        (
            "skip_float_literal_expr",
            "pub fn main() void {\n    var x: f64 = 1.5;\n    print(x);\n}\n",
        ),
        (
            "skip_string_expr",
            "pub fn main() void { print(\"hi\"); }\n",
        ),
        (
            "skip_struct_item",
            "const P = struct { x: i64 };\npub fn main() void { print(0); }\n",
        ),
        (
            "skip_labeled_loop",
            "pub fn main() void {\n    outer: while (true) { break :outer; }\n}\n",
        ),
        (
            "skip_for_stmt",
            "pub fn main() void {\n    var xs: [2]i64 = [2]i64{ 1, 2 };\n    for (xs) |x| { print(x); }\n}\n",
        ),
        (
            "skip_alloc_call",
            "pub fn main() void {\n    var a: Allocator = c_allocator();\n}\n",
        ),
        (
            "skip_std_import",
            "@import(\"std\");\npub fn main() void { print(imin(1, 2)); }\n",
        ),
        // -- an ERROR line (parse) ----------------------------------------------
        (
            "error_parse_missing_semi",
            "pub fn main() void { print(1) }\n",
        ),
    ];

    let failures: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());
    let next = AtomicUsize::new(0);
    let workers = std::thread::available_parallelism()
        .map(|n| n.get().min(8))
        .unwrap_or(4);
    std::thread::scope(|sc| {
        for _ in 0..workers {
            sc.spawn(|| loop {
                let i = next.fetch_add(1, Ordering::Relaxed);
                let Some((tag, src)) = cases.get(i) else { break };
                let input = temp_path(&format!("case_{tag}"));
                std::fs::write(&input, src).expect("write temp sema input");
                let want = rust_expected(&input);
                match run_driver(exe, &input) {
                    Ok(got) => {
                        if got != want {
                            failures.lock().unwrap().push(format!(
                                "[{tag}] verdict mismatch — rust `{}` vs selfhost `{}`",
                                want.trim_end(),
                                got.trim_end()
                            ));
                        }
                    }
                    Err(msg) => failures.lock().unwrap().push(format!("[{tag}] {msg}")),
                }
                let _ = std::fs::remove_file(&input);
            });
        }
    });
    let failures = failures.into_inner().unwrap();
    assert!(
        failures.is_empty(),
        "{} targeted inputs mismatched:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// (c) The in-language suite: `tests/selfhost/sema_suite.ks` must compile
/// in test mode and report every test passing.
#[test]
fn selfhost_sema_suite_passes() {
    let suite = repo_root().join("tests/selfhost/sema_suite.ks");
    let c = kardc::compile_program(&suite, EmitMode::Test).unwrap_or_else(|diags| {
        let text = std::fs::read_to_string(&suite).unwrap_or_default();
        panic!(
            "sema_suite.ks failed to compile:\n{}",
            kardc::diag::render_all(&diags, &suite.display().to_string(), &text)
        )
    });
    let exe = temp_path("ssuite");
    let opts = BuildOptions {
        opt: OptLevel::O0,
        ..BuildOptions::default()
    };
    kardc::backend::cc_build(&c, &exe, &opts).expect("cc should build the suite harness");
    let output = Command::new(&exe).output().expect("should run the harness");
    assert_eq!(
        output.status.code(),
        Some(0),
        "sema_suite.ks had failing tests:\n--- stderr ---\n{}\n--- stdout ---\n{}",
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout)
    );
}
