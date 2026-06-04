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

kardashev is **pre-1.0**. Every numbered roadmap through **v100** has shipped (v0.1.0 – v0.100.0), and the **v101–v110 "production-depth" arc** ([roadmap/ROADMAP-v101-v110.md](roadmap/ROADMAP-v101-v110.md)) is now underway — v101 **element-generic iterators**, v102 **recursive container `Debug`**, v103 **quicksort + `binary_search`**, v104 **slice utilities** (`slice_to_vec`/`slice_iter`/`chunks`/`windows`) — closing ARC A; v105 **generic `Eq`/`Hash` for `Option`/`Result`** (+ `#[derive(Eq,Hash)]` over Option fields → composite HashMap keys), v106 **codegen tail-call + bounds-elision lock** (a permanent gate proving `-O2` already lowers self-tail-recursion to a loop and elides monotone bounds checks — closing ARC B). Each release is green on a cleared clean build (6 unit suites plus the full smoke / fuzz aggregate, **JIT and AOT**) on **Linux + macOS** CI. The per-version detail lives in **[CHANGELOG.md](CHANGELOG.md)**; the current release is **[v0.106.0](https://github.com/kardashevlang/kardashev/releases/latest)**.

### Highlights

- **Ownership + borrowing** with non-lexical lifetimes and a sound escape analysis (no returning references to locals)
- **Effect system** — row-polymorphic labels, opt-in by default (an explicit `! { ... }` row is strictly checked; `--effects=strict` enforces it everywhere)
- **Traits + generics** — default methods, supertraits, blanket impls, coherence, associated types/consts/GATs, monomorphization
- **`Result<T, E>` + `?`** as the everyday error story (including `?`-with-`From`), plus `Option`
- **`async` / `await`** lightweight tasks with future combinators, `JoinHandle<T>`, `timeout`, cancellation, and user-defined algebraic effects with handlers
- **Deterministic `Drop` / RAII** — constant-memory loops, scope-exit frees
- **Concurrency** — OS threads, type-safe `Mutex<T>` / `RwLock<T>` + RAII guards, lock-free atomics, channels + `select`, scoped threads, `Arc<T>` / `Weak<T>`, and checked `Send` / `Sync`
- **LLVM backend** — AOT native binaries + an ORC JIT REPL at ~C parity (auto-vectorized), plus a portable **C-source backend** (`kardc --emit-c`)
- **Systems FFI** — the full sized-integer/float tower, `extern "C"`, raw pointers + `unsafe`, `#[repr(C)]` / `#[repr(packed)]` layout, stack arrays `[T; N]`, slices, and endianness/volatile intrinsics
- **Metaprogramming** — `macro_rules!`, user `#[derive(...)]`, operator overloading, `const fn`, and `#[cfg(...)]`
- **A self-hosted compiler subset** — a compiler written *in* kardashev that emits real LLVM IR (loops, generics, static trait dispatch, effect rows)
- **Tooling** — an LSP (diagnostics, hover, completion, rename, document outline), a formatter, and Markdown doc generation (`kardc --doc`)

## Roadmap

The numbered roadmap v1–v36 covered the core language, in thematic order:

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
| v21 | "prove it, and close the gaps" — a **benchmark suite** (kardashev is C-competitive), the `spawn`/`join` **frame-leak fix**, `HashMap`/`HashSet` **`remove`**, and a generic **`Mutex<T>`** cell |
| v22 | "ergonomics, docs, platform hygiene" — **`\|\|`** short-circuit or, **`&<temporary>`**, a docs reconciliation pass, a tighter macOS flaky-retry scope |
| v23 | "a second backend" — **`kardc --emit-c`**, a C-source backend for the i64/bool subset, **differentially gated** against LLVM |
| v24 | "diagnostics & the developer surface" — rustc-style **snippet+caret diagnostics**, an opt-in **lint** (`-W`), **error codes** + `--explain`, **`///` doc comments**, parser **panic-mode recovery** |
| v25 | "the trait system, finished" — **default methods**, **supertraits**, **blanket impls**, **coherence**, **associated consts**, the **`From`/`Into`** vocabulary |
| v26 | "patterns, types & borrow-check completeness" — match **guards** + or-patterns, struct/tuple/slice patterns, **type aliases**, the **`Fn`/`FnMut`/`FnOnce`** hierarchy, **two-phase borrows**, visibility + `use` |
| v27 | "strings, text & formatting" — a real **`char`** type, **UTF-8 correctness**, **`format!`/`println!`** + **`Display`**, **`Debug`** + **`{:?}`** + `#[derive(Debug)]` |
| v28 | "const-eval & generics, finished" — aggregate **`const`** values, **const-generics** (`bool`/`char`), deeper inference, **GATs**, monomorphization control |
| v29 | "the C backend, finished I" — `--emit-c` grows to structs, enums + `match`, references, `for`/`loop`-with-value + modules, a randomized C-vs-LLVM oracle |
| v30 | "the C backend, finished II" — `--emit-c` grows to **`String`**, scalar **`Vec`**, **`Drop`/RAII** (ASan-verified), **closures + fn-pointers**, generics |
| v31 | "concurrency, hardened" — real **`Send`/`Sync`** markers, **`RwLock<T>`** + RAII guards, **atomics**, channel **`select`** + scoped threads, **`Arc<T>`/`Weak<T>`** |
| v32 | "async & effects, matured" — future combinators, **`JoinHandle<T>`**, **`timeout`** + cancel, effect subtyping, user-defined **algebraic effects with handlers** |
| v33 | "systems-grade: FFI, `unsafe` & overflow control" — **raw pointers** + **`unsafe`**, FFI maturity (f64/f32, int tower, pointers), `checked_`/`wrapping_` overflow ops |
| v34 | "metaprogramming" — declarative **`macro_rules!`**, user-defined **`#[derive(...)]`**, **operator overloading**, richer **`const fn`**, **`#[cfg(...)]`** |
| v35 | "stdlib depth" — **`BTreeMap`/`BTreeSet`/`VecDeque`**, iterator-adaptor completeness, an error-handling ecosystem (`?`-with-`From`), a seeded **`Rng`** |
| v36 | "tooling & compiler performance" — LSP **document outline**, **`kardc --doc`** (Markdown API docs), **bounds-check elision** |

**v37–v100** continued in focused arcs (the road to 1.0, then **v54–66 / v67–80 / v81–90 / v91–100**) — sized integers, `repr(C)`/`repr(packed)` FFI, stack arrays + slices, opt-in effects, deeper self-hosting, and adversarial codegen audits. See **[CHANGELOG.md](CHANGELOG.md)** for the full per-version log and the archived arc roadmaps under [`roadmap/`](roadmap/).

## Documentation

- **[Language reference / docs site](https://kardashevlang.github.io/kardashev/)** — the mdBook (also in [`docs/`](docs/))
- **[CHANGELOG.md](CHANGELOG.md)** — the per-version history (Phases 0–196 and every release's details)
- **[ROADMAP.md](ROADMAP.md)** — the forward roadmap
- **[ROADMAP-1.0-AND-BEYOND.md](ROADMAP-1.0-AND-BEYOND.md)** — the road to 1.0 and beyond
- **[docs/road-to-1.0.md](docs/road-to-1.0.md)** — the measured 1.0-readiness ledger
- **[`roadmap/`](roadmap/)** — the archived completed-arc roadmaps ([v54–66](roadmap/ROADMAP-v54-v66.md), [v67–80](roadmap/ROADMAP-v67-v80.md), [v81–90](roadmap/ROADMAP-v81-v90.md), [v91–100](roadmap/ROADMAP-v91-v100.md))

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
