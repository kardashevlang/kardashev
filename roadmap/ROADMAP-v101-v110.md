# ROADMAP v101–v110 — production depth: a practical systems language, after v0.100.0

Designed against three read-only surveys of the real tree (stdlib gap;
type-system + codegen + optimization depth; self-hosting bootstrap + DX), then
**fact-checked against the shipped compiler** — which corrected several survey
premises that were already met (the v90 "premise often already met → refocus on
the real gap" pattern). The maintainer's 8th `/goal` (translated): **"polish into
a PRACTICAL SYSTEMS LANGUAGE with production depth — the next step after v1–v100
all shipped: deepen the stdlib, finish the half-shipped type-system/codegen
features, advance the self-hosting bootstrap one tractable rung at a time, and
mature the DX — no stubs, every version CI-green on both platforms."**

Each version is a **tractable per-version increment** — a real, tested core
shippable in one focused session with a JIT==AOT (or self==host differential, or
deterministic IR-grep) smoke gate, ending CI-green on ubuntu + macOS, plus honest
deferrals. The XL mega-arcs (full self-hosting bootstrap, register-ABI struct FFI,
WASM/Windows backends, hosted package registry, mechanized 1.0 proof) remain
deferred — they are not per-version tractable and several are
environment-blocked (no network / no `wasmtime`/`wine` in CI), so they are named
in *out of scope*, never scheduled as a version that cannot be CI-verified.

> **Ground-truth corrections (probed against `build.local/kardc` + `compiler/src`
> + `examples/selfhost/structgen.kd`, 1879 lines).** Three things the surveys
> listed as gaps are **already shipped**, so this arc does NOT re-cut them and
> redirects the slots to genuine confirmed gaps:
>
> 1. **Result/Option combinators** — `result_ok`/`result_err`/`result_is_ok`/
>    `result_map`/`result_map_err`/`result_and_then`/`result_unwrap_or`/
>    `result_unwrap_or_else`/`result_map_or` + `option_map`/`option_and_then`/
>    `option_or_else`/`option_filter`/`option_map_or` already exist in the prelude
>    (`main.cpp:1005–2083`). The *real* remaining Result/Option gap is **`Eq`/`Hash`
>    for `Option<T>`/`Result<T,E>`** (zero impls) → folded into v103/v105.
> 2. **Checked / saturating / wrapping arithmetic** — shipped in v70
>    (`typecheck.cpp:2010–2029`: `checked_add/sub/mul/div`, `wrapping_add/sub/mul`,
>    `saturating_add/sub/mul`). Not re-cut.
> 3. **Test harness** — `kardc --test` + the `kard test` shim discover + JIT-run
>    `test_*() -> i64` fns (`main.cpp:4128`, Phase 20a). The *real* remaining gap
>    is **`assert_eq!`/`assert_ne!` macros** + a `bench_*` discovery mode → folded
>    into v109.
>
> **Confirmed REAL gaps this arc closes** (each grep-verified absent): `Debug`
> for `Vec`/`HashMap`/`Option`/`Result` (0 impls), `Eq`/`Hash` for
> `Option`/`Result`/tuples (absent), `binary_search`/`partition`/`sort_by`
> (absent; `sort` is O(n²) insertion sort, `main.cpp:688`), `slice_to_vec`/
> `slice_iter`/`slice_chunks`/`slice_windows` (absent), `Path` (absent),
> element-generic iterators (v94 PART 2 deferred — the nested-adaptor PHI crash),
> bound-satisfaction diagnostics (still surfaces as `no impl provides method`,
> `typecheck.cpp:6539`), LSP code actions (absent), and the self-hosted emitter's
> lowering of `Option`/`match` + `Box<T>` heap indirection (the emitter *uses*
> `Box`/`match`/`Option` in its **own** host-compiled AST, but does not yet
> **lower** them — the bootstrap-spine gap named in `docs/bootstrap-status.md`).

> **Sequencing pivot.** Lead with **stdlib depth** (v101–v104) — the
> highest-leverage, most CI-testable, lowest-risk work that makes the language
> *usable for real programs today* (element-generic iterators, container `Debug`,
> sort/search, slice utilities). Then **type-system / codegen depth** (v105–v106):
> finish the half-shipped generic-impl story (tuple `Eq`/`Hash`) and land a
> measurable, deterministic optimization win. Then **self-hosting bootstrap
> progress** (v107–v108): advance the subset emitter one rung at a time
> (`Option`/`match`, then `Box<T>`), each self==host gated, each a real file-level
> unlock per `docs/bootstrap-status.md`. Close with **DX** (v109–v110): assertion
> macros + `bench`, then bound-satisfaction diagnostics + LSP code actions. Every
> version depends only on what shipped before it.

---

## ARC A — Stdlib depth (v101–v104): make it usable for real programs today

### v101 — element-generic iterators: fix the nested-adaptor PHI crash

**STATUS: ✅ SHIPPED (v0.101.0) — the "PHI crash" was a red herring; the real gap was generic-impl resolution + associated-output inference.** Ground-truth probing (built + ran the binary) corrected the premise: lowering element-generic adaptors over struct/String elements does NOT crash codegen and the `Var->i64` default never masks the payload — the actual block was a typecheck error `unknown type: T` on the impl header `impl<I: Iterator<T>, T> Iterator<T> for GTake<I,T>` (the trait type-arg `T` resolved to nothing). **Shipped:** (1) **the resolver fix** (`bindTraitParamsForImpl`, typecheck.cpp): a `typeRefMentions` helper + seeding the impl's own generic params **that a trait type-arg references** into the env. Restricting to *referenced* params is the key — the i64 tower's impls have trait arg `i64`/`(i64,i64)` (no param names) so they seed **zero** fresh Vars and stay **byte-identical** (the naive single-level fix shifts the global Var-ID counter and renames phantom-var-mangled symbols → IR drift; the restriction avoids it, verified by an empty `--emit-llvm` diff on a take/skip/zip program). (2) **A parallel element-generic prelude tower** under distinct `g*` names — `gvec_iter` / `gtake` / `gskip` / `gmap` / `gfilter` (the i64 `iter_*` tower is FROZEN: adding an element param renames its struct mangle, breaking byte-identity, so the generic tower is a sibling). Works over **i64, structs, and owned String**, fuses lazily, nests arbitrarily deep, and drains via the already-generic `iter_collect`. `gmap`'s mapper takes `fn(&T)->U` (by reference) — a by-value struct through the fn-value fat-pointer ABI mismatches the indirect call, so by-ref is the uniform, correct ABI. **GATE:** `smoke_test_iter_generic.sh` — i64 / struct / String chains JIT==AOT, a 3-deep nested struct chain (IR-grep shows `%Option__Pair` + distinct `%GTake__GFilter__GVecIter__Pair_*` instantiations, no PHI crash), and a **use-gated lock** (an i64-only program emits no `g*` symbols — the tower is fully monomorphize-on-use, zero bloat, i64 tower byte-identical). (Verified in-session: rigorous pre/post `--emit-llvm` byte-identity diff on the i64 tower [8141 lines identical]; 316 typecheck + 139 parser unit cases; full smoke sweep.) **DEFERRED, honestly (annotation workaround documented):** **unannotated** element inference — `let t = gtake(...)` where `T` is only in a bound + return leaves `T` unbound; the **annotated** forms (`let t: GTake<…> = …`, `let o: Vec<Pair> = iter_collect(…)`) work and are the supported idiom (consistent with the rest of the generic story). **Element-generic `zip`/`enumerate`** — their element is a *computed* pair (`TwoTup<T,U>`) present only in the impl trait-arg, not a struct param, so `iter_collect` cannot infer it without bound-output inference (the same gap); the i64 `iter_zip`/`iter_enumerate` remain, and a generic `GZip` is deferred to the inference follow-up. `--emit-c` refuses generic-trait programs (unchanged); the self-hosted subset stays i64.

**Theme:** Survey 1 §2 #1 + Survey 2 §1A + the v94 PART 2 explicit deferral
(`ROADMAP-v91-v100.md:202`): the lazy iterator adaptor tower
(`Take`/`Skip`/`Chain`/`Zip`/`Enumerate`/`Map`/`Filter`/`Peekable`,
`main.cpp:149–292`) is **`i64`-only** because a generic impl cannot bind a generic
param as a trait type-arg through a *transitive* bound. The single-level fix
(`assocEnv = implParamEnv(impl)`) is proven to work, but **nested adaptors**
(`impl<I: Iterator<T>, T> Iterator<T> for Take<I>`) crash codegen with a PHI type
mismatch — `T` is unresolved through the bound at monomorphization. This is the
#1 highest-leverage stdlib gap: it blocks every adaptor chain over `Vec<String>`,
`Vec<Struct>`, etc. L-difficulty codegen work (resolve `T` from the bound at
monomorphization), with M-risk to the 10+ shipped i64-adaptor tests — so the gate
is differential-against-the-existing-i64-tower.

**CORE.** (host: typecheck generic-impl resolution + codegen monomorphization)
- (1) **Single-level fix** (the proven one-liner, hardened): generic-impl
  resolution binds a generic param as a trait type-arg
  (`impl<T> Iterator<T> for VecIter<T>`) — `assocEnv` seeded from the impl's param
  env so `next()` returns `Option<T>` at the concrete element type.
- (2) **The nested fix** (the real work): when resolving
  `impl<I: Iterator<T>, T> Iterator<T> for Take<I>`, propagate `T` from the
  transitive bound `I: Iterator<T>` into the inner impl's substitution map during
  specialization, so the monomorphized `Take<VecIter<String>>::next()` emits a
  `phi`/`select` over the correct `Option<String>` payload type instead of an
  unresolved `T`. The fix lives where monomorphization threads the bound (already
  done for direct calls; extend to nested generic-impl resolution).
- (3) **Make the whole v61/v78 tower element-generic:** `take`/`skip`/`chain`/
  `zip`/`enumerate`/`map`/`filter`/`peekable` + `VecIter<T>` now work over any
  element type. The i64 specializations stay byte-identical (use-gated path).
- (4) **`collect` over a generic element:** `iter.collect()` builds `Vec<T>` for
  any `T`, not just `i64`.

**GATE.** `smoke_test_iter_generic.sh` — deterministic differential JIT==AOT:
(a) **no-regression lock** — every existing `i64` adaptor program in
`smoke_test_iter.sh`/`smoke_test_iter_lazy.sh` produces **byte-identical** IR
(`--emit-llvm` diff) before/after (the M-risk guard); (b) a
`Vec<String>.iter().filter(...).map(...).take(3).collect()` chain runs, exit +
stdout JIT==AOT; (c) a nested `Take<Skip<VecIter<Widget>>>` over a struct element
specializes — the `--emit-llvm` IR shows **distinct** instantiated types in the
`next()` signatures (grep, target-independent), no PHI-type-mismatch crash; (d) a
`zip` of `Vec<String>` × `Vec<i64>` yields `(String, i64)` pairs. All exits match
the correctness oracle.

**DEFERRALS.** Element-generic iterators in the **`--emit-c`** backend (the C
backend monomorphizes once at `int64_t`; non-scalar generic instances stay
refused there — LLVM/JIT/AOT full). Generic iterators in the **self-hosted**
subset (v94 shipped self-hosted monomorphic generics; the iterator *tower* in the
subset needs closures → its own bootstrap rung). `impl Iterator` for user types
via `for x in my_iter` desugaring beyond the prelude tower (a follow-on).

---

### v102 — generic `Debug` for containers + recursive `{:?}` printing

**STATUS: ✅ SHIPPED (v0.102.0) — prelude-only; the v101 resolver made every blanket impl Just Work.** Live probing confirmed the post-v101 generic-impl resolution handles all the needed `impl<T: Debug> Debug for Container<T>` blanket impls with **zero** codegen/resolver work — so v102 is a pure prelude addition. **Shipped** (blanket impls over each element's own `fmt_debug`, all under the existing `trait Debug` opt-out gate so they neither collide with a user `Debug` nor bloat opt-out programs): `Vec<T>` → `[a, b, c]`, `Option<T>` → `Some(x)`/`None`, `Result<T,E>` → `Ok(x)`/`Err(e)`, `BTreeMap<K,V>` → `{k: v, …}` and `BTreeSet<T>` → `{a, …}` (ordered/deterministic via direct `&self.keys`/`&self.vals`/`&self.items` index walks — **no `K: Ord`/`Clone` bound needed**), `HashMap<K: Hash+Eq+Clone+Debug, V: Debug>` (the bounds are mandatory — `K: Debug` alone fails the key-type check; gate tests single-entry since bucket order is non-deterministic), and `VecDeque<T>` (front→back = the `f` segment reversed then `b` forward). `Box<T>` Debug already works via deref (no impl). String elements are quoted + escaped (reusing the v27 `str_escape`, inherited free from `Debug for String`). **The headline DX win:** `#[derive(Debug)]` on a struct/enum with `Vec`/`Option`/… fields now recurses correctly (`Widget { id: 7, tags: ["a", "b"], parent: Some(3) }`) — the derive already called `fmt_debug` on each field; v102 makes those field impls exist. **GATE:** `smoke_test_debug_recursive.sh` — deterministic JIT==AOT stdout over Vec(+escaped String), Option/Result(+nested `Some([1, 2])`), ordered BTreeMap/BTreeSet, single-entry HashMap, VecDeque, the derive-with-container-field case, Box-via-deref, and a scalar-derive regression lock; the existing `phase150`/`coherence`/`fmt_specs`/`diag` Debug tests stay green. (Verified in-session: full container suite JIT==AOT identical; 316 typecheck unit cases; full smoke sweep.) **DEFERRED, honestly:** tuple `Debug` (`impl<A,B> Debug for (A,B)` — tuples are not a registrable nominal impl head today) and `&[T]` slice Debug → **v104**; **format-spec dispatch** (`{:x}`/`{:04d}` — the `str_pad_*`/radix helpers exist since v71 but `format!` doesn't route a spec to them) → its own version; `{:#?}` pretty/multi-line → a follow-on.

**Theme:** Survey 1 §2 #5 (grep-confirmed: `main.cpp` has `impl Debug for`
**only** i64/f64/bool/char/String at lines 384–401 — **zero** container impls).
Today `println!("{:?}", v)` over a `Vec<T>` or `HashMap<K,V>` is impossible: there
is no recursive `Debug`. This is the single highest-DX stdlib gap — debugging real
data structures requires hand-written formatters. H-tractability (generic blanket
impls over `fmt_debug`), medium risk to the existing `#[derive(Debug)]` path, so
the gate runs the derive cases as a regression lock. Depends on v101 only insofar
as a `Vec<T>` `Debug` impl is most useful once iterators are element-generic.

