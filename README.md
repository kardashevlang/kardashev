# kardashev

> **A small systems language with a Zig soul, built in Rust.**

kardashev is a statically-typed, ahead-of-time–compiled systems language and a
single self-contained toolchain. **Generation 2** is a ground-up
reimplementation in **Rust** (zero external crates), with the language
redesigned around Zig's philosophy:

- **No hidden control flow** — no exceptions, no operator overloading, no
  implicit destructors. The only deferred-execution constructs are `defer`
  and `errdefer`, and they are explicit.
- **No hidden allocations** — there is never an implicit global allocator.
  Everything that allocates takes an explicit `Allocator` value.
- **`comptime`, not macros** — compile-time evaluation instead of a textual
  preprocessor; generics are ordinary functions over `comptime T: type`.
- **Tests are first-class** — `test "name" { … }` blocks live in your source.
- **One toolchain** — a single `kard` binary is the compiler, build system,
  test runner, formatter, doc generator and benchmark runner, and the build
  is described in the language itself.

The compiler pipeline is `source → lex → parse → sema → emit C → cc → native
binary`: kardashev lowers to portable C11 and hands it to your system C
compiler. The compiler is now **rewriting itself in kardashev** — see
[Self-hosting](#self-hosting) below.

Docs live at **<https://kardashevlang.org>** and in [`docs/`](docs/).

> *Generation 1* (v0.1.0 – v0.110.0) was a C++/LLVM compiler for a
> Rust-flavoured language with an affine borrow checker and effect system. It
> is preserved in git history and GitHub releases. Generation 2 is a
> deliberate reset — see [`ROADMAP-RUST-ZIG.md`](ROADMAP-RUST-ZIG.md).

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
all tests passed
```

Errors are values, generics are `comptime`, and containers take an allocator:

```rust
// taste.ks
@import("std");

fn checked_div(a: i64, b: i64) !i64 {
    if (b == 0) {
        return error.DivByZero;
    }
    return a / b;
}

pub fn main() i32 {
    var a: Allocator = c_allocator();
    var xs: ArrayList(i64) = ArrayList(i64).init(a);
    defer xs.deinit(a);

    xs.push(a, 30);
    xs.push(a, 3);
    print(checked_div(xs.get(0), xs.get(1)) catch -1);   // 10
    print(checked_div(xs.get(0), 0) catch -1);           // -1
    return 0;
}
```

More in [`examples/`](examples/) — 41 focused programs, one per feature — and
in the [language tour](docs/language-tour.md).

## Install

You need a Rust toolchain and a C compiler (`cc`, `clang` or `gcc`).

```console
$ git clone https://github.com/kardashevlang/kardashev
$ cd kardashev
$ cargo build --release
$ ./target/release/kard version
kardashev 0.179.0
```

Put `target/release/kard` on your `PATH`. There are no other dependencies —
the compiler is plain Rust (std only), and the programs it emits depend only
on libc.

## The `kard` toolchain

```
kard build [FILE|TARGET] [-o OUT] [-target TRIPLE] [-c | --emit obj]
                                        # compile to a native executable (or object file)
kard run   [FILE|TARGET] [--release] [-- ARGS...]
                                        # build to a temp file and run it
kard test  [FILE|TARGET] [--filter SUBSTR] [--release]
                                        # build + run the test harness
kard bench [FILE|TARGET]                # run tests with per-test wall-clock timing
kard fmt   FILE [--check | -w]          # canonical formatting
kard doc   FILE                         # Markdown API docs from `///` comments
kard init  [NAME]                       # scaffold a new project
kard targets                            # list known cross-compilation triples
kard version                            # print the toolchain version
kard help                               # usage
```

`run` and `test` build fast unoptimized dev binaries (`-O0`); `--release`
restores `-O2`. `build` and `bench` always compile optimized. `-target
TRIPLE` cross-compiles via clang's `--target=` (a foreign target needs its
sysroot installed; `-c`/`--emit obj` emits an object file without linking,
which needs no sysroot).

With no `FILE`, `build`/`run`/`test` read `./build.ks` — the in-language
build description, with a single-target sugar and a multi-target graph form:

```rust
// build.ks — single target…
build {
    name = "hello";
    root = "src/main.ks";
}

// …or a graph of named executables:
build {
    exe "app"  { root = "src/main.ks"; }
    exe "tool" { root = "src/tool.ks"; }
}
```

`kard build` with no name builds every target; `kard run app` selects one.

```console
$ kard init demo && cd demo
$ kard run
```

## Language at a glance (v0.179.0)

**Types & bindings.** Fixed-width integers `i8…u64` + `usize`, `bool`,
`void`, and `f64`; `var`/`const` bindings with type inference;
comptime-evaluated top-level `const`; integer casts with `@as(T, e)`; no
implicit conversions, no operator overloading.

**Control flow.** `if`/`else`; `while` (with a `: (continue-expression)`
clause); `for (xs) |x|` and `for (xs, 0..) |x, i|` over arrays and slices;
exhaustive `switch` with multi-label arms, integer ranges and payload
capture; labeled `break`/`continue`; `defer`/`errdefer` with correct LIFO
flushing across every exit edge.

**Aggregates.** Structs with methods, associated functions and
pointer-receiver methods (`self: *Self`, auto-ref/auto-deref); enums with
explicit values and `@intFromEnum`/`@enumFromInt`; tagged unions
`union(enum)` with `switch` payload capture; fixed arrays `[N]T` and slices
`[]T` (bounds-checked, panic on violation); pointers `*T`.

**Errors & optionals.** Optionals `?T` with `null`, `orelse`, `.?` and
`if (x) |v|` capture; error unions `!T` (including `!void`) with `error.X`,
`try`, `catch`, `catch |e|` capture and named error sets
(`const E = error{ … };`, `E!T`).

**Memory.** No hidden allocations: the explicit `Allocator` interface
(`c_allocator()`), `alloc(a, T, n) -> []T` / `free(a, s)`; runtime-safety
primitives `@panic(msg)` and `unreachable` (exit 101).

**`comptime`.** Expression folding; generic functions over `comptime T:
type` **and** comptime value parameters (array-size generics), monomorphised;
generic structs via type-returning functions (`fn Pair(comptime T: type)
type`) with methods, multiple type parameters and direct `Name(T)`
application in type position; reflection with `@sizeOf(T)`, `@typeName(T)`,
`@This()`.

**Modules & strings.** `@import("file.ks")` multi-file programs (flattened,
cycle-checked); string literals as `[]u8` slices; `@import("std")` for the
bundled standard library.

**I/O & tests.** `print` for integers, strings and floats; `@readFile` /
`@readLine` / `@writeFile` / `@appendFile` / `@argc` / `@arg`; `test` blocks
with `expect`, test filtering, per-test benchmarking, and `///` doc comments
rendered by `kard doc`.

The full grammar and semantics live in [`SPEC.md`](SPEC.md) — the normative,
per-version-annotated specification. What is *deliberately not there yet*
(value-yielding blocks, `Name(T){…}` literals, bundled cross sysroots, …) is
tracked honestly in SPEC §8 and [`ROADMAP-RUST-ZIG.md`](ROADMAP-RUST-ZIG.md)
— nothing is stubbed; absent features are absent and planned.

## The standard library

`@import("std");` resolves to a standard library **embedded in the compiler**
(~3,000 lines of kardashev, every public item `///`-documented), and
dead-function elimination keeps it pay-as-you-go — unused std code costs a
program nothing.

| Area | Highlights |
|------|------------|
| Containers | `ArrayList(T)`, `HashMap(V)`, `Deque(T)`, `BitSet` |
| Algorithms | generic `sort`, `binary_search`, `reverse`, `contains`, `fill`, `is_sorted`, … |
| Integer math | `imin`/`imax`/`iabs` (+ 64-bit variants), `clamp64`, `gcd`/`lcm`, `ipow`, `isqrt`, `div_floor`/`mod_floor` |
| Text | `StrBuilder`, `parse_i64`/`parse_u64`/`parse_f64`, `fmt_i64`/`fmt_u64`/`fmt_f64`/`fmt_u64_hex`, case utils, `str_*` helpers |
| String ops | splitters (`split_init`, `split_collect`), `trim`/`trim_start`/`trim_end`, `join`, `replace` |
| Formats | JSON (parse + minified emit), base64, hex |
| Hashes | crc32 (one-shot + streaming), fnv1a32/64, adler32, djb2 |
| Misc | `glob_match`, deterministic `Rng` (xorshift64\*), `shuffle` |

See the [standard library reference](docs/stdlib.md), or run
`kard doc crates/kardc/src/std.ks` to render the API docs with the toolchain
itself.

## Self-hosting

The compiler is being rewritten in kardashev, under [`selfhost/`](selfhost/)
— a lexer, parser and C emitter written in the language, each developed as a
**differentially-tested mirror** of the Rust implementation: byte-identical
token dumps, AST dumps and generated C over the whole repo corpus (700+
source files), with everything outside the mirrored subset explicitly
detected and pinned, never silently divergent. As of v0.179.0 the
self-hosted emitter reproduces the Rust emitter's C **byte-for-byte on
365/384** of the conformance corpus (Program/Test mode) — generic structs,
`ArrayList`/`HashMap` and all. The differential harness has also caught 11
real bugs in the Rust compiler along the way.

The story so far, stage by stage: [`docs/selfhosting.md`](docs/selfhosting.md).

## Testing & quality

- `cargo test --workspace` runs ~1,100 Rust-side unit + e2e tests.
- [`tests/spec/`](tests/spec/) is a **617-program conformance corpus**
  (plus 24 import fixtures), directive-driven (`//SPEC:`, `//EXIT:`,
  `//OUT:`, `//STDIN:`, `//ERR:`), pinning the observable behaviour of every
  SPEC section plus feature-interaction matrices — run by a parallel Rust
  harness under both gcc and clang. Writing it found 9 real compiler bugs.
- [`tests/std/`](tests/std/) exercises the standard library in-language
  (12 suites of `test` blocks run through the real `kard test` pipeline).
- [`tests/selfhost/`](tests/selfhost/) + the differential drivers keep the
  self-hosted mirrors honest.
- CI runs all of it on **Ubuntu and macOS** on every PR, plus an end-to-end
  toolchain smoke test (`init` → `build` → `run` → `test`, multi-target
  builds, modules, docs, std, I/O, filtering, benching, cross-target).

## Repository layout

```
crates/kardc/          the compiler + `kard` toolchain (Rust, zero deps)
  src/
    lexer.rs parser.rs sema.rs const_eval.rs   # front end
    emit_c.rs backend.rs                        # C backend + cc driver
    cli.rs build_system.rs scaffold.rs fmt.rs   # toolchain
    modules.rs                                  # @import flattening
    std.ks                                      # the embedded standard library
    ast.rs types.rs token.rs span.rs diag.rs    # shared contract
  tests/               e2e + conformance/std/selfhost suite drivers
selfhost/              the compiler-in-kardashev (lexer, parser, emitter + dump tools)
tests/spec/            617 directive-driven conformance programs (38 sections)
tests/std/             in-language standard-library suites
tests/selfhost/        in-language suites for the self-hosted mirrors
examples/              41 example programs, one feature each (see examples/README.md)
docs/                  guides: getting started, language tour, stdlib, self-hosting
SPEC.md                the normative language + toolchain specification
ROADMAP-RUST-ZIG.md    the Generation-2 plan and shipped history
CHANGELOG.md           per-version release notes
```

## Contributing

Each roadmap version ships as a real, tested implementation (never silent
stubs), with honest deferrals documented, via PR with CI green on **both**
Ubuntu and macOS before merge. See [`CONTRIBUTING.md`](CONTRIBUTING.md) for
the workflow, test suites and conventions.

## License

Dual-licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE).
