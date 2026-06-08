# kardashev

> **A small systems language with a Zig soul, built in Rust.**

kardashev is a statically-typed, ahead-of-time–compiled systems language and a
single self-contained toolchain. **Generation 2** is a ground-up reimplementation
in **Rust** (zero external crates), with the language redesigned around Zig's
philosophy:

- **No hidden control flow** — no exceptions, no operator overloading, no
  implicit destructors. The only deferred-execution construct is `defer`, and
  it is explicit.
- **No hidden allocations** — there is never an implicit global allocator.
- **`comptime`, not macros** — compile-time evaluation instead of a textual
  preprocessor.
- **Tests are first-class** — `test "name" { … }` blocks live in your source.
- **One toolchain** — a single `kard` binary is the compiler, build system,
  test runner and formatter, and the build is written in the language itself.

The compiler pipeline is `source → lex → parse → sema → emit C → cc → native
binary`: kardashev lowers to portable C11 and hands it to your system C compiler.

> *Generation 1* (v0.1.0 – v0.110.0) was a C++/LLVM compiler for a Rust-flavoured
> language with an affine borrow checker and effect system. It is preserved in
> git history and GitHub releases. Generation 2 is a deliberate reset — see
> [`ROADMAP-RUST-ZIG.md`](ROADMAP-RUST-ZIG.md).

## A taste

```rust
// hello.ks
const LIMIT: i32 = comptime (5 * 2);

fn sum_to(n: i32) i32 {
    var total: i32 = 0;
    var i: i32 = 0;
    while (i < n) : (i = i + 1) {
        total = total + i;
    }
    return total;
}

pub fn main() i32 {
    defer print(999);          // runs at scope exit, after the line below
    print(sum_to(LIMIT));      // 45
    return 0;
}

test "sum_to adds 0..n" {
    expect(sum_to(5) == 10);
}
```

```console
$ kard run hello.ks
45
999
$ kard test hello.ks
ok: sum_to adds 0..n
1/1 tests passed
```

## Install

You need a Rust toolchain and a C compiler (`cc`, `clang` or `gcc`).

```console
$ git clone https://github.com/kardashevlang/kardashev
$ cd kardashev
$ cargo build --release
$ ./target/release/kard version
```

Put `target/release/kard` on your `PATH`.

## The `kard` toolchain

```
kard build [FILE] [-o OUT] [-target TRIPLE]   # compile to a native executable
kard run   [FILE] [-- ARGS...]                 # build to a temp file and run it
kard test  [FILE]                              # build + run the test harness
kard fmt   FILE [--check | -w]                 # canonical formatting
kard init  [NAME]                              # scaffold a new project
kard version                                   # print the toolchain version
kard help                                      # usage
```

With no `FILE`, `build`/`run`/`test` read `./build.ks` — the in-language build
description:

```rust
// build.ks
build {
    name = "hello";
    root = "src/main.ks";
}
```

```console
$ kard init demo && cd demo
$ kard run
```

## Language at a glance (v0.111.0)

Functions (`pub fn name(a: i32) i32 { … }`), recursion, the fixed-width integer
types `i8…u64` plus `usize`, `bool` and `void`; `var`/`const` bindings and
comptime-evaluated top-level `const`; arithmetic/comparison/logical operators
with no overloading; `if`/`else`, `while` (including `while (c) : (cont)`),
`break`, `continue`, `return`; `defer` with correct LIFO cleanup across
fall-through, `return`, `break` and `continue`; `comptime` expression folding;
built-in `test` blocks with `expect`; and a `print` builtin for integers.

The full grammar and semantics are specified in [`SPEC.md`](SPEC.md). What is
*deliberately not here yet* (optionals, error unions, structs, enums, slices,
the allocator interface, comptime generics, …) is scheduled in
[`ROADMAP-RUST-ZIG.md`](ROADMAP-RUST-ZIG.md) — nothing is stubbed; absent
features are absent and planned.

## Repository layout

```
crates/kardc/          the compiler + `kard` toolchain (Rust, zero deps)
  src/
    lexer.rs parser.rs sema.rs const_eval.rs   # front end
    emit_c.rs backend.rs                        # C backend + cc driver
    cli.rs build_system.rs scaffold.rs fmt.rs   # toolchain
    ast.rs types.rs token.rs span.rs diag.rs    # shared contract
SPEC.md                language + toolchain specification
ROADMAP-RUST-ZIG.md    the Generation-2 plan
```

## Contributing & discipline

Each roadmap version: a real, tested implementation (never silent stubs), with
honest deferrals documented, shipped via PR with CI green on **both** Ubuntu and
macOS before merge.

## License

Dual-licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE).
