//! Self-host stages 3+4 (v0.161–v0.162): differential test of
//! `selfhost/emit.ks` — a C emitter for the SCALAR + STRING SUBSET written
//! in kardashev — against the Rust reference emitter.
//!
//! `selfhost/cdump.ks` is compiled ONCE (full file-based pipeline + `-O0`
//! cc build) and then executed on every corpus file; its stdout must be
//! byte-identical to [`rust_expected`], which classifies the same file with
//! the Rust pipeline. Every file falls in exactly one bucket:
//!
//! - **`ERROR <code> <pos>`** — the input fails to lex or parse. Same line
//!   and same code mapping as the v0.159/v0.160 differentials (1/2 =
//!   E0001/E0002, 200/201 = E0200/E0201, pos = the first diagnostic's span
//!   start).
//! - **`SKIP <word> <pos>`** — the module parses but uses a construct
//!   outside the subset. `<word>`/`<pos>` name the FIRST unsupported
//!   construct in a fixed depth-first walk ([`detect_subset`], mirrored
//!   word-for-word by `es_detect` in `selfhost/emit.ks`): items in source
//!   order; per function, parameters (comptime flag, then type), return
//!   type, body; per statement/expression, children in field order. A
//!   module with no top-level `fn main` is `nomain 0` (checked first —
//!   Program-mode emission is meaningless without a root). So subset
//!   membership itself is differentially tested on all ~700 files.
//! - **the full C text** — the module is in the subset: byte-for-byte the
//!   Rust `emit_c::emit(.., EmitMode::Program)` output.
//!
//! The subset: `i32`/`i64`/`bool`/`void`/`u8`/`usize` bare types plus the
//! one composite `[]u8` (v0.162); top-level `fn`/`const`; `var`/`const`
//! lets, (compound) name-assignment, `if`/`else`, `while` with
//! continue-clause, unlabeled `break`/`continue`, `defer`, `return`, bare
//! blocks, expression statements; int/bool/STRING literals, names, unary
//! `-`/`!`/`~`, the full binary ladder, free calls, `print` (integers and
//! `[]u8` strings), `expect`, `comptime`, `.len` on a slice, and the read
//! index `s[i]` (index writes are place-assignments and stay out).
//!
//! # The sema-invalid remainder
//!
//! `emit_c` documents its input as a *validated* module, and the selfhost
//! emitter has no sema (that is a later stage). A corpus file that is
//! subset-shaped but rejected by `sema::check` (deliberate `*_err.ks`
//! fixtures) therefore has NO reference C to compare against: for exactly
//! the files in [`SEMA_INVALID`] the driver's output is unspecified — but it
//! must still exit 0 (emission is total). The list is pinned by exact path
//! and asserted EQUAL to the observed set, so a new subset-shaped sema
//! fixture (or a subset change) fails loudly instead of silently shrinking
//! the compared corpus.
//!
//! Corpus: the v0.159/v0.160 corpus — every `.ks` under `tests/spec`,
//! `tests/std`, `tests/selfhost`, `examples`, `selfhost`, plus the bundled
//! `crates/kardc/src/std.ks`.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

use kardc::ast::{Expr, Func, Item, Module, Stmt, TypeExpr};
use kardc::backend::{BuildOptions, OptLevel};
use kardc::emit_c::EmitMode;

