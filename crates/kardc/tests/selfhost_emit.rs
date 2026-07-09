//! Self-host stages 3–23 (v0.161–v0.180): differential test of
//! `selfhost/emit.ks` — a C emitter for the SCALAR + STRING + HEAP-BUFFER
//! SUBSET (with generalized `[]T` slices, `@as` casts, the `s[lo..hi]`
//! slicing view, `test` blocks with the full `EmitMode::Test` harness,
//! `@import` resolution, fixed arrays `[N]T` with array literals and
//! `for` loops, and — v0.169 — plain data STRUCTS: declarations, literals,
//! field reads, generalized place-assignment chains through fields and
//! indexes with the `_at` element-pointer lowering, the typedef
//! DEPENDENCY WALK over structs/arrays/slices, and — v0.170 — struct
//! METHODS and associated functions: `kd_<Struct>_<method>` lowering for
//! the receiver, explicit-self and `Type.assoc(…)` call forms, with
//! name-level method liveness — and, v0.171, ENUMS: declarations with
//! explicit values + C auto-increment, qualified `Enum.V` literals, enum
//! equality, and the `@intFromEnum`/`@enumFromInt` conversions — and,
//! v0.172, `switch` with enum/integer scrutinees, multi-label arms, GNU
//! range cases, exhaustiveness-aware divergence, plus CONTEXTUAL `.V`
//! literals through the expected-type coercion plumbing at every site:
//! let/assign/place-assign/return/call args/method args/struct-literal
//! fields/array elements/binary siblings — and, v0.173, OPTIONALS `?T`:
//! `null`/`T` widening through the same plumbing, `orelse`, `.?` unwrap,
//! `if (opt) |v|` captures, `kd_opt_<tag>` typedefs with `_orelse` /
//! `_unwrap` helpers seeded between structs and arrays — and, v0.174,
//! ERROR UNIONS `!T`: the GLOBAL 1-based error-code table (set members
//! then body-order `error.X` literals), `kd_err_<tag>` typedefs (+
//! `_catch`; the payload-less `!void` variant), `try` propagation with
//! errdefer-inclusive flushes, both `catch` forms, `errdefer`, and named
//! error sets whose membership stays sema's concern — and, v0.175,
//! POINTERS `*T`: the written-`*T` pre-pass registry with its
//! miss-untypeable `&place` mirror, `p.*` reads/writes, field/method
//! auto-deref through `*Struct`, and pointer receivers with the
//! auto-ref/deref call matrix — and, v0.176, LABELED LOOPS: `lab:`
//! while/for with `goto __kd_brk_L` / `__kd_cont_L` lowering, the
//! label-targeted defer flushes, and the diverged-body clause rule —
//! and, v0.177, F64: correctly-rounded literal parsing (big-integer
//! exact division for any digit count) and the `{:?}` shortest-nearest
//! formatting mirror with the Debug exponent thresholds
//! — and, v0.178, GENERIC FUNCTIONS (SPEC §17 + §24): comptime type
//! params (`comptime T: type`) and comptime value params over bare
//! subset-int annotations, with full monomorphisation — the intern replay
//! mirrors `check_generic_call` (comptime args, then runtime param types +
//! return UNDER the inner substitution, then runtime args under the OUTER,
//! then a new instantiation's recursive instance-body walk), instances
//! emit as `kd_<fn>__<mangles>` (a negative value arg mangles `m<digits>`
//! — the v0.178 fix; `-` broke the C identifier), every recorded instance
//! emits regardless of liveness, every generic body is an always-walked
//! §43.1 name source, `[n]T` sizes resolve through the value substitution,
//! and the detector admits generic calls whose type arguments name subset
//! scalars / declared structs+enums / bound type params (a method's
//! comptime param stays `generic-param`) — and, v0.179, GENERIC STRUCTS
//! (SPEC §25/§26/§31/§42): type-constructors (`fn Name(comptime T: type)
//! type { return struct { … }; }` — params all comptime-type or
//! `generic-param`; a non-conforming body walks as plain statements,
//! sema's E0310), `const Alias = Ctor(…);` aliases, direct applications
//! `Name(A, …)` in every type position and as assoc-call receivers
//! (arguments: admissible bare names or nested applications), instance
//! METHODS under `{ params → args, Self → instance }` (emitted per
//! recorded instance, liveness notwithstanding; an instantiated ctor's
//! body is an always-walked §43.1 name source), `Self`/`@This()` in
//! plain-struct methods (§32.2), `alloc(a, T, n)` over a ctor-bound `T`,
//! and instance names (`Ctor__<tags>`) memoised across every spelling —
//! and, v0.180, EVERY INTEGER WIDTH (i8/i16/u16/u32/u64 join the subset's
//! scalars everywhere a scalar may appear: bare, slice/array elements,
//! `alloc`/`@as` arguments, generic type/value-param annotations; the
//! §28.4 sub-`int` trunc-back set grows to i8/i16/u16) —
//! written
//! in kardashev —
//! against the Rust
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
use std::collections::HashSet;
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
    "tests/spec/s03_sema/unary_operand_rules_err.ks",                 // E0110 (v0.180: int widths subset-shaped)
    "tests/spec/s03_sema/usize_distinct_from_u64_err.ks",             // E0110 (v0.180)
    "tests/spec/s28_bitwise/shift_same_type_err.ks",                  // E0110 (v0.180)
    "tests/spec/s03_sema/void_result_unusable_err.ks",                // E0110
    "tests/spec/s09_structs/err_duplicate_field_decl.ks",             // E0162
    "tests/spec/s09_structs/err_field_access_on_non_struct.ks",       // E0165
    "tests/spec/s09_structs/err_field_assign_non_var_root.ks",        // E0167
    "tests/spec/s09_structs/err_field_assign_type_mismatch.ks",       // E0110
    "tests/spec/s09_structs/err_forward_or_cyclic_field.ks",          // E0160
    "tests/spec/s09_structs/err_literal_duplicate_init.ks",           // E0164
    "tests/spec/s09_structs/err_literal_extra_field.ks",              // E0164
    "tests/spec/s09_structs/err_literal_field_type_mismatch.ks",      // E0110
    "tests/spec/s09_structs/err_literal_missing_field.ks",            // E0164
    "tests/spec/s09_structs/err_literal_of_non_struct.ks",            // E0163
    "tests/spec/s09_structs/err_nominal_typing.ks",                   // E0110
    "tests/spec/s09_structs/err_print_struct.ks",                     // E0110
    "tests/spec/s09_structs/err_struct_equality.ks",                  // E0110
    "tests/spec/s09_structs/err_unknown_field_access.ks",             // E0166
    "tests/spec/s10_methods/err_assoc_fn_on_value.ks",                // E0172
    "tests/spec/s10_methods/err_method_arg_type_mismatch.ks",         // E0110
    "tests/spec/s10_methods/err_method_arity.ks",                     // E0171
    "tests/spec/s10_methods/err_static_call_missing_self.ks",         // E0171
    "tests/spec/s10_methods/err_unknown_assoc_fn.ks",                 // E0173
    "tests/spec/s10_methods/err_unknown_method.ks",                   // E0171
    "tests/spec/s11_optionals/if_capture_non_optional_err.ks",        // E0280
    "tests/spec/s11_optionals/null_without_expected_err.ks",          // E0180
    "tests/spec/s11_optionals/orelse_lhs_not_optional_err.ks",        // E0181
    "tests/spec/s11_optionals/orelse_rhs_mismatch_err.ks",            // E0110
    "tests/spec/s11_optionals/unwrap_non_optional_err.ks",            // E0182
    "tests/spec/s11_optionals/widen_mismatch_err.ks",                 // E0110
    "tests/spec/s12_errunions/catch_non_error_union_err.ks",          // E0195
    "tests/spec/s12_errunions/named_set_membership_err.ks",           // E0330
    "tests/spec/s12_errunions/try_nested_position_err.ks",            // E0191
    "tests/spec/s12_errunions/try_on_non_error_union_err.ks",         // E0192
    "tests/spec/s12_errunions/try_outside_error_fn_err.ks",           // E0190
    "tests/spec/s13_enums/bool_scrutinee_err.ks",                     // E0213
    "tests/spec/s13_enums/dup_switch_label_enum_err.ks",              // E0211
    "tests/spec/s13_enums/dup_switch_label_int_err.ks",               // E0211
    "tests/spec/s13_enums/dup_variant_decl_err.ks",                   // E0211
    "tests/spec/s13_enums/enum_lit_no_context_err.ks",                // E0215
    "tests/spec/s13_enums/int_switch_no_else_err.ks",                 // E0214
    "tests/spec/s13_enums/nonexhaustive_switch_err.ks",               // E0210
    "tests/spec/s13_enums/range_label_on_enum_err.ks",                // E0212
    "tests/spec/s13_enums/unknown_variant_label_err.ks",              // E0212
    "tests/spec/s13_enums/unknown_variant_qualified_err.ks",          // E0212
    "tests/spec/s14_arrays/index_assign_const_err.ks",                // E0223
    "tests/spec/s14_arrays/index_assign_param_err.ks",                // E0223
    "tests/spec/s14_arrays/index_non_array_err.ks",                   // E0220
    "tests/spec/s14_arrays/index_not_integer_err.ks",                 // E0110
    "tests/spec/s14_arrays/literal_count_mismatch_err.ks",            // E0221
    "tests/spec/s14_arrays/literal_element_type_err.ks",              // E0110
    "tests/spec/s14_arrays/negative_array_len_err.ks",                // E0224 (v0.178: `[n]T` subset-shaped)
    "tests/spec/s17_generics/err_too_few_type_args.ks",               // E0252 (v0.178)
    "tests/spec/s17_generics/err_type_arg_not_identifier.ks",         // E0251 (v0.178)
    "tests/spec/s24_comptime_vals/err_value_arg_bool.ks",             // E0253 (v0.178)
    "tests/spec/s24_comptime_vals/err_value_arg_runtime_var.ks",      // E0253 (v0.178)
    "tests/spec/s15_ptr_slices/slice_non_sliceable_err.ks",           // E0232
    "tests/spec/s16_alloc/free_non_slice_err.ks",                     // E0242
    "tests/spec/s18_inference/infer_const_stays_immutable_err.ks",    // E0110
    "tests/spec/s18_inference/infer_default_not_i32_err.ks",          // E0110
    "tests/spec/s23_strings/string_eq_operator_err.ks",               // E0110
    "tests/spec/s23_strings/string_print_non_u8_slice_err.ks",        // E0110
    "tests/spec/s23_strings/string_plus_operator_err.ks",             // E0110
    "tests/spec/s25_generic_structs/err_alias_of_non_ctor.ks",        // E0311
    "tests/spec/s25_generic_structs/err_alias_arg_not_ident.ks",      // E0311 (v0.179: ctor calls subset-shaped)
    "tests/spec/s25_generic_structs/err_alias_arity.ks",              // E0311 (v0.179)
    "tests/spec/s25_generic_structs/err_alias_as_value.ks",           // E0100 (v0.179)
    "tests/spec/s25_generic_structs/err_distinct_args_distinct_types.ks", // E0110 (v0.179)
    "tests/spec/s31_multi_typeparams/err_ctor_arity_mismatch.ks",     // E0311 (v0.179)
    "tests/spec/s31_multi_typeparams/err_distinct_tuples_type_mismatch.ks", // E0110 (v0.179)
    "tests/spec/s42_direct_generics/err_app_generic_fn_type_arg_deferred.ks", // E0251 (v0.179)
    "tests/spec/s42_direct_generics/err_e0311_arity_type_pos.ks",     // E0311 (v0.179)
    "tests/spec/s42_direct_generics/err_e0312_app_as_value.ks",       // E0312 (v0.179)
    "tests/spec/s27_compound/bool_rhs_err.ks",                        // E0110
    "tests/spec/s27_compound/const_place_err.ks",                     // E0110
    "tests/spec/s27_compound/mismatch_err.ks",                        // E0110
    "tests/spec/s37_enum_values/enum_from_int_first_arg_err.ks",      // E0321
    "tests/spec/s37_enum_values/enum_from_int_value_not_integer_err.ks", // E0321
    "tests/spec/s37_enum_values/int_from_enum_non_enum_err.ks",       // E0321
    "tests/spec/s15_ptr_slices/addr_of_non_lvalue_err.ks",            // E0231
    "tests/spec/s15_ptr_slices/deref_non_pointer_err.ks",             // E0230
    "tests/spec/s15_ptr_slices/err_addr_of_const_binding.ks",         // E0233
    "tests/spec/s18_inference/infer_enum_lit_err.ks",                 // E0215
    "tests/spec/s18_inference/infer_error_lit_err.ks",                // E0193
    "tests/spec/s18_inference/infer_null_err.ks",                     // E0180
    "tests/spec/s30_ptr_receivers/err_const_receiver_mutation.ks",    // E0233
    "tests/spec/s30_ptr_receivers/err_temp_receiver_autoref.ks",      // E0231
    "tests/spec/s34_error_sets/cross_set_nonmember_err.ks",           // E0330
    "tests/spec/s38_floats/float_const_err.ks",                       // E0134
    "tests/spec/s38_floats/mix_int_float_err.ks",                     // E0110
    "tests/spec/s38_floats/rem_f64_err.ks",                           // E0110
    "tests/spec/s40_labeled/unknown_label_err.ks",                    // E0301
    "tests/spec/s34_error_sets/dup_member_err.ks",                    // E0331
    "tests/spec/s34_error_sets/init_site_nonmember_err.ks",           // E0330
    "tests/spec/s34_error_sets/unknown_set_name_err.ks",              // E0331
    "tests/spec/s36_catch/capture_default_type_mismatch_err.ks",      // E0110
    "tests/spec/s36_catch/capture_out_of_scope_err.ks",               // E0100
    "tests/spec/s36_catch/capture_requires_error_union_err.ks",       // E0195
    "tests/spec/s21_captures/err_capture_non_optional.ks",            // E0280
    "tests/spec/s21_captures/err_capture_not_visible_in_else.ks",     // E0100
    "tests/spec/s39_switch_ranges/else_required_with_ranges_err.ks",  // E0214
    "tests/spec/s39_switch_ranges/overlapping_ranges_err.ks",         // E0211
    "tests/spec/s27_compound/f64_place_err.ks",                       // E0110
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

