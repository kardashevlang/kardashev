//! Self-host stages 3–10 (v0.161–v0.168): differential test of
//! `selfhost/emit.ks` — a C emitter for the SCALAR + STRING + HEAP-BUFFER
//! SUBSET (with generalized `[]T` slices, `@as` casts, the `s[lo..hi]`
//! slicing view, `test` blocks with the full `EmitMode::Test` harness,
//! `@import` resolution, and — v0.168 — fixed arrays `[N]T` with array
//! literals and `for` loops) written in kardashev — against the Rust
//! reference emitter. Since v0.166 every corpus file is classified and
//! compared in BOTH modes: `cdump <file>` prints the Program lowering,
//! `cdump <file> test` the Test-mode harness (no `nomain` gate; a module
//! without tests is the trivial harness with EVERY function live).
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
//! The subset: `i32`/`i64`/`bool`/`void`/`u8`/`usize`/`Allocator` bare
//! types plus `[]T` over the five scalar elements (v0.164); top-level
//! `fn`/`const`; `var`/`const` lets, (compound) name-assignment, the
//! (compound) DIRECT index write `s[i] (op)= e` (chains through an index
//! stay out), `if`/`else`, `while` with continue-clause, unlabeled
//! `break`/`continue`, `defer`, `return`, bare blocks, expression
//! statements; int/bool/STRING literals, names, unary `-`/`!`/`~`, the full
//! binary ladder, free calls, `print` (integers and `[]u8` strings),
//! `expect`, `comptime`, `@as(T, e)` casts (v0.164), `.len` on a slice, the
//! read index `s[i]`, the slicing view `s[lo..hi]` (v0.165 — a `{ptr, len}`
//! view with the bounds check folded into a `_Noreturn` conditional), and
//! the allocator builtins `c_allocator()` / `alloc(a, T, n)` / `free(a, s)`.
//!
//! v0.164's load-bearing piece: the typedef section emits one
//! `kd_slice_<tag>` block per interned slice IN SEMA'S FIRST-INTERN ORDER,
//! which `selfhost/emit.ks` reproduces by replaying sema's walk (all fn
//! signatures params-then-return in item order, then const annotations,
//! then bodies — with Let annotation-before-initializer, While
//! cond/CONT/body, index-writes index-first, `alloc` interning AFTER its
//! allocator/count args, and `comptime` subtrees never interning).
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
    "tests/spec/s14_arrays/index_assign_const_err.ks",                // E0223
    "tests/spec/s14_arrays/index_assign_param_err.ks",                // E0223
    "tests/spec/s14_arrays/index_non_array_err.ks",                   // E0220
    "tests/spec/s14_arrays/index_not_integer_err.ks",                 // E0110
    "tests/spec/s14_arrays/literal_count_mismatch_err.ks",            // E0221
    "tests/spec/s14_arrays/literal_element_type_err.ks",              // E0110
    "tests/spec/s15_ptr_slices/slice_non_sliceable_err.ks",           // E0232
    "tests/spec/s16_alloc/free_non_slice_err.ks",                     // E0242
    "tests/spec/s18_inference/infer_const_stays_immutable_err.ks",    // E0110
    "tests/spec/s18_inference/infer_default_not_i32_err.ks",          // E0110
    "tests/spec/s23_strings/string_eq_operator_err.ks",               // E0110
    "tests/spec/s23_strings/string_print_non_u8_slice_err.ks",        // E0110
    "tests/spec/s23_strings/string_plus_operator_err.ks",             // E0110
    "tests/spec/s25_generic_structs/err_alias_of_non_ctor.ks",        // E0311
    "tests/spec/s27_compound/bool_rhs_err.ks",                        // E0110
    "tests/spec/s27_compound/const_place_err.ks",                     // E0110
    "tests/spec/s27_compound/mismatch_err.ks",                        // E0110
    "tests/spec/s28_bitwise/bitand_bool_err.ks",                      // E0110
    "tests/spec/s28_bitwise/bitnot_bool_err.ks",                      // E0110
    "tests/spec/s29_for/elem_immutable_err.ks",                       // E0110
    "tests/spec/s29_for/index_type_err.ks",                           // E0110
    "tests/spec/s29_for/non_iterable_err.ks",                         // E0300
    "tests/spec/s33_casts/err_as_not_constant.ks",                    // E0130
    "tests/spec/s33_casts/err_as_value_not_numeric.ks",               // E0321
];