/// Subset-shaped corpus files that `sema::check` rejects (with the code the
/// pin was made under). The driver still runs on them (exit 0, output
/// uncompared); the corpus test asserts this list matches the observed set
/// exactly.
const SEMA_INVALID: &[&str] = &[
    "tests/spec/s02_syntax/chained_relational_type_err.ks",           // E0110
    "tests/spec/s02_syntax/prec_equality_binds_tighter_than_bitand_err.ks", // E0110
    "tests/spec/s03_sema/and_requires_bool_err.ks",                   // E0110
    "tests/spec/s03_sema/assign_to_const_err.ks",                     // E0110
    "tests/spec/s03_sema/assign_to_param_err.ks",                     // E0110
    "tests/spec/s03_sema/assign_type_mismatch_err.ks",                // E0110
    "tests/spec/s03_sema/block_scope_name_dies_err.ks",               // E0100
    "tests/spec/s03_sema/bool_arith_err.ks",                          // E0110
    "tests/spec/s03_sema/break_outside_loop_err.ks",                  // E0120
    "tests/spec/s03_sema/call_arg_type_mismatch_err.ks",              // E0110
    "tests/spec/s03_sema/call_arity_err.ks",                          // E0110
    "tests/spec/s03_sema/comparison_mixed_types_err.ks",              // E0110
    "tests/spec/s03_sema/condition_must_be_bool_err.ks",              // E0110
    "tests/spec/s03_sema/const_call_not_constant_err.ks",             // E0130
    "tests/spec/s03_sema/const_eval_type_error_err.ks",               // E0132
    "tests/spec/s03_sema/const_forward_reference_err.ks",             // E0131
    "tests/spec/s03_sema/expect_outside_test_err.ks",                 // E0140
    "tests/spec/s03_sema/redefine_builtin_err.ks",                    // E0101
    "tests/spec/s03_sema/return_type_mismatch_err.ks",                // E0110
    "tests/spec/s03_sema/return_void_rules_err.ks",                   // E0110
    "tests/spec/s03_sema/unknown_name_err.ks",                        // E0100
    "tests/spec/s03_sema/void_result_unusable_err.ks",                // E0110
    "tests/spec/s14_arrays/index_non_array_err.ks",                   // E0220
    "tests/spec/s18_inference/infer_const_stays_immutable_err.ks",    // E0110
    "tests/spec/s18_inference/infer_default_not_i32_err.ks",          // E0110
    "tests/spec/s23_strings/string_eq_operator_err.ks",               // E0110
    "tests/spec/s23_strings/string_plus_operator_err.ks",             // E0110
    "tests/spec/s25_generic_structs/err_alias_of_non_ctor.ks",        // E0311
    "tests/spec/s27_compound/bool_rhs_err.ks",                        // E0110
    "tests/spec/s27_compound/const_place_err.ks",                     // E0110
    "tests/spec/s27_compound/mismatch_err.ks",                        // E0110
    "tests/spec/s28_bitwise/bitand_bool_err.ks",                      // E0110
    "tests/spec/s28_bitwise/bitnot_bool_err.ks",                      // E0110
];

/// Floor on the number of corpus files whose C is byte-compared: catches a
/// subset-detector regression that silently skips what used to be compared.
const MIN_C_COMPARED: usize = 55;

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

// ---- the subset detector (the `es_detect` mirror) ----------------------------

