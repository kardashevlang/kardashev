# Contributing to kardashev

kardashev is developed with a deliberately strict cadence: every change ships
as a **real, tested implementation** — never a silent stub — with honest
deferrals documented, through a PR that is green on both Ubuntu and macOS.
This file explains the setup, the test suites, and the conventions that keep
that true.

## Setup

You need a Rust toolchain (stable) and a C compiler (`cc`, `clang` or `gcc`).
There are no other dependencies — the workspace is plain Rust with **zero
external crates**, and that is a design constraint, not an accident: don't
add any.

```console
$ cargo build --release
$ ./target/release/kard version
```

## Running the tests

`cargo test --workspace` runs everything. The suites, and what each is for:

| Suite | What it covers |
|-------|----------------|
| unit tests (in `crates/kardc/src/*.rs`) | per-module pins: lexer, parser, sema diagnostics, emitted-C shapes, CLI parsing, … |
| `tests/e2e.rs` | compile-and-run end-to-end programs through the real cc pipeline |
| `tests/spec_suite.rs` | the **conformance corpus**: every `tests/spec/**/*.ks` program, compiled, run and compared against its directives, on a thread pool, under the system C compiler |
| `tests/std_suite.rs` | the in-language standard-library suites in `tests/std/*.ks`, run through the real `kard test` pipeline |
| `tests/selfhost_lexer.rs` / `selfhost_parser.rs` / `selfhost_emit.rs` | the **differential mirrors**: the self-hosted lexer/parser/emitter compared against the Rust implementation over the whole repo corpus |

Tips:

- A single suite: `cargo test --test spec_suite`. The corpus is much faster
  under `--release`.
- The unit tests of one module: `cargo test -p kardc lexer::`.
- CI (`.github/workflows/ci.yml`) additionally runs an end-to-end toolchain
  smoke test (`kard init` → `build` → `run` → `test`, multi-target builds,
  `@import`, `kard doc`, std, I/O, `--filter`, `bench`, cross-target) on
  Ubuntu **and** macOS. A change is mergeable when both are green.

## The working discipline

Each roadmap version follows the same cadence (see
[ROADMAP-RUST-ZIG.md](ROADMAP-RUST-ZIG.md)):

1. **SPEC first.** Semantics land in [SPEC.md](SPEC.md) — the single source
   of truth — annotated with the version that introduces them. If behaviour
   isn't in SPEC.md, it isn't a feature yet.
2. **Real implementation.** No stubs, no `todo!()`, no silently-ignored
   syntax. If a corner is out of scope, it is **rejected with a diagnostic**
   and listed as an honest deferral (SPEC §8 and the per-section
   "Deferred (honest)" subsections).
3. **Tests at every layer.** Unit pins for the new module behaviour, e2e
   where it crosses the cc boundary, conformance-corpus files for every new
   observable rule, and (for std) in-language `test` blocks.
4. **CHANGELOG.** Every version gets a [CHANGELOG.md](CHANGELOG.md) entry
   recording what shipped, what was found, and what was deferred.
5. **Version bump.** Pre-1.0, each completed roadmap version is a MINOR
   bump. The version lives in **three places** that must stay in sync:
   `crates/kardc/Cargo.toml`, `VERSION` in `crates/kardc/src/lib.rs`, and
   the CHANGELOG heading.
6. **PR + CI.** Direct pushes to `main` are blocked. Branch
   (`feat/…`/`docs/…`/`fix/…`), open a PR, merge only with CI green on both
   OSes, then tag and release.

## Adding a conformance test

Every `tests/spec/**/*.ks` file is a self-contained program pinning **one
observable rule** of SPEC.md, declared by comment directives:

```text
//SPEC: §11.2 a `T` value widens to `?T` at an init site
//EXIT: 0            expected exit code (default 0)
//OUT: 42            one expected stdout line; repeat in order — stdout must
                     equal exactly these lines; no OUT lines = empty stdout
//STDIN: hello       one stdin line to feed; repeat in order
//ERR: E0312         the program must FAIL to compile and every listed code
                     must appear; mutually exclusive with EXIT/OUT/STDIN
```

Conventions: place the file in the section directory it pins
(`tests/spec/s11_optionals/…`), name it after the rule, and hand-compute the
expected output — the corpus exists to catch the compiler being wrong, so
never generate expectations *with* the compiler. Files or directories whose
name starts with `_` are import fixtures for `@import` tests and are skipped
by the walk.

## Standard-library changes

The std lives in `crates/kardc/src/std.ks`, embedded into the compiler via
`include_str!`. Rules of the house:

- Written in kardashev, tested in kardashev: add `test` blocks to the
  matching suite in `tests/std/`, with hand-pinned expectations (boundaries,
  extremes, growth scripts).
- Every `pub` item carries a `///` doc comment — `kard doc
  crates/kardc/src/std.ks` renders the API reference.
- Everything allocating takes an explicit `Allocator` parameter; document
  error conventions honestly (e.g. empty-slice-on-error where `![]u8` is
  inexpressible).
- Dead-function elimination keeps std pay-as-you-go — but check that a
  hello-world's emitted C didn't grow.

## Self-hosting changes

The mirrors under `selfhost/` are **rule-for-rule replicas** of the Rust
implementation, kept honest by byte-identical differential comparison over
the whole repo corpus (`cargo test --test selfhost_lexer` /
`selfhost_parser` / `selfhost_emit`):

- Change behaviour in the Rust compiler and the mirror in the same PR, or
  the differential suites will (correctly) fail.
- The suites assert **floors** on how much of the corpus is C-identical —
  floors only ratchet up. A file outside the mirrored subset must be
  *detected* as such (a verdict-pinned SKIP), never silently skipped.
- In-language suites live in `tests/selfhost/*.ks`.

## Style

- **Rust:** match the existing style — heavily doc-commented modules, no
  external crates, no `unsafe` without a documented invariant. Don't
  mass-reformat.
- **kardashev (`.ks`):** match the surrounding examples/std style. Note that
  `kard fmt` does not yet preserve comments, so don't run `fmt -w` over
  commented repo sources.
- **Docs:** SPEC.md is normative and version-annotated; keep additions in
  that voice, and record limitations under "Deferred (honest)" subsections
  rather than omitting them.

## Commit & PR conventions

Follow the log's existing shape — e.g.
`feat: v0.180.0 — <what shipped>`, `docs: <what changed>`,
`fix: <what was wrong>`. Branches: `feat/roadmap-vNNN-…`, `docs/…`, `fix/…`.
A release PR's description mirrors its CHANGELOG entry.

## License

Dual-licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE).
Unless you explicitly state otherwise, any contribution you intentionally
submit for inclusion is dual-licensed the same way, without additional terms
(Apache-2.0 §5).