const MIN_C_COMPARED_PROGRAM: usize = 379;
const MIN_C_COMPARED_TEST: usize = 398;

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
        "i32" | "i64" | "bool" | "void" | "u8" | "usize" | "f64" | "Allocator"
            | "i8" | "i16" | "u16" | "u32" | "u64"
    )
}

/// The subset slice ELEMENT spellings (`[]T` and `alloc(a, T, n)`, v0.164).
fn subset_slice_elem(name: &str) -> bool {
    matches!(
        name,
        "i32" | "i64" | "bool" | "u8" | "usize" | "f64" | "i8" | "i16" | "u16" | "u32" | "u64"
    )
}

/// The detector's name context (v0.178): the declared struct/enum names,
/// the top-level GENERIC-fn registry (any comptime param, type-constructors
/// excluded — first declaration wins, duplicates are sema's E0103), and the
/// ENCLOSING top-level generic's bound comptime param names — `tp` (type
/// params) and `vp` (value params), both EMPTY outside a top-level fn and
/// inside struct methods. Mirrors `Det`'s walk-by-need lookups.
struct Cx<'a> {
    sn: &'a HashSet<String>,
    gens: &'a std::collections::HashMap<String, &'a Func>,
    /// The top-level TYPE-CONSTRUCTOR registry (bare-`type` return, v0.179).
    tc: &'a std::collections::HashMap<String, &'a Func>,
    /// Type-ALIAS names — `const Alias = Ctor(…);` items (v0.179).
    an: &'a HashSet<String>,
    tp: HashSet<String>,
    vp: HashSet<String>,
    /// The enclosing type-constructor's params (inside its struct body).
    ctp: HashSet<String>,
    /// Whether `Self`/`@This()` is admissible — inside any struct method
    /// (plain §32.2 or generic-struct §26.1).
    dself: bool,
}

/// A bare base name in a general type position (v0.179): a subset scalar,
/// a declared struct/enum, a bound type param (fn or ctor), an alias, or
/// a method's `Self`.
fn base_name_ok(cx: &Cx, name: &str) -> bool {
    subset_type_name(name)
        || cx.sn.contains(name)
        || cx.tp.contains(name)
        || cx.ctp.contains(name)
        || cx.an.contains(name)
        || (cx.dself && name == "Self")
}

/// The slice/array ELEMENT variant: scalars restrict to the slice-element
/// set; named types as `base_name_ok`.
fn elem_name_ok(cx: &Cx, name: &str) -> bool {
    if subset_type_name(name) {
        return subset_slice_elem(name);
    }
    cx.sn.contains(name)
        || cx.tp.contains(name)
        || cx.ctp.contains(name)
        || cx.an.contains(name)
        || (cx.dself && name == "Self")
}

/// A type-position APPLICATION `Name(A, …)` (v0.179, SPEC §42): the name
/// must be a type-constructor (`type-form` otherwise); each argument — a
/// bare name or a nested application — checks recursively. Arity is
/// sema's E0311, never subset membership.
fn det_app(cx: &Cx, t: &TypeExpr) -> Option<Hit> {
    if !cx.tc.contains_key(&t.name) {
        return Some(("type-form", t.span.start));
    }
    if let Some(args) = &t.ctor_args {
        for a in args {
            if a.ctor_args.is_some() {
                if let Some(h) = det_app(cx, a) {
                    return Some(h);
                }
            } else if !base_name_ok(cx, &a.name) {
                return Some(("type-name", a.span.start));
            }
        }
    }
    None
}

/// Sema's `is_type_kw` (SPEC §17.1): the annotation is the bare `type`
/// keyword — no composite form bits.
fn ty_is_type_kw(t: &TypeExpr) -> bool {
    t.name == "type"
        && !t.optional
        && !t.error_union
        && t.array_len.is_none()
        && !t.pointer
        && !t.slice
}

/// A type reference: any composite form other than a slice or a
/// literal-length array is out; `[n]T` requires a bound comptime VALUE
/// param (v0.178); a bare base name must be a subset spelling, a declared
/// struct/enum, or a bound type param (`@This()` parses to the synthesized
/// name `Self`, which is none of those — the selfhost side reports its
/// `F_THIS` flag identically, sliced or not).
fn det_type(cx: &Cx, t: &TypeExpr) -> Option<Hit> {
    if let Some(kardc::ast::ArraySize::Param(name)) = &t.array_len {
        // `[n]T` (v0.178): in the subset iff `n` names a comptime VALUE
        // parameter of the enclosing top-level generic; the element rule
        // matches `[N]T`.
        if !cx.vp.contains(name) {
            return Some(("type-form", t.span.start));
        }
        if t.ctor_args.is_some() {
            return det_app(cx, t);
        }
        if !elem_name_ok(cx, &t.name) {
            return Some(("type-name", t.span.start));
        }
        return None;
    }
    if t.error_union && t.error_set.as_deref() == Some("Self") {
        // A `Self`-referencing set spelling stays a type-form.
        return Some(("type-form", t.span.start));
    }
    if t.ctor_args.is_some() {
        // A direct application `Name(A, …)` (v0.179, SPEC §42) — every
        // prefix wrapper composes over it, so the base check is the
        // whole check.
        return det_app(cx, t);
    }
    if t.pointer {
        // `*T` over a bare subset pointee name (v0.175); `*Self` /
        // `*@This()` are method receivers (v0.179).
        if !base_name_ok(cx, &t.name) {
            return Some(("type-name", t.span.start));
        }
        return None;
    }
    if t.error_union {
        // `!T` / `Set!T` over a bare subset payload name (v0.174; the set
        // name is sema's E0330 membership concern).
        if !base_name_ok(cx, &t.name) {
            return Some(("type-name", t.span.start));
        }
        return None;
    }
    if t.error_set.is_some() {
        return Some(("type-form", t.span.start));
    }
    if t.optional {
        // `?T` over a bare subset name (v0.173; a composite inner is a
        // parse error, so `?` never coexists with the other forms).
        if !base_name_ok(cx, &t.name) {
            return Some(("type-name", t.span.start));
        }
        return None;
    }
    if t.array_len.is_some() || t.slice {
        // `[N]T` (v0.168) / `[]T` (v0.164) over the five scalar elements,
        // a declared struct element (v0.169), a bound type param (v0.178),
        // an alias, or a method's `Self` (v0.179).
        if !elem_name_ok(cx, &t.name) {
            return Some(("type-name", t.span.start));
        }
        return None;
    }
    if !base_name_ok(cx, &t.name) {
        return Some(("type-name", t.span.start));
    }
    None
}

