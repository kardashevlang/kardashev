# kardashev

A systems programming language with lightweight effect-label typing, built on LLVM.

**[📖 Documentation](https://kardashevlang.github.io/kardashev/)** · [Language Reference](https://kardashevlang.github.io/kardashev/language-reference.html) · [Effects](https://kardashevlang.github.io/kardashev/effects.html) · [Stdlib](https://kardashevlang.github.io/kardashev/stdlib.html) · [Architecture](https://kardashevlang.github.io/kardashev/architecture.html) — Licensed [MIT](LICENSE-MIT) OR [Apache-2.0](LICENSE-APACHE)

## What it is

kardashev is a Rust-flavored systems language whose signature feature is **lightweight effect labels in the type system**: every function declares which side-effects it can produce (`io`, `alloc`, `panic`, `async`, ...) as part of its signature, and the compiler tracks them across call chains. Unlike Koka, there are no handlers or continuations — effects are pure type-system information, with zero runtime cost.

```rust
fn add(a: i64, b: i64) -> i64 { a + b }                       // pure

fn read_cfg(path: &str) -> Result<Config> ! { io, alloc } {   // effects in signature
    let s = std::fs::read_to_string(path)?;
    parse(s)
}

fn map<T, U, e>(xs: Vec<T>, f: fn(T) -> U ! {e}) -> Vec<U> ! { e, alloc } {
    let mut out = Vec::with_capacity(xs.len());
    for x in xs { out.push(f(x)); }
    out
}
```

The `! { ... }` syntax after the return type is the effect row. `e` is a row variable making the function effect-polymorphic — `map` is pure when `f` is pure, and propagates whatever effects `f` introduces.

## Design

- **Memory model**: ownership + borrowing (Rust-style affine, non-lexical lifetimes)
- **Type system**: HM-based with generics, ADTs, traits + `impl`, monomorphization
- **Errors**: `Result<T, E>` + `?` operator
- **Concurrency**: `async` / `await` lightweight tasks + OS threads with a checked `Send` / `share` rule
- **Memory management**: deterministic `Drop` / RAII (constant-memory loops)
- **Effect labels**: row-polymorphic effect sets, no handlers, compile-time only
- **Backend**: LLVM (AOT to a native binary + ORC JIT for the REPL)
- **Build**: Bazel + `rules_kardashev`, or a `Makefile.local` LLVM/clang shim
- **Source extension**: `.kd`

### Built-in effect labels

| Label    | Meaning                                                       |
|----------|---------------------------------------------------------------|
| `pure`   | No effects (empty row; the default if `! { ... }` is omitted) |
| `alloc`  | Heap allocation                                               |
| `io`     | File / network / stdio / general syscalls                     |
| `panic`  | Unrecoverable failure                                         |
| `async`  | Yields to the scheduler                                       |
| `unwind` | Stack unwinding for cancellation (distinct from `panic`)      |
| `share`  | Crosses a thread boundary (gates the `Send` rule)             |

Effect sets are unioned across the call graph and checked at definition sites; no runtime cost.

## A taste

```rust
// Generics + traits + borrowing + effects
trait Show { fn show(self) -> i64; }
struct Point { x: i64, y: i64 }
impl Show for Point { fn show(self) -> i64 { self.x + self.y } }

fn read(p: &Point) -> i64 { p.x + p.y }     // borrow; NLL lets you move after its last use

fn raw_read() -> i64 ! { io } { 42 }
fn main() -> i64 ! { io } { raw_read() }     // a pure-declared caller would be rejected
```

```rust
// async / await
async fn add(a: i64, b: i64) -> i64 { a + b }
async fn double(n: i64) -> i64 { add(n, n).await }
fn main() -> i64 ! { async, io } { print(double(21).await); 0 }   // 42
```

`Option` / `Result` ship via a built-in prelude; `Vec<T>`, growable `String`, and `HashMap<K, V>` are built-in containers. Multi-file programs use `mod foo;` (resolves `foo.kd` siblings); a `kard.toml` manifest with local-path dependencies drives `kard build` / `kard run`. More in the [examples](examples/) and the [docs site](https://kardashevlang.github.io/kardashev/).

## Using it

```
kardc <file.kd>              # JIT-run main() and print its result
kardc -o <out> <file.kd>     # AOT-compile to a native executable
kardc --test <file.kd>       # run every `test_*() -> i64` fn (0 = pass)
kardc -O0|-O1|-O2|-O3 ...     # optimization level (default -O2)
kardc                        # interactive REPL (JIT each expression)
kard-lsp                     # Language Server (diagnostics, hover, completion, rename, …)
kard build | kard run        # build/run a kard.toml project
```

Build with Bazel (`bazel build //... && bazel test //...`) on ubuntu or macOS, or — when Bazel isn't available — the `Makefile.local` LLVM/clang shim. Programs compile through lexer → parser → HM typechecker → NLL borrow-checker → effect inference → LLVM IR → ORC JIT (or an AOT native object linked with `clang`). `kardc -o` uses a content-addressed incremental AOT compile cache (under `${XDG_CACHE_HOME:-~/.cache}/kardashev`, keyed on the resolved source + flags); pass `--no-cache` to bypass it.

## Status

All thirty-six numbered roadmaps (**v1–v36**, Phases 0–196) have shipped, and the **Road to 1.0 and Beyond** ([ROADMAP-1.0-AND-BEYOND.md](ROADMAP-1.0-AND-BEYOND.md)) is now underway — **v37** (operator surface, turbofish, test framework) **v38** (object-safety), **v39** (raw-pointer arithmetic + write), **v40** (cancellation token), **v41** (deref-assign), **v42** (Duration), **v43** (built-in macros), **v44** (app-scale benchmarks), **v45** (language spec), **v46** (DoS fix + fuzzer + SECURITY.md), **v47** (totality via #[total]), **v48** (per-function codegen-quality contracts: `#[codegen(no_alloc, no_panic, no_io)]`), **v49** (compile-time reflection: `field_count!`/`variant_count!`/`size_of!`/`type_name!`), and **v50** (statically-verified exhaustive effect handling: no user effect escapes `main` unhandled) have shipped. **v0.51.0** is a codegen-efficiency pass: the optimizer now runs with a real `TargetTransformInfo` so auto-vectorization works (the `loop` benchmark went **2.2× C → 1.0× C parity**), array-field indexing no longer spills the whole array, HashMap probing uses bitwise-AND instead of `idiv`, and prelude scans early-exit. **v0.52.0** is a memory-safety fix: a sound escape analysis closes a dangling-reference UB — a function could return a struct/tuple/enum/array containing a reference to a local and read freed memory; the borrow checker now rejects any returned value whose references don't root in a by-reference parameter or a global (covering aggregates, `if`/`match`/`loop`, method receivers, and calls, with per-binding provenance). **v0.53.0** closes a companion soundness hole and adds a feature: a borrowed scalar `const` is **promoted** to a stable internal global, so `&C` reads correctly *and* can be safely returned (it was a frame-local temporary that dangled when returned); the escape checker now also correctly rejects returning `&<nullary-enum>` / `&<aggregate-const>` temporaries. **v0.54.0** completes the escape-analysis trilogy: storing a frame-local reference through a `&mut` out-parameter (`out.p = &local`) used to dangle after the call and is now rejected. **v0.55.0** fixes a UTF-8 correctness bug (`str_to_upper("café")` now `== "CAFÉ"`; casing extended to Latin-1) + adds char-indexed string helpers and makes `Drop` a built-in trait. **v0.56.0** makes effect handlers thread-local in AOT (two threads installing different handlers for the same effect no longer race; JIT stays process-global since TLS↛the ORC JIT). **v0.57.0** lifts the blanket `-> &T` ban (functions may return references, escape-gated). **v0.58.0** adds `if let` / `while let`. **v0.59.0** adds struct-update spread `S { x: 10, ..base }` (Copy base). **v0.60.0** is a type/effect-checker depth pass: it fixes an effect-row-variable soundness bug (an effect-polymorphic higher-order free function — e.g. the prelude `option_map(o, f)` — was mis-charged the effects of a top-level function sharing its fn-typed parameter's name, so merely defining `fn f ! {io}` broke compilation) and locks in the already-comprehensive inference engine with a 12-program regression suite. **v0.61.0** adds a **lazy iterator adaptor tower** — `iter_take`/`iter_skip`/`iter_chain`/`iter_zip`/`iter_enumerate` backed by stateful adaptor structs that pull one element at a time, so a chain like `iter_take(iter_skip(range, 20), 5)` fuses into a single O(1)-memory pass (a 50M-element skip+take runs in ~10 ms instead of materializing a ~400 MB Vec); pure-prelude, no codegen changes. **v0.62.0** adds three stdlib-runtime capabilities (thin libc wrappers, emitted only when used): a **monotonic clock** (`Instant`/`instant_now`/`instant_elapsed_millis` over `clock_gettime`), **environment variables** (`env_var -> Option<String>`, `env_var_set`), and a **seeded process-global RNG** (`rand_global`/`rng_seed_global`, lazily seeded from `KARDASHEV_SEED`, plus a `--fuzz-seed` flag — same seed reproduces the sequence, JIT == AOT). The roadmap is [ROADMAP-v54-v66.md](ROADMAP-v54-v66.md) (workflow-designed, critic-verified). Each release is green on a cleared clean build — 6 unit suites plus the full smoke / fuzz aggregate, **JIT and AOT**, on ubuntu + macOS CI. Current release: **[v0.62.0](https://github.com/kardashevlang/kardashev/releases/latest)**.

The north-star arc is **self-hosting**: v15–v17 build a complete compiler *in* kardashev — lexer → parser → type checker → code generator + VM, with `examples/selfhost/compile.kd` type-checking a whole function and then compiling + running its body. Dogfooding it found and fixed three real host-compiler bugs. v18–v19 added a differential fuzzer (random programs, `JIT == AOT == reference`) across the arithmetic, control-flow, memory-safety, and integer codegen paths.

> The per-phase history and every release's details live in **[CHANGELOG.md](CHANGELOG.md)**.

## Roadmap

| Version | Theme |
|---------|-------|
| v1  | MVP: the full pipeline (lexer → HM types → LLVM JIT/AOT), ownership + NLL borrow check, ADTs, traits/generics, `Result`/`?`, **effect labels**, `async`/`await`, modules, LSP |
| v2  | Iteration, closures + effect-carrying fn types, `dyn Trait` dispatch, a growable stdlib, `kardfmt` |
| v3  | `Drop` / RAII (constant memory), panic + unwinding, OS threads + `Mutex`, opt-levels + `--test` |
| v4  | Generic trait params + associated types + `where`, arrays/tuples, `const` evaluation, `extern "C"` FFI |
| v5  | Stdlib depth (strings, generic `HashMap`), file I/O + CLI args, self-written capstones (`calc`, `rpn`) |
| v6  | "make the heap recursive" — `Box` / recursive enums; a JSON parser written in kardashev |
| v7  | "real numbers, real abstraction" — `f64`, `#[derive]` Clone/Eq; JSON 2.0 |
| v8  | "generics, finished" — `Ord`/`Hash`/`Default` derives, generic trait objects; JSON 3.0 |
| v9  | "data in motion" — `Vec` combinators, string tools; a word-frequency capstone |
| v10 | "sized and sound at compile time" — const-generics, dimension-checked matrices, effect-subset soundness |
| v11 | "real machine integers" — the numeric tower (sized int/float, `as`, bitwise, defined wrapping) |
| v12 | "real stdlib" — parsing, `Vec`/`HashMap`/`String` methods, math helpers |
| v13 | "concurrency" — the `share` effect, typed MPSC channels, the structural `Send` rule |
| v14 | "hardening" — cross-platform CI (macOS green), a JIT-vs-AOT differential sweep |
| v15 | "self-hosting" — a compiler front-end written in kardashev |
| v16 | "self-hosting, continued" — the body grammar (parser + interpreter) |
| v17 | "a compiler in kardashev" — a self-hosted type checker + code generator; capstone `compile.kd` |
| v18 | "hardening II" — review-followup fixes + a differential fuzzer |
| v19 | "hardening III" — memory-safety + integer fuzzers, cleaner diagnostics |
| v20 | "toward a real bootstrap" — the self-hosted compiler emits **real LLVM IR** (clang → native, differential-gated vs the host), plus **structs** and **enums + match** |
| v21 | "prove it, and close the gaps" — a **benchmark suite** (kardashev is C-competitive), the `spawn`/`join` **frame-leak fix**, `HashMap`/`HashSet` **`remove`** (backward-shift deletion), and a generic **`Mutex<T>`** cell |
| v22 | "ergonomics, docs, and platform hygiene" — **`\|\|`** short-circuit logical-or, **`&<temporary>`** (ref-to-rvalue materializes a dropped slot), a docs reconciliation pass, and a tighter macOS flaky-retry scope |
| v23 | "a second backend" — **`kardc --emit-c`**, a C-source backend for the i64/bool subset, **differentially gated** against LLVM (breaking the LLVM/Linux monoculture) |
| v24 | "diagnostics & the developer surface" — rustc-style **snippet+caret diagnostics** (user-relative lines), an opt-in **lint** (`-W`), **error codes** + `--explain`, **`///` doc comments**, and parser **panic-mode recovery** |
| v25 | "the trait system, finished" — **default methods**, **supertraits**, **blanket impls**, **coherence**, **associated consts**, and the **`From`/`Into`** conversion vocabulary |

**v22 — ergonomics, docs, and platform hygiene (shipped):** two long-requested surface ergonomics plus housekeeping. **`||`** short-circuit logical-or lands (disambiguated positionally from the zero-param closure `|| body`, binding looser than `&&`); **`&<temporary>`** makes `&A(10)` / `&5` / `&Foo { .. }` work by materializing the rvalue into a statement-scoped, dropped slot (no more `let`-first workaround; drop-once verified leak-free). The language-reference docs are reconciled with reality (`%`, `&&`/`||`, `&`-of-temporary, enum-typed struct fields were all wrongly listed as unsupported), and the macOS `codegen_test` flaky-retry is raised + regex-scoped.

**v23 — a second backend (shipped):** kardashev gains a **C-source backend** (`kardc --emit-c`) — the first crack at breaking the LLVM/Linux monoculture. It walks the same typechecked AST the LLVM backend lowers and emits portable C (compiled by the system C compiler) for the **i64/bool subset** (the full operator set, `let`/`mut`, `if`/`else` as a value, `while`, recursion + mutual recursion, `const`), refusing anything outside the subset rather than miscompiling. It is **differentially gated** against LLVM: the C-backend and LLVM-AOT exit codes must agree across a spread of programs. The subset grows phase by phase (structs → enums + match → strings/Vec → Drop), with WASM and a Windows target as the follow-on reach.

**v24 — diagnostics & the developer surface (shipped):** the highest-ROI gap on the road to production. Errors became real, navigable **diagnostics** — a source snippet with a caret and the user's own line number (the ~450-line prelude offset is recovered) — plus an opt-in **lint** (`kardc -W`: unused vars + unreachable code, sound/no-false-positives), **error codes** with `kardc --explain Exxxx`, **`///` doc comments** (surfaced by the formatter and LSP hover), and parser **panic-mode recovery** (one diagnostic per error, no cascade). **v25 — the trait system, finished (shipped):** default trait methods, supertraits (`trait Ord: Eq`), blanket impls (`impl<T: B> Tr for T`), coherence/overlap rejection, associated consts (`const N: T`), and the `From`/`Into` conversion traits — backed by a new AST deep-clone utility. **v26 — patterns, types & borrow-check completeness (shipped):** match **guards** + **or-patterns**, **struct/tuple** and **slice** patterns (`[first, ..]`) + `&mut [T]`, **type aliases** (`type X = …`), the **`Fn`/`FnMut`/`FnOnce`** closure-trait hierarchy (each closure classified by how it captures; `Fn(A)->R` bounds enforce `Fn < FnMut < FnOnce`), **two-phase borrows** (`vec_push(&mut v, vec_len(&v))` now compiles while real aliasing stays rejected), and module **visibility** (`pub(crate)`/`pub(super)`/`pub(self)`) + **`use`/`pub use`** imports. **v27 — strings, text & formatting (shipped):** a real **`char`** type (Unicode scalar, distinct from the int tower) with literals/escapes/`\u{}`/casts/patterns and UTF-8 char↔string bridges; **UTF-8 correctness** (char iteration/indexing/validation, `string_chars`); **`format!`/`println!`/`print!`** (parser-desugared, no macro system yet) over **`Display`**; the **`Debug`** trait + **`{:?}`** + **`#[derive(Debug)]`**; and char classification + `str_join`/`str_replace`/`str_lines`. **v28 — const-eval & generics, finished (shipped):** aggregate **const** values (array/tuple/struct/enum + compile-time projection); **const-generics** beyond i64 (`bool`/`char`); deeper **bidirectional inference** (incl. fixing a generic-enum-struct-field bug); **GATs** (`type Out<T>;` → `Self::Out<i64>`); and **monomorphization control** — on-demand deduplicated instances, concrete-over-blanket **specialization**, and `kardc --mono-report` for code-bloat visibility. **v29 — the C backend, finished I (shipped):** the `--emit-c` C-source backend grew from the i64/bool subset to **structs**, **enums + `match`** (tagged unions + an if/else decision tree), **references/borrows** + `&<temporary>`, **`for`/`loop`-with-value** + **multi-file modules**, and a **randomized C-vs-LLVM differential oracle** — every phase gated so the emitted C's exit code matches LLVM's. **v30 — the C backend, finished II (shipped):** `--emit-c` grew to **`String`** + heap strings, scalar **`Vec`**, **`Drop`/RAII** (scope-exit frees, ASan-verified), **closures + fn-pointers** (hoisted fns + a stack capture env + a fat pointer), and **generics** via scalar monomorphization — each differentially gated vs LLVM, with the memory-safety phases also gated by an ASan/LeakSanitizer oracle. The C backend now covers the i64/bool/struct/enum/ref/control/String/Vec/Drop/closure/generic subset (HashMap/HashSet + async excepted). **v31 — concurrency, hardened (shipped):** the concurrency story goes from "structural Send + a type-erased i64 `Mutex` + i64-only threads" to a hardened, modern surface. Real **`Send`/`Sync`** marker traits — declarable, auto-derived structurally, manually grantable (`impl Send for Opaque {}`) and opt-out-able (`impl !Send for T {}`) via a marker oracle that the three live enforcement sites consult. A type-safe **`RwLock<T>`** plus move-only **RAII lock guards** (`MutexGuard`/`RwLockReadGuard`/`RwLockWriteGuard`, auto-unlock on `Drop`). Real lock-free **atomics** (`AtomicI64`/`AtomicBool`, `fetch_*`/`compare_exchange`, an `Ordering` enum) lowered to LLVM `atomicrmw`/`cmpxchg`/`fence`. Channel **`select`** (`select2`/`3`/`4` → `SelectResult { Ready, Closed }`) + **scoped threads** (a RAII `Scope` that joins every spawned thread before it ends). And **`Arc<T>`/`Weak<T>`** — atomically refcounted shared ownership that *is* `Send`+`Sync` when `T` is (the answer to "share owned data across threads" without lifetimes), proven atomic by a 4-thread clone+drop stress. (Honest deferrals: generic `thread_join<T>` — threads still return `i64`; `select` is poll-based; scoped threads join-but-don't-borrow-capture.) **v32 — async & effects, matured (shipped):** the two features that most distinguish kardashev — its async runtime and its zero-cost effect system — go from "they exist" to "they compose." **Future combinators** (`future_map`/`future_and_then`/`future_join2`/`future_select`, the last over a new `Either<A,B>`) thread the continuation's effects to the call site; a type-safe move-only **`JoinHandle<T>`** (`spawn` returns it, `join` consumes it — double-join is a compile error); **`timeout`** (race a future against a deadline → `Option<T>`) and **`task_cancel`**; **effect subtyping** (a pure fn coerces where an effectful one is expected); and the headline — user-defined **algebraic effects with handlers**: `effect E { fn op(..)->R; }` + `perform E::op(..)` (resumes with the handler's result) + `handle { body } with E { op(p) => .. }` (installs handlers for the body's dynamic extent and discharges `E`), the tail-resumptive subset that covers reader/state/logging/DI. (Also fixed a latent codegen DataLayout bug — an 8-byte heap overflow on `block_on` of a multi-payload-enum future. Honest deferrals: a multi-threaded work-stealing executor + macOS `kqueue` (Phase 174) — the executor stays single-threaded; non-tail/multi-shot effect resume; recursive `Future`-drop on cancellation.) **v33 — systems-grade: FFI, `unsafe` & overflow control (shipped):** the systems-programmer escape hatch. **Raw pointers** (`*const T` / `*mut T`, not borrow-checked, the same opaque pointer as `&T`) + **`unsafe { … }`** blocks (raw deref-read, ref↔rawptr and rawptr↔int casts; `unsafe`/`effect`/`handle` are contextual keywords so existing identifiers keep working). **FFI maturity** — `extern "C"` now takes f64/f32 (C double/float), the full int-width tower, and raw pointers (a C pointer), verified end to end against real libm/libc (`sqrt`/`pow`/`memset`/`memcpy`). And **overflow control** — a documented policy (default 2's-complement wrap) plus `checked_add/sub/mul/div -> Option<i64>` and `wrapping_add/sub/mul`. (Honest deferrals: a pluggable allocator + full `no_std` (Phase 179) and inline asm + SIMD (Phase 180) — rerouting the core allocator and platform-specific codegen are future systems work, alongside v32's multi-threaded executor; raw-pointer write / arithmetic; struct-by-value / callbacks / bindgen FFI.) **v34 — metaprogramming (shipped):** the tools to abstract over syntax and shift work to compile time. **Declarative `macro_rules!` macros** — a token-level expander with multiple rules, fragment metavariables (`$x:expr|ident|tt|…`), one repetition level `$( … )sep* / + / ?`, and recursion, expanding in expression / statement / item position (the built-in `format!`/`println!` compose with user macros). **User-defined `#[derive(...)]`** built on macros — `#[derive(Foo)]` synthesizes a `derive_Foo!{ <item> }` that the user's `macro_rules! derive_Foo` turns into an impl, composing with built-in derives (this is what made the macro matcher recursive over delimiter groups, also enabling `m!{ k => v, … }` map literals). **Operator overloading** via the prelude `Add`/`Sub`/`Mul`/`Div` traits (`a + b` desugars to the impl method; operators stay pure). **Richer comptime** — `const fn`s now evaluate imperative `let mut … ; while … { … }` loops with reassignment and early `return` at compile time (iterative factorial/fibonacci as `const` values + array lengths), bounded by the step budget. And **`#[cfg(...)]` conditional compilation** (`--cfg NAME` / `--cfg key=value`; `not`/`all`/`any`/`key="val"` predicates; disabled items dropped before type-checking; folded into the AOT cache key). (Honest deferrals: macro **hygiene** (expansions are unhygienic), nested repetitions / metavar-after-repetition; `kard.toml [features]` auto-feeding `--cfg`; `Index`/`Deref`/`Neg` + heterogeneous operators.) **v35 — stdlib depth (shipped):** broadening the standard library, almost all written in kardashev itself (prelude functions over the `Vec` primitive + existing traits) — evidence the language is now expressive enough to grow its own stdlib. **Ordered collections** — `BTreeMap<K: Ord, V>` / `BTreeSet<T: Ord>` (sorted parallel Vecs + binary search, ascending-key iteration, i64 + String keys) — and a **`VecDeque<T>`** (two-stack, O(1) amortized at both ends). **Iterator-adaptor completeness** — take / skip / chain / zip / enumerate, the reducers sum / any / all / find / min / max, and `iter_collect` which drains any `Iterator` (e.g. a Range) into a Vec. An **error-handling ecosystem** — an `Error` trait, generic Result combinators, and **`?`-with-`From`** (a `?` converts the error via `E2::from(e1)` when an `impl From<E1> for E2` exists). And a seeded deterministic **`Rng`** (LCG) + Fisher-Yates `vec_shuffle`. (Honest deferrals: buffered I/O / stdin / file-seek / full process+env (Phase 189) — runtime-heavy + non-deterministic to test; the collections are eager + Vec-backed, not balanced trees / lazy adaptor structs; `?`-with-`From` is one `From` impl per error type; wall-clock time + serde remain future work.) **v36 — tooling & compiler performance (shipped):** the developer surface + a concrete perf win. An LSP **document-outline** (`textDocument/documentSymbol` — top-level symbols with kinds + positions, user code only), **`kardc --doc`** (Markdown API docs generated from `///` comments + rendered signatures), and **bounds-check elision** (a constant array index provably in `[0, len)` emits no runtime check — verified at the IR level). (Honest deferrals: the debugger story (Phase 193, needs a gdb/lldb validation environment) and incremental compilation (Phase 195, a large query-engine rearchitecture); plus the remaining LSP features, a hosted docs site + doctests, and the rest of the codegen-perf gap.) **The full numbered roadmap v1–v36 is now released. Next — the four XL mega-arcs (real bootstrap, package registry, more backends/platforms, spec→1.0) — and an honest take on where this stands vs production languages:** see **[ROADMAP.md](ROADMAP.md)**.

## Why "kardashev"?

The [Kardashev scale](https://en.wikipedia.org/wiki/Kardashev_scale) ranks civilizations by how much energy they can harness. A systems language, in its own small way, is about controlling resources at scale — a fitting name for one that aims to be precise about effects, ownership, and computation.

## License

Licensed under either of

 * Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or
   <http://www.apache.org/licenses/LICENSE-2.0>)
 * MIT license ([LICENSE-MIT](LICENSE-MIT) or
   <http://opensource.org/licenses/MIT>)

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.
