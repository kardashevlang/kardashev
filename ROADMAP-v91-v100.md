# ROADMAP v91–v100 — toward a real bootstrap & a practical systems language, after v0.90.0

Designed against three read-only surveys of the real tree (self-hosting
completeness, practical-systems gaps, optimization/type-system depth), then
fact-checked against the compiler. The maintainer's 7th `/goal` (translated):
**"implement entirely, polish into a PRACTICAL SYSTEMS LANGUAGE, improve
SELF-HOSTING completeness, and maximize completeness / optimization /
efficiency — no stubs."**

Each version is a **tractable per-version increment** — a real, tested core
shippable in one focused session with a JIT==AOT (or self==host differential)
smoke gate, ending CI-green on ubuntu + macOS, plus honest deferrals. The four
XL mega-arcs (hosted package registry, WASM/Windows backends, register-ABI
struct FFI, mechanized 1.0 proof) remain deferred — they are not per-version
tractable. The *true bootstrap* (self-hosted compiler compiling itself) is the
through-line this arc deliberately walks toward, increment by increment, but the
fixed-point itself only becomes a candidate at v99–v100.

> **Honest framing of "self-hosting."** The self-hosted compiler lives in
> `examples/selfhost/` (~4.5k LOC of `.kd`, host-compiled). It already *emits*
> real LLVM IR for structs, enums+match, by-ref values, read-only strings, and
> user fn calls (v84–v86), differentially gated self-vs-host. What it **cannot
> yet emit** is the thing blocking every nontrivial program: it is a *branch-free
> SSA emitter* (`select`-chains, no `br`/`phi`, immutable let-bindings only). So
> the spine of this arc (v91→v92→v98) is: give the self-hosted emitter real CFG
> + mutable locals → scalar `Vec` → generics → modules → closures → trait
> dispatch → effects, until it can compile a multi-file program shaped like
> itself. That is the only path to the bootstrap fixed-point, and it is paced one
> tractable version at a time. **Note the two senses of "the subset":** the
> *host* compiler (`compiler/`) already has loops, `let mut`, `Vec`, generics,
> etc.; the *self-hosted* emitter does not yet **lower** them. v91–v98 grow what
> the self-hosted backend can lower, not what the host accepts.

> **Sequencing pivot.** Self-hosting CFG leads (v91) because it is the
> architectural fork the entire bootstrap depends on — `select`-only emission
> caps program size at "no loops, no mutation." Then the data-structure layer
> (`Vec`/strings, v92) that real compiler phases need; then a same-session
> practical-systems unlock interleaved every other version (slice mutation,
> allocator, FFI breadth, bit-fields) so the language keeps getting *usable* while
> the bootstrap deepens; with two optimization/depth versions (v95 perf gate, v96
> coherence) folded in where they unblock the most. The arc ends on the bootstrap
> fixed-point candidate (v99) and a measured 1.0-readiness audit (v100).

---

## ARC A — Self-hosting gains CFG & data (v91–v92): the bootstrap spine

### v91 — self-hosted CFG: mutable locals + `while`/`for` with real `br`/`phi`

**STATUS: ✅ SHIPPED (v0.91.0).** The self-hosted emitter (`examples/selfhost/structgen.kd`) moved from branch-free to **block-terminator-aware**. `enum Stmt` gained `LetMut`/`Assign`/`While(cond, Vec<Stmt>)`/`IfS`/`Break`/`Continue`; the lexer gained `..` (kind 25) and `<=` (kind 26); the parser parses `let mut`, `x = e`, `while`, statement-`if`, `break`/`continue`, and **desugars** `for i in lo .. hi { body }` → `let mut i = lo; while i < hi { body; i = i + 1; }`. Codegen: mutable locals lower to `alloca`/`store`/`load` (a `menv` map; immutable `let` keeps the original SSA path verbatim → **byte-identical** v84–v86 gates), `while` emits `loop.header/body/exit.N` with `br i1`, `break`/`continue` `br` to a loop-target stack, and a `termd` cursor enforces one-terminator-per-block (correctness-first: alloca+`-O2` mem2reg reclaims the SSA, no hand-emitted `phi`). The self-hosted type-checker fixes a `let mut`'s type, requires matching assignments, and rejects `break`/`continue` outside a loop. **Gate:** `smoke_test_selfhost_loops.sh` — differential self == host on while-sum (55), while/for factorial (120), break-early, continue-skip, mutable accumulator (iterative fib), nested loops, + a negative (break-outside-loop rejected); the phase115–118 + refs + calls gates stay byte-identical. (Implemented by a worktree subagent; independently re-verified — all 6 prior gates + 4 fresh differential loop cases self == host.) **Deferred:** labeled break, hand-emitted `phi`, `match`-as-decision-tree-CFG (the `select`-chain stays). Vec → v92.

