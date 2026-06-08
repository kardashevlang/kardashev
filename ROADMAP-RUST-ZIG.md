# kardashev Gen 2 — Roadmap (Rust implementation, Zig philosophy)

> **A complete change of direction.** Generation 1 (v0.1.0 – v0.110.0) was a
> C++/LLVM compiler for a Rust-flavoured language with an affine borrow checker
> and effect system, built with Bazel. It shipped 110 roadmap versions and is
> preserved in git history and GitHub releases.
>
> **Generation 2 is a ground-up reset.** The compiler is reimplemented in
> **Rust** (every implementation file is `.rs`, zero external crates), and the
> language is **redesigned around Zig's philosophy**: no hidden control flow,
> no hidden allocations, `comptime` instead of macros, explicit `defer`,
> first-class tests, and a single self-contained `kard` toolchain whose build
> system is written in the language itself.

The semantics of each version are specified in `SPEC.md`; this file is the plan.

## Generation policy

Pre-1.0, each completed roadmap version is a MINOR bump. The Gen-2 reboot opens
at **v0.111.0** (continuing the repo's tag line to avoid collisions while
clearly marking the reset). 1.0 remains reserved for a language-surface
stability commitment.

---

## ✅ v0.111.0 — Reboot: the procedural core + self-contained toolchain (THIS VERSION)

The foundation everything else builds on.

**Language**
- Functions (`pub fn name(p: T) R { … }`, Zig-style return type), recursion.
- Fixed-width integer types (`i8…u64`, `usize`), `bool`, `void`.
- `var` / `const` locals and top-level `const` (comptime-evaluated).
- Arithmetic / comparison / logical operators with no overloading.
- `if`/`else`, `while` (incl. `while (c) : (cont)`), `break`, `continue`,
  `return`.
- **`defer`** with correct LIFO flushing across fall-through, `return`,
  `break` and `continue` — a Zig signature feature.
- **`comptime`** expression folding + a const evaluator.
- **Built-in `test "name" { … }`** blocks with the `expect` builtin.
- `print` builtin for integer output (minimal runtime, explicit).

**Toolchain (single `kard` binary)**
- `kard build/run/test/fmt/init/version/help`.
- C backend: `lex → parse → sema → emit C → cc → native binary`.
- `build.ks` minimal declarative form; `kard init` scaffolding.
- Diagnostics with filename, line/column and a source caret.

**Engineering**
- Rust workspace, zero dependencies, `cargo test` (unit + compile-and-run e2e).
- CI on Ubuntu + macOS via cargo (replacing Bazel/LLVM).

---

## Planned

### v0.112.0 — Aggregates: `struct` (data) ✅
`const Name = struct { … };` declarations, struct literals (`Name{ .f = e }`),
field access (`a.b.c`), field assignment, struct-valued params/returns/locals,
nested structs. Emitted as C structs (by value). Methods / associated functions
are split out to v0.113 to keep each version complete and well-tested.

### v0.113.0 — Struct methods + associated functions ✅
Functions declared in a `struct` block; `Type.func(…)` and the `instance.method(…)`
call sugar (self-prepend), chained calls. Lowered to `kd_<Struct>_<method>(self, …)`.

### v0.114.0 — Optionals: `?T`, `null`, `orelse`, `.?` ✅
Null-safety the Zig way; lowered as a tagged `{ bool has; T val; }` value in C
with `T → ?T` coercion. `if (x) |v|` payload capture is deferred to a later
increment.

### v0.115.0 — Error unions: `!T`, `error.X`, `try`, `catch` ✅
Errors as values (implicit global error set), explicit propagation. Lowered as
`{ i32 err; T val; }` in C. Deferred: `errdefer`, `catch |e|` capture, named
error sets, `try` in nested expression positions.

### v0.116.0 — Enums & exhaustive `switch` ✅
Plain `enum` + `switch` with exhaustiveness checking (every variant or `else`;
`else` required for integers) — no hidden fall-through. Lowered to C `enum` +
`switch`. Tagged unions (`union(enum)`) + payload capture are a later item.

The original "arrays + slices + pointers + Allocator" version is split into
focused, fully-tested releases (quality over breadth):

### v0.117.0 — Fixed-size arrays `[N]T` ✅
Array types, array literals `[N]T{ … }`, indexing `a[i]` (read + write,
runtime-bounds-checked, panic on OOB), `a.len`, value semantics. Lowered to a
by-value C struct wrapper with a bounds-checked accessor.

### v0.118.0 — Pointers `*T` & slices `[]T` ✅
`&place`, `p.*`, `p.* = e`; slices as `{ ptr, len }` views with `a[i]` / `.len`
and slicing `a[lo..hi]`, bounds-checked. Raw (no lifetime checking).

### v0.119.0 — The **Allocator** interface + heap ✅
The explicit `Allocator` value (`c_allocator()`) + `alloc(a, T, n) -> []T` /
`free(a, s)` builtins; heap allocation takes an allocator parameter — no global
allocator. (Error-returning alloc, custom allocators, comptime-generic alloc:
later.)

### v0.120.0 — `comptime` generics (generic functions) ✅
Generic functions `fn f(comptime T: type, …)`, **monomorphised** (one C function
per concrete type argument), with transitive instantiation and type-parameter
forwarding. Generic structs / type-returning functions (`fn List(comptime T:
type) type`) and comptime *value* params are a later item.

### v0.121.0 — Type inference for `var`/`const` ✅
The `: T` annotation on a binding is optional — inferred from the initializer
(local `var`/`const` and top-level `const`). Inferred types are concrete (no
implicit conversions). A standard prelude/std is a later item.

### v0.122.0 — The build graph (`build.ks`) ✅
A `build.ks` describing a graph of one or more named executable targets
(`exe "name" { root = ".."; }`), with CLI target selection (`kard build/run/test
[TARGET]`; `build` with no name builds all). The full imperative `build.zig`
model (a kardashev program with a `build(*Builder)` entry point, step
dependencies and install artifacts) remains a future item.

### v0.123.0 — First-class cross-compilation
`kard build -target <triple>` producing binaries for foreign targets out of the
box (bundled/located cross C toolchains), the way `zig build` cross-compiles
trivially.

### Beyond
Re-self-hosting (the compiler in kardashev), a package registry, an LSP +
formatter parity pass, and a mechanized spec → 1.0 stability commitment.

---

## Working discipline (carried from Gen 1)

Per version: research live behaviour → real, tested implementation → honest
deferrals (never silent stubs) → PR → CI green on **both** Ubuntu and macOS →
merge → tag + GitHub release. Direct pushes to `main` are blocked; work on a
branch.
