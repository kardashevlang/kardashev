# ROADMAP v67–v80 — production depth, after v0.66.0

Designed by a 14-agent survey-→-synthesize-→-critique workflow over the real
tree, then **fact-checked against the compiler** (several first-draft versions
were dropped/narrowed because their premise was already shipped — see notes).
Each version is a **tractable per-version increment**: a real core shippable in
one version with honest deferrals, ending CI-green on ubuntu + macOS. The 4 XL
mega-arcs (real bootstrap, package registry, WASM/Windows backends, mechanized
1.0 proof) remain deferred — they are not per-version tractable.

> **Verification note (goal: 完成度の検証):** the workflow's critic flagged, and
> direct testing confirmed, that the project record over-claimed some features.
> Most importantly **match guards (`pat if cond =>`) do NOT parse today**
> (`expected =>, got KwIf`) despite the v26/Phase-141 record claiming them — so
> v68 is genuine, not a re-do. Conversely, sized integers (i8…u64) as runtime
> fn params/fields, `f32`, `checked_*`/`wrapping_*`, and `hashmap_values` **are**
> already shipped — so the first-draft "sized ints" version was dropped and
> v70/v72/v77 were narrowed to only their genuinely-missing parts.

---

## v67 — codebase optimization & efficiency cleanup (最適化・効率化)

> **Status:** ✅ SHIPPED v0.67.0. Added a `makeRuntimeFn(name,ret,params)` helper
> in codegen.cpp and routed the representative single-block runtime builtins
> (`monotonic_millis`/`rng_seed_global`/`__assert_report`) through it
> (byte-identical IR; behavior-preserving). `smoke_test_loc_audit.sh` gates
> helper adoption (≥4 sites) + behavior preservation. HONEST FINDING (7-reviewer
> audit): the codebase was already ~90% tight — no egregious waste, only
> ~6–10% factorable-but-defensible boilerplate — so this is a focused small pass,
> not a rewrite. DEFERRED (with rationale): remaining multi-block builtins
> through the helper (mechanical); a shared test harness lib (kept per-script for
> standalone-runnability + Bazel-runfiles risk); ROADMAP↔CHANGELOG overlap (kept
> — different audiences).

**Theme:** Pay down the ~250–400 LOC of factorable boilerplate the v54–v66 audit
found. Pure refactor, **zero behavior change**.

**CORE.** (1) Add a `createRuntimeFunction(name, retTy, paramTys) -> {Function*,
entry BB}` helper in codegen.cpp and route the repeated libc-wrapper skeletons
(v62/v63/v64 runtime builtins) through it. (2) Factor the duplicated AST-walk
visitors (`exprTakesAddr`/`exprCallsName`, v65) onto one shared recursion. (3)
Extract the repeated test preamble (KARDC-finder + `diff_run`) into
`tests/lib/harness.sh` sourced by the smoke scripts. (4) Trim README line-93
into a compact table and de-duplicate ROADMAP↔CHANGELOG narration.

**GATE.** `make test` + the full `tests/smoke_test_*.sh` sweep stay green (run
directly, not via the masked exit code); a `smoke_test_loc_audit.sh` asserts the
boilerplate counts dropped (fewer `FunctionType::get`/`BasicBlock::Create`
call-sites) and no behavior diff on a fixed sample of programs.

**DEFERRALS.** Correctness-load-bearing prelude guards left intact. No new
features. (Some doc dedup is cosmetic and can slip.)

---

## v68 — match guards (`pat if cond =>`)

> **Status:** ✅ SHIPPED v0.68.0. `MatchArm.guard` + parser (`if` before `=>` on
> all three arm paths) + typecheck (bool in binding scope; effect flows in) +
> guard-aware exhaustiveness (guarded arm doesn't count → E0004 for guarded-only)
> + codegen fall-through via per-guarded-arm **suffix decision trees**
> (`compileDecisionTree(firstArm=i+1)`), chaining across multiple guards +
> ast_clone. `smoke_test_match_guards.sh` (payload/bare/chained/binding/by-ref +
> 3 rejects), JIT==AOT. DEFERRED: a by-value guarded arm binding a non-Copy
> payload is rejected (suffix re-extraction would double-move — use `match &x`);
> `--emit-c` refuses guarded matches (subset).

**Theme:** Make guarded match arms actually work (verified missing today).

**CORE.** Add a `guard` field to `MatchArm` (ast.hpp:299); parse `pat if cond
=>`. Typecheck the guard as `bool` in the arm's binding scope (pattern bindings
visible to the guard); fold the guard's effect row into the match's row. Lower in
pattern_match.cpp: a guarded arm tests the pattern AND the guard; on guard-false
it **falls through** to the next arm (not the wildcard). Exhaustiveness must not
count a guarded arm as total.