/// Floor on the number of corpus files whose C is byte-compared: catches a
/// subset-detector regression that silently skips what used to be compared.
/// Files that additionally fail sema when classified for `EmitMode::Test`
/// (v0.166): a module fragment is `nomain`-skipped in Program mode but
/// reaches sema in Test mode, where its cross-module reference is E0100.
const SEMA_INVALID_TEST_ONLY: &[&str] = &[
    "tests/spec/s22_modules/_back_calls_root.ks",                     // E0100
];

const MIN_C_COMPARED_PROGRAM: usize = 115;
const MIN_C_COMPARED_TEST: usize = 130;

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
    matches!(
        name,
        "i32" | "i64" | "bool" | "void" | "u8" | "usize" | "Allocator"
    )
}

/// The subset slice ELEMENT spellings (`[]T` and `alloc(a, T, n)`, v0.164).
fn subset_slice_elem(name: &str) -> bool {
    matches!(name, "i32" | "i64" | "bool" | "u8" | "usize")
}

/// `Emitter::place_chain_has_index` (the `es_chain_has_index` mirror):
/// whether a place reaches its target THROUGH an index via value links.
fn chain_has_index(e: &Expr) -> bool {
    match e {
        Expr::Index { .. } => true,
        Expr::Field { base, .. } => chain_has_index(base),
        _ => false,
    }
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
        || t.pointer
        || t.ctor_args.is_some()
        || matches!(&t.array_len, Some(kardc::ast::ArraySize::Param(_)))
    {
        return Some(("type-form", t.span.start));
    }
    if t.array_len.is_some() || t.slice {
        // `[N]T` (v0.168) / `[]T` (v0.164) over the five scalar elements.
        if !subset_slice_elem(&t.name) {
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
            // `alloc(a, T, n)` is in the subset (v0.163, elements
            // generalized in v0.164) — exactly three arguments with a
            // scalar element type; any other shape is out. `free(a, s)` /
            // `c_allocator()` walk their arguments like ordinary calls.
            if callee == "alloc" {
                let shaped = args.len() == 3
                    && matches!(&args[1], Expr::Ident { name, .. } if subset_slice_elem(name));
                if !shaped {
                    return Some(("builtin-call", pos));
                }
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
        // `@as(T, e)` is in the subset (v0.164): exactly two arguments, the
        // first an identifier naming a subset type; only the VALUE argument
        // is walked. Every other `@`-builtin stays out.
        Expr::Builtin { name, args, .. } => {
            if name == "as"
                && args.len() == 2
                && matches!(&args[0], Expr::Ident { name, .. } if subset_type_name(name))
            {
                return det_expr(&args[1]);
            }
            Some(("builtin", pos))
        }
        Expr::StructLit { .. } => Some(("struct-lit", pos)),
        Expr::StructType { .. } => Some(("struct-type", pos)),
        Expr::MethodCall { .. } => Some(("method-call", pos)),
        Expr::Null { .. } => Some(("null", pos)),
        Expr::Orelse { .. } => Some(("orelse", pos)),
        Expr::Unwrap { .. } => Some(("unwrap", pos)),
        Expr::ErrorLit { .. } => Some(("error-lit", pos)),
        Expr::EnumLit { .. } => Some(("enum-lit", pos)),
        // An array literal `[N]T{ … }` is in the subset (v0.168): its
        // `[N]T` reference, then the elements, in order.
        Expr::ArrayLit { elem, elems, .. } => {
            det_type(elem).or_else(|| elems.iter().find_map(det_expr))
        }
        // The slicing view `base[lo..hi]` is in the subset (v0.165).
        Expr::SliceExpr { base, lo, hi, .. } => det_expr(base)
            .or_else(|| det_expr(lo))
            .or_else(|| det_expr(hi)),
        Expr::AddrOf { .. } => Some(("addrof", pos)),
        Expr::Deref { .. } => Some(("deref", pos)),
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
        // The one place-assignment in the subset (v0.163): a DIRECT index
        // write `s[i] (op)= e` — an `Index` place whose base does not
        // itself pass through an index (that shape takes the `_at`
        // lowering and stays out). Walk base, index, value, in that order.
        Stmt::FieldAssign { place, value, .. } => {
            if let Expr::Index { base, index, .. } = place {
                if !chain_has_index(base) {
                    return det_expr(base)
                        .or_else(|| det_expr(index))
                        .or_else(|| det_expr(value));
                }
            }
            Some(("place-assign", pos))
        }
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
        // `for` is in the subset (v0.168); a LABELED `for` stays out.
        Stmt::For {
            iter, body, label, ..
        } => {
            if label.is_some() {
                return Some(("label", pos));
            }
            det_expr(iter).or_else(|| det_block(body))
        }
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

// (The single-file `detect_subset` of v0.161–v0.166 is superseded by
// `detect_flat` over the resolved module — see the resolution mirror.)

// ---- the import-resolution mirror (v0.167) -------------------------------------
//
// `selfhost/modres.ks` resolves the root's `@import`s into one flattened
// module over a CONCATENATED virtual source; every downstream position
// (ERROR/SKIP lines, detector hits) is in those coordinates. This mirror
// replays the same walk over the Rust AST: files load depth-first in
// import order, each file's source base = the sum of previously-read
// files' lengths, a file's imported items precede its own, dedup/cycle
// keys are LEXICALLY normalized paths, a `std`/`std.ks` basename naming no
// readable (non-empty) file is the out-of-subset embedded library (SKIP
// `import`), a missing/empty import is E0291, a cycle E0292, an imported
// file's lex/parse failure E0294 at 0, and the first duplicate top-level
// name E0293 at the duplicate's rebased position.

/// One flattened file: its parsed module and its base offset into the
/// virtual concatenated source.
struct FlatFile {
    module: Module,
    base: usize,
}

enum Resolved {
    /// An `ERROR <code> <pos>` or `SKIP import <pos>` line.
    Line(String),
    /// The flattened module, per file in APPEND order.
    Flat(Vec<FlatFile>),
}

/// The `mr_normalize` mirror: fold `.` and `//`, resolve `..` against a
/// poppable segment, keep a leading `/`.
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

/// The `mr_dir_of` mirror (prefix including the trailing `/`).
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

struct Resolver {
    src_len: usize,
    /// normalized path → state (true = on the DFS stack, false = done).
    states: std::collections::HashMap<String, bool>,
    out: Vec<FlatFile>,
    fail: Option<String>,
}

impl Resolver {
    fn resolve_file(&mut self, norm: &str, import_pos: usize, is_root: bool) {
        if self.fail.is_some() {
            return;
        }
        let content = std::fs::read_to_string(norm).unwrap_or_default();
        let base_name = basename(norm);
        if (base_name == "std" || base_name == "std.ks") && content.is_empty() {
            self.fail = Some(format!("SKIP import {}\n", import_pos));
            return;
        }
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

        // Pass 1 — imports depth-first, in item order.
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
            // Pass 2 — append this file's own items.
            self.out.push(FlatFile { module, base });
        }
        self.states.insert(norm.to_string(), false);
    }
}

/// Resolve `root` exactly as `selfhost/modres.ks` does.
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
    // `check_unique` (E0293): first duplicate top-level name, at the
    // DUPLICATE item's rebased position. Tests carry no shared name.
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

/// The flattened-module detector: `nomain` (Program mode) over every
/// file's items, then each file's non-import items in append order, hit
/// positions rebased by the file's base.
fn detect_flat(files: &[FlatFile], program_mode: bool) -> Option<(String, usize)> {
    if program_mode {
        let has_main = files.iter().any(|ff| {
            ff.module
                .items
                .iter()
                .any(|it| matches!(it, Item::Func(f) if f.name == "main"))
        });
        if !has_main {
            return Some(("nomain".to_string(), 0));
        }
    }
    for ff in files {
        for item in &ff.module.items {
            if matches!(item, Item::Import(_)) {
                continue;
            }
            let hit = match item {
                Item::Func(f) => det_fn(f),
                Item::Const(c) => c
                    .ty
                    .as_ref()
                    .and_then(det_type)
                    .or_else(|| det_expr(&c.value)),
                Item::Test(t) => det_block(&t.body),
                Item::Struct(s) => Some(("struct", s.span.start)),
                Item::Enum(e) => Some(("enum", e.span.start)),
                Item::Union(u) => Some(("union", u.span.start)),
                Item::Import(_) => None,
                Item::ErrorSet(e) => Some(("errorset", e.span.start)),
            };
            if let Some((word, pos)) = hit {
                return Some((word.to_string(), ff.base + pos));
            }
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

/// Classify `path` with the Rust pipeline for `mode` (see the module docs):
/// resolve imports (ERROR 291/292/293/294 lines, the std-import SKIP), then
/// the flattened-module detector, then the REAL `compile_program` for the C
/// bytes (its internal `modules::resolve` agrees with the mirror on every
/// comparable input by construction).
fn rust_expected(path: &Path, _src: &str, mode: EmitMode) -> Expected {
    let files = match mirror_resolve(path) {
        Resolved::Line(line) => return Expected::Bytes(line),
        Resolved::Flat(files) => files,
    };
    if let Some((word, pos)) = detect_flat(&files, mode == EmitMode::Program) {
        return Expected::Bytes(format!("SKIP {} {}\n", word, pos));
    }
    match kardc::compile_program(path, mode) {
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

/// Run the cdump binary on `input` (passing `test` for `EmitMode::Test`);
/// assert exit 0 and return its stdout.
fn run_driver(exe: &Path, input: &Path, mode: EmitMode) -> Result<String, String> {
    let mut cmd = Command::new(exe);
    cmd.arg(input);
    if mode == EmitMode::Test {
        cmd.arg("test");
    }
    let out = cmd
        .output()
        .unwrap_or_else(|e| panic!("failed to run cdump on {}: {e}", input.display()));
    if out.status.code() != Some(0) {
        return Err(format!(
            "{} [{:?}]: cdump exited {:?}\n--- stderr ---\n{}",
            input.display(),
            mode,
            out.status.code(),
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Diff the driver's stdout for `input` in `mode` against the Rust
/// classification. `Ok(Some(bytes))` = compared (that many bytes
/// identical); `Ok(None)` = a declared-invalid file (exit checked, output
/// uncompared).
fn diff_one(
    exe: &Path,
    input: &Path,
    expected: &Expected,
    mode: EmitMode,
) -> Result<Option<usize>, String> {
    let got = run_driver(exe, input, mode)?;
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
            "{} [{:?}]: output mismatch at line {} — rust `{}` vs selfhost `{}` ({} vs {} lines)",
            input.display(),
            mode,
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
    for (mode, declared_list, floor) in [
        (EmitMode::Program, SEMA_INVALID.to_vec(), MIN_C_COMPARED_PROGRAM),
        (
            EmitMode::Test,
            SEMA_INVALID
                .iter()
                .chain(SEMA_INVALID_TEST_ONLY.iter())
                .copied()
                .collect::<Vec<_>>(),
            MIN_C_COMPARED_TEST,
        ),
    ] {
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
            let expected = rust_expected(file, &src, mode);
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
            match diff_one(&exe, file, &expected, mode) {
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

        // The sema-invalid remainder is pinned exactly PER MODE: a drift in
        // either direction (a new uncompared file, or a file that became
        // comparable) must update the lists consciously.
        let declared: BTreeSet<String> = declared_list.iter().map(|s| s.to_string()).collect();
        assert_eq!(
            sema_invalid_seen, declared,
            "[{mode:?}] subset-shaped sema-invalid files drifted:\n  observed only: {:?}\n  declared only: {:?}",
            sema_invalid_seen.difference(&declared).collect::<Vec<_>>(),
            declared.difference(&sema_invalid_seen).collect::<Vec<_>>()
        );
        assert!(
            n_c >= floor,
            "[{mode:?}] only {} corpus files were C-compared (floor {floor}) — did the subset detector regress?",
            n_c
        );
        println!(
            "selfhost emit differential [{:?}]: {} files — {} C byte-identical ({} bytes), {} SKIP-agreed, {} ERROR-agreed, {} declared sema-invalid (exit-checked)",
            mode,
            corpus.len(),
            n_c,
            c_bytes,
            n_skip,
            n_error,
            sema_invalid_seen.len()
        );
    }
    let _ = std::fs::remove_file(&exe);

    assert!(
        failures.is_empty(),
        "{} corpus comparisons mismatched the Rust emitter:\n{}",
        failures.len(),
        failures.join("\n")
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
        // -- index writes + allocator builtins (v0.163) --------------------------
        (
            "alloc_fill_print_free",
            "pub fn main() void {\n    var a: Allocator = c_allocator();\n    var buf: []u8 = alloc(a, u8, 5);\n    var i: usize = 0;\n    while (i < buf.len) : (i += 1) {\n        buf[i] = 65;\n    }\n    buf[0] = 107;\n    buf[1] += 2;\n    buf[2] *= 1;\n    print(buf);\n    print(buf[0]);\n    free(a, buf);\n}\n",
        ),
        (
            "index_write_counter_resets",
            "fn f(s: []u8) void {\n    s[0] = 1;\n    s[1] = 2;\n    print(s);\n}\npub fn main() void {\n    var a: Allocator = c_allocator();\n    var s: []u8 = alloc(a, u8, 3);\n    s[0] = 9;\n    f(s);\n    s[s.len - 1] = s[0] + 1;\n    print(s[2]);\n    free(a, s);\n}\n",
        ),
        (
            "index_write_in_defer",
            "pub fn main() void {\n    var a: Allocator = c_allocator();\n    var s: []u8 = alloc(a, u8, 2);\n    defer free(a, s);\n    defer s[0] = 42;\n    s[0] = 1;\n    s[1] = 2;\n    print(s[0] + s[1]);\n}\n",
        ),
        (
            "allocator_values_and_params",
            "fn fill(al: Allocator, n: usize) []u8 {\n    var s: []u8 = alloc(al, u8, n);\n    var i: usize = 0;\n    while (i < n) : (i += 1) {\n        s[i] = 48 + 1;\n    }\n    return s;\n}\npub fn main() void {\n    var a: Allocator = c_allocator();\n    var b: Allocator = a;\n    var s = fill(b, 4);\n    print(s);\n    free(a, s);\n}\n",
        ),
        (
            "typedef_gating_alloc_only",
            "pub fn main() void {\n    var a: Allocator = c_allocator();\n    free(a, alloc(a, u8, 3));\n    print(1);\n}\n",
        ),
        (
            "compound_index_write_single_eval",
            "fn idx() usize { return 1; }\npub fn main() void {\n    var a: Allocator = c_allocator();\n    var s: []u8 = alloc(a, u8, 4);\n    s[idx()] += 7;\n    s[idx() + 1] %= 5;\n    print(s[1]);\n    free(a, s);\n}\n",
        ),
        // -- generalized []T slices + @as (v0.164) --------------------------------
        (
            "slice_i64_fib_roundtrip",
            "fn make_fibs(a: Allocator, n: usize) []i64 {\n    var s: []i64 = alloc(a, i64, n);\n    s[0] = 1;\n    s[1] = 1;\n    var i: usize = 2;\n    while (i < n) : (i += 1) {\n        s[i] = s[i - 1] + s[i - 2];\n    }\n    return s;\n}\npub fn main() void {\n    var al: Allocator = c_allocator();\n    var fibs: []i64 = make_fibs(al, 8);\n    print(fibs[7]);\n    var sum: i64 = 0;\n    var i: usize = 0;\n    while (i < fibs.len) : (i += 1) {\n        sum = sum + fibs[i];\n    }\n    print(sum);\n    free(al, fibs);\n}\n",
        ),
        (
            "intern_order_sigs_before_bodies",
            "fn f() void { print(\"x\"); }\nfn g(v: []i64) usize { return v.len; }\npub fn main() void { f(); }\n",
        ),
        (
            "intern_order_params_then_ret_then_cont",
            "fn h(x: []i32) []i64 { return alloc(c_allocator(), i64, x.len); }\nfn cnt(x: usize) usize { return x; }\npub fn main() void {\n    var al: Allocator = c_allocator();\n    var i: usize = 0;\n    while (i < 3) : (i += cnt(\"ab\".len)) {\n        var v: []bool = alloc(al, bool, 1);\n        free(al, v);\n    }\n}\n",
        ),
        (
            "intern_order_alloc_after_count_arg",
            "pub fn main() void {\n    var al: Allocator = c_allocator();\n    var q = alloc(al, i64, \"x\".len);\n    print(q.len);\n    free(al, q);\n}\n",
        ),
        (
            "as_cast_zoo",
            "pub fn main() void {\n    var i: usize = 200;\n    var b: u8 = @as(u8, i);\n    var w: i64 = @as(i64, b) * 3;\n    var n: i32 = @as(i32, w);\n    var z: usize = @as(usize, n);\n    print(b);\n    print(w);\n    print(n);\n    print(z);\n    print(@as(i64, @as(u8, 300)));\n}\n",
        ),
        (
            "multi_elem_slices_and_writes",
            "pub fn main() void {\n    var al: Allocator = c_allocator();\n    var bs: []bool = alloc(al, bool, 2);\n    bs[0] = true;\n    bs[1] = !bs[0];\n    var us: []usize = alloc(al, usize, 2);\n    us[0] = bs.len;\n    us[1] += us[0];\n    if (bs[1]) { print(1); } else { print(@as(i64, us[1])); }\n    free(al, us);\n    free(al, bs);\n}\n",
        ),
        // -- the slicing view (v0.165) --------------------------------------------
        (
            "slicing_views_and_reslices",
            "fn head(s: []u8, n: usize) []u8 { return s[0..n]; }\npub fn main() void {\n    var al: Allocator = c_allocator();\n    var b: []u8 = alloc(al, u8, 5);\n    var i: usize = 0;\n    while (i < b.len) : (i += 1) {\n        b[i] = 65 + @as(u8, i);\n    }\n    print(b);\n    print(head(b, 3));\n    var mid: []u8 = b[1..4];\n    print(mid);\n    print(mid[2..3]);\n    var q: []i64 = alloc(al, i64, 4);\n    q[2] = 9;\n    var qv: []i64 = q[1..3];\n    print(qv[1]);\n    print(qv.len);\n    free(al, q);\n    free(al, b);\n}\n",
        ),
        (
            "slicing_string_literal_direct",
            "pub fn main() void {\n    print((\"kardashev\")[0..4]);\n    var e: []u8 = \"abc\"[1..1];\n    print(e.len);\n}\n",
        ),
        (
            "slicing_side_effect_free_ops_respliced",
            "fn lo2() usize { return 1; }\npub fn main() void {\n    var s: []u8 = \"abcdef\";\n    var t: []u8 = s[lo2()..s.len - 1];\n    print(t);\n    print(t.len);\n}\n",
        ),
        // -- fixed arrays [N]T + for (v0.168) -----------------------------------
        (
            "arrays_literals_reads_copies",
            "pub fn main() void {\n    var xs: [3]i64 = [3]i64{ 1, 2, 3 };\n    var ys: [3]i64 = xs;\n    ys[0] = 9;\n    print(xs[0]);\n    print(ys[0]);\n    print(xs.len);\n    var e: [0]u8 = [0]u8{};\n    print(e.len);\n    var one: [1]bool = [1]bool{ true };\n    if (one[0]) { print(1); }\n}\n",
        ),
        (
            "arrays_index_writes_bounds",
            "pub fn main() void {\n    var xs: [4]u8 = [4]u8{ 65, 66, 67, 68 };\n    xs[0] = 90;\n    xs[1] += 2;\n    xs[2] *= 2;\n    var i: i64 = 3;\n    xs[i] -= 1;\n    print(xs[0]); print(xs[1]); print(xs[2]); print(xs[3]);\n}\n",
        ),
        (
            "arrays_params_returns_byvalue",
            "fn sum(xs: [3]i64) i64 {\n    var t: i64 = 0;\n    for (xs) |x| { t += x; }\n    return t;\n}\nfn make(seed: i64) [3]i64 {\n    return [3]i64{ seed, seed + 1, seed + 2 };\n}\npub fn main() void {\n    var a: [3]i64 = make(10);\n    print(sum(a));\n    print(sum(make(1)));\n}\n",
        ),
        (
            "for_iterable_call_evaluated_once",
            "fn make() [2]i64 {\n    print(7);\n    return [2]i64{ 4, 6 };\n}\npub fn main() void {\n    var s: i64 = 0;\n    for (make()) |v| { s += v; }\n    print(s);\n}\n",
        ),
        (
            "for_index_form_and_nesting",
            "pub fn main() void {\n    var xs: [3]i64 = [3]i64{ 5, 6, 7 };\n    for (xs, 0..) |x, i| {\n        for (xs) |y| {\n            print(x + y);\n        }\n        print(i);\n    }\n}\n",
        ),
        (
            "for_defer_break_continue",
            "pub fn main() void {\n    var xs: [5]i64 = [5]i64{ 0, 1, 2, 3, 4 };\n    for (xs) |x| {\n        defer print(100 + x);\n        if (x == 1) { continue; }\n        if (x == 3) { break; }\n        print(x);\n    }\n    print(999);\n}\n",
        ),
        (
            "for_over_slice_and_array_view",
            "pub fn main() void {\n    var al: Allocator = c_allocator();\n    var s: []u8 = alloc(al, u8, 3);\n    s[0] = 10; s[1] = 20; s[2] = 30;\n    for (s) |b| { print(b); }\n    var xs: [4]i64 = [4]i64{ 1, 2, 3, 4 };\n    var v: []i64 = xs[1..3];\n    for (v, 0..) |x, i| { print(x); print(i); }\n    for (\"ab\") |c| { print(c); }\n    free(al, s);\n}\n",
        ),
        (
            "arrays_typedef_order_and_empty_view",
            "pub fn main() void {\n    var al: Allocator = c_allocator();\n    var h: []i64 = alloc(al, i64, 1);\n    var xs: [2]u8 = [2]u8{ 1, 2 };\n    var v: []u8 = xs[1..1];\n    print(v.len);\n    print(h.len);\n    print(xs[1]);\n    free(al, h);\n}\n",
        ),
        // -- SKIP verdict positions on tricky shapes ---------------------------
        (
            "skip_slice_elem_f64",
            "pub fn main() void {\n    var al: Allocator = c_allocator();\n    var s: []f64 = alloc(al, f64, 2);\n}\n",
        ),
        (
            "skip_as_wrong_shape",
            "pub fn main() void {\n    var x: i64 = 1;\n    print(@as(f64, x));\n}\n",
        ),
        (
            "skip_place_chain_through_index",
            "pub fn main() void {\n    var a: Allocator = c_allocator();\n    var s: []u8 = alloc(a, u8, 2);\n    s[0].f = 1;\n}\n",
        ),
        (
            "skip_alloc_wrong_arity",
            "pub fn main() void {\n    var a: Allocator = c_allocator();\n    var s: []u8 = alloc(a, u8);\n}\n",
        ),
        (
            "skip_alloc_elem_not_scalar",
            "pub fn main() void {\n    var a: Allocator = c_allocator();\n    var s = alloc(a, Allocator, 3);\n}\n",
        ),
        (
            "skip_float_deep_in_defer",
            "pub fn main() void {\n    defer {\n        var x: i64 = 1;\n        print(x + 1.5);\n    }\n}\n",
        ),
        (
            "skip_slice_of_f64",
            "pub fn main() void {\n    if (true) {\n        var s: []f64 = q();\n    }\n}\n",
        ),
        (
            "skip_field_not_len",
            "pub fn main() void {\n    var s: []u8 = \"x\";\n    print(s.ptr);\n}\n",
        ),
        ("skip_nomain", "fn helper() void {}\n"),
        ("skip_empty_module", ""),
        (
            "skip_alloc_call",
            "pub fn main() void {\n    var n: i64 = 4;\n    free(a, alloc(a, f64, n));\n}\n",
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
            "skip_array_size_param",
            "fn f(xs: [n]i64) void {}\npub fn main() void {}\n",
        ),
        (
            "skip_array_elem_f64",
            "pub fn main() void {\n    var xs: [2]f64 = [2]f64{ 1.5, 2.5 };\n}\n",
        ),
        (
            "skip_array_lit_elem_float",
            "pub fn main() void {\n    var xs: [2]i64 = [2]i64{ 1, 2.5 };\n}\n",
        ),
        (
            "skip_labeled_for",
            "pub fn main() void {\n    var xs: [1]i64 = [1]i64{ 1 };\n    lab: for (xs) |x| {\n        break :lab;\n    }\n}\n",
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
        // Every targeted case is checked in BOTH modes (v0.166): the same
        // source must byte-agree as a Program lowering and as a Test-mode
        // harness (with its distinct liveness and expect lowering).
        for mode in [EmitMode::Program, EmitMode::Test] {
            let expected = rust_expected(&input, src, mode);
            if let Expected::SemaInvalid(code) = &expected {
                failures.push(format!(
                    "[{tag} {mode:?}] targeted input is sema-invalid ({code}) — every case must classify as ERROR, SKIP or valid C"
                ));
                continue;
            }
            if let Err(msg) = diff_one(&exe, &input, &expected, mode) {
                failures.push(format!("[{tag}] {msg}"));
            }
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

/// (b2) Multi-file import fixtures (v0.167): each case builds a fresh temp
/// directory of `.ks` files and diffs the ROOT in both modes — flatten
/// order, diamond dedup, back references, `..` paths, import-at-end,
/// cycles (E0292), missing files (E0291), duplicate names (E0293),
/// wrapped sub-file errors (E0294), and root/nested `std` imports (SKIP).
#[test]
fn selfhost_emit_differential_import_fixtures() {
    let exe = build_cdump();
    // (tag, &[(filename, source)], root-filename)
    let cases: &[(&str, &[(&str, &str)], &str)] = &[
        (
            "flatten_chain_and_values",
            &[
                ("c.ks", "pub fn cv() i64 { return 7; }\n"),
                ("b.ks", "@import(\"c.ks\");\npub fn bv() i64 { return cv() * 2; }\n"),
                ("a.ks", "@import(\"b.ks\");\npub fn main() void { print(bv() + cv()); }\n"),
            ],
            "a.ks",
        ),
        (
            "diamond_dedup_once",
            &[
                ("d.ks", "pub fn fd() i64 { return 4; }\n"),
                ("l.ks", "@import(\"d.ks\");\npub fn fl() i64 { return fd() + 1; }\n"),
                ("r.ks", "@import(\"d.ks\");\npub fn fr() i64 { return fd() + 2; }\n"),
                ("a.ks", "@import(\"l.ks\");\n@import(\"r.ks\");\npub fn main() void { print(fl() + fr()); }\n"),
            ],
            "a.ks",
        ),
        (
            "back_reference_and_import_at_end",
            &[
                ("helper.ks", "pub fn helper() i64 { return root_fn() + 1; }\n"),
                ("a.ks", "pub fn root_fn() i64 { return 10; }\npub fn main() void { print(helper()); }\n@import(\"helper.ks\");\n"),
            ],
            "a.ks",
        ),
        (
            "parent_relative_paths",
            &[
                ("shared.ks", "pub fn sv() i64 { return 14; }\n"),
                ("sub/child.ks", "@import(\"../shared.ks\");\npub fn cvv() i64 { return sv() * 3; }\n"),
                ("a.ks", "@import(\"sub/child.ks\");\npub fn main() void { print(cvv()); }\n"),
            ],
            "a.ks",
        ),
        (
            "imports_with_tests_both_modes",
            &[
                ("util.ks", "pub fn twice(x: i64) i64 { return x * 2; }\ntest \"imported test\" { expect(twice(2) == 4); }\n"),
                ("a.ks", "@import(\"util.ks\");\ntest \"root test\" { expect(twice(3) == 6); }\npub fn main() void { print(twice(5)); }\n"),
            ],
            "a.ks",
        ),
        (
            "cycle_pair_e0292",
            &[
                ("a.ks", "@import(\"b.ks\");\nfn fa() void { }\npub fn main() void { }\n"),
                ("b.ks", "@import(\"a.ks\");\nfn fb() void { }\n"),
            ],
            "a.ks",
        ),
        (
            "cycle_self_e0292",
            &[("a.ks", "@import(\"a.ks\");\npub fn main() void { }\n")],
            "a.ks",
        ),
        (
            "missing_import_e0291",
            &[("a.ks", "@import(\"nope.ks\");\npub fn main() void { }\n")],
            "a.ks",
        ),
        (
            "empty_import_is_e0291_by_contract",
            &[
                ("empty.ks", ""),
                ("a.ks", "@import(\"empty.ks\");\npub fn main() void { }\n"),
            ],
            "a.ks",
        ),
        (
            "duplicate_name_e0293_position",
            &[
                ("one.ks", "fn shared() void { }\n"),
                ("a.ks", "@import(\"one.ks\");\nfn shared() void { }\npub fn main() void { }\n"),
            ],
            "a.ks",
        ),
        (
            "subfile_parse_error_e0294",
            &[
                ("bad.ks", "fn oops( {\n"),
                ("a.ks", "@import(\"bad.ks\");\npub fn main() void { }\n"),
            ],
            "a.ks",
        ),
        (
            "subfile_lex_error_e0294",
            &[
                ("bad.ks", "const X = \"unterminated;\n"),
                ("a.ks", "@import(\"bad.ks\");\npub fn main() void { }\n"),
            ],
            "a.ks",
        ),
        (
            "std_import_root_skip",
            &[("a.ks", "@import(\"std\");\npub fn main() void { print(1); }\n")],
            "a.ks",
        ),
        (
            "std_import_nested_skip_rebased",
            &[
                ("mid.ks", "pub fn mv() i64 { return 1; }\n@import(\"std\");\n"),
                ("a.ks", "@import(\"mid.ks\");\npub fn main() void { print(mv()); }\n"),
            ],
            "a.ks",
        ),
    ];

    let mut failures: Vec<String> = Vec::new();
    for (tag, files, root_name) in cases {
        let dir = temp_path(&format!("imp_{tag}"));
        std::fs::create_dir_all(&dir).expect("create fixture dir");
        for (name, src) in *files {
            let p = dir.join(name);
            if let Some(parent) = p.parent() {
                std::fs::create_dir_all(parent).expect("create fixture subdir");
            }
            std::fs::write(&p, src).expect("write fixture file");
        }
        let root = dir.join(root_name);
        for mode in [EmitMode::Program, EmitMode::Test] {
            let expected = rust_expected(&root, "", mode);
            if let Expected::SemaInvalid(code) = &expected {
                failures.push(format!(
                    "[{tag} {mode:?}] fixture is sema-invalid ({code}) — every case must classify as ERROR, SKIP or valid C"
                ));
                continue;
            }
            if let Err(msg) = diff_one(&exe, &root, &expected, mode) {
                failures.push(format!("[{tag}] {msg}"));
            }
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
    let _ = std::fs::remove_file(&exe);
    assert!(
        failures.is_empty(),
        "{} import fixtures mismatched:\n{}",
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