fn det_expr(cx: &Cx, e: &Expr) -> Option<Hit> {
    let pos = e.span().start;
    match e {
        Expr::Int { .. } | Expr::Bool { .. } | Expr::Ident { .. } => None,
        Expr::Unary { expr, .. } => det_expr(cx, expr),
        Expr::Binary { lhs, rhs, .. } => det_expr(cx, lhs).or_else(|| det_expr(cx, rhs)),
        Expr::Call { callee, args, .. } => {
            // `alloc(a, T, n)` is in the subset (v0.163, elements
            // generalized in v0.164; a bound type param since v0.178) —
            // exactly three arguments with a scalar element type; any
            // other shape is out. `free(a, s)` / `c_allocator()` walk
            // their arguments like ordinary calls.
            if callee == "alloc" {
                let shaped = args.len() == 3
                    && matches!(&args[1], Expr::Ident { name, .. }
                        if subset_slice_elem(name) || cx.tp.contains(name) || cx.ctp.contains(name));
                if !shaped {
                    return Some(("builtin-call", pos));
                }
                return args.iter().find_map(|x| det_expr(cx, x));
            }
            if cx.tc.contains_key(callee.as_str()) {
                // A type-constructor application in VALUE position
                // (v0.179): the associated-call receiver, an alias
                // initializer, or a stray value use (sema's E0312). The
                // arguments are TYPE arguments — an identifier must name
                // an admissible base; a nested constructor call recurses
                // through this same branch; anything else walks as an
                // expression (sema's E0311).
                for a in args {
                    if let Expr::Ident { name, span } = a {
                        if !base_name_ok(cx, name) {
                            return Some(("type-name", span.start));
                        }
                    } else if let Some(h) = det_expr(cx, a) {
                        return Some(h);
                    }
                }
                return None;
            }
            if let Some(g) = cx.gens.get(callee.as_str()) {
                // A call to a top-level GENERIC fn (v0.178): the leading
                // comptime arguments check per parameter kind — a TYPE
                // argument must be an identifier naming a subset scalar,
                // a declared struct/enum, or a bound type param (any
                // other name is `type-name` at the argument; a
                // non-identifier walks as an expression — sema's E0251);
                // a VALUE argument walks as an ordinary expression
                // (const-ness is sema's E0253). The remaining runtime
                // arguments walk in order; fewer args than comptime
                // params is sema's E0252.
                let mut ai = 0usize;
                for p in g.params.iter().filter(|p| p.is_comptime) {
                    if ai >= args.len() {
                        break;
                    }
                    if ty_is_type_kw(&p.ty) {
                        if let Expr::Ident { name, .. } = &args[ai] {
                            // An alias or a method's `Self` also names a
                            // concrete type here (v0.179 — the subst →
                            // `resolve_base` chain, aliases included).
                            if !base_name_ok(cx, name) {
                                return Some(("type-name", args[ai].span().start));
                            }
                        } else if let Some(hit) = det_expr(cx, &args[ai]) {
                            return Some(hit);
                        }
                    } else if let Some(hit) = det_expr(cx, &args[ai]) {
                        return Some(hit);
                    }
                    ai += 1;
                }
                return args[ai..].iter().find_map(|x| det_expr(cx, x));
            }
            args.iter().find_map(|x| det_expr(cx, x))
        }
        Expr::Comptime { expr, .. } => det_expr(cx, expr),
        // A string literal is in the subset (v0.162).
        Expr::StrLit { .. } => None,
        // Field access is in the subset (v0.169: struct fields; `.len`
        // since v0.162) — only the base walks, names are sema's business.
        Expr::Field { base, .. } => det_expr(cx, base),
        // A read index `s[i]` is in the subset (v0.162).
        Expr::Index { base, index, .. } => {
            det_expr(cx, base).or_else(|| det_expr(cx, index))
        }
        // A float literal is in the subset (v0.177).
        Expr::Float { .. } => None,
        // `@as(T, e)` is in the subset (v0.164): exactly two arguments, the
        // first an identifier naming a subset type or a bound type param
        // (v0.178); only the VALUE argument is walked. Every other
        // `@`-builtin stays out.
        Expr::Builtin { name, args, .. } => {
            if name == "as"
                && args.len() == 2
                && matches!(&args[0], Expr::Ident { name, .. }
                    if subset_type_name(name) || cx.tp.contains(name) || cx.ctp.contains(name))
            {
                return det_expr(cx, &args[1]);
            }
            // `@intFromEnum(e)` — exactly one argument, walked (v0.171).
            if name == "intFromEnum" && args.len() == 1 {
                return det_expr(cx, &args[0]);
            }
            // `@enumFromInt(E, n)` — exactly two, the first an identifier
            // (ANY name; a non-enum is sema's E0321); only `n` walks.
            if name == "enumFromInt"
                && args.len() == 2
                && matches!(&args[0], Expr::Ident { .. })
            {
                return det_expr(cx, &args[1]);
            }
            Some(("builtin", pos))
        }
        // A struct literal `Name{ .f = e, … }` is in the subset (v0.169):
        // the initializer values walk in source order.
        Expr::StructLit { fields, .. } => {
            fields.iter().find_map(|fi| det_expr(cx, &fi.value))
        }
        Expr::StructType { .. } => Some(("struct-type", pos)),
        // A method / associated call is in the subset (v0.170): the
        // receiver walks, then the arguments in order.
        Expr::MethodCall { receiver, args, .. } => {
            det_expr(cx, receiver).or_else(|| args.iter().find_map(|x| det_expr(cx, x)))
        }
        // `null`, `orelse` and `.?` are in the subset (v0.173).
        Expr::Null { .. } => None,
        Expr::Orelse { lhs, rhs, .. } => det_expr(cx, lhs).or_else(|| det_expr(cx, rhs)),
        Expr::Unwrap { expr, .. } => det_expr(cx, expr),
        // `error.X` is in the subset (v0.174): its `!T` comes from the
        // expected-type context (no context is sema's E0193).
        Expr::ErrorLit { .. } => None,
        // An unqualified `.V` is in the subset (v0.172): its enum comes
        // from the expected-type context (no-context = sema's E0215).
        Expr::EnumLit { .. } => None,
        // An array literal `[N]T{ … }` is in the subset (v0.168): its
        // `[N]T` reference, then the elements, in order.
        Expr::ArrayLit { elem, elems, .. } => {
            det_type(cx, elem).or_else(|| elems.iter().find_map(|x| det_expr(cx, x)))
        }
        // The slicing view `base[lo..hi]` is in the subset (v0.165).
        Expr::SliceExpr { base, lo, hi, .. } => det_expr(cx, base)
            .or_else(|| det_expr(cx, lo))
            .or_else(|| det_expr(cx, hi)),
        // `&place` and `p.*` are in the subset (v0.175).
        Expr::AddrOf { place, .. } => det_expr(cx, place),
        Expr::Deref { expr, .. } => det_expr(cx, expr),
        // `try e` and both `catch` forms are in the subset (v0.174).
        Expr::Try { expr, .. } => det_expr(cx, expr),
        Expr::Catch { expr, default, .. } => {
            det_expr(cx, expr).or_else(|| det_expr(cx, default))
        }
        Expr::Unreachable { .. } => Some(("unreachable", pos)),
    }
}

fn det_block(cx: &Cx, b: &kardc::ast::Block) -> Option<Hit> {
    b.stmts.iter().find_map(|x| det_stmt(cx, x))
}

