# Getting started

This guide takes you from a fresh clone to building, running, testing and
cross-compiling kardashev programs with the `kard` toolchain.

## Install

You need a Rust toolchain and a C compiler (`cc`, `clang` or `gcc` — `kard`
uses `$CC` if set, else the first of those it finds). Build the toolchain:

```console
$ git clone https://github.com/kardashevlang/kardashev
$ cd kardashev
$ cargo build --release
$ ./target/release/kard version
kardashev 0.179.0
```

Put `target/release/kard` on your `PATH`. That single binary is the
compiler, build system, test runner, formatter, doc generator and benchmark
runner. It compiles kardashev source to portable C11 and drives your system
C compiler to a native binary — there is no other runtime or dependency.

## One file

Source files end in `.ks`. Write one and run it:

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
$ kard run hello.ks            # compile to a temp binary and run it
45
999
$ kard build hello.ks          # compile to ./hello
$ kard test hello.ks           # run the file's `test` blocks
ok: sum_to adds 0..n
1/1 tests passed
all tests passed
```

`kard run` and `kard test` build fast unoptimized dev binaries (`-O0`); add
`--release` for `-O2`. `kard build` always compiles optimized. A program's
exit code is `main`'s return value, and arguments after `--` are passed to
the program (`kard run hello.ks -- a b c`).

## A project

`kard init` scaffolds a project; with no name it uses the current directory:

```console
$ kard init demo && cd demo
$ kard run
0
1
2
0
```

The layout is a `build.ks` build description plus your sources:

```
demo/
  build.ks        # the in-language build description
  README.md
  src/
    main.ks       # the entry point
```

`build.ks` has a single-target sugar and a multi-target graph form:

```rust
// single target…
build {
    name = "demo";
    root = "src/main.ks";
}

// …or a graph of named executables:
build {
    exe "app"  { root = "src/main.ks"; }
    exe "tool" { root = "src/tool.ks"; }
}
```

With no argument, `kard build` builds every target (each to its own name)
and `kard run`/`kard test` use the sole target — with several, name one:
`kard run tool`. A positional argument ending in `.ks` is always treated as
a direct file instead of a target name.

## Several files

`@import("path.ks");` at the top level pulls another file in — paths resolve
relative to the importing file, imports are deduplicated, and cycles are a
compile error:

```rust
// util.ks
pub fn triple(n: i32) i32 { return n * 3; }
```

```rust
// main.ks
@import("util.ks");

pub fn main() i32 {
    print(triple(7));   // 21
    return 0;
}
```

Imported files are flattened into one program: items are visible by bare
name across the whole program, and top-level names must be globally unique.

`@import("std");` is special — it resolves to the standard library embedded
in the compiler (containers, algorithms, text/format utilities; see the
[stdlib reference](stdlib.md)):

```rust
@import("std");

pub fn main() i32 {
    var a: Allocator = c_allocator();
    var xs: ArrayList(i64) = ArrayList(i64).init(a);
    defer xs.deinit(a);
    xs.push(a, 42);
    print(xs.get(0));   // 42
    return 0;
}
```

## Tests, filtering, benchmarks

`test "name" { … }` blocks live next to the code they test and run with
`kard test`. `expect(cond)` fails the enclosing test when `cond` is false.

```console
$ kard test app.ks                    # every test in the program
$ kard test app.ks --filter math      # only tests whose name contains "math"
$ kard bench app.ks                   # per-test wall-clock timing (always -O2)
```

The harness prints `ok:`/`FAIL:` per test and a `<passed>/<total> tests
passed` summary; the exit code is the number of failures.

## Formatting and API docs

```console
$ kard fmt file.ks             # print canonical formatting to stdout
$ kard fmt file.ks --check     # exit non-zero if not canonical (CI-friendly)
$ kard fmt file.ks -w          # rewrite in place
```

One honest caveat: the formatter works from the AST and **does not yet
preserve comments**, so prefer `--check`/stdout over `-w` on heavily
commented files.

`///` doc comments above `pub` items render to Markdown with `kard doc`:

```rust
/// Clamp `v` into the inclusive range `[lo, hi]`.
pub fn clamp(v: i32, lo: i32, hi: i32) i32 { … }
```

```console
$ kard doc lib.ks
# `lib.ks`

## `fn clamp(v: i32, lo: i32, hi: i32) i32`

Clamp `v` into the inclusive range `[lo, hi]`.
```

## Cross-compiling

```console
$ kard targets                 # known-good triples
x86_64-linux-gnu
aarch64-linux-gnu
x86_64-apple-darwin
arm64-apple-darwin
wasm32-wasi
x86_64-pc-windows-gnu
$ kard build app.ks -target aarch64-linux-gnu
```

`-target` drives clang's `--target=`. Honest limitation: linking a foreign
target needs that target's C sysroot installed; `-c` / `--emit obj` emits an
object file without linking, which works with no sysroot.

## Where next

- The [language tour](language-tour.md) — every feature with a runnable example.
- [`examples/`](../examples/) — 41 single-feature programs.
- [`SPEC.md`](../SPEC.md) — the normative specification.
- The [standard library reference](stdlib.md) and the
  [self-hosting story](selfhosting.md).
