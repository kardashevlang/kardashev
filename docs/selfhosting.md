# Self-hosting: the compiler, in kardashev

Since v0.159.0, the compiler is being rewritten **in kardashev itself**,
under [`selfhost/`](../selfhost/) — the language's 12th-generation goal made
concrete, one differentially-tested stage per release.

## The idea: mirrors, not rewrites

Each component is a **rule-for-rule replica** of the Rust reference
implementation, and its output is compared against the Rust side **over the
whole repository corpus** (700+ `.ks` files: the conformance corpus, std
suites, examples, the std itself, and the selfhost sources — the mirror
processes its own source):

| Mirror | Compared on | Driver |
|--------|-------------|--------|
| `selfhost/lexer.ks` | token dumps, byte-for-byte | `crates/kardc/tests/selfhost_lexer.rs` |
| `selfhost/parser.ks` + `ast.ks` | AST dumps, byte-for-byte | `crates/kardc/tests/selfhost_parser.rs` |
| `selfhost/emit.ks` (+ `modres.ks`) | **the generated C itself**, byte-for-byte | `crates/kardc/tests/selfhost_emit.rs` |

A file the emitter's subset can't handle yet is **explicitly detected** and
verdict-pinned as a SKIP with a reason — never silently divergent. The
differential suites assert *floors* on how much of the corpus is C-identical,
and the floors only ratchet up.

This discipline cuts both ways: building the mirrors has repeatedly caught
real bugs in the **Rust** compiler (11 by v0.178 — e.g. negative comptime
value arguments emitting invalid C identifiers, found by the stage-21
mirror before any user could).

## The pieces

- **`lexer.ks`** — the full lexer: 73 token kinds, maximal-munch operators,
  span-only tokens (`off`/`len` into the source), sticky first-error
  handling, the exact i64 overflow bound.
- **`ast.ks`** — the AST as an **arena of flat nodes** linked by `i32`
  indices. kardashev has no recursive types, so the recursive Rust enum
  forest becomes one node table — the same design the std's JSON parser
  proved out.
- **`parser.ks`** — a decision-for-decision replica of the Rust parser,
  producing the arena AST.
- **`modres.ks`** — the `@import` resolver: relative paths, depth-first
  walk, dedup, cycle detection, one merged arena.
- **`emit.ks`** — the C emitter for a growing subset of the language
  (stages 3–22 so far): scalars, strings, slices, arrays, `for`, structs +
  methods, enums, `switch`, optionals, error unions, pointers, labeled
  loops, `f64` (including a correctly-rounded, shortest-round-trip float
  formatter), and — as of v0.178/v0.179 — **generic functions and generic
  structs with full monomorphisation**.
- **`lexdump.ks` / `astdump.ks` / `cdump.ks`** — the dump drivers the
  differential harness runs, and handy tools in their own right:

```console
$ kard run selfhost/lexdump.ks -- examples/hello.ks   # one line per token
$ kard run selfhost/astdump.ks -- examples/hello.ks   # one line per AST node
$ kard run selfhost/cdump.ks   -- examples/hello.ks   # the mirror's C (or SKIP + reason)
```

## Status

As of **v0.179.0** the self-hosted emitter reproduces the Rust emitter's C
**byte-for-byte on 365/384** of the conformance corpus (Program/Test modes),
including the `ArrayList`/`HashMap` examples; subset membership is itself
differentially tested on all 706 corpus files in both modes, and
[`tests/selfhost/`](../tests/selfhost/) adds in-language `test` suites for
each mirror (14 lexer + 29 parser + 73 emitter blocks).

The stage history (one release per stage) is tabulated in
[ROADMAP-RUST-ZIG.md](../ROADMAP-RUST-ZIG.md); per-stage detail is in
[CHANGELOG.md](../CHANGELOG.md) from `[0.159.0]` on.

**Remaining** for a full mirror: the sema mirror (today, sema-invalid inputs
are pinned by diagnostic code rather than re-diagnosed in-language), the
last emitter corners, and the driver — then the compiler compiling itself.

## Running the differential suites

```console
$ cargo test --release --test selfhost_lexer
$ cargo test --release --test selfhost_parser
$ cargo test --release --test selfhost_emit
```

If you change **either side** of a mirrored behaviour, change the other in
the same PR — the suites will (correctly) fail otherwise. See
[CONTRIBUTING.md](../CONTRIBUTING.md#self-hosting-changes).