fn det_stmt(cx: &Cx, s: &Stmt) -> Option<Hit> {
    let pos = s.span().start;
    match s {
        Stmt::Let { ty, value, .. } => ty
            .as_ref()
            .and_then(|t| det_type(cx, t))
            .or_else(|| det_expr(cx, value)),
        Stmt::Assign { value, .. } => det_expr(cx, value),
        // A place-assignment over any FIELD/INDEX chain rooted at a NAME
        // is in the subset (v0.169; v0.163 admitted the direct index
        // write). Bases descend first, each index expression where it
        // sits, then the value; a non-name root stays out.
        Stmt::FieldAssign { place, value, .. } => {
            if !place_rooted_in_name(place) {
                return Some(("place-assign", pos));
            }
            det_place(cx, place).or_else(|| det_expr(cx, value))
        }
        Stmt::Expr(e) => det_expr(cx, e),
        Stmt::Return { value, .. } => value.as_ref().and_then(|v| det_expr(cx, v)),
        Stmt::If {
            cond,
            capture,
            then,
            els,
            ..
        } => {
            // The `if (opt) |v|` capture is in the subset (v0.173).
            let _ = capture;
            det_expr(cx, cond)
                .or_else(|| det_block(cx, then))
                .or_else(|| els.as_deref().and_then(|e| det_stmt(cx, e)))
        }
        // Labeled loops and labeled break/continue are in the subset
        // since v0.176 (an unknown target label is sema's E0301).
        Stmt::While {
            cond, cont, body, ..
        } => det_expr(cx, cond)
            .or_else(|| cont.as_deref().and_then(|c| det_stmt(cx, c)))
            .or_else(|| det_block(cx, body)),
        Stmt::For { iter, body, .. } => {
            det_expr(cx, iter).or_else(|| det_block(cx, body))
        }
        Stmt::Break { .. } | Stmt::Continue { .. } => None,
        Stmt::Defer { stmt, .. } => det_stmt(cx, stmt),
        // `errdefer <stmt>` is in the subset (v0.174).
        Stmt::ErrDefer { stmt, .. } => det_stmt(cx, stmt),
        Stmt::Block(b) => det_block(cx, b),
        // `switch` is in the subset (v0.172): scrutinee, then per arm —
        // a payload capture (tagged unions) stays out — labels and body;
        // the `else` block last. Ranges carry literal bounds only.
        Stmt::Switch {
            scrutinee,
            arms,
            default,
            ..
        } => det_expr(cx, scrutinee)
            .or_else(|| {
                arms.iter().find_map(|arm| {
                    if arm.capture.is_some() {
                        return Some(("capture", arm.span.start));
                    }
                    arm.labels
                        .iter()
                        .find_map(|l| det_expr(cx, l))
                        .or_else(|| det_block(cx, &arm.body))
                })
            })
            .or_else(|| default.as_ref().and_then(|d| det_block(cx, d))),
    }
}

/// Whether a place chain bottoms out at a bare name — the only
/// assignable root in the subset (v0.169).
fn place_rooted_in_name(e: &Expr) -> bool {
    match e {
        Expr::Ident { .. } => true,
        // A deref step roots a place regardless of its inner expression
        // (sema checks it as an ordinary expr — v0.175).
        Expr::Deref { .. } => true,
        Expr::Field { base, .. } | Expr::Index { base, .. } => place_rooted_in_name(base),
        _ => false,
    }
}

/// Walk a place chain: bases inward, each index expression where it sits.
fn det_place(cx: &Cx, e: &Expr) -> Option<Hit> {
    match e {
        Expr::Index { base, index, .. } => {
            det_place(cx, base).or_else(|| det_expr(cx, index))
        }
        Expr::Field { base, .. } => det_place(cx, base),
        Expr::Deref { expr, .. } => det_expr(cx, expr),
        _ => None,
    }
}

/// One fn's walk. A comptime param on a TOP-LEVEL fn is in the subset
/// (v0.178): a bare-`type` annotation binds a type param; any other
/// annotation is a VALUE param and must be a bare subset INT scalar. A
/// METHOD's comptime param stays out entirely. The bound sets are
/// POSITION-BLIND (sema binds by filter-zip, not source position), so they
/// pre-collect before any check.
fn det_fn(cx: &mut Cx, f: &Func, method: bool) -> Option<Hit> {
    cx.tp.clear();
    cx.vp.clear();
    // `Self` binds inside ANY struct method — plain (§32.2) or
    // generic-struct (§26.1) — for the signature and body alike.
    cx.dself = method;
    if !method {
        for p in &f.params {
            if p.is_comptime {
                if ty_is_type_kw(&p.ty) {
                    cx.tp.insert(p.name.clone());
                } else {
                    cx.vp.insert(p.name.clone());
                }
            }
        }
    }
    let mut hit = None;
    for p in &f.params {
        if p.is_comptime {
            if method {
                hit = Some(("generic-param", p.span.start));
                break;
            }
            if !ty_is_type_kw(&p.ty) {
                // The VALUE-param annotation: a bare subset INT scalar
                // (`comptime n: usize`); composites, `type`-adjacent
                // forms and non-int scalars are out.
                let bare = !p.ty.optional
                    && !p.ty.error_union
                    && p.ty.array_len.is_none()
                    && !p.ty.pointer
                    && !p.ty.slice
                    && p.ty.ctor_args.is_none()
                    && p.ty.error_set.is_none();
                let int_ok = matches!(
                    p.ty.name.as_str(),
                    "i32" | "i64" | "u8" | "usize" | "i8" | "i16" | "u16" | "u32" | "u64"
                );
                if !bare || !int_ok {
                    hit = Some(("type-name", p.ty.span.start));
                    break;
                }
            }
            continue;
        }
        if let Some(h) = det_type(cx, &p.ty) {
            hit = Some(h);
            break;
        }
    }
    let out = hit
        .or_else(|| det_type(cx, &f.ret))
        .or_else(|| det_block(cx, &f.body));
    cx.tp.clear();
    cx.vp.clear();
    cx.dself = false;
    out
}