**GATE.** `smoke_test_match_guards.sh`: `match x { Some(n) if n>5 => 1, Some(n)
=> 2, None => 3 }` → 1/2/3 for 7/3/None (JIT==AOT); a guarded-only match over a
non-wildcard scrutinee is rejected as non-exhaustive.

**DEFERRALS.** let-else / never-type stay deferred (a diverging guard body still
uses the fresh-Var workaround).

---

## v69 — range patterns (`0..10 =>`) + `@`-bindings (`name @ pat`)

**STATUS: ✅ SHIPPED (v0.69.0).** Integer range patterns `lo..hi` / `lo..=hi`
land as **sugar over v68 guards** — a range arm binds the scrutinee to a fresh
name and produces `(v >= lo) && (v < hi)` (or `<= hi`), reusing the suffix-tree
fall-through + guard-aware non-exhaustiveness. They chain, combine with explicit
`if` guards, and don't count toward coverage (range-only → E0004, needs `_`). The
`@` token + `AtPat` node exist but **`@`-bindings are DEFERRED** (rejected with a
clear message — bind in the arm body) along with nested/char ranges. Gate:
`smoke_test_range_pat.sh` (6 cases, JIT==AOT).

**CORE.** Add `RangePat{lo,hi,inclusive}` and `AtPat{name,inner}` to the pattern
hierarchy (ast.hpp:118-169). Parse `0..10 =>`, `0..=9 =>`, `name @ pat`.
Typecheck: RangePat requires an integer/char scrutinee with `lo<=hi`
const-checked; AtPat binds the whole matched value at the inner pattern's type.
Lower both in pattern_match.cpp (range = `lo<=v && v<hi/<=hi`; `@` = bind + recurse).

**GATE.** `smoke_test_range_at_pat.sh`: `match age { 0..13 => 0, 13..18 => 1, _
=> 2 }` → 0/1/2 for 5/15/30; `match p { whole @ Point{x,y} => use(whole,x,y) }`
binds all three (JIT==AOT).

**DEFERRALS.** Range patterns do not contribute to integer-domain exhaustiveness
(a full-domain range still needs `_`).

---

## v70 — saturating arithmetic + integer bit intrinsics

> Narrowed: `checked_*` and `wrapping_*` already exist (v33 Phase 181) — verified.

**CORE.** Add `saturating_add/sub/mul_i64` (clamp to i64 MIN/MAX via the existing
overflow intrinsics) and bit intrinsics `popcount`/`leading_zeros`/
`trailing_zeros`/`rotate_left`/`rotate_right`/`byteswap` (LLVM `ctpop`/`ctlz`/
`cttz`/`fshl`/`bswap`). Builtins lowered in codegen via the v67
`createRuntimeFunction` helper + typecheck schemas.

**GATE.** `smoke_test_bits_saturating.sh`: `saturating_add(i64::MAX,1)==i64::MAX`;
`popcount(7)==3`, `leading_zeros(1)==63`, `rotate_left(1,1)==2` (JIT==AOT).

**DEFERRALS.** Sized-int (i8…u32) variants of these ops stay i64-only (follow-on).

---

## v71 — string formatting specs (`{:width}`, alignment, fill, `{:x}`)

> Replaces the dropped "sized integers" version (i8…u64 already first-class).
> Deferred from v27.

**CORE.** Extend the `format!`/`print!`/`println!` desugar (parser
`parseFormatMacro`) to parse format specs after `:` — width, fill+align
(`{:>8}`/`{:<8}`/`{:^8}`/`{:08}`), and `{:x}`/`{:b}`/`{:o}` radix for integers —
lowering to prelude helpers (`str_pad_left`/`str_pad_right`/`int_to_radix`) over
existing String builtins. Pure-prelude + parser; no codegen changes.

**GATE.** `smoke_test_format_specs.sh`: `format!("{:>5}", 42)` == `"   42"`,
`{:08}` of 42 == `"00000042"`, `{:x}` of 255 == `"ff"` (JIT==AOT).