type Hit = (&'static str, usize);

/// The subset type spellings.
fn subset_type_name(name: &str) -> bool {
    matches!(name, "i32" | "i64" | "bool" | "void" | "u8" | "usize")
}

/// A type reference: any composite form other than a slice is out; a slice
/// must be exactly `[]u8` (v0.162); a bare base name must be a subset
/// spelling (`@This()` parses to the synthesized name `Self`, which is not
/// one — the selfhost side reports its `F_THIS` flag identically, sliced or
/// not).
fn det_type(t: &TypeExpr) -> Option<Hit> {
    if t.optional
        || t.error_union
        || t.error_set.is_some()
        || t.array_len.is_some()
        || t.pointer
        || t.ctor_args.is_some()
    {
        return Some(("type-form", t.span.start));
    }
    if t.slice {
        if t.name != "u8" {
            return Some(("type-name", t.span.start));
        }
        return None;
    }
    if !subset_type_name(&t.name) {
        return Some(("type-name", t.span.start));
    }
    None
}

fn det_expr(e: &Expr) -> Option<Hit> {
    let pos = e.span().start;
    match e {
        Expr::Int { .. } | Expr::Bool { .. } | Expr::Ident { .. } => None,
        Expr::Unary { expr, .. } => det_expr(expr),
        Expr::Binary { lhs, rhs, .. } => det_expr(lhs).or_else(|| det_expr(rhs)),
        Expr::Call { callee, args, .. } => {
            if callee == "alloc" || callee == "free" || callee == "c_allocator" {
                return Some(("builtin-call", pos));
            }
            args.iter().find_map(det_expr)
        }
        Expr::Comptime { expr, .. } => det_expr(expr),
        // A string literal is in the subset (v0.162).
        Expr::StrLit { .. } => None,
        // The one field access in the subset: `.len` (v0.162); the base is
        // walked either way.
        Expr::Field { base, field, .. } => {
            if field != "len" {
                return Some(("field", pos));
            }
            det_expr(base)
        }
        // A read index `s[i]` is in the subset (v0.162); index WRITES are
        // `Stmt::FieldAssign` places and stay out.
        Expr::Index { base, index, .. } => det_expr(base).or_else(|| det_expr(index)),
        Expr::Float { .. } => Some(("float", pos)),
        Expr::Builtin { .. } => Some(("builtin", pos)),
        Expr::StructLit { .. } => Some(("struct-lit", pos)),
        Expr::StructType { .. } => Some(("struct-type", pos)),
        Expr::MethodCall { .. } => Some(("method-call", pos)),
        Expr::Null { .. } => Some(("null", pos)),
        Expr::Orelse { .. } => Some(("orelse", pos)),
        Expr::Unwrap { .. } => Some(("unwrap", pos)),
        Expr::ErrorLit { .. } => Some(("error-lit", pos)),
        Expr::EnumLit { .. } => Some(("enum-lit", pos)),
        Expr::ArrayLit { .. } => Some(("array-lit", pos)),
        Expr::AddrOf { .. } => Some(("addrof", pos)),
        Expr::Deref { .. } => Some(("deref", pos)),
        Expr::SliceExpr { .. } => Some(("slice-expr", pos)),
        Expr::Try { .. } => Some(("try", pos)),
        Expr::Catch { .. } => Some(("catch", pos)),
        Expr::Unreachable { .. } => Some(("unreachable", pos)),
    }
}

fn det_block(b: &kardc::ast::Block) -> Option<Hit> {
    b.stmts.iter().find_map(det_stmt)
}

fn det_stmt(s: &Stmt) -> Option<Hit> {
    let pos = s.span().start;
    match s {
        Stmt::Let { ty, value, .. } => ty
            .as_ref()
            .and_then(det_type)
            .or_else(|| det_expr(value)),
        Stmt::Assign { value, .. } => det_expr(value),
        Stmt::FieldAssign { .. } => Some(("place-assign", pos)),
        Stmt::Expr(e) => det_expr(e),
        Stmt::Return { value, .. } => value.as_ref().and_then(det_expr),
        Stmt::If {
            cond,
            capture,
            then,
            els,
            ..
        } => {
            if capture.is_some() {
                return Some(("capture", pos));
            }
            det_expr(cond)
                .or_else(|| det_block(then))
                .or_else(|| els.as_deref().and_then(det_stmt))
        }
        Stmt::While {
            cond,
            cont,
            body,
            label,
            ..
        } => {
            if label.is_some() {
                return Some(("label", pos));
            }
            det_expr(cond)
                .or_else(|| cont.as_deref().and_then(det_stmt))
                .or_else(|| det_block(body))
        }
        Stmt::For { .. } => Some(("for", pos)),
        Stmt::Break { target, .. } | Stmt::Continue { target, .. } => {
            if target.is_some() {
                return Some(("label", pos));
            }
            None
        }
        Stmt::Defer { stmt, .. } => det_stmt(stmt),
        Stmt::ErrDefer { .. } => Some(("errdefer", pos)),
        Stmt::Block(b) => det_block(b),
        Stmt::Switch { .. } => Some(("switch", pos)),
    }
}

fn det_fn(f: &Func) -> Option<Hit> {
    for p in &f.params {
        if p.is_comptime {
            return Some(("generic-param", p.span.start));
        }
        if let Some(hit) = det_type(&p.ty) {
            return Some(hit);
        }
    }
    det_type(&f.ret).or_else(|| det_block(&f.body))
}

/// The subset verdict for a parsed module: `None` = in the subset, else the
/// FIRST unsupported construct. Mirrors `es_detect` in `selfhost/emit.ks`
/// (which walks the arena in the same order); the differential compares both
/// the word and the position on every corpus file.
fn detect_subset(module: &Module) -> Option<Hit> {
    let has_main = module
        .items
        .iter()
        .any(|it| matches!(it, Item::Func(f) if f.name == "main"));
    if !has_main {
        return Some(("nomain", 0));
    }
    for item in &module.items {
        let hit = match item {
            Item::Func(f) => det_fn(f),
            Item::Const(c) => c
                .ty
                .as_ref()
                .and_then(det_type)
                .or_else(|| det_expr(&c.value)),
            Item::Test(t) => Some(("test", t.span.start)),
            Item::Struct(s) => Some(("struct", s.span.start)),
            Item::Enum(e) => Some(("enum", e.span.start)),
            Item::Union(u) => Some(("union", u.span.start)),
            Item::Import(i) => Some(("import", i.span.start)),
            Item::ErrorSet(e) => Some(("errorset", e.span.start)),
        };
        if hit.is_some() {
            return hit;
        }
    }
    None
}

// ---- the reference classifier -------------------------------------------------

/// What the driver must print for one input.
enum Expected {
    /// Compare stdout to these exact bytes (an ERROR line, a SKIP line, or
    /// the full C text).
    Bytes(String),
    /// Subset-shaped but sema-rejected: no reference output — only assert
    /// exit 0. Carries the first diagnostic code for the list's bookkeeping.
    SemaInvalid(String),
}

/// Classify `path` with the Rust pipeline (see the module docs).
fn rust_expected(path: &Path, src: &str) -> Expected {
    let tokens = match kardc::lexer::lex(src) {
        Ok(t) => t,
        Err(diags) => {
            let d = &diags[0];
            let code = match d.code {
                "E0001" => 1,
                "E0002" => 2,
                other => panic!("unexpected lexer diagnostic code {other}"),
            };
            return Expected::Bytes(format!("ERROR {} {}\n", code, d.span.start));
        }
    };
    let module = match kardc::parser::parse(&tokens) {
        Ok(m) => m,
        Err(diags) => {
            let d = &diags[0];
            let code = match d.code {
                "E0200" => 200,
                "E0201" => 201,
                other => panic!("unexpected parser diagnostic code {other}"),
            };
            return Expected::Bytes(format!("ERROR {} {}\n", code, d.span.start));
        }
    };
    if let Some((word, pos)) = detect_subset(&module) {
        return Expected::Bytes(format!("SKIP {} {}\n", word, pos));
    }
    match kardc::compile_program(path, EmitMode::Program) {
        Ok(c) => Expected::Bytes(c),
        Err(diags) => Expected::SemaInvalid(diags[0].code.to_string()),
    }
}

// ---- harness --------------------------------------------------------------------

/// Compile `selfhost/cdump.ks` (program mode, `-O0`) to a temp executable.
fn build_cdump() -> PathBuf {
    let src = repo_root().join("selfhost/cdump.ks");
    let c = kardc::compile_program(&src, EmitMode::Program).unwrap_or_else(|diags| {
        let text = std::fs::read_to_string(&src).unwrap_or_default();
        panic!(
            "selfhost/cdump.ks failed to compile:\n{}",
            kardc::diag::render_all(&diags, &src.display().to_string(), &text)
        )
    });
    let exe = temp_path("cdump");
    let opts = BuildOptions {
        opt: OptLevel::O0,
        ..BuildOptions::default()
    };
    kardc::backend::cc_build(&c, &exe, &opts).expect("cc should build cdump");
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

/// Run the cdump binary on `input`; assert exit 0 and return its stdout.
fn run_driver(exe: &Path, input: &Path) -> Result<String, String> {
    let out = Command::new(exe)
        .arg(input)
        .output()
        .unwrap_or_else(|e| panic!("failed to run cdump on {}: {e}", input.display()));
    if out.status.code() != Some(0) {
        return Err(format!(
            "{}: cdump exited {:?}\n--- stderr ---\n{}",
            input.display(),
            out.status.code(),
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Diff the driver's stdout for `input` against the Rust classification.
/// `Ok(Some(bytes))` = compared (that many bytes identical); `Ok(None)` =
/// a declared-invalid file (exit checked, output uncompared).
fn diff_one(exe: &Path, input: &Path, expected: &Expected) -> Result<Option<usize>, String> {
    let got = run_driver(exe, input)?;
    let want = match expected {
        Expected::Bytes(b) => b,
        Expected::SemaInvalid(_) => return Ok(None),
    };
    if &got != want {
        let g: Vec<&str> = got.lines().collect();
        let e: Vec<&str> = want.lines().collect();
        let mut i = 0;
        while i < g.len() && i < e.len() && g[i] == e[i] {
            i += 1;
        }
        return Err(format!(
            "{}: output mismatch at line {} — rust `{}` vs selfhost `{}` ({} vs {} lines)",
            input.display(),
            i + 1,
            e.get(i).unwrap_or(&"<eof>"),
            g.get(i).unwrap_or(&"<eof>"),
            e.len(),
            g.len()
        ));
    }
    Ok(Some(want.len()))
}

/// (a) The full-repository differential corpus: every real `.ks` source in
/// the repo, each classified and byte-compared (or, for the pinned
/// sema-invalid remainder, exit-checked). One shared `-O0` cdump build, one
/// subprocess execution per file, so the corpus is NOT capped.
#[test]
fn selfhost_emit_differential_corpus() {
    let root = repo_root();
    let exe = build_cdump();

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
    let mut sema_invalid_seen: BTreeSet<String> = BTreeSet::new();
    let mut n_error = 0usize;
    let mut n_skip = 0usize;
    let mut n_c = 0usize;
    let mut c_bytes = 0usize;
    for file in &corpus {
        let src = match std::fs::read_to_string(file) {
            Ok(s) => s,
            Err(e) => {
                failures.push(format!("{}: unreadable corpus file: {e}", file.display()));
                continue;
            }
        };
        let expected = rust_expected(file, &src);
        match &expected {
            Expected::Bytes(b) if b.starts_with("ERROR ") => n_error += 1,
            Expected::Bytes(b) if b.starts_with("SKIP ") => n_skip += 1,
            Expected::Bytes(_) => {}
            Expected::SemaInvalid(_) => {
                let rel = file
                    .strip_prefix(&root)
                    .expect("corpus file under repo root")
                    .display()
                    .to_string();
                sema_invalid_seen.insert(rel);
            }
        }
        match diff_one(&exe, file, &expected) {
            Ok(Some(bytes)) => {
                if matches!(&expected, Expected::Bytes(b) if !b.starts_with("ERROR ") && !b.starts_with("SKIP "))
                {
                    n_c += 1;
                    c_bytes += bytes;
                }
            }
            Ok(None) => {}
            Err(msg) => failures.push(msg),
        }
    }
    let _ = std::fs::remove_file(&exe);

    // The sema-invalid remainder is pinned exactly: a drift in either
    // direction (a new uncompared file, or a file that became comparable)
    // must update SEMA_INVALID consciously.
    let declared: BTreeSet<String> = SEMA_INVALID.iter().map(|s| s.to_string()).collect();
    assert_eq!(
        sema_invalid_seen, declared,
        "subset-shaped sema-invalid files drifted from SEMA_INVALID:\n  observed only: {:?}\n  declared only: {:?}",
        sema_invalid_seen.difference(&declared).collect::<Vec<_>>(),
        declared.difference(&sema_invalid_seen).collect::<Vec<_>>()
    );
    assert!(
        n_c >= MIN_C_COMPARED,
        "only {} corpus files were C-compared (floor {MIN_C_COMPARED}) — did the subset detector regress?",
        n_c
    );
    assert!(
        failures.is_empty(),
        "{} of {} corpus files mismatched the Rust emitter:\n{}",
        failures.len(),
        corpus.len(),
        failures.join("\n")
    );
    println!(
        "selfhost emit differential: {} files — {} C byte-identical ({} bytes), {} SKIP-agreed, {} ERROR-agreed, {} declared sema-invalid (exit-checked)",
        corpus.len(),
        n_c,
        c_bytes,
        n_skip,
        n_error,
        sema_invalid_seen.len()
    );
}

/// (b) Targeted inputs (written to temp files): emit-specific edges the
/// corpus under-exercises — the `defer` matrix (LIFO, loop edges, nested
/// scopes, the `__kd_ret` hoist), dead-function elimination, inference
/// quirks, const folding — plus SKIP-verdict positions on tricky shapes.
/// Every case must produce byte-identical driver output.
#[test]
fn selfhost_emit_differential_targeted_inputs() {
    let exe = build_cdump();
    let cases: &[(&str, &str)] = &[
        // -- the defer matrix ------------------------------------------------
        (
            "defer_lifo_return_temp",
            "fn f() i32 {\n    defer print(1);\n    defer print(2);\n    return 7;\n}\npub fn main() void {\n    print(f());\n}\n",
        ),
        (
            "defer_loop_edges",
            "pub fn main() i32 {\n    var i: i32 = 0;\n    while (i < 6) : (i = i + 1) {\n        defer print(100 + i);\n        if (i == 2) { continue; }\n        if (i == 4) { break; }\n        print(i);\n    }\n    defer print(999);\n    return 0;\n}\n",
        ),
        (
            "defer_nested_loops_return",
            "fn g(n: i32) i32 {\n    defer print(10);\n    var i: i32 = 0;\n    while (i < n) : (i = i + 1) {\n        defer print(20);\n        var j: i32 = 0;\n        while (j < n) {\n            defer print(30);\n            j = j + 1;\n            if (j == 2) { break; }\n            if (i + j == 3) { return 42; }\n            continue;\n        }\n    }\n    return 0;\n}\npub fn main() void { print(g(3)); }\n",
        ),
        (
            "defer_void_returns",
            "fn v() void {\n    defer print(5);\n    print(1);\n    return;\n}\nfn v2() void {\n    defer print(6);\n    print(2);\n}\npub fn main() void { v(); v2(); }\n",
        ),
        (
            "defer_bare_block_scope",
            "pub fn main() void {\n    defer print(1);\n    {\n        defer print(2);\n        print(3);\n    }\n    print(4);\n}\n",
        ),
        (
            "defer_in_defer_block",
            "pub fn main() void {\n    defer {\n        defer print(1);\n        print(2);\n    }\n    print(3);\n}\n",
        ),
        (
            "defer_no_value_flush_order",
            "fn f() void {\n    defer print(1);\n    defer print(2);\n    if (true) { return; }\n    print(9);\n}\npub fn main() void { f(); }\n",
        ),
        // -- control flow / divergence ---------------------------------------
        (
            "else_if_ladder_divergence",
            "fn c(x: i32) i32 {\n    if (x == 1) {\n        return 10;\n    } else if (x == 2) {\n        return 20;\n    } else {\n        return 30;\n    }\n}\npub fn main() void { print(c(2)); }\n",
        ),
        (
            "statements_after_return_dropped",
            "fn f() i32 {\n    return 1;\n    print(999);\n}\npub fn main() void { print(f()); }\n",
        ),
        (
            "while_cont_compound",
            "pub fn main() void {\n    var i: i64 = 0;\n    var s: i64 = 0;\n    while (i < 10) : (i += 3) {\n        s += i;\n    }\n    print(s);\n}\n",
        ),
        (
            "bare_block_shadowing",
            "pub fn main() void {\n    {\n        var t: i64 = 5;\n        print(t);\n    }\n    {\n        var t: bool = true;\n        if (t) { print(1); }\n    }\n}\n",
        ),
        // -- dead-function elimination ----------------------------------------
        (
            "dead_functions_dropped",
            "fn used(x: i64) i64 { return x + 1; }\nfn dead(x: i64) i64 { return unused_helper(x); }\nfn unused_helper(x: i64) i64 { return x; }\nfn used_via_defer() void { print(7); }\npub fn main() void {\n    defer used_via_defer();\n    print(used(1));\n}\n",
        ),
        (
            "mutual_recursion_live",
            "fn even(n: i64) bool { if (n == 0) { return true; } return odd(n - 1); }\nfn odd(n: i64) bool { if (n == 0) { return false; } return even(n - 1); }\npub fn main() i32 {\n    if (even(10)) { print(1); }\n    return 3;\n}\n",
        ),
        // -- consts + comptime -------------------------------------------------
        (
            "const_fold_chain",
            "const A: i64 = comptime (3 * 4 + 1);\nconst B: bool = comptime (A > 10);\nconst C = A + 2;\nconst D = B;\npub fn main() void {\n    print(A);\n    print(C);\n    if (B and D) { print(1); }\n}\n",
        ),
        (
            "comptime_expr_positions",
            "pub fn main() void {\n    var t: i64 = comptime (2 + 2);\n    var u: i64 = comptime (1 << 6) + 1;\n    print(t + u);\n    print(comptime (10 / 3));\n    print(comptime (10 % 3));\n    if (comptime (3 > 2)) { print(1); }\n}\n",
        ),
        (
            "const_annotated_i32",
            "const M: i32 = 100;\nconst F: bool = false;\npub fn main() void {\n    print(M);\n    if (!F) { print(1); }\n}\n",
        ),
        // -- inference ----------------------------------------------------------
        (
            "inference_defaults_and_quirks",
            "const K: i64 = 9;\nfn h() void {}\nfn gi() i32 { return 3; }\npub fn main() void {\n    var x = 5;\n    var y = x;\n    var b = true;\n    var n = !b;\n    var m = -x;\n    var q = K;\n    var r = comptime (K + 1);\n    const s: i32 = 3;\n    var t = s + 1;\n    var u = gi();\n    print(x); print(y); print(m); print(q); print(r); print(t);\n    if (n) { print(0); }\n    h();\n    print(u);\n}\n",
        ),
        // -- operators -----------------------------------------------------------
        (
            "operator_zoo",
            "pub fn main() void {\n    var x: i64 = 0 - 5;\n    var y: i64 = ~x;\n    var z: i64 = (x << 3) >> 1;\n    var w: i64 = (x & y) | (x ^ 7);\n    var b: bool = (x < y) or ((y >= z) and !(w == 0));\n    var c: bool = b != false;\n    x += 2; x -= 1; x *= 3; x /= 2; x %= 5;\n    print(x); print(y); print(z); print(w);\n    if (c) { print(1); }\n    var m: i32 = 2147483647;\n    print(m);\n    var big: i64 = 9223372036854775807;\n    print(big);\n}\n",
        ),
        (
            "int_main_wire",
            "pub fn main() i64 { print(1); return 2; }\n",
        ),
        (
            "bool_main_wire",
            "fn t() bool { return true; }\npub fn main() bool { return t(); }\n",
        ),
        // -- strings (v0.162) ----------------------------------------------------
        (
            "string_escape_zoo",
            "pub fn main() void {\n    print(\"hello\");\n    print(\"a\\nb\\tc\");\n    print(\"q\\\"w\\\\e\");\n    print(\"\");\n}\n",
        ),
        (
            "string_hex_split",
            "pub fn main() void {\n    print(\"a\x07fb\");\n    print(\"\x01\x02X\x030\");\n    print(\"\u{e9}\");\n}\n",
        ),
        (
            "string_slices_params_returns",
            "fn pick(s: []u8, alt: []u8, b: bool) []u8 {\n    if (b) { return s; }\n    return alt;\n}\nfn measure(s: []u8) usize {\n    return s.len;\n}\npub fn main() void {\n    var s: []u8 = \"kardashev\";\n    var t = pick(s, \"other\", true);\n    print(t);\n    print(t.len);\n    print(measure(\"abc\"));\n    var i: usize = 0;\n    while (i < s.len) : (i += 1) {\n        print(s[i]);\n    }\n    print(s[s.len - 1]);\n}\n",
        ),
        (
            "u8_bytes_and_promotion",
            "fn double_u8(n: u8) u8 { return n * 2; }\npub fn main() void {\n    var s: []u8 = \"kz\";\n    var c: u8 = s[0];\n    var d = double_u8(c);\n    print(d);\n    print(~c);\n    print(c << 1);\n    print(~(c << 1));\n    var e: u8 = 65;\n    var f = e + 1;\n    print(f);\n}\n",
        ),
        (
            "string_defer_and_hoist_counter",
            "fn f() []u8 {\n    defer print(\"bye\");\n    defer print(\"later\");\n    return \"val\";\n}\npub fn main() void {\n    defer print(\"end\");\n    print(f());\n    print(\"mid\");\n}\n",
        ),
        (
            "slice_typedef_gating_absent",
            "pub fn main() void { print(1); }\n",
        ),
        (
            "slice_typedef_gating_dead_fn",
            "fn dead() void { print(\"never\"); }\npub fn main() void { print(1); }\n",
        ),
        // -- SKIP verdict positions on tricky shapes ---------------------------
        (
            "skip_float_deep_in_defer",
            "pub fn main() void {\n    defer {\n        var x: i64 = 1;\n        print(x + 1.5);\n    }\n}\n",
        ),
        (
            "skip_slice_of_i32",
            "pub fn main() void {\n    if (true) {\n        var s: []i32 = q();\n    }\n}\n",
        ),
        (
            "skip_field_not_len",
            "pub fn main() void {\n    var s: []u8 = \"x\";\n    print(s.ptr);\n}\n",
        ),
        ("skip_nomain", "fn helper() void {}\n"),
        ("skip_empty_module", ""),
        (
            "skip_alloc_call",
            "pub fn main() void {\n    var n: i64 = 4;\n    free(a, alloc(a, i64, n));\n}\n",
        ),
        (
            "skip_test_block_after_main",
            "pub fn main() void { print(1); }\ntest \"t\" { expect(true); }\n",
        ),
        (
            "skip_comptime_param",
            "fn id(comptime T: type, x: i64) i64 { return x; }\npub fn main() void { print(id(i64, 1)); }\n",
        ),
        (
            "skip_labeled_while",
            "pub fn main() void {\n    outer: while (true) {\n        break :outer;\n    }\n}\n",
        ),
        (
            "skip_unreachable_stmt",
            "pub fn main() void {\n    if (false) { unreachable; }\n    print(1);\n}\n",
        ),
    ];
    let mut failures: Vec<String> = Vec::new();
    for (tag, src) in cases {
        let input = temp_path(&format!("cerr_{tag}"));
        std::fs::write(&input, src).expect("write temp emit input");
        let expected = rust_expected(&input, src);
        if let Expected::SemaInvalid(code) = &expected {
            failures.push(format!(
                "[{tag}] targeted input is sema-invalid ({code}) — every case must classify as ERROR, SKIP or valid C"
            ));
            let _ = std::fs::remove_file(&input);
            continue;
        }
        if let Err(msg) = diff_one(&exe, &input, &expected) {
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

/// (c) The in-language suite: `tests/selfhost/emit_suite.ks` must compile in
/// test mode and report every test passing (exit code 0 = failure count).
#[test]
fn selfhost_emit_suite_passes() {
    let suite = repo_root().join("tests/selfhost/emit_suite.ks");
    let c = kardc::compile_program(&suite, EmitMode::Test).unwrap_or_else(|diags| {
        let text = std::fs::read_to_string(&suite).unwrap_or_default();
        panic!(
            "emit_suite.ks failed to compile:\n{}",
            kardc::diag::render_all(&diags, &suite.display().to_string(), &text)
        )
    });
    let exe = temp_path("esuite");
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
        "emit_suite.ks had failing tests:\n--- stderr ---\n{}\n--- stdout ---\n{}",
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout)
    );
}
