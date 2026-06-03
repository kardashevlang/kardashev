# ROADMAP v81–v90 — the practical-systems-language turn, after v0.80.0

Designed against three read-only surveys of the real tree (effect system,
self-hosting completeness, practical-language gaps), then fact-checked against
the compiler. The maintainer's directive (translated): **"implement the whole
roadmap; drastically SIMPLIFY effects and make them OPT-IN, center the language
on Result + ownership; grow SELF-HOSTING; maximize completeness / optimization /
efficiency."**

Each version is a **tractable per-version increment** — a real, tested core
shippable in one session with a JIT==AOT differential smoke gate, ending
CI-green on ubuntu + macOS, plus honest deferrals. The 4 XL mega-arcs (real
self-hosting bootstrap, hosted package registry, WASM/Windows backends,
mechanized 1.0 proof) remain deferred — not per-version tractable.

> **Hard backward-compat constraint (load-bearing):** ~235 existing smoke tests
> and ~192 prelude functions carry explicit `! { io }`/`! { alloc, panic }`
> rows. The effects arc (v81–v83) MUST keep every one of them passing. The pivot
> is *opt-in*, not *removal*: an explicit `! { ... }` row stays **strictly
> checked**; the change is only that an *absent* row no longer means "asserted
> pure". This is grounded in the survey finding that **codegen never reads effect
> rows** (typecheck.cpp:9165; codegen panic/alloc gating is an independent
> AST scan in codegen.cpp:322–419) — so the whole arc is **zero codegen impact,
> zero runtime cost**.

> **Design pivot ordering.** Effects-opt-in leads (v81–v83) because it is the
> design fork everything downstream assumes; then the Result+ownership ergonomics
> that *replace* effects as the day-to-day error story; then self-hosting growth
> (v84–v86) which benefits from the simpler surface; then the practical
> systems-gaps (v87–v90) ordered by leverage × dependency.

---

## ARC A — Effects: simplify & make opt-in (v81–v83)

### v81 — effects become opt-in (absent row = unchecked, not asserted-pure)

