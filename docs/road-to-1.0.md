# The road to 1.0 — a measured readiness ledger

*Introduced in v0.100.0 (the close of the v91–v100 arc). This is a **ledger**, not
a declaration: kardashev is **pre-1.0** and makes no blanket "1.0-ready" claim.
Every row below is tagged **shipped** / **measured-gap** / **mega-arc** and is
**cross-checked against a named in-tree test or measured artifact** — if a claim
cannot be tied to something that runs, it is downgraded with the blocking evidence
cited. `1.0` is reserved for a language-surface stability commitment backed by the
mechanized-spec capstone (the last mega-arc).*

## The five dimensions

### 1. Performance — **shipped**
- `fib(40)` and the 200M `loop` run at **≈1.0× C** (measured best-of-5, cache
  cleared); the v51 `TargetTransformInfo` fix neutralized the old alloca-heavy
  lowering.
- **Evidence:** `tests/smoke_test_perf_regression.sh` — 3 BLOCKING deterministic
  IR-greps (`@fib` 0 allocas @-O2, the loop's `@main` 0 allocas, the loop
  auto-vectorizes: a `<N x i64/i32>` op on x86-64, the v51 lock) + an advisory
  best-of-5 wall-time check. Tight numbers in `BENCHMARKS.md`.
- **Measured gap (honest):** a residual ≈1.2× on the tightest `fib` micro-bench
  vs clang at `-O3` is documented in `BENCHMARKS.md`; closing it fully needs LTO /
  cross-module inlining (a mega-arc), not a point fix.

### 2. Tooling — **shipped**
- An LSP server (`compiler/src/lsp_main.cpp` → `kard-lsp`), a formatter
  (`compiler/src/fmt_main.cpp` → `kardfmt`), and doc extraction (`kardc --doc`,
  Phase 194).
- **Evidence:** `tests/smoke_test_lsp.sh` (+ `_rich`, `_edit`),
  `tests/smoke_test_fmt.sh`, `tests/smoke_test_phase194.sh`.
- **Measured gap:** incremental / query-based compilation is not implemented (the
  compiler is monolithic) — a scoped single-file version is M, full cross-file is
  a rearchitecture (mega-arc).

### 3. Standard library & language surface — **shipped + measured-gap + mega-arc**
- **shipped:** mutable slices `&mut [T]` (v93, `tests/smoke_test_slice_mut.sh`);
  buffered I/O + file metadata (v63, host); sized ints + `#[repr(C)]`/`repr(packed)`
  + endianness + volatile (v87/v88/v97, `tests/smoke_test_repr_packed.sh`);
  traits/generics/effects/patterns (v25–v50).
- **measured-gap:** iterator adaptors are i64-only — making them element-generic
  hits a PHI-type crash on nested adaptors (documented in `ROADMAP-v91-v100.md`,
  v94 PART 2 deferral).
- **mega-arc:** `HashMap` is host-only (the `--emit-c` backend cleanly refuses it,
  needs a keyed-hash runtime); a user-replaceable `GlobalAlloc`.

### 4. Platform — **shipped + mega-arc**
- **shipped:** Linux + macOS AOT, green on every release.
- **Evidence:** `.github/workflows/ci.yml` matrix `os: [ubuntu-latest,
  macos-latest]` — both platforms green is the merge gate for every version.
- **mega-arc:** WASM + Windows backends (new codegen targets + ABIs); register-ABI
  struct-by-value FFI (v88 ships struct FFI by pointer; zero-copy small-struct
  C interop is the System V eightbyte-classifier mega-arc).

### 5. Self-hosting — **measured-gap + mega-arc**
- **shipped (candidate):** the self-hosted emitter (`examples/selfhost/structgen.kd`)
  compiles a per-feature corpus deterministically and `self == host`.
- **Evidence:** `tests/smoke_test_bootstrap.sh` (determinism + an 11-program
  corpus) + `tests/smoke_test_selfhost_effects.sh` + the per-feature
  `smoke_test_selfhost_{traits,generics,refs,calls,loops,vec}.sh`.
- **measured-gap:** two self/host divergences are fixed in v100 (binary `-`,
  packed-store alignment) and two are documented (`for`+`continue`; effect
  enforcement / generic-struct params) — see `docs/bootstrap-status.md`.
- **mega-arc:** the **full-tree fixed point** (structgen compiling the real
  library-shaped `examples/selfhost/*.kd`, and ultimately `compiler/` itself) is
  blocked on `Box`/`Option`/`HashMap`/multi-param-generics/closures/`dyn`/modules
  — tracked file-by-file in `docs/bootstrap-status.md`.

## Summary

| Dimension | Status | Headline evidence |
|---|---|---|
| Performance | shipped | `smoke_test_perf_regression.sh` (parity locked) |
| Tooling | shipped (incremental-compile = gap) | `smoke_test_lsp.sh`, `kardfmt`, `--doc` |
| Stdlib/surface | shipped + gaps | `smoke_test_slice_mut.sh`, `smoke_test_repr_packed.sh` |
| Platform | shipped Linux/macOS; WASM/Windows = mega-arc | `.github/workflows/ci.yml` |
| Self-hosting | candidate; full bootstrap = mega-arc | `smoke_test_bootstrap.sh`, `docs/bootstrap-status.md` |

## What stands between here and 1.0 (the entry criteria)

The remaining work is the four XL mega-arcs (sized + sequenced in
`ROADMAP-v91-v100.md` → "v101 and beyond"): full self-hosting bootstrap;
register-ABI struct-by-value FFI; WASM + Windows backends; a hosted package
registry — and the **1.0 capstone**, a mechanized spec (normative grammar +
type/ownership/effect rules) cross-checked against the implementation. Each is
multi-session; none is faked or stubbed here.
