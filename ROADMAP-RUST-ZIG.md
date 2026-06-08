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

### v0.117.0 — Arrays, slices, pointers; the **Allocator** interface
`[N]T`, `[]T`, `*T`, `[*]T`. Introduce the explicit `Allocator` interface —
every heap allocation takes an allocator parameter. No global allocator.

### v0.118.0 — `comptime` generics: `fn F(comptime T: type) type`
Real compile-time generics (containers, `ArrayList(T)`), monomorphised — Zig's
metaprogramming model.

### v0.119.0 — Type inference for `var`/`const`; the standard prelude
Inferred local types; a small std built on the allocator interface.

### v0.120.0 — The full in-language build graph (`build.ks`)
The imperative `build.zig`-style build description: steps, dependencies,
install artifacts, declared dependencies — run by `kard build`.

### v0.121.0 — First-class cross-compilation
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