**Theme:** The single architectural unlock that the bootstrap is blocked on.
Today the self-hosted emitter (`examples/selfhost/structgen.kd` /
`enumgen.kd`) is *branch-free*: `if/else` lowers to `select i1`, every binding is
immutable, and there are no basic blocks beyond the function entry. That caps the
programs it can compile at "straight-line + recursion." This version rewrites the
emitter to **block-terminator discipline** — loop header/body/latch/exit basic
blocks, real `br` and `phi`, and `alloca`-backed mutable locals — which is the
prerequisite for *every* later self-hosting increment (Vec, real lexers, the
compiler's own phase loops). It is M-difficulty but contained: no new type-system,
just a codegen-shape rewrite of one program.

**CORE.** (all in `examples/selfhost/structgen.kd` / `enumgen.kd`, the
self-hosted emitter — the *host* already supports all of this)
- (1) Parse `let mut x = e;` and `x = e;` reassignment in the self-hosted parser
  (the AST gains a `mutable` flag on the let node + an `Assign(name, expr)`
  statement node). The self-hosted lexer already tokenizes `=` and the `mut`
  keyword (it lexes the host language); this is parser + AST work only.
- (2) Codegen mutable locals as `alloca` slots: a mutable binding emits
  `%x = alloca i64`; a use emits `load`; an assign emits `store`. Immutable
  `let` keeps the current SSA-value path (no alloca) for zero regressions on the
  v84–v86 gates.
- (3) Real CFG for `while cond { body }`: emit a `loop.header` block (eval cond,
  `br i1 %c, label %loop.body, label %loop.exit`), a `loop.body` block ending in
  `br label %loop.header`, and a `loop.exit` block. This is the move from the
  current `select`-everything emitter to a **block-terminator-aware** one: track a
  "current block" cursor, ensure every block ends in exactly one terminator.
- (4) `for i in 0..n { body }` desugared in the self-hosted parser to the
  mutable-local + `while` form (`let mut i = 0; while i < n { body; i = i + 1; }`).
- (5) `break` / `continue` lower to `br` to the loop-exit / loop-header blocks
  (a small block-target stack in the emitter).
- (6) Soundness: the self-hosted type-checker (`structgen.kd` `type_of`) gains a
  rule that a mutable local's type is fixed at its `let mut` and assignments must
  match; `break`/`continue` outside a loop is rejected.

**GATE.** `smoke_test_selfhost_loops.sh` (the name `selfhost_loops_vec` was
pre-named in the v86 deferrals; split into v91 loops + v92 vec). Differential
self==host: (a) `let mut r = 0; let mut i = 1; while i <= 10 { r = r + i; i = i + 1; } f() -> r` exits **55**;
(b) `for i in 1..6 { acc = acc * i; }` exits **120** (factorial); (c) a
`while`-with-`break` early-exit and a `continue`-skip case; (d) a mutable
accumulator over a struct field; (e) the existing
`smoke_test_phase115–118` + `selfhost_refs` + `selfhost_calls` gates still pass
**byte-for-byte** (the immutable-SSA path is untouched). Each program: self-hosted
LLVM-IR exit == host exit.

**DEFERRALS.** `Vec` (v92 — needs the mutable-locals foundation this ships).
Nested-loop labeled break (`'outer: while`). Loop-carried `phi` *optimization*
(we use alloca+mem2reg-shaped IR and let LLVM's `-O2` promote; we do **not**
hand-emit minimal `phi` networks — correctness first, the optimizer reclaims the
allocas). `match` still lowers branch-free (the `select`-chain is correct and
fine for small arms; a decision-tree CFG for `match` is a later optimization).

---

### v92 — self-hosted scalar `Vec<i64>` + growable strings

**STATUS: ✅ SHIPPED (v0.92.0).** The self-hosted emitter (`examples/selfhost/structgen.kd`) now emits a growable scalar `Vec<i64>` and owned (heap) strings into its SELF-CONTAINED LLVM IR. New work: a `Vec<i64>` type-tag (4 → `{ ptr, i64, i64 }`, same shape as String); a **use-gated runtime preamble** emitting libc `declare`s (`malloc`/`realloc`/`free`/`memcpy`) + LLVM `define`s for `@kdvec_new/push/get/len/set/drop_i64` and `@kdstr_char_at/concat/drop` *only when a Vec/owned-String is used* (so v84–v91 gates stay byte-identical); builtin dispatch in `type_of`/`lower` for `vec_new`/`vec_push`/`vec_get`/`vec_len`/`vec_set` + `str_concat` (mirroring `str_len`); and **Drop-free-at-exit** for non-escaping owned locals (now possible thanks to v91's real exit block). Two foundational fixes the plan needed: `&mut <mutable-local>` now yields the local's actual `alloca` slot (not a load+re-alloca copy — else `vec_push` mutates a throwaway and `vec_len` stays 0), and a new `ExprStmt` so a bare `vec_push(...);` statement parses. **Gate:** `smoke_test_selfhost_vec.sh` — differential self == host on vec build+sum, `for`-push + `vec_len`, growable `str_concat` (incl. in a loop), a **tokenizer capstone** (push a kind per char, return the count), grow-boundary cases, negatives (missing `&mut`/arity), and a 100k-push **leak check** (`MALLOC_CHECK_=3` clean + RSS flat). (Implemented by a worktree subagent; independently re-verified — all 8 self-host gates + my own vec/string differential cases + a 200k-push RSS-flat leak check.) **Deferred (honest):** `vec_set` is self-only-tested (no host counterpart); string drop-on-*reassign* leaks the prior buffer (bounded, freed-at-exit — true drop needs liveness); `Vec<T>` non-scalar / nested `Vec` / `HashMap` → v94+ (needs generics).

**Theme:** With loops + mutable locals in hand (v91), give the self-hosted
emitter the one heap data structure every compiler phase needs: a growable
`Vec<i64>` and growable (allocated, not read-only) strings. This is what lets the
self-hosted compiler build its *own* token stream and AST node lists — the
milestone "self-hosted compiles a real ~500-line data-driven program." M-difficulty:
runtime emission, no type theory.

**CORE.** (self-hosted emitter)
- (1) Lower `vec_new()`, `vec_push(&mut v, x)`, `vec_get(v, i)`, `vec_len(v)`,
  `vec_set(&mut v, i, x)` to calls into the **host's existing `kdvec` runtime**
  (the same `{ptr,len,cap}` layout the host compiler uses — the self-hosted
  emitter emits `call`s to the runtime symbols, it does not reimplement the
  allocator). Scalar `Vec<i64>` only.
- (2) Growable strings: lower `str_concat(&a, &b)`, `str_push(&mut s, &t)` to the
  host string runtime (the read-only `{ptr,len,cap=0}` borrowed-literal path from
  v86 stays for literals; concatenation produces an owned `cap>0` string).
- (3) Drop discipline in the self-hosted subset: a `Vec`/owned-string local that
  does not escape is freed at function exit (reuse the host's Drop-on-non-escape
  logic, expressed in the self-hosted emitter as a `free` call at the exit block —
  now possible because v91 gave us a real exit block).
- (4) Type-checker: `Vec<i64>` and `String` become first-class self-hosted types
  (tags reusing the v84 type-tag scheme); `vec_get` returns `i64`, `vec_len`
  returns `i64`, etc., with arity/type checks.

**GATE.** `smoke_test_selfhost_vec.sh` (the v92 half of the pre-named
`selfhost_loops_vec`). Differential self==host: (a)
`let mut v = vec_new(); for i in 0..5 { vec_push(&mut v, i); } f() -> vec_len(v)` exits **5**;
(b) build a `Vec<i64>`, sum it in a loop → exit == sum; (c) string building
`let mut s = ""; s = str_concat(&s, "ab"); s = str_concat(&s, "cd"); f() -> str_len(&s)`
exits **4**; (d) **capstone:** the self-hosted compiler tokenizes a tiny source
string into a `Vec` of token kinds and exits the token count — a real loop-driven
data program (self == host); (e) `MALLOC_CHECK_=3` + RSS-flat over a 100k-push
loop (no leak: the v92(3) Drop discipline holds).

**DEFERRALS.** `Vec<T>` for non-scalar `T` (needs generics — v94). `HashMap` in
the self-hosted subset (keyed-hash runtime). String *slicing* with owned
substrings (read-only `str_substring` over a borrowed view is fine; owned
substring fold into v94). `Vec` of `Vec` (nested generics — v94).

---

## ARC B — Practical systems, interleaved (v93–v94, v97): make it usable now

### v93 — slice mutation (`&mut [T]`) + variadic-C FFI + C-backend slice-from-array

**STATUS: ✅ SHIPPED (v0.93.0).** `&mut [T]` is now write-capable end-to-end. A dedicated `Type.sliceIsMut` field (copied at the 4 Struct-rebuild sites; `unify` ignores it so `&mut [T]` coerces to `&[T]` for free) + an explicit mutability check at the `slice_set`/`slice_get_mut` argument site (a shared `&[T]` is rejected — the soundness gate). New builtins `slice_set(&mut [T], i, v) -> ()` and `slice_get_mut(&mut [T], i) -> &mut T` (LLVM: `slice_get_mut` = `slice_get_ref`, `slice_set` = GEP + `store`; the existing deref-assign means `*slice_get_mut(s,i) = v` needed no new code). `&mut v[a..b]` / `&mut arr[a..b]` construction (`SliceExpr.isMut`); `checkSlice`/`emitSlice` now accept an **array** operand (slice-from-array, the v89/v90 deferral). C backend: `kd_slice_get_mut`/`kd_slice_set` drop the v90 read-only restriction. **Variadic-C FFI**: a `DotDotDot` token + `ExternFn.isVarArg` thread `isVarArg=true` into the extern `FunctionType`, with C default-argument promotion (f32→double, narrow-int→i32) on the trailing args — `extern "C" fn printf(fmt: &String, ...) -> i32` works. **Gate:** `smoke_test_slice_mut.sh` — in-place sort over `&mut [i64]` / `slice_set`-fill / array-slice read+write / `&mut[T]→&[T]` coercion each **JIT == AOT == C**, `*slice_get_mut=v` + variadic `printf("n=%d",7)` JIT == AOT, and **two** soundness negatives (E0502 aliasing-read-across-`slice_set`; `slice_set` on a shared `&[T]`). (Worktree subagent; independently re-verified — 6 unit suites + existing slice/array/FFI gates + the new gate's 9 cases, after a `make clean` rebuild for the new `Type`/AST fields.) **Deferred (honest):** variadic + `*slice_get_mut=v` deref-assign in the C backend (`--emit-c` refuses extern fns / non-var-place assigns — LLVM/JIT/AOT full); non-scalar `&mut [String]` in C (LLVM full); mutable-slice *iteration* (`for x in &mut s`) → v94; register-ABI struct-by-value FFI → XL mega-arc.

**Theme:** Survey 2's #1 gap: **mutation-through-slice does not exist in any
backend** — `slice_set`/`slice_get_mut` are absent and `&mut [T]` writes are
rejected even in LLVM (`E0384`). That blocks an entire class of in-place
algorithms (in-place quicksort/merge-sort over a buffer, slice-fill loops). This
version makes `&mut [T]` a real, write-capable type end-to-end, and folds in two
small adjacent FFI/C-backend unlocks. M/L difficulty: the borrow-check rules are a
near-copy of the array rules already proven in v26/v89.

**CORE.** (host: typecheck + borrow_check + both backends)
- (1) `&mut [T]` as a distinct slice type from `&[T]` in the type system: a
  mutable-slice flag on the slice type (typecheck.cpp slice handling); `&mut v[a..b]`
  and `&mut arr[a..b]` construction.
- (2) `slice_set(s: &mut [T], i, v) -> ()` and `slice_get_mut(s: &mut [T], i) -> &mut T`
  builtins: LLVM codegen lowers a bounds-checked GEP + `store` (mirrors the v90
  read-only `slice_get` GEP+`load`); the C backend lowers the same over the v90
  `struct kdslice { T* ptr; i64 len; }` (drop the v90 read-only restriction for
  `&mut`).
- (3) Borrow-check: `&mut [T]` follows the existing `&mut place` exclusivity
  rules (borrow_check.cpp) — one active `&mut` slice, no aliasing `&[T]` read live
  across a `slice_set`; reuse the v26 two-phase-borrow machinery so
  `slice_set(&mut s, i, slice_get(&s, j))` is rejected at same depth but the
  vec-push-len idiom shape works.
- (4) Adjacent fold-ins (both small, both named in Survey 2): **variadic C
  functions** in extern sigs (`extern "C" fn printf(fmt: &String, ...) -> i32`) —
  C-backend varargs lowering + LLVM varargs call; and **slice-from-fixed-array in
  the C backend** (`let s = &arr[1..3]` over a stack `[T; N]` — the v89/v90 C
  deferral, a single GEP-style lowering).

**GATE.** `smoke_test_slice_mut.sh`: (a) an in-place merge-sort over a
`&mut [i64]` produces a sorted array, **JIT == AOT == C backend**; (b) a
slice-fill loop writes then reads back the right total; (c) `printf("%d\n", x)`
via a variadic extern prints the value (exit/stdout match, C backend); (d)
`&arr[1..3]` over a stack array sums correctly in the C backend == LLVM; (e) a
borrow-check negative: aliasing `&[T]` read live across a `slice_set` is rejected
(`E0502`).

**DEFERRALS.** Mutable-slice *iteration* (`for x in &mut s` yielding `&mut T`
each step — needs the element-generic iterator work in v94). `&mut [T]` of a
non-scalar element in the C backend (scalar only there; LLVM handles non-scalar).
Register-ABI struct-by-value FFI (XL mega-arc — System V eightbyte classifier,
~2000 LOC). `repr(packed)`/bit-fields (v97).

---

### v94 — self-hosted generics (monomorphic specialization) + element-generic stdlib iterators

**STATUS: ✅ SHIPPED (v0.94.0) — PART 1 (self-hosted generics); PART 2 (element-generic iterators) deferred with verified evidence.** The self-hosted emitter (`examples/selfhost/structgen.kd`) gains single-type-parameter monomorphic generics: `Fn.gp`/`SDef.gp` generic-param-count fields, tag `-1` for the unbound `T` (`&T` = 199), `<T>` parse in `parse_fn`/`parse_structs`, a **monomorphization registry** in `main` (dedup by mangled name, mirroring the host's `emittedInstances_`), `mangle`/`specialize_*` helpers, and `Call`/`SLit` routing to the per-concrete-type instance (T inferred from the first generic-typed argument/field). Use-gated so non-generic programs stay **byte-identical** (the 6 prior self-host gates). **Gate:** `smoke_test_selfhost_generics.sh` — differential self == host on `fn id<T>(x:T)->T` specialized at i64 + at a struct, a generic struct `Pair<T>` build+sum, a generic call in a loop, two-types-dedup (exactly 2 instances), and an ill-typed-generic-call negative. (Worktree subagent; independently re-verified — 7 self-host gates + my own cases incl. a generic call feeding a generic struct literal, self == host; a no-debug-instrumentation audit.) **PART 2 deferred (honest, evidence-based):** making the host iterator adaptor tower element-generic — empirically, the typecheck fix is a one-liner (`assocEnv = implParamEnv(impl)`) that unblocks a *single-level* `impl<T> Iterator<T> for VecIter<T>`, but **nested adaptors** (`impl<T, I: Iterator<T>> Iterator<T> for Take<I>`) crash codegen with a PHI type mismatch (`T` is unresolved through the transitive bound — real L codegen work to resolve `T` from the bound at monomorphization), and even the one-liner carries M-risk to the 10+ shipped i64-adaptor tests. So the i64 adaptor tower stays as-is; element-generic iterators move to a later line. Also deferred: generic trait dispatch (vtables) → v98; const-generics / multi-param `<A,B>` in the self-hosted subset.

**Theme:** The boundary where the self-hosting subset gains the feature the *real*
compiler uses pervasively — type parameters. This ships **monomorphic
specialization** in the self-hosted emitter (`fn pair<T>(x: T, y: T) -> T`,
`Vec<Struct>`, `Result<T,E>` in the subset), which is the well-understood,
per-version-tractable cut of the generics mega-arc. In the same session, close the
matching *host* stdlib gap: the iterator adaptor tower is `i64`-only because a
generic impl can't yet bind a generic param as a trait type-arg — fixing that
makes `take`/`skip`/`map`/`filter` element-generic. Together: generics for the
bootstrap *and* a finished generic iterator story.

**CORE.**
- (1) **Self-hosted generics (the bootstrap-spine half):** type parameters in
  self-hosted fn signatures and struct/enum decls; **monomorphic specialization at
  codegen** — emit one specialized copy per concrete type used at a call site
  (dedup by mangled name, mirroring the host's `emittedInstances_`). Self-hosted
  `Vec<Struct>` / `Vec<Enum>` via specialization; `Result<T,E>` / `Option<T>` in
  the subset. (`examples/selfhost/` — the emitter clones + substitutes type tags.)
- (2) **Element-generic host iterators (the stdlib half):** fix generic-impl
  resolution so `impl<I: Iterator<T>, T> Iterator<T> for Take<I>` binds a generic
  param as a trait type-arg (typecheck impl resolution + codegen monomorphization,
  the limitation named in Survey 3 §3). Makes the v61/v78 adaptor tower
  (`take`/`skip`/`chain`/`zip`/`map`/`filter`) work over any element type, not just
  `i64`.

**GATE.** `smoke_test_selfhost_generics.sh` (self-hosted half) +
`smoke_test_iter_generic.sh` (host half). (a) self-hosted `fn id<T>(x: T) -> T`
specialized at `i64` and at a struct, each correct (self == host); (b)
self-hosted `Vec<Struct>` build+sum (self == host); (c) host
`vec_of_strings.iter().map(...).filter(...).collect()` over `Vec<String>` runs,
JIT==AOT; (d) a `Vec<Struct>` adaptor chain, JIT==AOT. The self-hosted cases stay
differentially gated self-vs-host.

**DEFERRALS.** Generic *trait dispatch* in the self-hosted subset (vtables — v98).
Const-generics in the self-hosted subset. HKT / parameterized GATs (`type Out<T>;`
with bounded `Self` — still concrete-Self only; a future type-theory version).
Overlapping-blanket-impl rejection (coherence — v96). Modules in the self-hosted
subset (v98).

---

### v97 — `#[repr(packed)]` + bit-fields + volatile + endianness intrinsics

**STATUS: PLANNED.** (host; binary-format / device-driver systems gap)

**Theme:** Survey 2 §2A/§2B/§2F: packed binary layouts, volatile MMIO access, and
endianness. None of these exist today (`repr(packed)` rejected, no
`volatile_load`/`store`, no `to_le`/`to_be`/`swap_bytes`). This is the
"parse-a-packet-header / touch-a-device-register / read-a-binary-file" version —
the gateway to real low-level systems code. The `repr` infrastructure already
exists (`#[repr(C)]` shipped v88), so packed layout is a follow-on, not a
from-scratch build. M-difficulty (bit-field parsing is the intricate part).

**CORE.** (host: parser + typecheck + codegen)
- (1) `#[repr(packed)]` structs: no inter-field padding; codegen uses LLVM packed
  struct types + `align 1` loads/stores; extends the v88 `#[repr(C)]` repr-attribute
  infrastructure.
- (2) Bit-field syntax + lowering: `struct Hdr { version: u3, flags: u5, len: u8 }` —
  parser accepts a sub-byte width on a field, typecheck computes bit offsets, codegen
  emits bitwise extract (`(word >> off) & mask`) on read and insert
  (`word = (word & ~mask) | (v << off)`) on write. Field access reads/writes by name
  with type safety (vs. today's manual `(x >> 3) & 0x7`).
- (3) `volatile_load(p: *const T) -> T` / `volatile_store(p: *mut T, v: T)`
  intrinsics → LLVM `load volatile` / `store volatile` (Survey 2 §2F — silent-drop
  miscompile risk today). Uses the v33 `unsafe` for the raw-ptr operands.
- (4) Endianness intrinsics: `i64::to_le/to_be/from_le/from_be`, `swap_bytes`
  (LLVM `bswap`) + a `cfg(target_endian = "little")` build cfg.

**GATE.** `smoke_test_repr_packed.sh`: (a) a `#[repr(packed)]` 3-field header
round-trips through a `&[u8]` byte buffer with the exact byte layout (no padding —
assert `size_of` == sum of field bytes, JIT==AOT); (b) a bit-field header packs
`{version:u3, flags:u5}` into one byte and reads each field back correctly; (c)
`volatile_store` then `volatile_load` round-trips and the IR shows `volatile`
(grep `--emit-llvm`); (d) `swap_bytes(0x0102030405060708)` ==
`0x0807060504030201`. C backend: packed structs + bswap intrinsics where scalar;
bit-fields cleanly refused (LLVM-only) if the C lowering is non-trivial.

**DEFERRALS.** `#[repr(transparent)]` / `#[repr(align(N))]` (the rest of the
repr-family — a small follow-on). Unions (`union { }` — its own version). SIMD
*intrinsics* (`Simd<i32,4>` — auto-vectorization already works; explicit
intrinsics are M/XL, later). `cfg(target_endian)` for big-endian *targets* (we
only cross-compile-cfg, no big-endian backend ships).

---

## ARC C — Optimization & type-system depth (v95–v96): lock the gains

### v95 — codegen perf: close the ~1.2× fib gap + a permanent perf-regression gate

**STATUS: ✅ SHIPPED (v0.95.0) — re-scoped by ground-truth measurement.** The "~1.2× fib gap" was **stale text**: measured on this host (best-of-5, cache cleared), `fib(40)` and the 200M `loop` are at **1.00× C** — `@fib`'s asm is byte-identical to clang's and `@main` (loop) has **0 allocas + 16 vector ops** at -O2 (the v51 TargetMachine/TTI fix already neutralized the old alloca-heavy lowering via mem2reg/SROA). So there is **no codegen change to make** — an inliner-threshold tweak would be a no-op stub or a regression risk. v95 therefore ships the version's actual unaddressed risk: a **permanent, CI-robust perf-regression gate** that LOCKS the measured parity invariants so a future PassBuilder/codegen refactor can't silently regress perf (the v51 win was stumbled upon, never regression-tested). **Gate:** `smoke_test_perf_regression.sh` — BLOCKING **deterministic structural IR-greps** (identical on x86-64 + arm64, zero wall-time): `@fib` 0 allocas at -O2, `@main`(loop) 0 allocas at -O2, loop auto-vectorizes (arch-aware: x86-64 strict, arm64 soft); plus an **advisory, generous (≤2.0×), best-of-5, x86-64-only, skippable** wall-time sanity (catches a gross >2× regression only — the tight 1.00× numbers live in BENCHMARKS.md, never asserted in CI, so the gate can't flake). Complements (not duplicates) the v65 codegen-perf + v90 vector-lock gates. **Deferred (honest):** LTO / cross-module inlining, true tail-call elimination, escape-to-stack for closure envs (all XL / their own version) — the fib gap is irreducible below 1.00× without them, and it is already at 1.00×.

**Theme:** Survey 3 §1: `fib(40)` is reproducibly **~1.2× C** (the others —
collatz/loop/primes — are at parity since the v51 TTI fix). Root cause is
alloca-heavy `let`/parameter lowering (codegen.cpp:506–514, 763–807) plus an
inliner cost model that doesn't account for kardashev's let-binding overhead. Just
as load-bearing: **CI has no perf-regression gate** — the v51 vectorization fix
was *stumbled upon*, not regression-tested, so a future refactor could silently
regress perf. This version closes the fib gap *and* installs the gate so perf
never silently rots again. M-difficulty, high ROI.

**CORE.** (host codegen + CI)
- (1) Inliner cost-model tuning: adjust the LLVM inliner threshold/cost for small
  kardashev functions (recursive-call wrappers, parameter setup) so the
  `fib`-shaped recursive wrapper inlines like Clang/Rust does. Measure against
  `bench/fib.kd`.
- (2) Alloca→register pressure: ensure every immutable `let` and by-value
  parameter that never has its address taken stays an SSA value (relies on
  mem2reg/SROA running with the right datalayout — re-verify the v51 TM/TTI
  registration covers these paths, MEMORY: missing-TM killed vectorization once;
  the same TM enables proper promotion).
- (3) LICM for allocation-free loops: confirm loop-invariant code (and any
  loop-invariant closure-env / Vec-capacity setup) is hoisted; add the passes if
  the default `-O2` pipeline misses kardashev's lowering shape.
- (4) **Permanent perf-regression gate** (`smoke_test_perf_regression.sh`,
  CI-enforced): time `bench/fib.kd` and `bench/loop.kd` vs the C reference, assert
  the ratio stays within a threshold (≤1.1× fib after this version's fix, ≤1.05×
  loop). Tolerant enough for CI noise, tight enough to catch a real regression.

**GATE.** `smoke_test_perf_regression.sh`: (a) `bench/fib.kd` AOT runs within
**≤1.1× C** wall-time (down from ~1.2×); (b) `bench/loop.kd` stays **≤1.05× C**;
(c) the vector-op IR assertion from v90 still holds; (d) outputs still match the
correctness oracle (perf changes must not change results). Threshold is a
documented constant so a maintainer can re-tune for slower CI hardware.

**DEFERRALS.** LTO / cross-module inlining (needs whole-program link
infrastructure — XL). Target-specific regalloc hints / spill minimization
(LLVM-target work). Escape-to-stack for closure envs (M, but its own version —
folds into a future allocator/closure pass). Application-scale benchmark suite
(only `fib`/`loop` are reliably timeable above the resolution floor;
matmul/primes are <14ms and below the timer — correctness-only).

---

### v96 — coherence oracle + generalized negative impls

**STATUS: ✅ SHIPPED (v0.96.0) — re-scoped by ground-truth probing.** Three of the four CORE premises were **already met** by the shipped compiler (verified against `build.local/kardc`, the v90 "premise often already met → refocus on real gap" pattern): (1) overlapping blanket impls are **already rejected** — `impl<T> Foo for T` + `impl<U> Foo for U` errors `conflicting implementations of trait` (typecheck.cpp coherence pass via `expandBlanketImpls`); (4) **concrete-beats-blanket already works** — `impl Foo for W` + `impl<T:Clone> Foo for T` dispatches `W.f()` to the concrete impl (main.cpp:2721 skips W during expansion); and a duplicate concrete impl is already a clean error. So v96 invests the real work in the genuine gaps: **(R1)** the existing coherence diagnostic had **no stable error code** — added **E0119** (+ `kardc --explain E0119`), keyed on the `conflicting implementations` / `conflicting \`impl\`` / `duplicate impl of marker` / `duplicate negative impl` substrings, placed ahead of the broad `E0308` fallback; **(R2)** negative impls were hard-restricted to `Send`/`Sync` (typecheck.cpp:2216) — **generalized to any declared trait**: `impl !Tr for X {}` (method-less, trait must be declared) opts X out of a blanket `impl<T> Tr for T`. The enforcement needed **no new machinery** — `expandBlanketImpls`'s existing `impld` set already seeds `"X/Tr"` from the negative impl, so the blanket simply never synthesizes `impl Tr for X`, and a later `X{}.tr_method()` then fails with the existing `E0277 no impl provides method`. The coherence pass gained a separate `seenNegPairs` set so a positive `impl Tr` + negative `impl !Tr` (either order) and a duplicate `!Tr` are clean `E0119` conflicts, while a negative never falsely reads as a second positive. **Orphan rule (CORE 2) — DEFERRED, honestly, in-source** (typecheck.cpp comment): it has **no soundness value in a single-crate language** (every impl shares one prelude; a foreign-trait+foreign-type impl can only conflict — already caught — or be a benign extension), so enforcing it would forbid working code while catching nothing new; revisit at the package-ecosystem mega-arc. **GATE:** `smoke_test_coherence.sh` — 11 cases: a true overlap errors (now `E0119`); **concrete-beats-blanket compiles AND the binary exits 111 not 222** (the #1 false-positive guard, with dispatch asserted); the blanket applies without the opt-out (exit 7); `impl !Greet for H {}` makes `H{}.g()` fail to resolve; `impl Tr`+`impl !Tr` and duplicate `!Tr` conflict; a negative impl of an unknown trait / with a method body is rejected; **`#[derive(Clone)]` over a `Vec` field deep-copies (exit 7) and `#[derive(Debug)]` formats (exit 0)** — the highest-risk derive regression, locked by running the binaries; and `--explain E0119` prints. (Implemented + independently verified in-session: 316 typecheck unit cases, 139 parser cases, the v31 marker gate `smoke_test_phase167` and v25 `smoke_test_phase138` both green, full smoke sweep clean.) **Deferred (honest):** a call-site **bound-satisfaction** checker (an unsatisfied generic bound still surfaces as `E0277 no impl provides method` at the resolution site rather than a dedicated "T does not implement Tr" message — that needs a full bound-satisfaction subsystem, its own version); the orphan rule (above); full RFC-1023 covered-types lattice / `default fn` specialization / cross-crate coherence / assoc-type-projection disjointness (all pre-deferred in the roadmap, none regressed).

**Original plan (for the record):** (type-system correctness)

**Theme:** Survey 3 §2.5: overlapping blanket impls are **not rejected** —
monomorphization is silent on shadowing, which is a latent unsoundness as
impl-heavy codebases (including the growing self-hosted compiler) accumulate
impls. This version adds a formal overlap oracle to impl resolution and
generalizes the existing `!Send`/`!Sync` negative-impl machinery to arbitrary
user traits. M-difficulty (~200 LOC for the coherence algorithm), needed for
correctness as the codebase scales toward bootstrap.

**CORE.** (host typecheck)
- (1) Coherence overlap oracle in impl resolution (typecheck.cpp:2720–2850): an
  `impl<T> Foo for T` overlaps with `impl Foo for i64` and is **rejected at
  typecheck** unless the concrete type is provably disjoint — replaces today's
  silent shadowing with an `E`-coded coherence error.
- (2) Orphan rule: an impl must own either the trait or the type (same-module
  rule for now, since cross-crate isn't a concept) — rejects "impl a foreign trait
  for a foreign type."
- (3) Generalized negative impls: extend the v31 `impl !Send for T {}` /
  `markerImpls_` oracle to *any* user trait — `impl !Clone for Handle {}` opts a
  type out of an auto/blanket impl, consulted before the structural/blanket check.
- (4) Specialization-aware: keep the existing "a concrete impl wins over a blanket"
  behavior (the v28 specialization that already works) — the oracle only rejects
  *true* overlaps, not a concrete-beats-blanket refinement.

**GATE.** `smoke_test_coherence.sh`: (a) two overlapping impls
(`impl Foo for i64` + `impl<T> Foo for T` with no disjointness proof) now error at
typecheck (previously silent); (b) a concrete-beats-blanket pair still compiles
and dispatches to the concrete impl (no false positive); (c) `impl !Clone for
Handle {}` makes a generic `clone`-bound call on `Handle` fail to resolve; (d) an
orphan impl (foreign trait, foreign type) is rejected. Unit tests for the oracle
on overlap/disjoint/refinement cases.

**DEFERRALS.** Full RFC-1023-style "covered types" lattice (we ship the
common-case oracle, not the full negative-reasoning calculus). Lattice-based
specialization (`default fn` in impls). Cross-crate coherence (no crate boundary
yet). Associated-type-projection-driven disjointness (concrete-type disjointness
only).

---

## ARC D — Self-hosting: the bootstrap home stretch (v98–v100)

### v98 — self-hosted modules + closures + trait dispatch (the bootstrap feature-complete set)

**STATUS: PLANNED.** (largest version of the arc; the last feature gate before fixed-point)

**Theme:** The real compiler is multi-file, uses closures (map/filter/reduce in
its iterators), and dispatches on traits (Drop, Copy, Display, …). For the
self-hosted compiler to be shaped like itself, the subset needs all three. This is
the **L→XL boundary** version — deliberately the heaviest single increment — that
takes the self-hosted emitter from "monomorphic single-file" to "feature-complete
enough to express a real compiler." It is gated incrementally (modules, then
closures, then dispatch) so each lands tested even though they ship together.

**CORE.** (self-hosted emitter — three coupled features)
- (1) **Modules:** inline `mod lexer { } mod parser { }` + `pub`/private item
  visibility + intra-file `use` in the self-hosted subset; cross-*file* `mod foo;`
  loading (so the self-hosted compiler can split into `lexer.kd`, `parser.kd`,
  `typeck.kd`, `llvmgen.kd` — the shape it already has in `examples/selfhost/`).
  Scope tracking + visibility checks, no new codegen.
- (2) **Closures:** `|x, y| x + y` lambda syntax + capture analysis (by-ref /
  by-move) + higher-order fn params (`fn call(f: fn(i64)->i64, x)`) in the
  self-hosted emitter — closure-env struct generation + fat-ptr call, mirroring
  the host's v65/closure ABI. Enables `vec.map(|x| x+1)` in the subset.
- (3) **Trait dispatch:** user `trait` + `impl` + vtable codegen for `dyn Trait`
  in the self-hosted subset; basic trait bounds on generics (`fn f<T: Show>(x: T)`).
  Static (monomorphized) dispatch first; `dyn` vtable second.

**GATE.** Three differential self==host gates landing in sequence:
`smoke_test_selfhost_modules.sh` (a 2-module lexer+parser split, each tested),
`smoke_test_selfhost_closures.sh` (`let f = |x| x*2; f(5)` → 10; a `map`-shaped
chain), `smoke_test_selfhost_traits.sh` (a `Show` trait impl'd for two types,
called both statically and via `dyn`). Each program: self-hosted LLVM-IR exit ==
host exit. **Capstone:** the self-hosted compiler, split across ~4 real files,
compiles a small multi-fn program through all four phases (self == host exit).

**DEFERRALS.** `FnMut`/`FnOnce` capture-by-move *drop semantics* in the
self-hosted subset (Fn-by-ref first). Trait *coherence* in the subset (host has it
from v96; the subset assumes well-formed input). Effect typing in the self-hosted
type-checker (v99). The fixed-point itself (v99–v100).

---

### v99 — self-hosted effects + the bootstrap fixed-point candidate

**STATUS: PLANNED.** (the bootstrap milestone — multi-session-class, scoped to a candidate)

**Theme:** With data (v92), generics (v94), modules/closures/dispatch (v98) in the
subset, the self-hosted compiler is *feature-complete enough to express its own
type-checker* — including the effect rows the host uses. This version threads
effect typing through the self-hosted type-check pass and then attempts the
**bootstrap fixed-point**: the host compiles the self-hosted compiler to a binary,
that binary compiles the self-hosted compiler *again*, and the two outputs match.
This is honestly an XL milestone; v99 ships the *candidate* (the fixed-point on a
substantial subset of the self-hosted sources), with the remaining gap to the
*full* tree named explicitly.

**CORE.**
- (1) **Effects in the self-hosted subset:** the self-hosted type-checker tracks
  the opt-in effect rows (`! { alloc }`, `! { io }`) the host uses (v81 made them
  opt-in, so an absent row is fine — the subset only needs to *parse and
  propagate*, not enforce strictly). Just enough to type-check the self-hosted
  compiler's own source.
- (2) **Prelude-in-subset:** the handful of prelude builtins the self-hosted
  sources call (`vec_*`, `str_*`, `option_*`, `print`) expressed/declared in the
  self-hosted subset so a self-hosted-compiled binary has them.
- (3) **Fixed-point harness:** `kardc --selfhost -o stage1-kardc examples/selfhost/<sources>`,
  then `./stage1-kardc <sources> -o stage2.ll`, assert `stage2.ll` byte-identical
  to the host-produced `stage1`-input IR on a target subset of the self-hosted
  files (the largest set that v91–v98 features cover).
- (4) Honest accounting: a `docs/bootstrap-status.md` that lists, file by file,
  which `examples/selfhost/*.kd` the self-hosted compiler can already compile and
  which still need a deferred feature — the precise remaining gap to a *full*
  bootstrap.

**GATE.** `smoke_test_selfhost_effects.sh` (effect rows parse + propagate in the
subset, self == host on an effectful program) + `smoke_test_bootstrap.sh` (the
fixed-point candidate: stage1 → stage2 byte-identical on the covered file set; if
a file is out of subset, it is *listed* as deferred, not silently skipped — the
gate asserts the covered set is non-trivial and grows vs v98).

**DEFERRALS.** The *full*-tree fixed-point (every `examples/selfhost/*.kd`
self-compiling) — named per-file in `bootstrap-status.md`; closing the last files
is v100 + beyond. Non-tail / multi-shot effect resume in the subset (the host
defers it too). Self-hosted HashMap (keyed-hash runtime, still deferred).

---

### v100 — bootstrap close-out + codegen audit + the 1.0-readiness ledger

**STATUS: PLANNED.** (consolidation + the gateway to the 1.0 mega-arc)

**Theme:** Close the v91–v100 arc: push the bootstrap fixed-point across the last
tractable self-hosted files, run a comprehensive codegen-correctness audit over
every lowering path the arc touched, and produce a measured, honest **1.0-readiness
ledger** that turns the four remaining XL mega-arcs into entry criteria. Mostly
audit + documentation + the final bootstrap files — M-difficulty, high
consolidation value.

**CORE.**
- (1) **Bootstrap close-out:** convert the remaining "deferred" files in
  `docs/bootstrap-status.md` that v91–v99 features actually cover into passing
  fixed-point files; for any that need a genuinely new feature, document it as a
  post-1.0 line (no stub). Re-run `smoke_test_bootstrap.sh` with the expanded set.
- (2) **Codegen audit:** survey every lowering path the arc added or changed
  (mutable-slice store, packed/bit-field extract-insert, volatile load/store,
  self-hosted CFG `br`/`phi`, monomorphized self-hosted generics, vtable dispatch)
  for off-by-one / aliasing / signedness risks; add unit tests for any gap found;
  document findings. Re-verify the v51 TM/TTI vectorization invariant holds across
  all new paths (the perf gate from v95 enforces it going forward).
- (3) **1.0-readiness ledger** (`docs/road-to-1.0.md`): a measured checklist —
  perf (parity on the timeable benchmarks; the perf gate green), tooling
  (LSP + formatter + doc-gen state), stdlib (slices mutable, iterators
  element-generic, I/O state), platform (Linux + macOS AOT green; WASM/Windows
  greenlit as a mega-arc), and self-hosting (the bootstrap-status file as the
  bootstrap criterion). Each item: shipped / measured-gap / mega-arc.
- (4) **Forward roadmap:** `ROADMAP-v101+` stub naming the four XL mega-arcs as
  the next arc (full bootstrap, package registry, WASM/Windows, mechanized 1.0
  proof), each with its honest multi-session size — so the next `/goal` has a
  grounded starting point.

**GATE.** `smoke_test_v100_close.sh`: (a) the expanded bootstrap fixed-point set
is green and strictly larger than v99's; (b) every new unit test from the codegen
audit passes; (c) the v95 perf gate + v90 vector gate both still green; (d) full
local smoke sweep + all unit suites green; CI both platforms. The
`docs/road-to-1.0.md` ledger is committed (a doc deliverable, not a code gate, but
its claims are each cross-checked against a passing test).

**DEFERRALS.** Everything in the four XL mega-arcs below — explicitly out of the
v91–v100 scope, handed forward with honest sizing.

---

## Out of scope (the XL mega-arcs, still deferred past v100)

1. **Full self-hosting bootstrap** — v91–v99 grow the subset to a *candidate*
   fixed-point on a substantial file set; the *complete* tree self-compiling
   (every `examples/selfhost/*.kd`, and ultimately `compiler/` itself) is the
   continuing mega-arc, paced file-by-file via `docs/bootstrap-status.md`.
2. **Register-ABI struct-by-value FFI** — the per-platform System V eightbyte
   classifier + `sret` (~2000 LOC, Survey 2 §1D); v88 ships struct FFI by pointer,
   this completes zero-copy small-struct C interop. Multi-session, platform-specific.
3. **WASM + Windows backends** — new codegen targets + ABIs; multi-session.
4. **Hosted package registry** — sandbox-blocked (no network in CI); `kard.toml`
   local-path deps work, a real registry does not.
5. **Named lifetimes + full NLL + `Pin`/self-referential safety** — a multi-month
   type-theory rewrite (region unification, lifetime inference, pinning); Survey 2
   §2C and Survey 3 §2.3/§2.9. Folded here because each is XL on its own.
6. **Full file-I/O + environment + process spawn** (Phase 189) —
   non-deterministic-testing blocked; needs a sandbox/mock layer for CI.
7. **Incremental compilation** — the compiler is monolithic, not query-based;
   a scoped single-file version is M but full cross-file incremental is a
   rearchitecture (Survey 3 §4 / v99-analysis).
8. **Mechanized spec → 1.0 proof** — a normative grammar + type/ownership/effect
   rules cross-checked against the implementation; the 1.0 capstone.

---

## Sequencing rationale (leverage × dependency)

```
v91 selfhost: CFG + mutable locals   ← the architectural fork the bootstrap needs
 └ v92 selfhost: Vec + strings         ← the data layer real compiler phases use
v93 slice mutation + variadic FFI      ← #1 practical gap; in-place algorithms
 └ v94 selfhost generics + iter-generic← generics for bootstrap + finished stdlib iterators
v95 perf: fib gap + regression gate    ← close the last perf softspot; lock it forever
 └ v96 coherence oracle + neg impls    ← correctness as impl-count scales toward bootstrap
v97 repr(packed) + bit-fields + volatile ← the low-level/binary-format systems gateway
v98 selfhost: modules+closures+traits  ← feature-complete the subset (heaviest version)
 └ v99 selfhost effects + bootstrap candidate ← the fixed-point milestone
    └ v100 bootstrap close + 1.0 ledger ← consolidate; hand forward the mega-arcs
```

Self-hosting CFG leads (v91) because `select`-only emission is the hard ceiling on
the bootstrap — nothing larger compiles without it. The data layer (v92) follows
because every compiler phase needs `Vec`. The practical-systems unlocks
(v93/v97) and the optimization/depth versions (v95/v96) interleave so the language
keeps getting *usable and correct* while the self-hosting subset deepens, and each
depends only on what shipped before it. The arc converges on v98 (the last feature
gate), v99 (the bootstrap fixed-point candidate), and v100 (consolidation + the
1.0 ledger that turns the remaining XL work into entry criteria). Every version:
real tested core, a JIT==AOT or self==host differential smoke gate, CI-green on
ubuntu + macOS, honest deferrals — the established cadence, no stubs.