**DEFERRALS.** `{:.prec}` float precision, sign-aware padding, and named/`$`
dynamic widths deferred.

---

## v72 — f64 transcendental math library

> Narrowed: `f32` runtime + sqrt/floor/ceil/abs already exist — verified.

**CORE.** Extend f64 math to `sin/cos/tan/atan2/asin/acos/atan`,
`log/log10/log2/ln/exp/exp2`, `pow/powi`, `sinh/cosh/tanh`,
`round/trunc/fmod/copysign/hypot` — via LLVM intrinsics where available, else
libm externs routed through the v67 `createRuntimeFunction` helper.

**GATE.** `smoke_test_float_math.sh`: `f64_sin(0.0)==0.0`, `f64_cos(0.0)==1.0`,
`f64_pow(2.0,10.0)==1024.0`, `f64_log2(8.0)==3.0`, `f64_exp(0.0)==1.0` within ε
(JIT==AOT).

**DEFERRALS.** Decimal/arbitrary-precision, rounding-mode control, f16/bf16.

---

## v73 — associated consts in traits + where-clauses on impls/type-aliases

**CORE.** Add `AssocConstDecl{name,type,default?}` to `TraitDecl` and
`AssocConstDef` to `ImplDecl`, mirroring the existing `AssocTypeDecl` machinery
(ast.hpp:910). Parse `const NAME: T;` in traits / `const NAME: T = expr;` in
impls. Typecheck: every impl provides each non-defaulted assoc const; resolve
`Type::NAME` and `Self::NAME` to the impl's value (const-evaluated). Also accept
`where` clauses on type aliases (generic params + bounds).

**GATE.** `smoke_test_assoc_const.sh`: `trait Bounded { const MAX: i64; }` +
`impl Bounded for Foo { const MAX: i64 = 100; }` → `Foo::MAX==100`, `Self::MAX`
in a default method works; a missing assoc const is rejected.

**DEFERRALS.** const-generic-valued assoc consts; orphan-rule enforcement.

---

## v74 — dyn upcasting (single-level) + turbofish on methods

> Scoped (critic): single-level prefix-vtable upcast; method turbofish is the
> easy half.

**CORE.** (1) **Dyn upcast**: when `trait Ord: Eq`, allow `&dyn Ord -> &dyn Eq`
by laying out a supertrait's method slots as a **prefix** of the subtrait
vtable and recording a `dynUpcast` coercion (extend `dynCoercions_`,
typecheck.cpp:6030). (2) **Method turbofish**: parse `recv.method::<T>()` and
`Type::assoc_fn::<T>()` (add `explicitTypeArgs` to `MethodCallExpr`, ast.hpp:1002),
feeding the existing free-fn turbofish substitution path.

**GATE.** `smoke_test_dyn_upcast.sh`: `let e: &dyn Eq = ord_ref;` dispatches the
Eq method through the upcast; `v.collect::<i64>()`-style method turbofish resolves
(JIT==AOT).

**DEFERRALS.** Multi-level upcast chains (A:B:C) only if prefix layout makes them
free; else single-level + a documented note.

---

## v75 — C-backend: tuple types

> Swapped before param-destructuring (v76) so v76's C-backend test has tuples.

**CORE.** Lift the C-backend's categorical tuple refusal (emit_c.cpp:186). Lower
`(T,U,…)` to a generated C struct `kdtuple_<mangled>` with fields `_0/_1/…`;
tuple literals → compound literals; `t.0` → `t._0`; tuple `let`-destructuring →
field reads. In-subset element types only.

**GATE.** `smoke_test_emit_c_tuples.sh`: a program returning/destructuring
`(i64,bool)` and a struct holding a tuple field produce `--emit-c` output whose
exit code matches the LLVM build (differential).

**DEFERRALS.** Tuples holding out-of-subset elements (HashMap/dyn/non-scalar
generic Vec) stay refused.

---

## v76 — parameter destructuring (patterns in fn params)

**CORE.** Add a `pattern` field to `Param` (ast.hpp:628); parse
`fn f(Point{x,y}: Point)` and `fn g((a,b): (i64,i64))`. At fn entry
(codegen.cpp:9505+) bind the param to a synthetic slot, then reuse the existing
struct/tuple-pattern desugar (already shipped for `let`). The C-backend path
reuses v75's tuples.