**STATUS: ✅ SHIPPED (v0.81.0).** `FnDecl.sawEffectRow` (threaded from the
parser's `sawEffectRow_`) distinguishes an absent row from an explicit one;
`checkEffects` gates the undeclared-effect loop on it (an absent row is
unchecked + still inferred/propagated; an explicit row — incl. `! {}` — is
strictly checked). `--effects=strict` restores the old rule. Codegen contracts +
user-effect exhaustiveness unchanged. Backward-compatible (all explicit rows
still enforced — caught + updated ~15 negative effect *unit* tests that had
relied on "no row ⇒ pure", giving them explicit `! {}`). Gate:
`smoke_test_effects_optin.sh` (7 cases). **Deferred:** `#[allow(missing_effect)]`
+ migration lint (v82).

**Theme:** Flip the default. Today `fn f() { print(42); }` is an error
(E0710 "uses effect io but declares none"); after v81 it compiles, while
`fn f() ! { } { print(42); }` (an *explicit* empty row = a pure assertion) still
errors. The headline mandate, done backward-compatibly.

**CORE.**
- (1) Distinguish "no row" from "explicit row" end-to-end. The parser already
  sets `sawEffectRow_` (parser.cpp:1124–1126) and threads it to extern decls
  (parser.cpp:985–987); thread the same bit onto `ast::FnDecl` (a new
  `bool sawEffectRow` beside `effects`, ast.hpp:769 EffectRow context / the
  FnDecl struct) for fn/trait-method/fn-pointer sites (parser.cpp:1063, 2434,
  1907, 1951).
- (2) In `checkEffects()` (typecheck.cpp:3694–3739), gate the
  "uses effect X but does not declare it" loop (lines 3701–3712) on
  `fn.sawEffectRow`. If the fn wrote a row (including `! { }`), enforce it
  exactly as today; if it wrote none, **skip the undeclared-effect error** but
  still *infer* the effect set (the inference at 3698–3699 already runs) and
  propagate it to callers, so an annotated caller still sees the callee's real
  effects.
- (3) Keep the v48 codegen contracts (`#[codegen(no_alloc/no_panic/no_io)]`,
  typecheck.cpp:3719–3738) **always enforced** — they check emitted code, not
  declarations, and must hold regardless of opt-in.
- (4) Keep `checkExhaustiveEffects()` (typecheck.cpp:2928 / 9879–9905) for
  *user-defined* effects unchanged: a `perform E::op` that reaches `main`
  unhandled is still an error (a real soundness property, not a style rule).
- (5) Add a `--effects=strict` flag (default `opt-in`) that restores the old
  "absent row ⇒ asserted pure" behavior for anyone who wants the old discipline,
  reusing the same `sawEffectRow` gate.

**GATE.** `smoke_test_effects_optin.sh`: (a) `fn f(){ print(42); }`
(no row) compiles and runs, JIT==AOT; (b) `fn f() ! { }{ print(42); }`
(explicit empty) still errors with E0710; (c) an annotated caller
`fn g() ! { io }{ f() }` still type-checks; (d) `--effects=strict` makes (a)
error again; (e) the full existing `smoke_test_effects*.sh` trio passes byte
-for-byte unchanged. Plus the whole `tests/smoke_test_*.sh` sweep stays green
(all 192 prelude rows still enforced because they are explicit).

**DEFERRALS.** No per-fn `#[allow(missing_effect)]` attribute yet (v82). No
edition boundary (effects stay opt-in for everyone, no `edition 2027` concept).
Migration lint deferred to v82.

---

### v82 — Result + ownership become the error story; effect ergonomics shrink

**STATUS: ✅ SHIPPED (v0.82.0).** `fn main() -> Result<T,E>` via a codegen i64 exit-code wrapper (Ok→0/Err→1; AOT exit code, JIT prints); `#[allow(missing_effect)]` attribute (FnDecl.allowMissingEffect, consulted in checkEffects, silences strict mode); `result_flatten`/`option_flatten` prelude combinators. v81's opt-in `?` already covers the no-row `?` audit. Gate: `smoke_test_result_main.sh`. C backend refuses a Result-main cleanly. **Deferred:** the `-W effect-unchecked` migration lint (needs inferred effects exposed) + custom Error-trait hierarchy/backtraces.

**Theme:** With effects now optional, make `Result<T,E>` + `?` + ownership the
*primary* error/resource story so users reach for effects only when they truly
want effect typing. Centers the language where the mandate asks.

**CORE.**
- (1) `?`-operator completeness audit + fixes: ensure `?` works in any
  `Result`-returning fn **without** requiring a `panic`/`unwind` row (since rows
  are now opt-in), and that `?` on an `Option` in an `Option`-returning fn works.
  Touch typecheck `?`-lowering and the prelude `From`-conversion path.
- (2) `#[allow(missing_effect)]` per-fn attribute (parser attribute infra exists,
  parser.cpp:2315+): suppresses the *strict-mode* undeclared-effect error for one
  fn, so a codebase can run `--effects=strict` with surgical opt-outs. In
  `checkEffects()` (typecheck.cpp:3694) skip the loop when the fn carries it.
- (3) Result/ownership ergonomics in the prelude: `Result::ok()/err()`,
  `ok_or`/`ok_or_else` (Option→Result), `Result::context(msg)` /
  `map_err`-with-context for error-chaining, and a `?`-friendly
  `main() -> Result<(), E>` entrypoint (codegen lowers a non-zero exit on `Err`).
- (4) `kardc -W effect-unchecked` migration lint (lint.cpp): warns on an
  un-annotated fn that *infers* a non-trivial effect set, helping teams that want
  to adopt rows. Non-fatal, off by default; `#[allow(missing_effect)]` silences.

**GATE.** `smoke_test_result_ergonomics.sh`: (a) a no-row fn using `?` on a
`Result` compiles and propagates `Err`, JIT==AOT; (b) `main() -> Result<(),E>`
returns exit 0 on `Ok`, non-zero on `Err`; (c) `ok_or`/`context` round-trip;
(d) `-W effect-unchecked` prints exactly one warning for an effectful no-row fn
and none for a pure one; (e) `#[allow(missing_effect)]` silences strict mode.

**DEFERRALS.** Custom `Error` trait hierarchy / backtraces (heavy). `?` across
async boundaries beyond what v32 already supports. No `try { }` block.

---

### v83 — collapse the effect surface to one opt-in feature set + docs

**STATUS: ✅ SHIPPED (v0.83.0) — scope adjusted.** `div` demoted to an extension label behind `--effects=extended` (g_effectsExtended + isBuiltinEffect gate; 0 real uses); `docs/effects.md` rewritten around the v81 opt-in model + `--effects` modes; `kardc --explain effects` consolidated guide added. **Adjusted from plan:** `share` is KEPT as a recognized core-adjacent label (it is auto-inferred by thread/channel primitives and declared by ~15 unit + dozens of smoke tests; gating it needs inferred-filtering + edge-case row rewrites with no real simplification gain — the opt-in model already removes the *requirement*). The prelude row-trim pass is likewise deferred (churn-heavy, low value). Gate: `smoke_test_effects_surface.sh`.

**Theme:** "Drastically simplify." Reduce the surface area users must learn:
prune redundant built-in labels, unify the docs around opt-in, and make the
*remaining* effect machinery a coherent single feature rather than scattered
rules — without removing the soundness-load-bearing parts.

**CORE.**
- (1) Built-in label rationalization. Survey lists 5 core + 2 extension labels
  (`alloc, io, panic, async, unwind` + `share, div`, typecheck.cpp:3345–3349).
  Demote `div` and `share` to **opt-in extension labels** gated behind
  `--effects=extended` (they exist for niche analyses); the default vocabulary is
  the 5 core. `isBuiltinEffect()` (typecheck.cpp:3345) consults the mode.
- (2) Prelude row cleanup pass: where a prelude fn's row is *inferable*, keep it
  (explicit is documentation), but remove rows that exist only to satisfy the old
  asserted-pure default and add noise. Net effect: fewer rows in `main.cpp`,
  identical checking (verified by re-inferring).
- (3) Rewrite `docs/effects.md` around the opt-in model: "effects are an optional
  typed-side-channel; for everyday error handling use `Result` + `?`; reach for
  `! { ... }` rows when you want to *prove* a fn is pure / IO-free / non-alloc
  (esp. with `#[codegen(no_*)]`)." Document `--effects=strict|opt-in|extended`.
- (4) A single consolidated `kardc --explain effects` entry replacing the
  scattered E0710-family explanations.

**GATE.** `smoke_test_effects_surface.sh`: (a) `! { div }` errors by default but
compiles under `--effects=extended`; (b) the trimmed prelude still infers the
same effect sets (a checker that re-infers every prelude fn and diffs against the
declared row, asserting no *new* undeclared effect); (c) `--explain effects`
prints the consolidated text. Full smoke sweep green.

**DEFERRALS.** User-defined multi-shot / resumable effects (Koka-style) stay
deferred (XL, beyond parity). No removal of the user-effect `effect E { }` /
`perform` / `handle` machinery (it is real and tested; just de-emphasized in
docs).

---

## ARC B — Self-hosting completeness (v84–v86)

> Grounded in the self-hosting survey: the tower lives in
> `examples/selfhost/` (compile.kd 345 lines, structgen.kd 498, enumgen.kd 501).
> The self-hosted subset is i64/bool-only, all struct fields i64
> (structgen.kd:242–244 hardcodes `// type (i64)`), exactly one i64 enum payload
> (enumgen.kd:240–242). Each step keeps the **differential gate**: self-hosted IR
> → clang → native exit code MUST equal the host compiler's exit on equivalent
> `.kd`.

### v84 — heterogeneous struct fields + multi-payload enums in compile.kd

**STATUS: ✅ SHIPPED (v0.84.0).** (1) `structgen.kd` `SDef.fields` now stores per-field types (`Param{name,ty}`): `parse_structs` reads the type token via `ty_tag`, `ty_llvm` emits the real per-field LLVM type recursively (nested structs → `{ i64, { i64, i64 } }`), and `type_of`/`lower` for `SLit`/`Field` carry the field's declared type. (2) `enumgen.kd` variants now carry 1..N payloads: `EDef.variants: Vec<VDef{name,arity}>`, `ECon` holds `Vec<Box<Expr>>`, `Arm.binds: Vec<String>`; the enum layout widens to `{ i64 tag, i64 p0, …, i64 p<maxArity-1> }` (narrower variants leave trailing slots `undef`), with multi-`insertvalue`/`extractvalue` + positional bind. All-i64 structs and single-payload enums stay **byte-identical** (`{ i64, i64 }`), so the existing demo greps hold. **Gates:** extended `smoke_test_phase117.sh` (nested-struct + bool-field) and `smoke_test_phase118.sh` (2-payload, mixed-arity widest-second, 3-payload) — each self-hosted exit == host exit. **Exceeds plan** (3+ payloads tested, not just 2). **Deferred:** payloadless/nullary variants (`None` — needs paren-less match/construct syntax the toy parser lacks) and String/Vec fields (heap, v85+).

**Theme:** Data completeness — the two lowest-risk, highest-ROI self-hosting
unlocks (survey Increments 1 & 2).

**CORE.**
- (1) Heterogeneous struct fields (structgen.kd): extend the struct registry
  (`SDef`, structgen.kd:228–254) to store *per-field types* (i64 | bool |
  nested-struct#idx) instead of discarding the type token at lines 242–244.
  Refactor `type_of` (lines ~278–305) for heterogeneous field types; emit
  type-correct LLVM aggregates (`{ i64, i1, { i64, i64 } }`) with
  `insertvalue`/`extractvalue` per real field type.
- (2) Multi-payload + payloadless enum variants (enumgen.kd): `EDef` stores
  variant → payload-count (0, 1, or 2) instead of assuming one i64
  (enumgen.kd:226–251). Fixed-width representation `{ i64 tag, i64 p0, i64 p1 }`
  (pad unused slots) keeps codegen simple; `ECon` validates payload count
  (type_of ~292–294). Unlocks `Option { Some(i64), None }` and
  `Result { Ok(i64), Err(i64) }` *inside the self-hosted language*.

**GATE.** `smoke_test_selfhost_data.sh`: differential on
`struct P { x: i64, y: bool, z: Pair }` (build+field-in-if+three-field) and on
`enum Option { Some(i64), None }` / `enum Result { Ok(i64), Err(i64) }` — each
self-hosted exit code == host exit code.

**DEFERRALS.** String/Vec fields (needs the heap, v85+). >2 payloads. Recursive
structs (Box).

---

### v85 — references & strings in compile.kd

**Theme:** Reach completeness — `&T` parameters (survey Increment 3, the gate to
everything) and read-only `String` (Increment 4).

**CORE.**
- (1) `&T` parameter types (no returned refs, no `&mut`, no NLL — strictly
  by-ref params). New `Val { op, ty, borrowed }`; ref type tags (`ty=101+` for
  `&i64`, `&Struct`, …); parser handles `&Type` in param position; the
  self-hosted type-checker rejects returning a `&param` and taking `&` of a let
  local that outlives use (dangling). Codegen: `&T` → an i64 pointer; field
  access through a ref does `load` + `extractvalue`.
- (2) Read-only `String`: lexer recognizes `"..."` (emit a STRING token); parser
  `Expr::Str`; `ty=3`; codegen emits a module-level `@.str` constant per literal;
  builtins `str_len`/`str_char_at`/`str_eq` lowered as runtime calls. No string
  mutation, no `str_substring` (needs alloc) in this cut.

**GATE.** `smoke_test_selfhost_ref_str.sh`: differential on
`fn sum(p: &Point) -> i64 { p.x + p.y }` and `fn greet(s: String) -> i64 { str_len(s) }`
— self-hosted exit == host exit; plus a negative test that returning `&param` is
rejected by the self-hosted checker.

**DEFERRALS.** `&mut`, returned references, mutable strings, NLL. Generics/Vec in
the subset (v86). This is the survey's L-sized item — keep the rules simple.

---

### v86 — loops + Vec + a self-hosted-compiles-a-real-program milestone

**Theme:** Close enough of the gap that the self-hosted compiler can compile a
*nontrivial* program (not yet itself — that's the XL mega-arc — but a real
multi-function program with data + loops + a Vec).

**CORE.**
- (1) `while` as an expression / `for`-over-range in the self-hosted subset
  (loops are currently hardcoded in each phase, not user-programmable — survey
  gap #13). Codegen emits a real CFG (loop header/body/latch BBs) — the first
  time the self-hosted backend emits non-branch-free control flow.
- (2) Scalar `Vec<i64>` in the subset: `vec_new`/`vec_push`/`vec_get`/`vec_len`
  lowered to the runtime `kdvec` (mirrors the host's scalar-Vec C-backend
  support). Monomorphic i64 only.
- (3) A capstone differential test: a ~60-line self-hosted program (e.g. a
  tokenizer-and-sum over a Vec) compiled by the self-hosted compiler, run, and
  exit-matched against the host.

**GATE.** `smoke_test_selfhost_loops_vec.sh`: differential on a `while`-sum, a
`for`-range factorial, and the capstone Vec program — each self-hosted exit ==
host exit. CFG validity checked (clang accepts the emitted IR).

**DEFERRALS.** Generics/trait dispatch/closures/modules/effects in the subset
(the rest of the XL bootstrap, survey Increment 5 / "full bootstrap = v24–v27"
of that arc). HashMap. Recursion-through-Vec.

---

## ARC C — Practical systems-language gaps (v87–v90)

> Ordered by leverage × dependency from the practical-gaps survey. Runtime-first
> -class sized ints (v87) unblocks FFI structs (v88); stack arrays (v89) and the
> allocator/slice/iterator cleanup (v90) are independent follow-ons.

### v87 — runtime-first-class sized integers (i8–u64, f32)

**Theme:** Promote sized ints from const-only to **runtime** types (survey gap
#3 / Recommendation L). The single biggest "real systems code" unlock: memory
-efficient numerics + the prerequisite for C-ABI struct fields.

**CORE.** Add sized-int branches to the codegen Call / return-type / field-access
paths (codegen lowers `i8/i16/i32/u8/u16/u32/u64/f32` as their LLVM widths with
correct sign/zero-extension at boundaries), reusing the existing const-eval
sized-type logic (v64–v66). Allow `fn(x: i32) -> u16`, sized struct fields, and
sized array elements. Typecheck: relax the i64-hardcoded runtime builtin param
types (typecheck.cpp:387–483) to accept declared sized types with explicit
widening/`as` at call boundaries. Both LLVM and C backends.

**GATE.** `smoke_test_sized_runtime.sh`: a fn `fn add(a: u8, b: u8) -> u8`
wraps at 256 (JIT==AOT, and == C-backend exit); an `i32` struct field round-trips
its value; an `[u16]`-ish element read is correct. Overflow semantics match
`-fwrapv` for the C path.

**DEFERRALS.** 128-bit ints. SIMD vector types. f16. Saturating sized-int ops
beyond what v70 shipped for i64.

---

### v88 — repr(C) struct layout + FFI struct-by-value

**Theme:** C-ABI parity (survey gap #5 / Phase 178 deferred). Depends on v87
(sized fields). Unblocks calling real C APIs that pass structs by value.

**CORE.** `#[repr(C)]` attribute: parser recognition (attribute infra exists),
typecheck marks the struct, codegen computes **ABI-compliant field offsets**
(natural alignment, C field order, tail padding) for x86-64 / aarch64 psABI.
Enable struct arguments and struct returns in `extern "C"` signatures (extend the
scalar+ptr-only extern lowering at codegen.cpp:2418–2450 to pass small structs by
value per ABI and large ones by hidden-pointer/sret). Both LLVM and C backends
(the C backend gets it nearly free — emit the C struct verbatim).

**GATE.** `smoke_test_repr_c.sh`: define `#[repr(C)] struct Point { x: i32, y: i32 }`,
declare `extern "C" fn dist2(p: Point) -> i64`, link against a tiny C `dist2`,
and assert the result (JIT==AOT==C-backend). A layout test asserts
`size_of!`/offsets match the equivalent C struct (compared against a clang-built
oracle).

**DEFERRALS.** `#[repr(packed)]` / bit-fields (survey gap #6 — sub-byte layout,
its own version later). Unions. Windows x64 ABI specifics (the WASM/Windows
mega-arc). Variadic C functions.

---

### v89 — stack arrays `[T; N]` (first-class, alloca-backed)

**Theme:** Fixed-size stack temporaries without heap (survey gap #4 /
Recommendation L). Pure codegen layer — no type-theory.

**CORE.** Add `[T; N]` as a runtime array type (not just array literals):
type-system recognition of the array type, `let arr: [i64; 16]`, indexing with
bounds-checked access (reuse the existing panic-on-OOB path), and codegen via
LLVM `alloca [N x T]` with per-element drop tracking at scope exit (reuse the
struct-field drop machinery). Const `N` from the const-eval engine (already
exists). C backend emits a C array. Enables a 256-bucket histogram, scratch
buffers, etc. on the stack.

**GATE.** `smoke_test_stack_array.sh`: a `[i64; 8]` histogram fills and sums to
the right total (JIT==AOT==C-backend); an out-of-bounds index panics; a
`[Droppable; 4]` drops each element exactly once (MALLOC_CHECK + RSS-flat at
500k iterations, reusing the field-move drop harness).

**DEFERRALS.** Multi-dimensional arrays. `[T; N]` as a fn return (large arrays
need sret — fold into v88's by-value work). Array-of-`&T`.

---

### v90 — closing-pass: mutable-slice C backend + pluggable allocator + LOC/perf cleanup

**Theme:** "Maximize completeness / optimization / efficiency." A consolidation
version that closes three independent smaller gaps and does an optimization pass,
closing the v81–v90 arc.

**CORE.**
- (1) Mutable slices in the C backend (survey gap #2 / Recommendation S): remove
  the categorical `&mut [T]` refuse in emit_c.cpp; lower a slice as a
  `{ T* ptr; i64 len }` pair with a mutation flag, reusing the scalar-slice
  logic. Eliminates one of the last C-backend categorical refuses.
- (2) Pluggable allocator (survey gap #7 / Recommendation M): a `GlobalAlloc`
  trait (`alloc`/`dealloc`/`realloc`) in the prelude; a codegen hook so heap
  call-sites (codegen.cpp:1260–1280) dispatch through a selected allocator when
  one is registered, else fall back to libc malloc/free. Completes the `no_std`
  story (bump/arena allocator possible). Uses the v33 `unsafe` for raw-ptr math.
- (3) Optimization + LOC efficiency pass: ensure the v87 sized-int paths and v89
  arrays vectorize (verify the v51 TM/TTI-in-PassBuilder fix still applies to the
  new codegen paths — survey/MEMORY: missing TTI killed vectorization once);
  route any new runtime builtins through the v67 `createRuntimeFunction` helper;
  trim duplicated layout code shared between v88/v89.

**GATE.** `smoke_test_v90_close.sh`: (a) a C-backend program sorting via a
`&mut [i64]` slice produces a sorted array == LLVM exit; (b) a program with a
registered bump `GlobalAlloc` allocates+frees with zero libc malloc calls
(verified by an `LD_PRELOAD`/symbol-intercept count or a malloc-counter); (c) a
sized-int hot loop emits vector IR (`grep` for `<N x i32>` in `--emit-llvm`);
(d) full smoke sweep + `make test` green both platforms.

**DEFERRALS.** `#[repr(packed)]`/bit-fields, file-I/O depth (Phase 189 — streams
/seek/readdir/env), networking, named lifetimes/full NLL, element-generic
iterator adaptors, multi-shot effects — each is its own future version or an XL
mega-arc, called out honestly in the survey.

---

## Out of scope (the XL mega-arcs, still deferred)

Real self-hosting bootstrap (the self-hosted compiler compiling *itself* — v84–v86
grow the subset but the full bootstrap is multi-version: generics + trait
dispatch + closures + modules + effects in the subset), a hosted package
registry (sandbox-blocked), WASM/Windows backends, full file-I/O + networking
(Phase 189, non-deterministic-testing blocked), named lifetimes + full NLL
(multi-month type theory), element-generic iterator fusion, and a mechanized
spec→1.0 proof. Each is multi-session and/or environment-blocked, not
per-version tractable.

---

## Sequencing rationale (leverage × dependency)

```
v81 effects opt-in            ← the design pivot; everything assumes it
 └ v82 Result+ownership story  ← what *replaces* effects day-to-day
    └ v83 shrink effect surface ← finish the simplification, docs
v84 selfhost: data            ← lowest-risk highest-ROI selfhost unlock
 └ v85 selfhost: refs+strings  ← the gate to feature reach
    └ v86 selfhost: loops+Vec   ← compile a real program milestone
v87 sized ints (runtime)      ← biggest practical unlock; prereq for…
 └ v88 repr(C) + struct FFI    ← C-ABI parity (needs sized fields)
v89 stack arrays              ← independent codegen unlock
v90 close: slices+alloc+opt   ← consolidation + optimization pass
```

Effects-opt-in leads because it is the irreversible design fork. Result+ownership
follows because, with effects optional, it must carry the everyday error story.
Self-hosting sits in the middle (benefits from the simpler surface, gates the
bootstrap mega-arc). The systems-gaps close last, ordered so each depends only on
what shipped before it. Every version: real tested core, JIT==AOT smoke gate,
CI-green both platforms, honest deferrals — the established cadence.
