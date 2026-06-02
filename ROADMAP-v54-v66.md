# Kardashev Roadmap v54–v66

The compiler is at **v0.53.0**. This roadmap continues the cadence that carried
v32→v53: each version ships one **tractable real core**, names its **honest
deferrals** out loud, and closes with a release (version bump + CHANGELOG +
README/ROADMAP + PR → CI-green on both platforms → tag → release). Every version
below is implementable with the host toolchain (clang/LLVM + Bazel + a POSIX
shell) — no new analysis framework, no environment-blocked dependency. The four
**environment-blocked mega-arcs** (real bootstrap, package registry, WASM/Windows
backends, spec→1.0) stay out of this numbered track and are revisited in the
closing note. Line citations are against the v0.53.0 tree and are meant as
starting points, not exact post-edit coordinates.

---

## v54 — Soundness: reference-store-into-out-param escape + aggregate-const promotion

> **Status:** ✅ SHIPPED v0.54.0 — out-param store escape (Part 1). Aggregate-const promotion (Part 2) deferred honestly (needs AST→llvm::Constant lowering; current behaviour already sound).


**Theme:** Close the two open escape-analysis holes documented in
`CHANGELOG.md` (v0.52.0 / v0.53.0 "Known limitations", lines 44–50).

**CORE**