**CORE.** (host prelude — generic blanket impls over existing `Debug`)
- (1) `impl<T: Debug> Debug for Vec<T>` — `fmt_debug` produces `[a, b, c]` by
  iterating `vec_len`/`vec_get_ref` and recursing into each element's
  `fmt_debug` (separator `", "`, brackets `[`…`]`).
- (2) `impl<K: Debug, V: Debug> Debug for HashMap<K, V>` — produces
  `{k: v, k: v}` over `hashmap_keys` + `hashmap_get_ref` (deterministic iteration
  order is NOT guaranteed by HashMap, so the **gate uses a single-entry map or a
  BTreeMap** for deterministic output — see GATE).
- (3) `impl<T: Debug> Debug for Option<T>` → `Some(x)` / `None`;
  `impl<T: Debug, E: Debug> Debug for Result<T, E>` → `Ok(x)` / `Err(e)`.
- (4) `impl<T: Debug> Debug for BTreeMap<K, V>` / `BTreeSet<T>` (these *do* have a
  deterministic ordered iteration — the gate's deterministic multi-entry case).
- (5) Confirm `#[derive(Debug)]` on a struct/enum with a `Vec<T>`/`Option<T>`
  field now recurses correctly (the derive already calls `fmt_debug` on fields;
  this version makes those field impls exist).

**GATE.** `smoke_test_debug_recursive.sh` — deterministic JIT==AOT on **stdout**:
(a) `println!("{:?}", vec)` over a `Vec<i64>` prints `[1, 2, 3]`; (b) over a
`Vec<String>` prints `["a", "b"]` (strings quoted + escaped, reusing the v27
`str_escape`); (c) a `BTreeMap<i64, String>` prints `{1: "a", 2: "b"}`
(**ordered → deterministic**); (d) `Option<Vec<i64>>` prints `Some([1, 2])`;
`Result<i64, String>` prints `Ok(7)` / `Err("bad")`; (e) `#[derive(Debug)]` on
`struct Widget { tags: Vec<String>, parent: Option<i64> }` round-trips; (f)
**regression lock** — every existing `#[derive(Debug)]` case in
`smoke_test_debug*`/`smoke_test_diag*` produces identical output. HashMap (unordered)
is debug-printed only in a **single-entry** assertion to stay deterministic.

**DEFERRALS.** Format-spec dispatch (`{:x}`/`{:04d}`/`{:e}` — the `str_pad_*` +
radix helpers exist from v71 but `format!` doesn't yet route a spec to them;
Survey 1 §2 #13, its own version). `Debug` for `&[T]` slices (folds into v104).
Pretty-printing (`{:#?}` multi-line) — a follow-on. Cycle detection (ownership
already prevents reference cycles in `Debug`-able data, so no guard needed).

---

### v103 — sort/search algorithms: quicksort + `binary_search` + `partition`

**STATUS: ✅ SHIPPED (v0.103.0) — prelude-only; every design decision empirically probed.** **Shipped:** (1) `sort<T: Ord>` upgraded in place from the O(n²) insertion sort to **quicksort** (median-of-three pivot + insertion-sort cutoff ≤12) — **same signature + `! {}` effect row** preserved (a named `qsort<T: Ord>` helper recurses, calling `.cmp()` directly; only `vec_get_ref`/`vec_len`/`vec_swap`, all effect-free, so no `alloc`). Median-of-three bounds depth to O(log n) on the sorted/reverse inputs that kill a naive pivot (probed: 5000-elem random + 3000-elem pre-sorted complete without stack blowup). (2) **`sort_by<T>(v, cmp: Fn(&T,&T)->i64)`** — quicksort with a caller comparator. It is **iterative (an explicit `(lo,hi)` stack)** by necessity: a closure is a move-only fat-pointer value, so it cannot be threaded through a recursive helper (the 2nd recursive call would use-after-move) and calling through a `&Fn` is unsupported — so the closure lives in one frame and is invoked directly (`! { alloc }` for the range stack). (3) **`binary_search<T: Ord>` / `binary_search_by<T>(…, cmp)`** → `Option<i64>`, `! {}`. (4) **`partition<T>(v, pred: Fn(&T)->bool) -> i64`** — in-place, returns the pivot index (count satisfying), `! {}`. Each comparison is hoisted to a `let` before `vec_swap` (the two-phase-borrow E0499 gotcha). `vec_swap` keeps non-Copy `T` (e.g. `String`) safe through the sort. **GATE:** `smoke_test_sort_search.sh` — deterministic, **seeded-RNG** (v62 `rng_seed_global`/`rand_global`, never wall-time): a 1000-elem random sort (sortedness oracle), adversarial already-sorted + reverse-sorted inputs that **complete + sort correctly** (the median-of-three guard, asserted by completion not timing → never flaky), `binary_search` present→index/absent→`None`, descending `sort_by`, `partition` pivot index, a non-Copy `String` sort, and a struct-field `sort_by`. (Verified in-session: all cases JIT==AOT; **every existing sort consumer stays green** — `wordfreq`/`phase55`/`json`/`phase47`/`phase48` — because each uses a *total* comparator so quicksort's instability is unobservable; 316 typecheck cases; full smoke sweep.) **DEFERRED, honestly:** **stable sort** (the old insertion sort was stable; quicksort is not — a `sort_stable` merge-sort variant if a non-total comparator ever needs it; documented in-source). `sort_unstable`/`sort_by_key` sugar (thin wrappers). Radix/counting sort. Sorting `&mut [T]` slices directly → folds into v104.

**Theme:** Survey 1 §2 #7 (grep-confirmed: the only sort in the prelude is
`fn sort<T: Ord>(v: &mut Vec<T>)`, an **O(n²) insertion sort**, `main.cpp:688`;
`binary_search`/`partition`/`sort_by` are absent). Data-structure-heavy systems
code needs O(n log n) sorting, custom comparators, and binary search. This is a
pure, deterministic, CI-trivial algorithm version with a sortedness/found-index
oracle. M-difficulty, low risk (new fns + one in-place rewrite). Depends on
nothing.

**CORE.** (host prelude — new generic fns over `Vec`/`&mut Vec`)
- (1) **`sort<T: Ord>` upgrade to in-place quicksort** with median-of-three pivot
  (avoids the O(n²) worst case on sorted/reverse input) + an insertion-sort cutoff
  for small partitions (≤16). Replaces the insertion sort; same signature, same
  `! {}` effect row, same `vec_swap`-based in-place mutation.
- (2) **`sort_by<T, e>(v: &mut Vec<T>, cmp: fn(&T, &T) -> i64 ! {e})`** — quicksort
  driven by a user comparator (negative/zero/positive convention).
- (3) **`binary_search<T: Ord>(v: &Vec<T>, x: &T) -> Option<i64>`** and
  **`binary_search_by<T, e>(v: &Vec<T>, x: &T, cmp: fn(&T,&T)->i64 ! {e}) -> Option<i64>`**
  over a sorted vector — `Some(idx)` if found, `None` otherwise.
- (4) **`partition<T, e>(v: &mut Vec<T>, pred: fn(&T) -> bool ! {e}) -> i64`** —
  in-place partition, returns the pivot index (count of elements satisfying the
  predicate, all moved to the front).
- (5) Reuse the proven `vec_swap`/`vec_get_ref`/`vec_len` builtins; no new codegen.

**GATE.** `smoke_test_sort_search.sh` — deterministic JIT==AOT, **seeded** RNG
(the v62 seeded `rng_seed_global`/`rand_global`, never wall-time): (a) a 1000-element
seeded-random `Vec<i64>` sorts — assert sortedness (`v[i] <= v[i+1]` for all i,
exit 0); (b) **adversarial input** — already-sorted + reverse-sorted 1000-element
inputs sort without timing out (the median-of-three guard against O(n²) blowup —
asserted by *completion* + correctness, NOT wall-time, so non-flaky); (c)
`binary_search` finds every present element + returns `None` for absent ones;
(d) `sort_by` over a `Vec<Widget>` with a custom field comparator sorts by that
field; (e) `partition` on a predicate moves all matching elements before the
returned index. All exits match the oracle.

**DEFERRALS.** Stable sort (the v100 insertion sort was stable; quicksort is not —
a `sort_stable` merge-sort variant is a follow-on if needed; document the change).
`sort_unstable`/`sort_by_key` sugar (thin wrappers, a follow-on). Radix/counting
sort for integer keys (specialized, later). Sorting `&mut [T]` slices directly
(folds into v104's slice utilities).

---

### v104 — slice utilities: `&[T]` → `Vec`, slice iteration, chunks/windows

**STATUS: ✅ SHIPPED (v0.104.0) — prelude-only; all six utilities probe-confirmed, the borrowing forms ship.** Closes ARC A (stdlib depth). **Shipped:** (1) **`slice_to_vec<T: Clone>(s: &[T]) -> Vec<T>`** — an owned deep-copy via `slice_get_ref(s,i).clone()` (i64 + struct/String elements). (2) **`SliceIter<T> { s: &[T], pos }` + `slice_iter<T>(s: &[T]) -> SliceIter<T>`** — a **real borrowing `Iterator<T>`** that holds the `&[T]` directly (the make-or-break `&[T]`-in-struct case PASSES) and chains into the v101 `g*` adaptor tower (`slice_iter(&v[1..4])` → `gmap` → `iter_collect`). (3) **`slice_chunks<T>(v: &Vec<T>, n) -> Vec<&[T]>`** and **`slice_windows<T>(v: &Vec<T>, n) -> Vec<&[T]>`** — **zero-copy** views (a `Vec` of sub-slices into the original buffer). They take `&Vec<T>` (not `&[T]`) deliberately: re-slicing a `&[T]` is rejected, and — crucially — the returned views must root in a ref-param to stay sound (the escape checker does *not* track refs nested inside a `Vec`, so returning `Vec<&[local]>` would be unchecked UB; rooting in `&Vec<T>` makes the views borrow the caller's data). (4) **`slice_contains<T: Eq>` / `slice_index_of<T: Eq>`** — linear search via `slice_get_ref(s,i).eq(x)`. Key probe-driven details baked in: slice builtins take the slice **by value** (a Copy `{ptr,len}` aggregate), so `SliceIter::next` uses `self.s` not `&self.s`; loops use `break` not an early `return x;` (which would move `x` before a later `vec_push`). **GATE:** `smoke_test_slice_methods.sh` — JIT==AOT (no `--emit-c` leg: generics over `Vec<&[T]>` are cleanly refused there): `slice_to_vec` independence (mutate the source via a `&mut [T]` slice, copy unchanged), slice iteration chaining the v101 tower (`[2,4,6]`), `slice_chunks` 10-by-3 → `[3,3,3,1]`, `slice_windows` → `[[10,20],[20,30]]`, `contains`/`index_of` present+absent, and a non-Copy `String` `slice_to_vec`. (Verified in-session: all cases JIT==AOT; 316 typecheck cases; full smoke sweep.) **DEFERRED, honestly:** mutable-slice iteration (`for x in &mut s` yielding `&mut T` — needs mutable-element-generic iterators, a later iterator-mut version); `slice_split_at`/`first`/`last` thin wrappers; slice utilities in `--emit-c` (the `Vec<&[T]>` return is non-scalar — LLVM/JIT/AOT full); `chunks_exact`/`rchunks` variants.

**Theme:** Survey 1 §2 #6 (grep-confirmed: only scalar `slice_get`/`slice_len`/
`slice_set`/`slice_get_ref`/`slice_get_mut` builtins exist; `slice_to_vec`/
`slice_iter`/`slice_chunks`/`slice_windows`/`slice_contains` are absent). Slices
are a first-class type (v89/v90/v93) but have no conversion, iteration, or
windowing — blocking buffer-processing systems code. M-difficulty (prelude fns +
one small `Iterator` struct over a slice), low risk. Depends on v101
(element-generic iterators) so `slice_iter` can chain with the adaptor tower.

**CORE.** (host prelude — fns + a `SliceIter<T>` struct)
- (1) **`slice_to_vec<T: Clone>(s: &[T]) -> Vec<T>`** — deep-copies a slice into an
  owned `Vec` (over `slice_len`/`slice_get` + `vec_push` + the v25 `clone`
  intrinsic for non-Copy `T`).
- (2) **`struct SliceIter<T> { s: &[T], pos: i64 }` + `impl<T> Iterator<T> for SliceIter<T>`**
  and **`slice_iter<T>(s: &[T]) -> SliceIter<T>`** — element-generic (rides on the
  v101 generic-iterator fix), so `slice_iter(s).map(...).filter(...).collect()`
  composes with the whole adaptor tower.
- (3) **`slice_chunks<T>(s: &[T], n: i64) -> Vec<&[T]>`** and
  **`slice_windows<T>(s: &[T], n: i64) -> Vec<&[T]>`** — non-overlapping chunks /
  sliding windows as a `Vec` of sub-slices (each a `slice_get_ref`-style view into
  the original buffer — zero-copy views, no element deep-copy).
- (4) **`slice_contains<T: Eq>(s: &[T], x: &T) -> bool`** and
  **`slice_index_of<T: Eq>(s: &[T], x: &T) -> Option<i64>`** — linear search over
  a slice (mirrors `vec_contains`/`vec_index_of`).

**GATE.** `smoke_test_slice_methods.sh` — deterministic JIT==AOT: (a)
`slice_to_vec` of `&arr[1..4]` produces a `Vec` whose elements equal the slice +
is independent (mutating the source array after the copy doesn't change the vec);
(b) `slice_iter(&v[..]).map(|x| x*2).filter(|y| y>4).collect()` chains with the
v101 tower, exit matches the eager equivalent; (c) `slice_chunks(s, 3)` of a
10-element slice yields chunks of `[3,3,3,1]` (assert chunk count + last-chunk
length); (d) `slice_windows(s, 2)` of `[1,2,3]` yields `[[1,2],[2,3]]`; (e)
`slice_contains`/`slice_index_of` find present + report absent. All JIT==AOT.

**DEFERRALS.** Mutable-slice iteration (`for x in &mut s` yielding `&mut T` — needs
mutable-element-generic iterators; folds into a later iterator-mut version).
`slice_split_at`/`slice_first`/`slice_last` (thin wrappers, a follow-on). Slice
utilities in the `--emit-c` backend (scalar-only there; the `Vec<&[T]>` return is
non-scalar — LLVM/JIT/AOT full). `chunks_exact`/`rchunks` variants (later).

---

## ARC B — Type-system & codegen depth (v105–v106): finish + measure

### v105 — generic `Eq`/`Hash` for `Option`/`Result`/tuples + the bound it unlocks

**STATUS: ✅ SHIPPED (v0.105.0) — Option/Result Eq+Hash + derive'd composite keys; tuples + generic-keys honestly deferred (probe-confirmed blockers).** Opens ARC B. Live probing decided the scope: **`impl Eq for (T,U)` FAILS** (`impl for unsupported type` — a tuple is not a registrable nominal impl head, as v102 flagged), and a **generic type used *directly* as a HashMap key** (`HashMap<Option<i64>,V>` / `HashMap<Pair<T>,V>`) is **blocked at codegen** (the eager-emit pass at codegen.cpp:287 skips monomorphized generic-impl methods, so the bare `__impl_Hash_for_Option__hash` symbol the key machinery looks up at codegen.cpp:2976-2994 is never emitted — a codegen-dispatch item, not a prelude fix). **Shipped (prelude blanket impls, verified):** `impl<T: Eq> Eq for Option<T>` + `impl<T: Hash> Hash for Option<T>`, and the `Result<T,E>` pair — structural eq (recursing into the payload via bare-variant match arms) and a derive-convention hash (per-variant seed `527+ordinal`, fold payload `*31`, so a value's hash agrees with `#[derive(Hash)]` on an equivalent enum). This makes Option/Result usable in `==`, as `Vec`/`Box` elements (`vec_contains` over `Vec<Option<i64>>` works), and — the headline — **`#[derive(Eq, Hash)]` on a struct with an `Option`/`Result` field now resolves**, so that concrete struct keys a `HashMap` (verified: a `struct Key { id: i64, tag: Option<String> }` round-trips as a HashMap key across distinct String allocations — eq+hash commute end-to-end). **The composite-key story is therefore a nominal `#[derive(Eq,Hash)]` struct, NOT `HashMap<(K1,K2),V>`** — documented as the supported pattern. **GATE:** `smoke_test_composite_eq_hash.sh` — JIT==AOT: Option/Result value eq, hash-commute (equal → hash equal), the derive'd-struct-with-Option-field HashMap key round-trip (insert + retrieve a freshly-built equal key + absent→None), and `Option` Vec membership. (Verified in-session: all JIT==AOT; `phase48`/`hash`/`hashremove`/`json` regression-green; 316 typecheck cases; full smoke sweep.) **DEFERRED, honestly (with evidence):** **tuple `Eq`/`Hash`** (tuple is not a registrable impl head — the composite-key path is a derive'd struct); **a generic type directly as a HashMap/HashSet key** (`HashSet<Option<T>>` / `HashMap<Pair<T>,V>` — the codegen eager-emit + key-fn-name-mangling fix, its own version); `char` `Eq`/`Hash` (no `char_to_int` builtin to forward to — a minor pre-existing gap); `Ord`/`cmp` for tuples/Option/Result (sorting tuple vectors — its own slice); arity > 4.

**Theme:** Survey 1 §2 #8 + #11 (grep-confirmed: `Eq`/`Hash` are impl'd **only**
for i64/bool/f64/char/String; **no** generic impls for `Option<T>`, `Result<T,E>`,
or tuples `(T, U)`). This blocks `HashMap<(K1, K2), V>`, `HashSet<Option<T>>`, and
`#[derive(Eq, Hash)]` on any struct with an `Option`/tuple field — pervasive in
real code. This is the natural completion of the v101/v102 generic-impl work
(now that nested generic-impl binding is fixed, blanket impls over composite types
resolve cleanly). M-difficulty (generic blanket impls), low codegen risk.

**CORE.** (host prelude — generic blanket impls)
- (1) `impl<T: Eq> Eq for Option<T>` / `impl<T: Hash> Hash for Option<T>`
  (None == None; Some(a) == Some(b) iff a == b; hash mixes a discriminant).
- (2) `impl<T: Eq, E: Eq> Eq for Result<T, E>` /
  `impl<T: Hash, E: Hash> Hash for Result<T, E>`.
- (3) `impl<T: Eq, U: Eq> Eq for (T, U)` … up to arity-4
  (`(T,U,V)`, `(T,U,V,W)`); same arity coverage for `Hash` (FNV/Murmur-mix the
  per-field hashes, reusing the v31 `hash_*` builtins).
- (4) Ensure `#[derive(Eq, Hash)]` on a struct/enum with `Option`/tuple fields
  resolves (the derive iterates fields calling `eq`/`hash`; this version provides
  the field impls).

**GATE.** `smoke_test_tuple_eq_hash.sh` — deterministic JIT==AOT: (a) a
`HashMap<(i64, String), i64>` inserts + retrieves by a tuple key (round-trip exit
matches); (b) a `HashSet<Option<i64>>` deduplicates `Some(1)`/`Some(1)`/`None`
correctly (len == 2); (c) `Some(3) == Some(3)`, `None == None`,
`Ok(1) != Err(1)`, `(1, "a") == (1, "a")` all evaluate correctly; (d)
`#[derive(Eq, Hash)]` on `struct K { id: i64, tag: Option<String> }` lets it key a
`HashMap`; (e) equality and hashing **commute** (equal keys hash equal — asserted
by a HashMap collision-then-retrieve test). All JIT==AOT.

**DEFERRALS.** `Ord`/`cmp` for tuples/`Option`/`Result` (a follow-on — needed for
sorting tuple vectors, but `Eq`/`Hash` is the HashMap blocker; `Ord` is M and its
own slice). Arity > 4 tuples (diminishing returns; documented). Generic
`Eq`/`Hash` for user enums beyond derive (derive covers the common case).
`PartialEq`/`PartialОrd` split (kardashev has total `Eq`/`Ord` only — a
type-theory decision, not regressed here).

---

### v106 — a measurable codegen win: tail-call lock + bounds-check elision gate

**STATUS: PLANNED.**

**Theme:** Survey 2 §2 — the fib gap is already 1.00× C (v95 re-measured + locked
via IR-grep), so there is **no perf slack to chase**; the honest remaining codegen
work is (a) **guaranteeing** tail-call lowering for self-tail-recursive fns (today
LLVM *may* do it at -O2 but it is unverified, so deep recursion can blow the
stack) and (b) **bounds-check elision** in monotone loops (redundant `index < len`
checks the optimizer should hoist/eliminate). Both are deterministic,
IR-grep-gatable, target-aware wins (per the v90 lesson: IR-grep gates are
x86-64-enforce / arm64-soft). M-difficulty, the gate is the deliverable's spine
(it *locks* the win so a future PassBuilder refactor can't silently regress it).

**CORE.** (host codegen + a deterministic IR-grep gate)
- (1) **Self-tail-recursion → guaranteed `tail call`:** detect a recursive call in
  tail position (the function's result is exactly that call) and emit it with the
  LLVM `tail` marker + ensure the `-O2` pipeline runs `TailCallElim` so it lowers
  to a loop (no stack growth). Correctness-first: only mark *provably* tail-position
  self-calls; everything else is unchanged.
- (2) **Bounds-check elision in monotone index loops:** confirm that a
  `for i in 0..vec_len(v) { ... v[i] ... }` loop's per-iteration `i < len` bounds
  check is hoisted/eliminated by the `-O2` SCEV/LICM passes given kardashev's
  lowering shape; add the analysis-enabling pass(es) if the default pipeline misses
  it (re-verify the v51 TargetMachine/TTI registration covers these paths — MEMORY:
  a missing TM once killed vectorization).
- (3) **Lock both with a permanent gate** (mirrors v95's perf-regression-lock
  philosophy — structural IR-greps, zero wall-time).

**GATE.** `smoke_test_codegen_tco.sh` — **deterministic, target-aware, zero
wall-time**: (a) a self-tail-recursive `fn sum(n, acc)` — `--emit-llvm` shows the
recursive call as a `tail call` AND the -O2 IR shows it lowered to a loop (no
`call @sum` remaining); x86-64 **enforce**, arm64 **soft** (warn-not-fail, per the
v90 arch-dependent-IR lesson); (b) **runtime proof** — `sum(0, 1_000_000)` runs to
completion without a stack overflow (a depth that would crash without TCO —
asserted by *exit code*, deterministic, not wall-time); (c) a
`for i in 0..vec_len(v)` summing loop's -O2 IR has **≤1** `icmp ult` bounds-check
(elided/hoisted), x86-64 enforce / arm64 soft; (d) the v95 perf-lock + v90
vector-lock IR-greps still green (no regression); (e) all outputs match the
correctness oracle (a codegen change must never change results).

**DEFERRALS.** General (non-self) tail-call elimination + mutual-tail-recursion
(needs whole-program analysis or a `become`/`#[must_tail]` annotation — its own
version). LTO / cross-module inlining (XL — needs whole-program link + crate
boundaries; Survey 2 §2F). Escape-to-stack for closure envs (M but depends on
named lifetimes — XL mega-arc; Survey 2 §2C). Application-scale benchmark suite
(only `fib`/`loop` are above the timer-resolution floor — correctness-only for the
rest, per v95).

---

## ARC C — Self-hosting bootstrap progress (v107–v108): one rung at a time

### v107 — self-hosted `Option<T>` + `match`-on-enum lowering

**STATUS: PLANNED.**

**Theme:** Survey 3 §1 (rank #1, "M, unlocks Result + most library code") +
`docs/bootstrap-status.md`. The self-hosted emitter (`structgen.kd`) lowers
user enums + `match` only as a branch-free `select`-chain over positional payloads
(v98), and has **no `Option<T>`** in its lowering subset — yet `Option`/`match` is
what every real compiler phase (`checker.kd`, `front.kd`, `expr.kd`, `stmt.kd`)
needs for fallible lookups. (Note: structgen's *own* host-compiled source uses
`Option`/`match`/`Some`/`None` freely — `structgen.kd:712`; the gap is what the
emitter can **lower into self-contained IR**, not what the host accepts.) This is
the next bootstrap rung: give the subset emitter a real `Option<T>` + a `match` it
can lower with CFG (now possible thanks to v91's `br`/`phi`). M-difficulty, reuses
v94 monomorphization + v91 CFG; self==host gated.

**CORE.** (self-hosted emitter `examples/selfhost/structgen.kd`)
- (1) **`Option<T>` as a first-class self-hosted type** — a built-in generic enum
  in the subset (tag for the `Option` shape, monomorphized per `T` via the v94
  registry: `Option__i64`, `Option__Widget`); `Some(x)` / `None` constructors lower
  to a tagged `{ i64 tag, T payload }` aggregate.
- (2) **`match` on `Option<T>`** (and on user enums) lowered with **real CFG**
  (not the `select`-chain): a `switch`/`br`-on-tag to per-arm blocks, each binding
  the positional payload, a phi/alloca for the result — riding on v91's block
  discipline. This generalizes the existing user-enum match.
- (3) **Self-hosted type-checker:** `match` arm exhaustiveness on `Option`
  (Some + None required), payload binding types, result-type unification across
  arms; a non-exhaustive `match` is a self-hosted `TYPE ERROR`.
- (4) Follow the structgen conventions: structs-precede-fns ordering, a new `Expr`
  variant (`MatchE` if not already an emitter primitive) wired into **all**
  `match`-over-`Expr` sites in the emitter (`type_of`, `lower`), a trailing `;`
  after any `while` before a following expr, `&mut`-local alloca-slot for any
  mutable accumulator.

**GATE.** `smoke_test_selfhost_option.sh` — differential **self==host** (the
self-hosted LLVM-IR exit == host exit, the established bootstrap discipline):
(a) `let o = Some(7); f() -> match o { Some(x) => x, None => 0 }` exits **7**;
(b) `None` arm taken → exits the default; (c) a `match` in a `while` loop
(Option-returning step, accumulate until `None`) — exits the accumulated total;
(d) `Option<Widget>` (a struct payload) — `match` binds + reads a field;
(e) a **negative**: a non-exhaustive `match o { Some(x) => x }` → self-hosted
`TYPE ERROR`; (f) **byte-identity** — every prior self-host gate
(`selfhost_traits`/`_generics`/`_vec`/`_loops`/`_refs`/`_calls`, phase117/118)
stays byte-for-byte (the Option/match path is use-gated). The
`docs/bootstrap-status.md` ledger is updated (which files this unblocks).

**DEFERRALS.** `Result<T, E>` in the subset (a two-type-param generalization of
this same machinery — a thin follow-on once `Option` + multi-param generics align;
the v94 deferral of multi-param `<A,B>` gates it, tracked). `match` *guards* /
or-patterns / nested patterns in the subset (the host has them; the subset ships
flat single-level patterns first). `if let` / `while let` sugar in the subset.
`Box<T>` (v108 — the next rung).

---

### v108 — self-hosted `Box<T>`: heap indirection for recursive data

**STATUS: PLANNED.**

**Theme:** Survey 3 §1 (rank #2, "M–L, recursive AST nodes need it") +
`docs/bootstrap-status.md`. With `Option`/`match` lowered (v107), the next
bootstrap rung is **`Box<T>` heap indirection** — the feature every recursive data
structure (every compiler AST node) needs. (Again: structgen's own source already
uses `Box<Expr>`/`Box<Stmt>` pervasively — `structgen.kd:16–38` — because it is
host-compiled; the gap is the emitter lowering `Box` into `malloc`/`load`/`store`
in its self-contained IR.) Codegen is straightforward (a heap pointer:
`box_new` → `malloc` + `store`; deref → `load`; drop → `free` at the v91 exit
block), no new type theory. M–L difficulty, self==host gated, reuses the v92 Vec
runtime-preamble + drop-at-exit machinery.

**CORE.** (self-hosted emitter)
- (1) **`Box<T>` type-tag** in the subset (monomorphized per `T` via the v94
  registry); `Box::new(e)` lowers to a `malloc(sizeof T)` + `store` of the value,
  yielding a pointer; the v92 use-gated runtime preamble emits the `malloc`/`free`
  `declare`s only when a `Box` is used (so non-Box programs stay byte-identical).
- (2) **Deref + field/variant access through a `Box`:** `*b` and `b.field` /
  `match *b { ... }` lower to a `load` of the boxed value then the existing
  field/match path — generalizing v85's `&Struct` auto-deref to a heap pointer.
- (3) **Drop discipline:** a non-escaping `Box<T>` local is `free`d at the v91 exit
  block (reuse the v92 Vec/owned-string drop-at-exit logic — `free` the heap slot,
  recursing into a boxed aggregate's own owned fields). An escaping `Box` (returned)
  is a sound leak (documented), as in the host.
- (4) **Recursive enum via `Box`:** a self-hosted `enum Expr { Bin(Box<Expr>, Box<Expr>) }`
  builds + traverses (the capstone — a real recursive AST in the subset). Wire the
  `Box` deref into **all** relevant emitter match sites.

**GATE.** `smoke_test_selfhost_box.sh` — differential **self==host**: (a)
`let b = Box::new(7); f() -> *b` exits **7**; (b) a boxed struct `*b` reads a
field; (c) **capstone** — a self-hosted recursive `enum Expr { Num(i64), Bin(i64, Box<Expr>, Box<Expr>) }`,
build `Bin(+, Box(Num 2), Box(Num 3))`, recursively evaluate via `match` → exits
**5** (self == host); (d) a boxed value in a `Vec<Box<T>>` (composes with v92 Vec
+ v94 generics); (e) **leak check** — a loop building + dropping 100k `Box`es is
`MALLOC_CHECK_=3` clean + RSS-flat (the drop-at-exit discipline holds); (f)
**byte-identity** — all prior self-host gates byte-for-byte (use-gated `Box` path).
`docs/bootstrap-status.md` updated: recursive-AST-shaped files now move from
"blocked on Box" to "in-subset" (the ledger grows, honestly counted).

**DEFERRALS.** `HashMap` codegen in the subset (the keyed-hash runtime — Survey 3
rank #3, L; the next bootstrap rung after Box, needed for symbol tables).
Multi-param generics `<A, B>` in the subset (gates `Result<T,E>` + `Pair<K,V>` —
v94 deferral, tracked). Closures + `dyn` vtables in the subset (XL — v98
deferrals; the emitter has zero indirect-call machinery). Modules in the subset
(the emitter is single-source-string; real value needs a multi-file bootstrap
arc). The **full**-tree fixed point (still XL — paced file-by-file in the ledger).

---

## ARC D — Developer experience depth (v109–v110): mature the surface

### v109 — `assert_eq!`/`assert_ne!` macros + `kard bench`

**STATUS: PLANNED.**

**Theme:** Survey 3 §3f / §1 §2 #4 (corrected: the `--test` harness + `test_*`
discovery **already ship** — `main.cpp:4128`, the `kard test` shim; the genuine
gaps are *assertion macros* and a *bench* mode). Today a test must hand-write
`if a != b { return 1; }` — there is no `assert_eq!`/`assert_ne!`/`assert!`, and
no `bench_*` discovery to measure a fn. Both are parser-desugar work (no full macro
system — the same `format!`-style parser desugaring from v27/v71), high-DX,
low-risk. Depends on nothing.

**CORE.** (host: parser desugaring + the `kard`/`kardc` test runner)
- (1) **`assert_eq!(a, b)` / `assert_ne!(a, b)`** parser-desugared (like `format!`,
  `ident!(`): `assert_eq!(a, b)` → `if !((a) == (b)) { panic!(...) }` with a
  message embedding both sides' `Debug` (`{:?}` — rides on v102's container Debug,
  so a failing `assert_eq!` over a `Vec` prints both values). `assert!(cond)` /
  `assert!(cond, msg)` desugar to a guarded `panic!`.
- (2) **`kardc --bench` + `kard bench`** — discover `bench_*() -> i64` fns
  (mirroring the `test_*` discovery), run each, report a **deterministic
  iteration count** (NOT wall-time — to stay CI-non-flaky, `bench` reports the
  result + a fixed loop count; wall-time is advisory/local-only, never gated). The
  shim mirrors the `kard test` shape.
- (3) **Structured assertion-failure output:** on `assert_eq!` failure, print
  `assertion failed: left == right\n  left:  {:?}\n  right: {:?}` and a non-zero
  exit (so `kard test` counts it as a failure).

**GATE.** `smoke_test_assert_macros.sh` — deterministic exit/stdout: (a)
`assert_eq!(2 + 2, 4)` passes (exit 0); (b) `assert_eq!(2, 3)` fails with the
`left: 2 / right: 3` message + non-zero exit; (c) `assert_eq!` over two `Vec<i64>`
prints both via `{:?}` on mismatch (rides v102); (d) `assert_ne!(1, 1)` fails;
`assert!(false, "boom")` panics with "boom"; (e) `kard test` over a file with 3
passing + 2 failing `test_*` fns (using these macros) exits non-zero + reports the
count; (f) `kard bench` discovers + runs a `bench_*` fn deterministically. No
wall-time assertion anywhere (CI-non-flaky).

**DEFERRALS.** A general user-definable macro system (`macro_rules!`-style
recursive expansion + hygiene — XL, Survey 1 §2 #14, its own mega-arc-class
version). `assert!` with format-arg messages beyond a literal (folds into the
format-spec version). `#[should_panic]` / `#[ignore]` test attributes (a follow-on
to the test harness). Wall-time benchmark *gating* (kept advisory/local — never
CI-gated, per the established non-flaky rule).

---

### v110 — bound-satisfaction diagnostics + LSP code actions

**STATUS: PLANNED.**

**Theme:** Survey 2 §1E (the v96 deferral, `ROADMAP-v91-v100.md:340`: an
unsatisfied generic bound surfaces as the generic `E0277 no impl provides method`,
`typecheck.cpp:6539`, not a dedicated "T does not implement Tr" message) +
Survey 3 §3a (the LSP — 1257 lines — has hover/completion/definition/references/
rename but **no `codeAction`**). This is the DX-polish close of the arc: make the
two most-used surfaces (compiler errors + editor) name the *actual* problem and
offer fixes. S/M-difficulty (a diagnostic-message refinement + an LSP request
handler over the existing diagnostic codes), low risk, high daily-use leverage.

**CORE.** (host typecheck diagnostics + the LSP)
- (1) **Bound-satisfaction diagnostic:** when a trait-bounded generic call fails
  because the concrete type lacks the bound's impl, emit a dedicated
  `error[E0277]: the trait bound \`T: Clone\` is not satisfied` (naming the type +
  the unsatisfied trait), with a note pointing at the bound's declaration site —
  replacing the bare `no impl provides method`. Reuse the existing impl-resolution
  failure point (`typecheck.cpp:6539`); classify by the missing (type, trait) pair.
  **Full-suite grep sweep** for the old message string per the v88 reworded-error
  lesson (update any test asserting the old text).
- (2) **`kardc --explain E0277`** entry updated to the new wording (the curated
  explain table).
- (3) **LSP `textDocument/codeAction`:** surface the existing diagnostic codes as
  quick-fix actions — e.g. a missing-`;` (parser recovery) suggests "add
  semicolon"; an `E0308` type mismatch suggests an `as`-cast where sound; an
  unsatisfied-bound `E0277` suggests "add `#[derive(Clone)]` to `T`" when `T` is a
  local struct. Each action returns a concrete `WorkspaceEdit` (a text insertion at
  a computed position).
- (4) **LSP `textDocument/implementation`** (find-implementations): from a trait
  under the cursor, list its `impl` sites (reverse-lookup over the existing
  occurrence index).

**GATE.** `smoke_test_bound_diag.sh` + the LSP gate extension: (a) a call requiring
`T: Clone` on a non-`Clone` type errors with `the trait bound \`...: Clone\` is not
satisfied` (grep the message, not the old `no impl provides method`); (b) the
**full reworded-error grep sweep** is clean (no test still asserts the old string);
(c) `--explain E0277` prints the updated text; (d) `smoke_test_lsp.sh` extension:
a `codeAction` request on a missing-`;` diagnostic returns an "add semicolon" edit
with the correct position; (e) an `implementation` request on a trait returns its
impl sites; (f) the existing LSP hover/completion/definition/references/rename
cases stay green (no regression). Deterministic (LSP requests over fixed source,
exact JSON-RPC response assertions — no timing).

**DEFERRALS.** A full call-site bound-satisfaction *engine* (proving/disproving
arbitrary bounds independent of impl resolution — M, its own version; this version
ships the *diagnostic*, which is the high-leverage S part). Incremental AST caching
in the LSP (full re-parse per change today — XL rearchitecture, Survey 3 §4;
deferred to the incremental-compile mega-arc). Auto-import code actions (needs the
module/registry infra). Inlay hints / semantic tokens (follow-on LSP polish).

---

## Out of scope (the XL mega-arcs, still deferred past v110)

Each is multi-session and/or environment-blocked — none is per-version tractable,
and the environment-blocked ones cannot be CI-differentially-gated, so they are
**not scheduled as a version** (per the no-unverifiable-version rule). Named here
with honest sizing, the grounded starting point for a future `/goal`.

1. **Full self-hosting bootstrap** — v107–v108 advance the subset emitter two rungs
   (`Option`/`match`, then `Box<T>`); the *complete* tree self-compiling (every
   `examples/selfhost/*.kd`, then `compiler/` itself) is the continuing mega-arc,
   paced file-by-file via `docs/bootstrap-status.md`. The remaining subset gaps
   after this arc: `HashMap` codegen, multi-param generics (`Result<T,E>`/`Pair<K,V>`),
   closures, `dyn` vtables, modules — each an owned future rung.
2. **Register-ABI struct-by-value FFI** — the per-platform System V eightbyte
   classifier + `sret` (~2000 LOC, Survey 2 §1D / `ROADMAP-v91-v100.md:528`); v88
   ships struct FFI by pointer, this completes zero-copy small-struct C interop.
   Platform-specific, multi-session.
3. **WASM + Windows backends** — new codegen targets + ABIs (Win64 calling
   convention, WASM linear-memory + table model). **Environment-blocked for CI**:
   differential gating needs `wasmtime` / `wine`, absent in the sandbox — so this
   cannot ship as a CI-verifiable version until that tooling is available.
4. **Hosted package registry** — `kard.toml` local-path deps work today; a real
   registry needs **network access (absent in CI)** plus resolution / lockfile /
   auth machinery. Environment-blocked; also gates the orphan rule (v96's honest
   deferral — no soundness value without crate boundaries).
5. **Named lifetimes + full NLL + `Pin`/self-referential safety** — a multi-month
   type-theory rewrite (region unification, lifetime inference, pinning; Survey 2
   §1F). Blocks inter-procedural borrow analysis, escape-to-stack for closure envs,
   returning references to struct fields, and lifetime params on structs. XL.
6. **Incremental compilation** — the compiler is monolithic, not query-based; a
   scoped single-file AST cache is M (Survey 3 §4) but full cross-file incremental
   is a salsa-style rearchitecture. XL; the LSP's full-reparse-per-change folds in.
7. **Format-spec dispatch + a user macro system** — `{:x}`/`{:04d}`/`{:e}` routing
   (M, a near-term follow-on the v71 helpers already support) and a real
   `macro_rules!`-style recursive-expansion + hygiene system (XL, Survey 1 §2 #14).
8. **The full numeric tower** — i32/u64/u32/u16/u8 beyond the v87 sized ints used
   in FFI/packed contexts, as fully-first-class arithmetic types with their own
   overflow/cast rules everywhere (Survey 1 §2 #12). XL (type-system + codegen
   specialization across all paths).
9. **Mechanized spec → 1.0 proof** — a normative grammar + type/ownership/effect
   rules cross-checked against the implementation; the 1.0 capstone.

---

## Sequencing rationale (leverage × dependency)

```
v101 stdlib: element-generic iterators   ← #1 stdlib gap; fix the nested-adaptor PHI crash
 └ v102 stdlib: container Debug ({:?})     ← highest-DX gap; needs element-generic Vec
 └ v103 stdlib: sort/search algorithms     ← O(n log n) + binary_search + partition
    └ v104 stdlib: slice utilities          ← slice→Vec / iter / chunks; rides v101 iterators
v105 type-system: tuple/Option/Result Eq+Hash ← finishes generic-impl; unblocks HashMap<(K,V)>
 └ v106 codegen: tail-call lock + bounds elision ← the measurable, deterministic codegen win
v107 selfhost: Option<T> + match lowering  ← bootstrap rung; needs v91 CFG
 └ v108 selfhost: Box<T> heap indirection   ← bootstrap rung; recursive AST; needs v107
v109 DX: assert macros + bench             ← finishes the test surface (harness already shipped)
 └ v110 DX: bound diagnostics + LSP code actions ← polish the two most-used surfaces
```

Stdlib depth leads (v101–v104) because it is the **highest leverage × lowest
risk × most CI-testable** work — it makes the language usable for real programs
*today*, every increment is a pure differential JIT==AOT gate, and v101 (the
element-generic iterator fix) is a prerequisite for v102's `Vec` Debug and v104's
slice iterators. Type-system/codegen depth follows (v105–v106): v105 *finishes* the
generic-impl story v101/v102 opened (composite `Eq`/`Hash`), and v106 lands the one
honest, measurable, deterministic codegen win remaining (TCO + bounds elision —
locked by IR-grep, never wall-time). The self-hosting rungs (v107–v108) advance the
bootstrap mega-arc one tractable, self==host-gated file-unlock at a time
(`Option`/`match`, then `Box`), each updating the `docs/bootstrap-status.md`
ledger honestly. DX closes the arc (v109–v110): assertion macros + `bench` finish
the test surface (the harness already ships), and bound-satisfaction diagnostics +
LSP code actions polish the two surfaces a working programmer touches most. Every
version depends only on what shipped before it; every version is a real tested
core + a deterministic differential / IR-grep / self==host gate, CI-green on
ubuntu + macOS, honest deferrals — the established cadence, no stubs.

> **Cadence + memory gotchas to respect each version** (carried from the v91–v100
> arc): `make clean` after any `ast.hpp`/`types.hpp`/enum change (stale `.o`
> reads wrong values / segfaults); run the **full smoke sweep including
> `kardfmt` + `kard-lsp`** before push; on any reworded error message, do a
> **full-suite grep + sweep** for the old string (v88 lesson); structgen
> conventions — structs precede fns, a trailing `;` after a `while` statement
> before a following expr, `&mut`-local alloca slot for mutable accumulators,
> `Widget`/`Gadget` names (single-uppercase collide with prelude generic params),
> and any new `Expr`/`Stmt` variant must get arms in **all** match sites
> (`type_of`, `lower`, and the host parser/typecheck/codegen/emit_c mirrors);
> IR-grep gates are **target-dependent → x86-64-enforce / arm64-soft** (v90
> lesson); gates must be **deterministic, never flaky wall-time** (the perf/bench
> numbers live in `BENCHMARKS.md`, never asserted in CI).