**GATE.** `smoke_test_param_destructure.sh`: `fn f(Point{x,y}:Point)->i64{x+y}`
and `fn g((a,b):(i64,i64))->i64{a-b}` return correct results; nested patterns
work (JIT==AOT); the C-backend differential matches for the tuple case.

**DEFERRALS.** `..rest` tuple params, slice-pattern params, refutable param
patterns.

---

## v77 — stdlib container ops (HashMap/HashSet/Vec)

> `hashmap_values` already exists (dropped from scope).

**CORE.** Fill container-API gaps, **all prelude Kardashev** (no codegen): HashMap
`hashmap_clear`/`hashmap_is_empty`/`hashmap_contains_key`; HashSet
`hashset_is_empty`/`hashset_clear`; Vec `vec_dedup<T:Eq>` (consecutive),
`vec_binary_search<T:Ord>`, `vec_contains<T:Eq>`, `vec_with_capacity`/
`vec_reserve` (over the existing growth allocator), and a small `entry_or_insert`
helper.

**GATE.** `smoke_test_container_ops.sh`: `hashmap_contains_key` true/false;
`clear`→`is_empty`; `vec_dedup([1,1,2,3,3])->[1,2,3]`; `vec_binary_search` finds
present / reports absent (JIT==AOT).

**DEFERRALS.** Exact `capacity` values are implementation-defined; a real Entry
API with in-place mutation deferred.

---

## v78 — lazy iterator adaptors: map / filter / fold / peekable

**CORE.** Extend the v61 lazy tower (main.cpp:105-226) with fusing i64-element
adaptors: `Map<I>` (`iter_map`), `Filter<I>` (`iter_filter`), an eager-drain
`iter_fold`, and `Peekable<I>` (`iter_peekable` with `peek()`), each a prelude
struct `impl Iterator<i64>` + a bridge fn. Pure-prelude.

**GATE.** `smoke_test_lazy_iter2.sh`: `take(filter(map(range(0,100), dbl),
is_even), 5)` fuses and yields the right 5 values RSS-flat over a 50M-element
source (JIT==AOT).

**DEFERRALS.** Element-generic adaptors stay blocked by the
`impl<T> Iterator<T> for Adaptor<T>` resolver limitation (v61 follow-on).

---

## v79 — generic Result/Option combinators + `?` ergonomics + custom error trait

**CORE.** Make the combinator library generic (currently i64-monomorphic):
generic `option_map<T,U>`/`option_and_then<T,U>`, `result_map<T,U,E>`/
`result_map_err<T,E,F>`/`result_and_then<T,U,E>` (effect-polymorphic closures via
the v60 effect-row machinery), an `Error` trait (`fn message(&self)->String`)
with a blanket Display bridge, and verify the `?` operator threads through them.

**GATE.** `smoke_test_error_combinators.sh`: `result_map` on
`Result<String,IoError>` transforms the Ok type; `result_and_then` chains two
fallible steps and short-circuits on the first Err; `?` propagates (JIT==AOT).

**DEFERRALS.** Multi-shot/continuation effect handlers; MonadError-style trait
abstraction; effect-row subsumption/variance.

---

## v80 — diagnostics depth: multi-char spans + JSON output + fix-its + LSP rename

> Lands the v64.x-deferred spans.

**CORE.** (1) Add `endLine`/`endColumn` to Parse/Type/Borrow errors and propagate
from AST node spans; `renderDiagnostic` (main.cpp:2869) underlines the full
`^~~~~` range. (2) `kardc --json` emits structured diagnostics (code, message,
start/end span). (3) A few fix-it/suggestion hints for the common codes (missing
`mut`, missing effect, missing `;`). (4) LSP `textDocument/rename` across a
file's references.

**GATE.** `smoke_test_diag_depth.sh`: a type error on `let x:bool = 1+2;`
underlines all of `1 + 2` (multi-char `^~~`); `kardc --json bad.kd` emits valid
JSON with a span; the missing-`mut` hint appears.

**DEFERRALS.** LSP incremental (range) didChange; cross-function breadcrumbs;
kardfmt range-format.

---

## Out of scope (the mega-arcs)

Real self-hosting bootstrap, a hosted package registry, WASM/Windows backends,
and a mechanized spec→1.0 proof remain deferred — each is multi-session and
environment-blocked, not per-version tractable.