1. **Reject reference STORES into mutable out-param fields.** The v0.52 escape
   checker only guards *returns*; `out.p = &local` through a `&mut` parameter is
   unchecked (CHANGELOG:49–50). Add `checkFieldAssignEscape()` modeled on the
   existing `checkReturnEscape()` (`borrow_check.cpp:564`). At the two real
   `AssignStmt` handling sites — **`borrow_check.cpp:786` and `:1512`** (the
   roadmap-draft's "~11228" was wrong; the file is 1906 lines) — when the LHS is
   a field-chain rooted in a `&mut` parameter and the stored field type contains
   a reference, run the existing `classifyRoot` (`borrow_check.cpp:368`) /
   `rootEscapes` (`:414`) machinery on the RHS and reject if the stored reference
   roots in a local, a by-value param, or a temporary.

2. **Promote scalar-only aggregate consts to stable globals.** v0.53 promoted
   *scalar* consts; aggregate (`array`/`struct`/`enum`) consts remain frame-local
   temporaries (CHANGELOG:45–48). This needs a genuine **new data-export path**,
   not pure reuse: `tc_.constExprValues` is **scalar-only** today —
   `ConstFolded{isBool,i}` (`typecheck.hpp:209`) and the export at
   `typecheck.cpp:2795` explicitly drops aggregates (`if (!v.isAgg)` at
   `:5495`); the aggregate const data (`isAgg`/`enumTag`/`elems`,
   `typecheck.cpp:2833+`) never reaches the borrow checker or codegen. So:
   - Extend `ConstFolded` (or add a parallel exported aggregate-const map) in
     `typecheck.hpp` / `typecheck.cpp:2795` to carry `isAgg` plus
     recursively-Copy/scalar element info.
   - In `classifyRoot`, when an `&`-operand resolves to such a const aggregate,
     classify it `Global` (not `Temporary`).
   - In codegen `emitConstGlobal` (`codegen.cpp:11937`), emit the aggregate
     initializer to a deduped internal global so `&CONST_ARRAY` returns a live
     `'static` ref. The escape-check signal and the codegen promotion must key on
     the *same* condition, exactly as the scalar path already does.

**ACCEPTANCE GATE**

- `smoke_test_field_ref_escape.sh`: ≥5 reject (store `&local` / `&temp` /
  `&by-val-param` into a `&mut` struct field — each exits non-zero with a clear
  `cannot store reference to local into out-parameter` diagnostic) + ≥3 accept
  (store `&param` into the field).
- `smoke_test_const_agg_promotion.sh`: ≥6 accept (return `&CONST_ARRAY` /
  `&CONST_TUPLE` of scalars; value readable and not dropped) + ≥4 reject
  (aggregate const with a non-Copy field).
- Existing `smoke_test_escape_analysis.sh` and `smoke_test_const_ref.sh` stay
  green; a previously-undetected dangling-store UB is now caught.

**DEFERRALS**

- Named lifetimes / full NLL region inference (the v38+ mega-track).
- Struct-field reference-lifetime drop-check beyond the conservative store rule.
- Promotion of aggregate consts containing references to *other* consts.

---

## v55 — Correctness: UTF-8-safe string casing (+ the genuinely-missing char API)

> **Status:** ✅ SHIPPED v0.55.0 — UTF-8-safe casing (café→CAFÉ; char_to_upper/lower extended to Latin-1) + str_split_char/str_get_char/str_index_char + Drop as a built-in prelude trait. (vec_reverse already existed — not re-added.)


**Theme:** Fix the documented UTF-8 corruption bug and add only the helpers that
are actually absent — pure-prelude Kardashev, no codegen changes.

> Scope note (per critic): the original draft padded this with ~10 functions that
> **already exist** (`str_trim` @`main.cpp:695`, `str_starts_with` @`:1143`,
> `str_ends_with` @`:1156`, `str_contains` @`:1192`, `str_char_count` @`:1335`,
> `str_lines` @`:1475`, `str_split` @`:675`, `char_to_upper` @`:1416`,
> `char_to_lower` @`:1423`, `string_chars` @`:1348`) and a duplicate encoder
> (`str_encode_char` ≈ the existing `str_push_char` @`:1254`, the real 1–4-byte
> UTF-8 codec). Those are removed from the deliverable. The remaining genuine
> work is a small bug-fix + 4 helpers; the **built-in `Drop` prelude trait** (the
> only real sliver salvaged from the old v57 — see v57 below) is folded in here
> to make a coherent version.

**CORE**

1. **The bug.** `str_to_upper` / `str_to_lower` (`main.cpp:1198–1225`) map only
   ASCII bytes 97–122, corrupting multi-byte UTF-8 — `str_to_upper("café")`
   garbles `é`. Rewrite both to iterate **by char** via the existing
   `str_decode_char_at` / `str_char_width_at`, case-map the codepoint with the
   existing `char_to_upper` / `char_to_lower`, and re-encode with the existing
   `str_push_char` (do **not** add `str_encode_char`).
2. **Genuinely-missing helpers only:** `str_split_char(&String, char)` (split by
   char vs the existing by-substring `str_split`), `str_get_char(&String, idx)`,
   `str_index_char(&String, char) -> Option<i64>`, and `vec_reverse`.
3. **Built-in `Drop` prelude trait** (folded from old v57): inject
   `trait Drop { fn drop(&mut self) ! {} }` into the prelude in `main.cpp`,
   alongside `Send`/`Sync`, so user `impl Drop for T` resolves **without** the
   user having to redeclare the trait. The drop *glue* (user destructor first,
   then reverse-field drop, drop-flag machinery) already exists since Phase 16
   (`codegen.cpp:209/744/10594/10677`) — this is only the ~5-line declaration gap
   that `typecheck.cpp:2084` otherwise rejects as "unknown trait Drop".

**ACCEPTANCE GATE**

- `smoke_test_utf8_casing.sh`: `str_to_upper("café") == "CAFÉ"` (the exact bug
  case) under JIT **and** AOT; ≥8 mixed-script casing roundtrips.
- `smoke_test_string_api.sh`: `str_split_char("hello·world", '·')` → 2 parts
  `"hello"`/`"world"`; `str_get_char("café", 3) == 'é'`;
  `str_index_char("café", 'f') == Some(2)`; `vec_reverse` correct; JIT==AOT.
- `smoke_test_builtin_drop.sh`: `impl Drop for T` compiles with **no** user
  `trait Drop` declaration and fires on scope exit; existing
  `smoke_test_drop.sh` stays green.

**DEFERRALS**

- Full Unicode case folding (locale / special cases like `ß`→`SS`) and grapheme
  segmentation (UAX#29) — need a Unicode DB.
- `str_split_str` (multi-byte-needle substring split) — a later stdlib version.

---

## v56 — Soundness under concurrency: thread-local effect handlers

> **Status:** ✅ SHIPPED v0.56.0 — per-(effect,op) handler global is thread-local in AOT (no cross-thread handler race); JIT stays process-global (TLS↛emutls under ORC). Conditional via a new codegen `forJit` flag.


**Theme:** Eliminate the documented handler-dispatch data race.

**CORE**

Two threads installing different handlers for the same effect currently race a
process-global `InternalLinkage` global: `effectHandlerGlobal()`
(`codegen.cpp:14352`) builds one per-`(effect,op)` process-wide slot, and the
`handle…with` save/restore (`codegen.cpp:14428–14440`) mutates it process-wide.
Change `effectHandlerGlobal()` to construct the per-`(effect,op)` global with
`GlobalValue::ThreadLocalMode` (`GeneralDynamicTLSModel`) so each thread reads and
writes its **own** handler slot; the existing save/restore then mutates only
thread-local storage. Verify LLVM lowers TLS to `__thread` on Linux. This is
exercised under **AOT only** for the concurrent gate, because TLS is known-broken
under the ORC JIT (a `thread_local` global lowers to `__emutls_get_address`,
which the JIT cannot resolve — documented at `codegen.cpp:4948–4956`).

**ACCEPTANCE GATE**

- `smoke_test_thread_local_handlers.sh` (**AOT-only — TLS is unavailable under
  JIT; do not port this test to JIT mode**): two threads install different
  handlers for one effect and each performs it 100k times concurrently;
  per-thread counters prove each thread invoked **only** its own handler (no
  cross-talk); deterministic over 5+ runs; MALLOC_CHECK / LSan clean; emitted IR
  for the op global shows `thread_local`.
- Existing `smoke_test_effects.sh` / `smoke_test_effect_exhaustive.sh` stay green
  (single-thread behavior unchanged).

**DEFERRALS**

- TSan CI gate (needs sanitizer-instrumented codegen — a separate track).
- Multi-shot / continuation-capturing handlers (handlers stay tail-resumptive).
- Any JIT-mode concurrent-handler test (TLS unavailable under ORC).

---

## v57 — Overloadable `Index`/`IndexMut` + `Deref`/`DerefMut` for custom types

> **Status:** ✅ SHIPPED v0.57.0 — RESCOPED. Index/Deref methods must return `&Self::Output`, but kardashev blanket-rejected ALL user `-> &T` returns (a rule predating the v52–v54 escape analysis). v0.57.0 ships the unblocking prerequisite: **reference-returning functions, gated by escape analysis** (a returned ref must root in a by-ref parameter / `&self` / global; local/temp rejected). This enables `&self.field` accessor methods. The Index/Deref *operator sugar* (assoc-type Output + `[]`/`*` dispatch) is the documented follow-on built on this.


**Theme:** Smart-pointer & collection ergonomics — make `[]` and `*` dispatch to
user traits, completing the operator surface alongside the already-shipped
arithmetic/bit overloading (`Add`/`Sub`/… from v34 Phase 184).

> The old v57 ("user-defined Drop impls") was a **duplicate** — `impl Drop for T`
> has shipped since Phase 16, and `smoke_test_drop.sh:51–53` already tests the
> exact scenario. Its only real sliver (Drop as a built-in prelude trait) is
> folded into v55. v57 is repurposed here to a verified-absent, leverage-matched
> feature: today `checkIndex` (`typecheck.cpp:9506`) **rejects** `[]` on anything
> but a fixed-size array ("indexing `[i]` requires a fixed-size array"), and there
> is **no** `trait Index`/`trait Deref` in the prelude (confirmed absent).

**CORE**

1. **`Index`/`IndexMut`.** Add prelude
   `trait Index<Idx> { fn index(&self, i: Idx) -> &Self::Output; }` and
   `trait IndexMut<Idx> { fn index_mut(&mut self, i: Idx) -> &mut Self::Output; }`
   (associated output type via the existing GAT/assoc-type machinery). In
   `checkIndex` (`typecheck.cpp:9506`), when the object type is **not** a
   fixed-size array, look for an `Index`/`IndexMut` impl on the type (mirroring
   the `binOpMethod_` operator-trait lookup at `typecheck.cpp:6822+`) and route to
   `index`/`index_mut` instead of erroring; mut-context (LHS of assign) selects
   `IndexMut`. Codegen: at the `IndexExpr` sites (`codegen.cpp:11421/11487`),
   when a resolved index-method is recorded, emit a method call rather than the
   array GEP+bounds-check path.
2. **`Deref`/`DerefMut`.** Add prelude
   `trait Deref { fn deref(&self) -> &Self::Target; }` and `DerefMut`. Wire
   `*expr` (UnaryExpr deref) and method-call autoderef (`typecheck.cpp:6058+`
   "Phase 2.4b auto-deref") to consult a `Deref` impl when the operand is a
   user struct rather than a built-in `&T`, so `*my_box` and `my_box.method()`
   work for a user smart pointer. Codegen emits the `deref` call and follows the
   returned reference.

Both reuse the existing operator-trait dispatch map and method-call lowering — no
new ABI, no new analysis.

**ACCEPTANCE GATE**

- `smoke_test_index_overload.sh`: a user `Matrix`/`Grid` with `impl Index` —
  `let g = grid_new(); g[2]` reads the element; `impl IndexMut` lets `g[2] = 9`
  write it back; ≥6 programs JIT==AOT; a type with no `Index` impl still errors
  cleanly.
- `smoke_test_deref_overload.sh`: a user `MyBox<T>` with `impl Deref` — `*b`
  reads the target and `b.target_method()` autoderefs; `impl DerefMut` allows
  `*b = v`; ≥5 programs JIT==AOT; MALLOC_CHECK clean (no double-drop of the
  pointee).

**DEFERRALS**

- `Index`/`Deref` chains through *multiple* user levels (single-level autoderef
  only). Deref-coercion in argument position (`&MyBox<T>` → `&T` at a call
  boundary). `IndexMut` returning into a complex place expression beyond a single
  field/element.

---

## v58 — Ergonomics: if-let / while-let / let-else

> **Status:** ✅ SHIPPED v0.58.0 (partial) — `if let` / `while let` desugar at parse time to `match` / `loop { match … _ => break }`. **let-else DEFERRED**: a diverging `panic` else block types as `()` not bottom (no never-type yet), so the else arm fails to unify, and a `_ => return` arm trips a pre-existing effect-inference quirk; both need a never-type / divergence-typing pass first.


**Theme:** The three highest-leverage pattern-binding forms, all desugaring to
existing match lowering (no new codegen).

**CORE**

Verified absent across parser, typecheck, AST, tests, and examples.

1. **`IfLetExpr`** — `if let PAT = expr { … } else { … }`: parser lookahead in
   `parseIfExpr` (`parser.cpp:3547`) before the boolean-condition path.
2. **`WhileLetExpr`** — `while let PAT = expr { … }`: extend `parseWhileExpr`
   (`parser.cpp:3577`); loops until the pattern fails to bind.
3. **`LetElseStmt`** — `let PAT = expr else { <diverging block> };`: binds in the
   enclosing scope; the else block **must diverge** (panic/return/break/continue),
   enforced by a small divergence/reachability check in typecheck.

All three rewrite to `match` in typecheck and reuse the decision-tree compiler
`dtCompile` (`pattern_match.cpp:611`) — no new codegen.

**ACCEPTANCE GATE**

- `smoke_test_if_let.sh`: `let o = Some(5); if let Some(x) = o { print(x) } else { print(0-1) }`
  prints `5`; `while let Some(x) = vec_pop(&mut v) { … }` drains a Vec.
- `smoke_test_let_else.sh`:
  `let Some(x) = Some(42) else { panic("") }; print(x)` prints `42`;
  `let Some(x) = None else { panic("fail") };` exits with the panic code; a
  **non-diverging** let-else block is rejected at compile time.
- JIT==AOT for positives.

**DEFERRALS**

- Match guards on if-let arms; nested let-else binding-merge edge cases;
  or-patterns inside let-else (carry over only where existing match or-pattern
  support already allows).

> Implementation note: the let-else divergence check is the one non-trivial bit —
> budget the small reachability analysis in typecheck.

---

## v59 — Ergonomics: struct-update spread + function-parameter destructuring

> **Status:** ✅ SHIPPED v0.59.0 (partial) — struct-update spread `S { x: 10, ..base }` for a Copy base (all-Copy-field structs: base byte-copied, explicit fields overwritten; base consumed). Move-field spread + **parameter destructuring** DEFERRED (param-destructure touches the pervasive `Param` struct + fn-entry codegen — higher risk; struct-update spread is the self-contained, higher-leverage half).


**Theme:** Two AST-level ergonomic features over existing struct/pattern
machinery.

**CORE**

Verified absent: `StructLitExpr` (`ast.hpp:274`) has no spread field; `Param`
(`ast.hpp:628`) is `{name, type}` only.

1. **Struct update** `S { x: 10, ..old }`: add `spreadExpr` to `StructLitExpr`;
   parser accepts a trailing `..expr`; typecheck verifies the spread expr is the
   same struct type and that `union(explicit fields, spread fields)` covers all
   required fields with no duplicates; codegen copies the spread value then
   overwrites the explicit fields, **respecting Copy/move + drop of the consumed
   spread base**.
2. **Parameter destructuring** `fn f(P { x, y }: P)` and tuple params: extend
   `Param` to hold a `Pattern`; parser detects `{`/`(` after the binder and
   disambiguates it from a type annotation (distinct from the separate
   `ClosureParam` struct at `ast.hpp:646`, so no collision); typecheck
   destructures the param type against the pattern; codegen emits the destructure
   at fn entry, reusing match-binding lowering.

**ACCEPTANCE GATE**

- `smoke_test_struct_update.sh`:
  `let s = S{x:1,y:2}; let s2 = S{x:10, ..s}; print(s2.x); print(s2.y)`
  prints `10` then `2`; reject spread + duplicate field; reject missing required
  field; **no double-drop of the spread base under MALLOC_CHECK**.
- `smoke_test_param_destructure.sh`: `fn f(P{x,y}:P)->i64 { x+y } f(P{x:3,y:4})`
  returns `7`; nested + tuple param destructure works; mismatched param pattern
  rejected.
- JIT==AOT.

**DEFERRALS**

- Spread from a value of a *different* (structurally-compatible) type;
  `@`-bindings and `..rest` ignore-patterns in params; default-field-value syntax.

---

## v60 — Inference depth: match-arm + nested-closure type inference

**Theme:** Propagate bidirectional type context where it currently stops,
eliminating spurious annotation requirements.

**CORE**

Extends existing inference; does not build it from scratch.

1. **Match-arm inference** (`typecheck.cpp:~9867`): before checking arms, resolve
   the scrutinee's concrete type-arg structure and the match expression's
   expected type; in `checkPattern`, bind enum-payload variables to the resolved
   field type (unify the fresh `Var` against the schema's `typeArgs`) instead of
   an unconstrained `Var`; set `expectedArgType_` per arm body to the match's
   expected type so literal narrowing and generic-method inference fire.
2. **Closure-param inference through nested contexts** (`typecheck.cpp:7884`,
   already partial — "Phase 61 closure-param INFERENCE"): flow `expectedFnType_`
   through struct-field closures, closure-typed `let`/assign bindings, and
   `Box<fn…>` fields (using the existing `currentExpectedType_`@`3128` /
   `expectedArgType_`@`3136` plumbing) so unannotated closure params infer from
   the declared fn type.

**ACCEPTANCE GATE**

- `smoke_test_match_infer.sh`: ≥10 programs (enum scrutinee with generic payload,
  Vec-element match, Option-of-generic, nested enum patterns) compile and run with
  inferred payload/param types correct; ≥6 negatives (type-param method on the
  wrong scrutinee type) rejected with a stable code.
- `smoke_test_closure_infer.sh`: ≥8 programs (closure in a struct field,
  closure-typed `let`, closure in `Box<dyn Fn>`, closure in a Vec element)
  compile/run; ≥4 mismatched-param negatives rejected.
- JIT==AOT for all positives; no regression on existing generic/closure smoke
  tests.

**DEFERRALS**

- Full GAT-bounded assoc-type projection at call time; effect-row turbofish; HM
  completeness across mutually-recursive generic closures; const-generic inference
  from non-array contexts (its own later version).

> Scope discipline: inference-completeness work is the likeliest to leak. Keep the
> ≥10 / ≥6 gate tight so the version closes rather than chasing HM completeness.

---

## v61 — Lazy iterator adaptor tower (fuse take/skip/chain/zip/enumerate)

**Theme:** Replace the eager Vec-materializing adaptors with a lazy stateful-struct
tower so adaptor chains fuse into a single pass.

**CORE**

`vec_take`/`vec_skip`/`vec_chain`/`vec_zip`/`vec_enumerate`
(`main.cpp:888–930`) each materialize a fresh Vec. Add prelude adaptor structs
`Take<I>{iter,rem}`, `Skip<I>{skip_cnt}`, `Chain<A,B>`, `Zip<A,B>`,
`Enumerate<I>{idx}`, each `impl Iterator<T>` (trait at `main.cpp:105`); add
bridges `iter_take`/`iter_skip`/`iter_chain`/`iter_zip`/`iter_enumerate(it: impl
Iterator<T>, …)`. Redefine the `vec_*` helpers as `iter_*(vec_iter(v))` +
`iter_collect` (exists, `main.cpp:122`). Pure-prelude Kardashev over existing
generic monomorphization — no codegen changes.

**ACCEPTANCE GATE**

- `smoke_test_iter.sh` stays green **and** `smoke_test_iter_lazy.sh`:
  `take(skip(range(100),20),5)` yields exactly `[20,21,22,23,24]`;
  zip/enumerate/chain compose into correct fused results; JIT==AOT.
- **Allocation discipline** (one of):
  (a) add a tiny prelude alloc counter (a global incremented in `vec_new`,
  exposed via a debug builtin) and assert the fused chain does exactly **one**
  `vec_new` (on the final collect); **or**
  (b) a behavioral RSS proxy: `take(skip(range(10_000_000), …), 5)` completes in
  O(1) extra memory and does not OOM (observable via RSS-flat, the pattern used
  elsewhere). Pick (a) if the counter lands cheaply, else (b).

**DEFERRALS**

- `fold`/`scan`/`flat_map`/`peekable` and the `DoubleEndedIterator` family.
- C-backend lowering of the lazy tower (depends on Option/trait dispatch — outside
  the emit-c subset, `emit_c.cpp:547`).

---

## v62 — Stdlib runtime: monotonic clock, env vars, seeded global RNG

**Theme:** Three small deterministic-when-seeded runtime capabilities, each a thin
libc-wrapper builtin (typecheck registration + codegen lowering to a C runtime fn).

**CORE**

1. **`struct Instant{ms:i64}` + `instant_now() -> Instant ! { io }`** over
   `clock_gettime(CLOCK_MONOTONIC)` / `mach_absolute_time` (already used for the
   async timer, `codegen.cpp:831/1118`), with elapsed arithmetic reusing the
   existing `Duration` struct (`main.cpp:1625`).
2. **`env_var(&String) -> Option<String> ! { alloc, io }`** over `getenv`
   (already used in the driver, `main.cpp:3007`) + **`env_var_set(&String,
   &String) -> i64 ! { io }`** over `setenv`.
3. **`rng_seed_global(seed:i64)` + `rand_global() -> i64`** over a process-global
   (or thread-local) seeded RNG, default seed from `KARDASHEV_SEED`, layered on
   the existing `Rng` struct (`main.cpp:1591`); wire a `--fuzz-seed` CLI arg that
   exports `KARDASHEV_SEED`.

All follow the proven `getOrInsertFunction` libc-wrapper pattern (as `fopen`).

**ACCEPTANCE GATE**

- `smoke_test_instant.sh`: two `instant_now()` readings are monotonic; after a
  sleep the delta is `>= requested` and within a wide tolerance (e.g. `[50ms,
  500ms]`, to avoid CI flake) — assert monotonicity + lower bound, not a tight
  window (AOT).
- `smoke_test_env.sh`: `env_var("TEST_VAR")` returns `Some("v")` when
  `TEST_VAR=v`, `None` for an absent var, and a `setenv` round-trip reads back
  `Some("123")`.
- `smoke_test_rng_seeding.sh`: the same `KARDASHEV_SEED` reproduces an identical
  5-value sequence; a different seed differs.
- JIT==AOT where applicable; deterministic across runs with fixed env.

**DEFERRALS**

- Process/subprocess control (spawn/exec); wall-clock/system-time formatting;
  cryptographically-secure RNG. (Buffered I/O + file metadata → v63.)

---

## v63 — Stdlib I/O depth: buffered reader + file metadata

**Theme:** Two deterministic-with-temp-files file capabilities.

**CORE**

Reuses the existing I/O scaffolding: `enum IoError { IoNotFound,
IoPermissionDenied, IoOther }` (`main.cpp:732`), `fs_read_to_string` →
`Result<String, IoError>` over `fopen` (`main.cpp:743`), `fs_write`
(`main.cpp:753`).

1. **Buffered line reading:** `struct BufReader<T>` holding fd + heap buffer +
   position; `buf_reader_new(&String) -> Result<BufReader, IoError> ! { io, alloc }`
   and `buf_read_line(&mut BufReader) -> Option<String> ! { alloc }` (refills via
   `read()`, yields `\n`-delimited lines, `None` at EOF).
2. **File metadata:** `struct Metadata{size:i64, is_dir:bool, is_file:bool,
   mtime:i64}` + `fs_metadata(&String) -> Result<Metadata, IoError> ! { io }` over
   `stat()`, plus thin `fs_is_dir` / `fs_is_file` wrappers.

New C runtime fns lowered from codegen, exactly like the existing file ops.

**ACCEPTANCE GATE**

- `smoke_test_buf_reader.sh`: write a 3-line temp file, open with
  `buf_reader_new`, call `buf_read_line` 4 times → 3 exact lines + `None`; an
  empty file → immediate `None`.
- `smoke_test_fs_metadata.sh`: a 100-byte temp file → `Ok` with `size 100`,
  `is_file true`, `is_dir false`; a temp dir → `is_dir true`; a missing path →
  `Err(IoNotFound)`.
- JIT==AOT, LSan-clean, deterministic.

**DEFERRALS**

- `BufWriter`, seek/random-access, directory listing/walk, permissions/chmod,
  symlink resolution, and mtime-based incremental-build wiring.

> Portability note: `struct stat` field layout differs Linux vs macOS — read
> `size`/`mtime` defensively, mirroring the timespec-layout handling at
> `codegen.cpp:1131`.

---

## v64 — Diagnostics depth: more error codes, borrow codes, multi-char spans, value-printing asserts

**Theme:** Four cheap-but-high-leverage diagnostics improvements.

**CORE**

> Bundles four sub-features — borderline on size. Sequence internally and be ready
> to split: the multi-char-span axis is the heaviest and may slip to **v64.x**;
> the other three are independently shippable, lower-risk — land them first so the
> version closes even if spans defer.

1. **Expand the error-code table** from ~8 (`errorCodes()`, `main.cpp:2412`) to
   ~20: enumerate ~12 more patterns from typecheck/borrow_check; replace the
   substring scan (`classifyError`, `main.cpp:2462`) with a deterministic
   priority-ordered classifier (most-specific first); add `E0502/E0505/E0506/
   E0597/E0499` for borrow errors (currently uncoded). *(Land first.)*
2. **Value-printing asserts:** `assert_eq!`/`assert_ne!` currently do
   `if $a==$b {} else { return 1; }` — silently returning 1 with no value
   (`main.cpp:1648`). Print actual left/right values (and position) on failure via
   a `__assert_report` builtin using `Display`. *(Land second.)*
3. **`--explain Exxxx` entries** for every new code (existing `--explain` at
   `main.cpp:3583`). *(Land with #1.)*
4. **Multi-character spans:** add `endColumn` to `ParseError`/`TypeError`/
   `BorrowError`/`Lint`, propagate child-expr end positions, render `^^^^`
   underlines covering the full offending subexpression. *(Heaviest — split to
   v64.x if it balloons.)*

**ACCEPTANCE GATE**

- `smoke_test_diagnostics.sh`: a struct type-mismatch and a scalar type-mismatch
  emit **distinct** codes; move-after-borrow emits `error[E050X]`; `--explain`
  for each new code prints a multi-line explanation; `assert_eq!(1,2)` prints
  `left=1 right=2` before failing; `assert_ne!(1,1)` similarly; no cascading
  duplicate diagnostics; existing diag smoke tests green.
- (If spans land this version) a type error on `let x: bool = 1 + 2;` underlines
  all of `1 + 2`, not just `1`.

**DEFERRALS**

- Cross-function breadcrumb context ("in function foo:"); nested multi-line span
  rendering with multiple labels; structured JSON diagnostics; suggestion/fix-it
  hints. (Multi-char spans → v64.x if split.)

---

## v65 — Codegen perf: param-reg lowering + inline/opt hints (close the fib gap)

**Theme:** Reduce the documented ~1.2× `fib` gap with opt-in codegen contracts
gated by attributes (no default behavior change).

**CORE**

Attribute infra exists (`parseAttributes`, `parser.cpp:2315`; `no_alloc`/
`no_panic`/`no_io` from v48 at `:2349–2351`). The fib gap is real and
`BENCHMARKS.md:24,68–69` attributes it to recursive call/param overhead — so
`param_regs` targets the documented cause. (This is the ~1.2× **fib** gap; the
2.2× loop gap was already closed in v0.51.0.)

1. **`#[codegen(param_regs)]`:** for by-value Copy params, skip the alloca+store
   at fn entry (`codegen.cpp:9505`) and SSA the argument directly; leave
   ref/droppable params untouched.
2. **`#[codegen(inline)]`:** set LLVM `InlineHint`; auto-detect small leaf-/
   recursive functions and apply implicitly.
3. **OptLevel-aware per-fn hints:** when a fn is `param_regs`/`no_alloc`/`total`
   and `optLevel >= O2`, add `AlwaysInline`/appropriate attrs via a per-`FnDecl`
   intent map consulted at call emission.

Parse the new attribute alongside `no_alloc`/`no_panic` in `parser.cpp`.

**ACCEPTANCE GATE**

- `smoke_test_codegen_contracts.sh` stays green + new `param_regs`/`inline` cases:
  `fib(40)` correctness unchanged; a `#[codegen(param_regs)]`+`#[codegen(inline)]`
  fib at `-O2` emits **no per-call entry alloca** for the param (verified via
  `-emit-llvm-ir` grep — the reliable, blocking signal).
- Performance: **advisory, not blocking** — assert "measurably not slower than the
  unannotated baseline" rather than a hard `>=5%`, since `mem2reg` already SSAs
  the alloca at `-O2` (`codegen.cpp:9500` comment) so the remaining win can be
  below noise. Record the delta in `tests/smoke_test_bench.sh` output.

**DEFERRALS**

- Bounds-check elision for loop-invariant (non-literal) indices;
  `#[codegen(vectorized)]` + verification; whole-program LTO/PGO. The closing here
  is **incremental — 1.0× is not guaranteed.**

---

## v66 — Test infrastructure: borrow-checker differential fuzzer + sanitizer sweep + property harness

**Theme:** Harden correctness with three reusable test rigs — pure test infra (no
compiler changes) that the prior soundness work earns.

**CORE**

Precedent exists: seeded fuzzers (`smoke_test_fuzz_arith/control/div/memsafety.sh`,
`smoke_test_compiler_fuzz.sh`, `smoke_test_differential.sh`,
`smoke_test_grammar_conformance.sh`) and ASan wiring
(`smoke_test_phase164/165.sh`).

1. **`smoke_test_fuzz_borrow.sh`:** a seeded generator emitting 100+ random
   programs exercising mutable refs, reborrows, ref returns, field/tuple access
   through refs, closure captures, and match-through-`&T`, each carrying an inline
   **SOUND/UNSOUND oracle**; assert every SOUND program compiles and every UNSOUND
   one errors (zero false pos/neg). **Highest-value rig** — hand-verify a sample
   of generated programs so the oracle does not silently bless a false-negative.
2. **`smoke_test_asan_ubsan_c_backend.sh`:** sweep ≥10 in-subset C-backend
   programs (struct/enum/ref/for/String/Vec/Drop/closure/generic) under
   `-fsanitize=address,undefined`, assert clean, plus ≥3 intentional-UB programs
   caught; **skip gracefully if clang/ASan absent**.
3. **`property_harness`:** ≥15 prelude invariants (`vec_remove`, HashMap
   insert/remove, `str_split` roundtrip, BTree ordering) over 50 seeded inputs
   each, JIT==AOT. **Safest to trim** if size pressure hits.

Wire all three into `BUILD.bazel` + `Makefile.local`.

**ACCEPTANCE GATE**

- All three scripts pass: fuzz_borrow runs 100+ seeded programs with oracle-exact
  accept/reject; the asan_ubsan sweep reports zero sanitizer errors on in-subset
  programs and catches all intentional-UB cases (or cleanly skips when clang is
  unavailable); property_harness runs ≥15 properties × 50 inputs with no failures
  and JIT==AOT. Deterministic under a fixed seed.

**DEFERRALS**

- TSan concurrency fuzzing (needs instrumented codegen); a full grammar-conformance
  EBNF corpus (2000+ cases); whole-program type+effect interaction fuzzing — each
  a follow-on rig.

---

## What stays deferred — the mega-arcs (and why)

These are **deliberately excluded** from v54–v66 because each is a multi-session
arc whose blocker is the *environment*, not the design — they cannot be honestly
closed inside a single per-version cadence here:

1. **Real bootstrap.** A self-hosting Kardashev compiler that emits the full
   language (not the i64/bool/struct/enum subset of v20). Requires the whole
   stdlib + generics + traits to be expressible and stable in Kardashev itself —
   an arc that grows as the language does, not a fixed task.
2. **Package registry / ecosystem.** Blocked by the Bazel sandbox (no network /
   registry host available in this environment), so it cannot be built or tested
   end-to-end here.
3. **WASM + Windows backends.** New target triples + ABIs + a CI matrix this
   environment does not provide; each is its own backend the size of the existing
   C backend.
4. **Language spec → 1.0.** A normative spec, conformance suite, and a stability
   guarantee — a documentation-and-process arc that should trail, not lead, the
   feature work above.

The cadence for v54–v66 is unchanged: implement the tractable core, gate it with a
bash-runnable smoke test on **both** platforms, document the deferrals honestly,
then close out with a release. Implement in order; each version is self-contained
and leaves the tree green.