/// A TYPE-CONSTRUCTOR item (v0.179, SPEC §25): every parameter must be a
/// comptime TYPE parameter (`generic-param` at the first violation); a
/// conforming body (`return struct { … };`) walks its field types (ctor
/// params bound) then its methods (params + `Self` bound); any other
/// body shape walks as ordinary statements — sema's E0310 remainder.
fn det_ctor(cx: &mut Cx, f: &Func) -> Option<Hit> {
    for p in &f.params {
        if !p.is_comptime || !ty_is_type_kw(&p.ty) {
            return Some(("generic-param", p.span.start));
        }
    }
    cx.ctp.clear();
    for p in &f.params {
        cx.ctp.insert(p.name.clone());
    }
    let st = match f.body.stmts.as_slice() {
        [Stmt::Return {
            value: Some(Expr::StructType { fields, methods, .. }),
            ..
        }] => Some((fields, methods)),
        _ => None,
    };
    let out = if let Some((fields, methods)) = st {
        fields
            .iter()
            .find_map(|fd| det_type(cx, &fd.ty))
            .or_else(|| methods.iter().find_map(|m| det_fn(cx, m, true)))
    } else {
        det_block(cx, &f.body)
    };
    cx.ctp.clear();
    out
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
    // The declared struct AND enum names (v0.169/v0.171): collected over
    // ALL flattened files before the walk (sema pass 0/0a interns every
    // name before any resolution).
    let sn: HashSet<String> = files
        .iter()
        .flat_map(|ff| ff.module.items.iter())
        .filter_map(|it| match it {
            Item::Struct(s) => Some(s.name.clone()),
            Item::Enum(e) => Some(e.name.clone()),
            _ => None,
        })
        .collect();
    // The top-level GENERIC-fn registry (v0.178): any comptime param,
    // type-constructors excluded; first declaration wins.
    let mut gens: std::collections::HashMap<String, &Func> = std::collections::HashMap::new();
    for ff in files {
        for item in &ff.module.items {
            if let Item::Func(f) = item {
                if f.params.iter().any(|p| p.is_comptime) && !ty_is_type_kw(&f.ret) {
                    gens.entry(f.name.clone()).or_insert(f);
                }
            }
        }
    }
    // The TYPE-CONSTRUCTOR registry (v0.179): bare-`type` returns; then
    // the ALIAS names — `const A = Ctor(…);` items.
    let mut tc: std::collections::HashMap<String, &Func> = std::collections::HashMap::new();
    for ff in files {
        for item in &ff.module.items {
            if let Item::Func(f) = item {
                if ty_is_type_kw(&f.ret) {
                    tc.entry(f.name.clone()).or_insert(f);
                }
            }
        }
    }
    let mut an: HashSet<String> = HashSet::new();
    for ff in files {
        for item in &ff.module.items {
            if let Item::Const(c) = item {
                if let Expr::Call { callee, .. } = &c.value {
                    if tc.contains_key(callee.as_str()) {
                        an.insert(c.name.clone());
                    }
                }
            }
        }
    }
    let mut cx = Cx {
        sn: &sn,
        gens: &gens,
        tc: &tc,
        an: &an,
        tp: HashSet::new(),
        vp: HashSet::new(),
        ctp: HashSet::new(),
        dself: false,
    };
    for ff in files {
        for item in &ff.module.items {
            if matches!(item, Item::Import(_)) {
                continue;
            }
            let hit = match item {
                // A type-returning fn is a TYPE CONSTRUCTOR (v0.179).
                Item::Func(f) if ty_is_type_kw(&f.ret) => det_ctor(&mut cx, f),
                Item::Func(f) => det_fn(&mut cx, f, false),
                Item::Const(c) => c
                    .ty
                    .as_ref()
                    .and_then(|t| det_type(&cx, t))
                    .or_else(|| det_expr(&cx, &c.value)),
                Item::Test(t) => det_block(&cx, &t.body),
                // A struct declaration is a subset item (v0.169 fields;
                // v0.170 admits its METHODS): field types walk in order,
                // then every struct function exactly like a top-level one
                // (a pointer receiver is a `type-form` skip; a comptime
                // METHOD param stays a `generic-param` skip, v0.178).
                Item::Struct(s) => s
                    .fields
                    .iter()
                    .find_map(|fd| det_type(&cx, &fd.ty))
                    .or_else(|| {
                        s.methods
                            .iter()
                            .find_map(|m| det_fn(&mut cx, m, true))
                    }),
                // An enum declaration is a subset item (v0.171): variant
                // names and literal values carry nothing to walk.
                Item::Enum(_) => None,
                Item::Union(u) => Some(("union", u.span.start)),
                Item::Import(_) => None,
                // A named error set is a subset item (v0.174): members
                // carry nothing to walk (a duplicate is sema's E0331).
                Item::ErrorSet(_) => None,
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
        // -- plain data structs (v0.169) ------------------------------------------
        (
            "structs_literals_fields_copies",
            "const Point = struct {\n    x: i64,\n    y: i64,\n};\nfn make(seed: i64) Point {\n    return Point{ .y = seed * 2, .x = seed };\n}\npub fn main() void {\n    var p: Point = make(5);\n    var q: Point = p;\n    q.x = 100;\n    print(p.x);\n    print(p.y);\n    print(q.x);\n    q.y += 3;\n    print(q.y);\n}\n",
        ),
        (
            "structs_nested_and_empty",
            "const Empty = struct {};\nconst Inner = struct {\n    v: i64,\n};\nconst Outer = struct {\n    a: Inner,\n    b: Inner,\n    tag: u8,\n};\npub fn main() void {\n    var e: Empty = Empty{};\n    var o: Outer = Outer{ .a = Inner{ .v = 1 }, .b = Inner{ .v = 2 }, .tag = 7 };\n    o.a.v = 10;\n    o.b.v += 5;\n    print(o.a.v);\n    print(o.b.v);\n    print(o.tag);\n    var e2 = e;\n}\n",
        ),
        (
            "structs_array_fields_and_arrays_of_structs",
            "const Buf = struct {\n    data: [4]i32,\n    n: i32,\n};\nconst Cell = struct {\n    v: i64,\n};\npub fn main() void {\n    var b: Buf = Buf{ .data = [4]i32{ 0, 0, 0, 0 }, .n = 0 };\n    var i: i32 = 0;\n    while (i < 4) : (i += 1) {\n        b.data[i] = (i + 1) * (i + 1);\n        b.n += 1;\n    }\n    print(b.data[2]);\n    print(b.data.len);\n    print(b.n);\n    var cs: [2]Cell = [2]Cell{ Cell{ .v = 1 }, Cell{ .v = 2 } };\n    cs[1].v += 10;\n    cs[0].v = cs[1].v;\n    print(cs[0].v);\n    for (cs) |c| { print(c.v); }\n}\n",
        ),
        (
            "structs_slice_fields_and_views",
            "const Named = struct {\n    name: []u8,\n    id: i64,\n};\nconst B = struct {\n    buf: [3]i64,\n};\npub fn main() void {\n    var n: Named = Named{ .name = \"kardashev\", .id = 42 };\n    print(n.name);\n    print(n.name.len);\n    print(n.name[0]);\n    print(n.id);\n    var xs: [2]B = [2]B{ B{ .buf = [3]i64{ 1, 2, 3 } }, B{ .buf = [3]i64{ 4, 5, 6 } } };\n    var v: []i64 = xs[0].buf[0..3];\n    v[1] = 99;\n    print(xs[0].buf[1]);\n    print(v[2]);\n    var sl: []B = xs[0..2];\n    print(sl.len);\n    print(sl[1].buf[0]);\n}\n",
        ),
        (
            "structs_deep_chain_compound_writes",
            "const Cell = struct {\n    v: i64,\n    w: i64,\n};\nconst Pack = struct {\n    cells: [3]Cell,\n    top: Cell,\n};\npub fn main() void {\n    var p: Pack = Pack{ .cells = [3]Cell{ Cell{ .v = 1, .w = 0 }, Cell{ .v = 2, .w = 0 }, Cell{ .v = 3, .w = 0 } }, .top = Cell{ .v = 0, .w = 0 } };\n    var i: i64 = 1;\n    p.cells[i].v += 10;\n    p.cells[0].w = p.cells[i].v;\n    p.top.v = p.cells[2].v;\n    p.top.w += 9;\n    print(p.cells[1].v);\n    print(p.cells[0].w);\n    print(p.top.v);\n    print(p.top.w);\n}\n",
        ),
        (
            "structs_params_returns_and_liveness",
            "const Acc = struct {\n    total: i64,\n    count: i64,\n};\nfn bump(acc: Acc, x: i64) Acc {\n    return Acc{ .total = acc.total + x, .count = acc.count + 1 };\n}\nfn dead_helper(acc: Acc) Acc {\n    return acc;\n}\npub fn main() void {\n    var a: Acc = Acc{ .total = 0, .count = 0 };\n    a = bump(bump(a, 5), 7);\n    print(a.total);\n    print(a.count);\n}\n",
        ),
        (
            "structs_typedef_dependency_order",
            "const Late = struct {\n    s: []u8,\n};\npub fn main() void {\n    var xs: [1]Late = [1]Late{ Late{ .s = \"z\" } };\n    var t: [2]i64 = [2]i64{ 1, 2 };\n    print(xs[0].s);\n    print(t[0]);\n}\n",
        ),
        // -- struct methods + associated functions (v0.170) ----------------------
        (
            "methods_value_assoc_explicit_self",
            "const Counter = struct {\n    n: i32,\n\n    fn get(self: Counter) i32 {\n        return self.n;\n    }\n\n    fn plus(self: Counter, k: i32) i32 {\n        return self.n + self.get() + k;\n    }\n\n    fn make(n: i32) Counter {\n        return Counter{ .n = n };\n    }\n\n    fn dead_method(self: Counter) i32 {\n        return 0 - self.n;\n    }\n};\n\nfn dead_fn() void {}\n\npub fn main() void {\n    var c: Counter = Counter.make(4);\n    print(c.get());\n    print(c.plus(2));\n    print(Counter.plus(c, 5));\n    print(Counter.make(1).get());\n}\n",
        ),
        (
            "methods_name_level_liveness_across_structs",
            "const A = struct {\n    x: i64,\n    fn ping(self: A) i64 { return self.x; }\n};\nconst B = struct {\n    y: i64,\n    fn ping(self: B) i64 { return self.y * 2; }\n    fn solo(self: B) i64 { return 9; }\n};\npub fn main() void {\n    var a: A = A{ .x = 3 };\n    print(a.ping());\n}\n",
        ),
        (
            "methods_sig_interning_and_string_args",
            "fn helper(xs: []i64) usize { return xs.len; }\n\nconst S = struct {\n    id: i64,\n\n    fn tag(self: S, name: []u8) usize {\n        return name.len + @as(usize, self.id);\n    }\n};\n\npub fn main() void {\n    var s: S = S{ .id = 2 };\n    print(s.tag(\"ab\"));\n    var al: Allocator = c_allocator();\n    var q: []i64 = alloc(al, i64, 1);\n    print(helper(q));\n    free(al, q);\n}\n",
        ),
        (
            "methods_field_vs_method_namespace",
            "const P = struct {\n    v: i64,\n\n    fn v2(self: P) i64 { return self.v * 2; }\n};\npub fn main() void {\n    var p: P = P{ .v = 7 };\n    print(p.v);\n    print(p.v2());\n    p.v = 9;\n    print(p.v2());\n}\n",
        ),
        (
            "methods_on_elements_and_test_mode",
            "const Cell = struct {\n    v: i64,\n\n    fn dbl(self: Cell) i64 { return self.v * 2; }\n};\npub fn main() void {\n    var cs: [2]Cell = [2]Cell{ Cell{ .v = 1 }, Cell{ .v = 5 } };\n    print(cs[1].dbl());\n    var sl: []Cell = cs[0..2];\n    print(sl[0].dbl());\n}\ntest \"elems\" {\n    var c: Cell = Cell{ .v = 3 };\n    expect(c.dbl() == 6);\n}\n",
        ),
        // -- enums (v0.171) -------------------------------------------------------
        (
            "enums_decls_literals_equality",
            "const Color = enum { Red, Green, Blue };\nconst Status = enum { Ok = 200, NotFound = 404, Teapot };\nfn next(c: Color) Color {\n    if (c == Color.Red) { return Color.Green; }\n    if (c == Color.Green) { return Color.Blue; }\n    return Color.Red;\n}\npub fn main() void {\n    var c: Color = Color.Red;\n    c = next(c);\n    if (c == Color.Green) { print(1); }\n    if (c != Color.Blue) { print(2); }\n    print(@intFromEnum(Status.Teapot));\n    var s: Status = @enumFromInt(Status, 404);\n    print(@intFromEnum(s));\n    print(@as(i32, @intFromEnum(c)));\n}\n",
        ),
        (
            "enums_in_arrays_params_returns",
            "const Dir = enum { N, E, S, W };\nfn spin(d: Dir) Dir {\n    if (d == Dir.W) { return Dir.N; }\n    return @enumFromInt(Dir, @intFromEnum(d) + 1);\n}\npub fn main() void {\n    var ds: [2]Dir = [2]Dir{ Dir.E, Dir.W };\n    print(@intFromEnum(ds[1]));\n    ds[0] = spin(ds[0]);\n    print(@intFromEnum(ds[0]));\n    for (ds) |d| { print(@intFromEnum(d)); }\n    var v: []Dir = ds[0..2];\n    print(@intFromEnum(v[1]));\n    print(v.len);\n}\n",
        ),
        (
            "enums_values_wrap_and_negative_start",
            "const T = enum { A = 9223372036854775807, B, C = 0 - 3, D };\npub fn main() void {\n    print(@intFromEnum(T.A));\n    print(@intFromEnum(T.B));\n    print(@intFromEnum(T.C));\n    print(@intFromEnum(T.D));\n}\n",
        ),
        // -- switch + contextual enum literals (v0.172) ---------------------------
        (
            "switch_enum_exhaustive_diverging",
            "const Op = enum { Add, Sub, Mul };\nfn apply(op: Op, x: i64, y: i64) i64 {\n    switch (op) {\n        .Add => { return x + y; },\n        .Sub => { return x - y; },\n        .Mul => { return x * y; },\n    }\n}\npub fn main() void {\n    print(apply(.Add, 6, 4));\n    print(apply(Op.Sub, 6, 4));\n    print(apply(.Mul, 6, 4));\n}\n",
        ),
        (
            "switch_int_labels_ranges_else",
            "pub fn main() void {\n    var i: i64 = 0;\n    while (i < 8) : (i += 1) {\n        switch (i) {\n            0, 1 => { print(10); },\n            2 .. 4 => { print(20); },\n            5 => { print(50); },\n            else => { print(99); },\n        }\n    }\n}\n",
        ),
        (
            "switch_contextual_literals_everywhere",
            "const Color = enum { Red, Green, Blue };\nfn classify(c: Color) i64 {\n    if (c == Color.Red) { return 1; }\n    switch (c) {\n        .Green => { return 2; },\n        else => { return 3; },\n    }\n}\nfn first(c: Color, d: Color) Color {\n    var out: Color = c;\n    out = d;\n    out = .Red;\n    return .Green;\n}\npub fn main() void {\n    print(classify(.Red));\n    print(classify(.Green));\n    print(classify(Color.Blue));\n    print(@intFromEnum(first(.Blue, Color.Red)));\n    var cs: [2]Color = [2]Color{ .Green, Color.Blue };\n    print(@intFromEnum(cs[0]));\n    print(@intFromEnum(cs[1]));\n}\n",
        ),
        (
            "switch_nested_in_loops_with_defer",
            "const S = enum { A, B };\npub fn main() void {\n    var xs: [4]i64 = [4]i64{ 0, 1, 0, 1 };\n    for (xs) |x| {\n        defer print(100 + x);\n        switch (x) {\n            0 => { print(0); },\n            else => {\n                var s: S = .B;\n                switch (s) {\n                    .A => { print(701); },\n                    .B => { print(702); },\n                }\n            },\n        }\n    }\n}\n",
        ),
        (
            "switch_divergence_shapes",
            "fn f(n: i64) i64 {\n    switch (n) {\n        0 => { return 10; },\n        else => { return 20; },\n    }\n}\nfn g(n: i64) i64 {\n    switch (n) {\n        0 => { print(1); },\n        else => { return 5; },\n    }\n    return 6;\n}\npub fn main() void {\n    print(f(0));\n    print(g(0));\n    print(g(1));\n}\n",
        ),
        // -- optionals ?T (v0.173) ------------------------------------------------
        (
            "optionals_widen_orelse_unwrap_capture",
            "fn find(n: i64) ?i64 {\n    if (n > 0) { return n * 2; }\n    return null;\n}\npub fn main() void {\n    var x: ?i64 = find(5);\n    if (x) |v| {\n        print(v);\n    } else {\n        print(0 - 1);\n    }\n    print(find(0) orelse 99);\n    var y: ?i64 = null;\n    y = 7;\n    print(y.?);\n}\n",
        ),
        (
            "optionals_struct_enum_payloads_and_fields",
            "const Color = enum { Red, Green };\nconst P = struct {\n    x: i64,\n    tag: ?u8,\n};\nfn pick(b: bool) ?Color {\n    if (b) { return .Green; }\n    return null;\n}\npub fn main() void {\n    var p: P = P{ .x = 1, .tag = null };\n    p.tag = 7;\n    if (p.tag) |t| { print(t); }\n    var c: ?Color = pick(true);\n    if (c) |col| {\n        if (col == Color.Green) { print(2); }\n    }\n    var s: ?P = P{ .x = 9, .tag = null };\n    if (s) |sv| { print(sv.x); }\n}\n",
        ),
        (
            "optionals_params_args_defer_for",
            "fn use(o: ?i64, d: i64) i64 {\n    return o orelse d;\n}\npub fn main() void {\n    print(use(null, 5));\n    print(use(42, 5));\n    var xs: [2]i64 = [2]i64{ 1, 2 };\n    for (xs) |x| {\n        defer print(100 + x);\n        var o: ?i64 = x;\n        if (o) |v| {\n            if (v == 2) { continue; }\n            print(v);\n        }\n    }\n    print(use(3, 0));\n}\n",
        ),
        (
            "optionals_capture_counter_and_nesting",
            "fn side(n: i64) ?i64 {\n    return n;\n}\npub fn main() void {\n    if (side(41)) |a1| {\n        print(a1);\n    }\n    if (side(1)) |a2| {\n        if (side(2)) |a3| {\n            print(a2 + a3);\n        }\n    }\n}\n",
        ),
        // -- error unions !T (v0.174) ---------------------------------------------
        (
            "errunions_try_catch_errdefer_roundtrip",
            "fn may(n: i64) !i64 {\n    if (n < 0) { return error.Neg; }\n    return n * 2;\n}\nfn chain(n: i64) !i64 {\n    defer print(700);\n    errdefer print(800);\n    var v: i64 = try may(n);\n    return v + 1;\n}\npub fn main() void {\n    print(chain(5) catch 0 - 1);\n    print(chain(0 - 2) catch |e| @as(i64, e) * 10);\n}\n",
        ),
        (
            "errunions_void_payload_forms",
            "fn step(n: i64) !void {\n    if (n == 0) { return error.Zero; }\n    print(n);\n    if (n > 3) { return; }\n}\nfn run(n: i64) !void {\n    try step(n);\n    try step(n + 1);\n}\npub fn main() void {\n    run(1) catch print(0 - 1);\n    run(0) catch |e| print(100 + e);\n}\n",
        ),
        (
            "errunions_code_space_and_sets",
            "const E = error{ Alpha, Beta };\nfn a1() !i64 { return error.Shared; }\nfn a2() E!i64 { return error.Alpha; }\nfn a3() !i64 { return error.Other; }\npub fn main() void {\n    print(a1() catch |e| @as(i64, e));\n    print(a2() catch |e| @as(i64, e));\n    print(a3() catch |e| @as(i64, e));\n    print(a1() catch |e| @as(i64, e));\n}\n",
        ),
        (
            "errunions_coercion_sites",
            "const Box = struct {\n    r: !i64,\n};\nfn wrap(v: !i64) !i64 { return v; }\npub fn main() void {\n    var b: Box = Box{ .r = 7 };\n    b.r = error.Bad;\n    var x: !i64 = 5;\n    x = wrap(x);\n    print(x catch 0);\n    print(b.r catch |e| @as(i64, e));\n}\n",
        ),
        // -- pointers *T (v0.175) --------------------------------------------------
        (
            "pointers_addrof_deref_writes",
            "fn bump(p: *i64) void {\n    p.* += 1;\n}\npub fn main() void {\n    var x: i64 = 41;\n    var p: *i64 = &x;\n    bump(p);\n    print(p.*);\n    p.* = 5;\n    print(x);\n    var xs: [2]i64 = [2]i64{ 7, 8 };\n    var q: *i64 = &xs[1];\n    q.* *= 3;\n    print(xs[1]);\n}\n",
        ),
        (
            "pointers_receivers_autoref_matrix",
            "const Counter = struct {\n    n: i64,\n\n    fn bump(self: *Counter, k: i64) void {\n        self.n += k;\n    }\n\n    fn get(self: Counter) i64 {\n        return self.n;\n    }\n};\npub fn main() void {\n    var c: Counter = Counter{ .n = 1 };\n    c.bump(4);\n    print(c.get());\n    var pc: *Counter = &c;\n    pc.bump(10);\n    print(pc.get());\n    print(pc.n);\n    var cs: [2]Counter = [2]Counter{ Counter{ .n = 0 }, Counter{ .n = 5 } };\n    cs[1].bump(7);\n    print(cs[1].get());\n}\n",
        ),
        (
            "pointers_struct_fields_and_chains",
            "const Inner = struct {\n    v: i64,\n};\nconst Holder = struct {\n    ptr: *Inner,\n};\npub fn main() void {\n    var i: Inner = Inner{ .v = 3 };\n    var h: Holder = Holder{ .ptr = &i };\n    h.ptr.v = 9;\n    print(i.v);\n    print(h.ptr.v);\n    var pp: *Inner = h.ptr;\n    pp.v += 1;\n    print(i.v);\n}\n",
        ),
        // -- labeled loops (v0.176) -------------------------------------------------
        (
            "labeled_break_continue_jumps",
            "pub fn main() void {\n    var total: i64 = 0;\n    outer: while (true) {\n        var i: i64 = 0;\n        while (i < 10) : (i += 1) {\n            defer total += 1;\n            if (i == 2) { continue :outer; }\n            if (total > 5) { break :outer; }\n            print(i);\n        }\n    }\n    print(total);\n}\n",
        ),
        (
            "labeled_for_and_clause_order",
            "pub fn main() void {\n    var xs: [3]i64 = [3]i64{ 1, 2, 3 };\n    var seen: i64 = 0;\n    a: for (xs) |x| {\n        b: for (xs) |y| {\n            defer seen += 1;\n            if (y == 2) { continue :b; }\n            if (x == 3) { break :a; }\n            if (x + y == 4) { continue :a; }\n            print(x * 10 + y);\n        }\n    }\n    print(seen);\n    lab: while (seen > 0) : (seen -= 1) {\n        if (seen == 1) { continue :lab; }\n        print(seen);\n    }\n}\n",
        ),
        // -- f64 (v0.177) -----------------------------------------------------------
        (
            "f64_literals_arith_print",
            "fn mid(x: f64, y: f64) f64 {\n    return (x + y) / 2.0;\n}\npub fn main() void {\n    var a2: f64 = 3.14;\n    var b: f64 = 0.1;\n    print(a2);\n    print(b);\n    print(mid(a2, b));\n    print(a2 * b - 1.5);\n    if (a2 > b) { print(1); }\n    print(@as(i64, a2));\n    print(@as(f64, 7));\n    var c: ?f64 = 2.5;\n    print(c orelse 0.0);\n}\n",
        ),
        (
            "f64_formatting_edges",
            "pub fn main() void {\n    var xs: [6]f64 = [6]f64{ 100.0, 0.0001, 0.30000000000000004, 1000000000000000.0, 9007199254740993.0, 123456789.123456789 };\n    for (xs) |x| { print(x); }\n    var s: []f64 = xs[1..4];\n    print(s[0]);\n    print(s.len);\n}\n",
        ),
        // -- generic functions (v0.178) --------------------------------------------
        (
            "generic_type_params_two_instances",
            "fn imax(comptime T: type, x: T, y: T) T {\n    if (x > y) { return x; }\n    return y;\n}\npub fn main() void {\n    print(imax(i64, 3, 9));\n    print(imax(i32, @as(i32, 4), @as(i32, 2)));\n    print(imax(i64, 8, 1));\n}\n",
        ),
        (
            "generic_value_param_array_size",
            "fn total(comptime n: usize, xs: [n]i64) i64 {\n    var s: i64 = 0;\n    var i: usize = 0;\n    while (i < n) : (i = i + 1) { s = s + xs[i]; }\n    return s;\n}\npub fn main() void {\n    var z: [4]i64 = [4]i64{ 1, 2, 3, 4 };\n    print(total(4, z));\n    var w: [2]i64 = [2]i64{ 10, 20 };\n    print(total(2, w));\n}\n",
        ),
        (
            "generic_negative_value_arg_mangle",
            "fn addk(comptime k: i64, x: i64) i64 {\n    return x + k;\n}\npub fn main() void {\n    print(addk(-3, 10));\n    print(addk(3, 10));\n}\n",
        ),
        (
            "generic_nested_transitive_instantiation",
            "fn imax(comptime T: type, x: T, y: T) T {\n    if (x > y) { return x; }\n    return y;\n}\nfn twice(comptime T: type, x: T) T {\n    return imax(T, x, x);\n}\npub fn main() void {\n    print(twice(i64, 21));\n    print(twice(i32, @as(i32, 7)));\n}\n",
        ),
        (
            "generic_recursive_dedup",
            "fn down(comptime T: type, n: T) T {\n    if (n <= 0) { return n; }\n    return down(T, n - 1);\n}\npub fn main() void {\n    print(down(i64, 5));\n}\n",
        ),
        (
            "generic_zero_instantiation_liveness_source",
            "fn kept() i64 { return 41; }\nfn unused_gen(comptime T: type, x: T) T {\n    return @as(T, kept()) + x;\n}\npub fn main() void {\n    print(1);\n}\n",
        ),
        (
            "generic_instance_from_test_body_in_program_mode",
            "fn id(comptime T: type, x: T) T { return x; }\npub fn main() void { print(2); }\ntest \"t\" { expect(id(i64, 1) == 1); }\n",
        ),
        (
            "generic_value_arg_const_env_and_shadow",
            "const BASE = 3;\nfn addn(comptime n: i64, x: i64) i64 {\n    return n + x;\n}\nfn thru(comptime n: i64, x: i64) i64 {\n    return addn(n * 2, x);\n}\npub fn main() void {\n    print(addn(BASE, 5));\n    print(thru(BASE, 1));\n}\n",
        ),
        (
            "generic_comptime_fold_with_value_param",
            "fn f(comptime n: i64, x: i64) i64 {\n    return comptime (n * 2) + x;\n}\npub fn main() void {\n    print(f(5, 1));\n}\n",
        ),
        (
            "generic_alloc_and_slice_of_t",
            "fn head(comptime T: type, xs: []T) T {\n    return xs[0];\n}\nfn make(comptime T: type, al: Allocator, v: T) []T {\n    var s: []T = alloc(al, T, 2);\n    s[0] = v;\n    s[1] = v;\n    return s;\n}\npub fn main() void {\n    var al: Allocator = c_allocator();\n    var s: []i32 = make(i32, al, @as(i32, 7));\n    print(head(i32, s));\n    free(al, s);\n}\n",
        ),
        (
            "generic_composite_forms_opt_err_ptr",
            "fn pick(comptime T: type, o: ?T, d: T) T {\n    return o orelse d;\n}\nfn bump(comptime T: type, p: *T) void {\n    p.* = p.* + 1;\n}\npub fn main() void {\n    var o: ?i64 = 5;\n    print(pick(i64, o, 0));\n    print(pick(i64, null, 9));\n    var x: i64 = 41;\n    bump(i64, &x);\n    print(x);\n}\n",
        ),
        (
            "generic_only_comptime_params_void_c_signature",
            "fn k(comptime T: type) T {\n    return @as(T, 12);\n}\npub fn main() void {\n    print(k(i64));\n}\n",
        ),
        (
            "generic_dead_call_site_instance_still_emitted",
            "fn id(comptime T: type, x: T) T { return x; }\nfn dead() void { print(id(i32, @as(i32, 3))); }\npub fn main() void { print(id(i64, 4)); print(dead_gate()); }\nfn dead_gate() i64 { return 0; }\n",
        ),
        (
            "generic_enum_type_arg",
            "const Color = enum { Red, Green };\nfn same(comptime T: type, x: T, y: T) bool {\n    return x == y;\n}\npub fn main() void {\n    var c: Color = Color.Red;\n    if (same(Color, c, Color.Red)) { print(1); }\n}\n",
        ),
        (
            "generic_value_param_in_body_and_cond",
            "fn rep(comptime n: usize, v: i64) i64 {\n    var s: i64 = 0;\n    var i: usize = 0;\n    while (i < n) : (i = i + 1) { s = s + v; }\n    if (n > 2) { s = s + 100; }\n    return s;\n}\npub fn main() void {\n    print(rep(3, 5));\n    print(rep(1, 7));\n}\n",
        ),
        // -- generic structs / type-constructors (v0.179) ----------------------------
        (
            "gstruct_alias_fields_methods_ptr_receiver",
            "fn Box(comptime T: type) type {\n    return struct {\n        val: T,\n        fn init(v: T) Self { return Self{ .val = v }; }\n        fn get(self: Self) T { return self.val; }\n        fn set(self: *Self, v: T) void { self.val = v; }\n    };\n}\nconst IntBox = Box(i64);\npub fn main() void {\n    var b: IntBox = IntBox.init(41);\n    print(b.get());\n    b.set(7);\n    print(b.get());\n}\n",
        ),
        (
            "gstruct_direct_application_forms",
            "fn Box(comptime T: type) type {\n    return struct {\n        val: T,\n        fn init(v: T) Self { return Self{ .val = v }; }\n        fn get(self: Self) T { return self.val; }\n    };\n}\npub fn main() void {\n    var c: Box(i32) = Box(i32).init(@as(i32, 5));\n    print(c.get());\n    var d: Box(i64) = Box(i64).init(9);\n    print(d.get());\n}\n",
        ),
        (
            "gstruct_multi_type_params",
            "fn Pair(comptime A: type, comptime B: type) type {\n    return struct {\n        a: A,\n        b: B,\n        fn mk(x: A, y: B) Self { return Self{ .a = x, .b = y }; }\n        fn sum(self: Self) i64 { return @as(i64, self.a) + @as(i64, self.b); }\n    };\n}\nconst PI = Pair(i32, i64);\npub fn main() void {\n    var p: PI = PI.mk(@as(i32, 3), 4);\n    print(p.sum());\n    var q: Pair(i64, i32) = Pair(i64, i32).mk(10, @as(i32, 20));\n    print(q.sum());\n}\n",
        ),
        (
            "gstruct_nested_composition_fields",
            "fn Slot(comptime T: type) type {\n    return struct {\n        v: T,\n        fn of(x: T) Self { return Self{ .v = x }; }\n    };\n}\nfn Pair(comptime T: type) type {\n    return struct {\n        lo: Slot(T),\n        hi: Slot(T),\n        fn mk(x: T, y: T) Self { return Self{ .lo = Slot(T).of(x), .hi = Slot(T).of(y) }; }\n        fn total(self: Self) T { return self.lo.v + self.hi.v; }\n    };\n}\npub fn main() void {\n    var p: Pair(i64) = Pair(i64).mk(4, 5);\n    print(p.total());\n}\n",
        ),
        (
            "gstruct_alloc_t_growable",
            "fn Vec(comptime T: type) type {\n    return struct {\n        buf: []T,\n        n: i64,\n        fn init(al: Allocator) Self {\n            return Self{ .buf = alloc(al, T, 2), .n = 0 };\n        }\n        fn push(self: *Self, al: Allocator, v: T) void {\n            if (@as(usize, self.n) == self.buf.len) {\n                var nb: []T = alloc(al, T, self.buf.len * 2);\n                var i: usize = 0;\n                while (i < self.buf.len) : (i = i + 1) { nb[i] = self.buf[i]; }\n                free(al, self.buf);\n                self.buf = nb;\n            }\n            self.buf[@as(usize, self.n)] = v;\n            self.n = self.n + 1;\n        }\n        fn get(self: Self, i: i64) T { return self.buf[@as(usize, i)]; }\n        fn deinit(self: Self, al: Allocator) void { free(al, self.buf); }\n    };\n}\npub fn main() void {\n    var al: Allocator = c_allocator();\n    var v: Vec(i64) = Vec(i64).init(al);\n    var i: i64 = 0;\n    while (i < 5) : (i = i + 1) { v.push(al, i * 10); }\n    print(v.get(0));\n    print(v.get(4));\n    v.deinit(al);\n}\n",
        ),
        (
            "gstruct_app_in_method_signature",
            "fn Box(comptime T: type) type {\n    return struct {\n        v: T,\n        fn init(x: T) Self { return Self{ .v = x }; }\n        fn get(self: Self) T { return self.v; }\n    };\n}\nfn Wrap(comptime T: type) type {\n    return struct {\n        x: T,\n        fn boxed(self: Self) Box(T) { return Box(T).init(self.x); }\n    };\n}\nconst W = Wrap(i64);\npub fn main() void {\n    var w: W = W{ .x = 6 };\n    print(w.boxed().get());\n}\n",
        ),
        (
            "plain_struct_self_this_methods",
            "const Point = struct {\n    x: i64,\n    y: i64,\n    fn mk(x: i64, y: i64) Self { return Self{ .x = x, .y = y }; }\n    fn norm1(self: Self) i64 { return self.x + self.y; }\n    fn bump(self: *@This()) void { self.x = self.x + 1; }\n};\npub fn main() void {\n    var p: Point = Point.mk(3, 4);\n    print(p.norm1());\n    p.bump();\n    print(p.norm1());\n}\n",
        ),
        (
            "gstruct_generic_fn_with_alias_type_arg",
            "fn Box(comptime T: type) type {\n    return struct {\n        v: T,\n        fn init(x: T) Self { return Self{ .v = x }; }\n        fn get(self: Self) T { return self.v; }\n    };\n}\nconst IB = Box(i64);\nfn pick(comptime T: type, x: T, y: T) T {\n    if (x > y) { return x; }\n    return y;\n}\nfn peek(b: IB) i64 { return b.get(); }\npub fn main() void {\n    print(pick(i64, 2, 8));\n    var b: IB = IB.init(5);\n    print(peek(b));\n}\n",
        ),
        (
            "gstruct_slice_of_instance_and_ptr_param",
            "fn Cnt(comptime T: type) type {\n    return struct {\n        v: T,\n        fn init(x: T) Self { return Self{ .v = x }; }\n    };\n}\nconst C64 = Cnt(i64);\nfn bump(p: *C64) void {\n    p.v = p.v + 1;\n}\npub fn main() void {\n    var c: C64 = C64.init(41);\n    bump(&c);\n    print(c.v);\n    var al: Allocator = c_allocator();\n    var s: []C64 = alloc(al, C64, 2);\n    s[0] = C64.init(7);\n    print(s[0].v);\n    free(al, s);\n}\n",
        ),
        (
            "skip_app_not_a_ctor",
            "fn f() i64 { return 1; }\npub fn main() void {\n    var x: f(i64) = 0;\n}\n",
        ),
        (
            "skip_app_arg_inadmissible_name",
            "fn Box(comptime T: type) type {\n    return struct { v: T, fn get(self: Self) T { return self.v; } };\n}\npub fn main() void {\n    var b: Box(NoSuch) = Box(NoSuch).init(1);\n}\n",
        ),
        (
            "skip_ctor_body_not_struct_return",
            "fn F(comptime T: type) type {\n    var x: i64 = 1.5;\n    return struct { v: T };\n}\npub fn main() void { print(1); }\n",
        ),
        (
            "skip_self_outside_method",
            "fn f(x: Self) i64 { return 0; }\npub fn main() void { print(1); }\n",
        ),
        // -- the remaining integer widths (v0.180) -----------------------------------
        (
            "widths_narrow_trunc_back_zoo",
            "pub fn main() void {\n    var a2: u8 = 200;\n    var b: i8 = @as(i8, 100);\n    var c: i16 = @as(i16, 30000);\n    var d: u16 = 43690;\n    print(@as(i64, ~a2));\n    print(@as(i64, ~b));\n    print(@as(i64, ~c));\n    print(@as(i64, ~d));\n    print(@as(i64, d << 1));\n    print(@as(i64, b << 1));\n}\n",
        ),
        (
            "widths_u64_boundary_ops",
            "pub fn main() void {\n    var ones: u64 = @as(u64, 0) - 1;\n    var hi: u64 = ones >> 1;\n    print(@as(i64, hi & 255));\n    var x: u32 = 4294967295;\n    print(@as(i64, x >> 16));\n    print(@as(i64, x & 65535));\n}\n",
        ),
        (
            "widths_widening_casts_sign_zero_extend",
            "pub fn main() void {\n    var a2: i8 = @as(i8, 0) - 100;\n    var w: i64 = @as(i64, a2);\n    print(w);\n    var b: u16 = 40000;\n    var wu: u32 = @as(u32, b);\n    print(@as(i64, wu));\n    var c: u8 = 255;\n    print(@as(i64, @as(u64, c)));\n}\n",
        ),
        (
            "widths_slices_arrays_alloc",
            "fn total(xs: []u32) i64 {\n    var s: i64 = 0;\n    var i: usize = 0;\n    while (i < xs.len) : (i = i + 1) { s = s + @as(i64, xs[i]); }\n    return s;\n}\npub fn main() void {\n    var al: Allocator = c_allocator();\n    var xs: []u32 = alloc(al, u32, 3);\n    xs[0] = 7;\n    xs[1] = 9;\n    xs[2] = 16;\n    print(total(xs));\n    free(al, xs);\n    var a2: [2]u64 = [2]u64{ 1, 2 };\n    print(@as(i64, a2[0] + a2[1]));\n    var b: [3]i16 = [3]i16{ @as(i16, 1), @as(i16, 2), @as(i16, 3) };\n    print(@as(i64, b[2]));\n}\n",
        ),
        (
            "widths_generics_and_value_params",
            "fn pick(comptime T: type, x: T, y: T) T {\n    if (x > y) { return x; }\n    return y;\n}\nfn repw(comptime n: u16, v: i64) i64 {\n    var s: i64 = 0;\n    var i: i64 = 0;\n    while (i < @as(i64, n)) : (i = i + 1) { s = s + v; }\n    return s;\n}\nfn Box(comptime T: type) type {\n    return struct { v: T, fn init(x: T) Self { return Self{ .v = x }; } };\n}\npub fn main() void {\n    print(@as(i64, pick(u32, 10, 20)));\n    print(@as(i64, pick(i16, @as(i16, 5), @as(i16, 3))));\n    print(repw(3, 7));\n    var b: Box(u64) = Box(u64).init(@as(u64, 9));\n    print(@as(i64, b.v));\n}\n",
        ),
        (
            "widths_print_all_int_routes",
            "pub fn main() void {\n    var a2: i8 = @as(i8, 0) - 8;\n    var b: i16 = @as(i16, 0) - 16;\n    var c: u16 = 16;\n    var d: u32 = 32;\n    var e: u64 = 64;\n    print(a2);\n    print(b);\n    print(c);\n    print(d);\n    print(e);\n}\n",
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
            "skip_place_root_call",
            "pub fn main() void {\n    var a: Allocator = c_allocator();\n    var s: []u8 = alloc(a, u8, 2);\n    g()[0] = 1;\n}\n",
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
            "float_in_defer",
            "pub fn main() void {\n    defer {\n        var x: f64 = 1.5;\n        print(x + 0.25);\n    }\n    print(1);\n}\n",
        ),
        (
            "slice_of_f64_roundtrip",
            "fn head(s: []f64) f64 { return s[0]; }\npub fn main() void {\n    var al: Allocator = c_allocator();\n    var s: []f64 = alloc(al, f64, 2);\n    s[0] = 2.5;\n    s[1] = 0.125;\n    print(head(s));\n    print(s[1]);\n    free(al, s);\n}\n",
        ),
        (
            "skip_optional_f64_inner",
            "pub fn main() void {\n    var x: ?f64 = null;\n}\n",
        ),
        (
            "skip_ptr_f64_pointee",
            "pub fn main() void {\n    var x: f64 = 1.5;\n    var p: *f64 = &x;\n}\n",
        ),
        ("skip_nomain", "fn helper() void {}\n"),
        ("skip_empty_module", ""),
        (
            "skip_alloc_unknown_elem",
            "pub fn main() void {\n    var al: Allocator = c_allocator();\n    var n: i64 = 4;\n    free(al, alloc(al, Foo, n));\n}\n",
        ),
        (
            "skip_test_block_after_main",
            "pub fn main() void { print(1); }\ntest \"t\" { expect(true); }\n",
        ),
        (
            "generic_in_subset_since_v178",
            "fn id(comptime T: type, x: i64) i64 { return x; }\npub fn main() void { print(id(i64, 1)); }\n",
        ),
        (
            "skip_generic_type_arg_not_subset",
            "fn id(comptime T: type, x: i64) i64 { return x; }\npub fn main() void { print(id(u16, 1)); }\n",
        ),
        (
            "skip_generic_method_comptime_param",
            "pub fn main() void {}\nconst S = struct { x: i32, fn m(comptime T: type) void {} };\n",
        ),
        (
            "skip_generic_value_annotation_not_int",
            "fn rep(comptime b: bool, x: i64) i64 { return x; }\npub fn main() void { print(rep(true, 4)); }\n",
        ),
        (
            "skip_arrparam_outside_generic",
            "fn f(xs: [n]i64) i64 { return 0; }\npub fn main() void { print(0); }\n",
        ),
        (
            "labeled_while_minimal",
            "pub fn main() void {\n    outer: while (true) {\n        break :outer;\n    }\n    print(1);\n}\n",
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
            "array_lit_f64_elems",
            "pub fn main() void {\n    var xs: [2]f64 = [2]f64{ 1.5, 2.5 };\n    print(xs[0]);\n    print(xs[1]);\n}\n",
        ),
        (
            "labeled_for_minimal",
            "pub fn main() void {\n    var xs: [1]i64 = [1]i64{ 1 };\n    lab: for (xs) |x| {\n        if (x == 1) { break :lab; }\n        print(x);\n    }\n    print(2);\n}\n",
        ),
        (
            "skip_ptr_receiver_method",
            "const P = struct {\n    x: i64,\n    fn bump(self: *P) void { self.x += 1; }\n};\npub fn main() void {\n    var p: P = P{ .x = 1 };\n    print(p.x);\n}\n",
        ),
        (
            "skip_self_type_in_method_ret",
            "const P = struct {\n    x: i64,\n    fn me(self: P) Self { return self; }\n};\npub fn main() void { print(1); }\n",
        ),
        (
            "skip_struct_field_f64",
            "const P = struct {\n    x: f64,\n};\npub fn main() void { print(1); }\n",
        ),
        (
            "contextual_enum_lit_let",
            "const Color = enum { Red, Green };\npub fn main() void {\n    var c: Color = .Red;\n    if (c == Color.Red) { print(1); }\n}\n",
        ),
        (
            "skip_switch_capture_arm",
            "const Color = enum { Red, Green };\npub fn main() void {\n    var c: Color = Color.Red;\n    switch (c) {\n        .Red => |v| { print(1); },\n        else => { print(2); },\n    }\n}\n",
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
