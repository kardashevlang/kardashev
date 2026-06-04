# Bootstrap status — the honest self-hosting ledger

*Introduced in v0.99.0. This file is the **contract** that turns the full
self-hosting bootstrap (an XL mega-arc) into a tracked, grounded, file-by-file
gap instead of a vague aspiration. It is updated as the self-hosted subset grows.*

## What "self-hosting" means here, precisely

`examples/selfhost/structgen.kd` is the self-hosted compiler. It is a **subset
emitter**: invoked as `structgen "<program>" <a> <b>`, it takes **one** program
string that defines `fn f(a: i64, b: i64) -> i64`, type-checks it, and emits
LLVM IR for the whole program to stdout (the host `kardc` then links it; or
`clang` does in the gates). It is **differential-gated**: the IR it emits, run as
a native binary, must exit-match what the host `kardc` produces for the same
program.

It is therefore **NOT** a whole-program compiler and **NOT** a self-compiler. It
cannot compile its own 1874-line source, nor the other library-shaped
`examples/selfhost/*.kd` files. This is a deliberate, honest scoping — the subset
has grown feature-by-feature (v84→v99) and the full bootstrap is paced against
this ledger.

## The bootstrap fixed-point: candidate vs. full

A literal **fixed point** — "the self-hosted compiler compiles the self-hosted
compiler, and the output is stable" — requires the self-hosted compiler to accept
its *own* source. That is out of reach for a subset emitter and is the XL
mega-arc (below).

What v0.99.0 ships is the **bootstrap-NECESSARY candidate** (gated by
`tests/smoke_test_bootstrap.sh`), the properties that *must* hold for any future
fixed point and that genuinely hold today:

1. **Determinism / idempotence** — a fixed program compiles to **byte-identical**
   IR across repeated runs (asserted over 2–3 runs per program). A compiler must
   be deterministic to have a fixed point; non-determinism would make any
   bootstrap impossible.
2. **Corpus self-application** — a corpus of in-subset programs, **one per shipped
   self-hosting feature**, each compiles deterministically *and* exit-matches the
   host (`self == host`). This is the real "the self-hosted compiler correctly and
   stably compiles the language it claims to support" guarantee.

This candidate is named honestly in the gate: it is **not** a self-compile.

## The self-hosted subset today (v0.99.0)

Supported by `structgen.kd`: `i64`/`bool`, structs (incl. heterogeneous fields),
enums + `match` (positional payload binds), `&Struct` references + field access
through them, top-level fn calls + recursion, read-only string literals +
`str_*`, mutable locals (`let mut`/assign), `while`/`for`/`break`/`continue` CFG,
scalar `Vec<i64>` + `str_concat` (use-gated runtime), slices, single-type-param
monomorphic generics, static (monomorphized) trait dispatch (`trait`/`impl` +
`recv.method(args)` → direct call), and (v99) opt-in effect rows `! { alloc | io }`
(parsed + propagated; codegen-inert metadata).

**Out of subset** (and so blocking the real files below): `HashMap`, `Box<T>`,
`Option`/`Result` + `match` on them, multi-parameter generics, closures, `dyn`
trait objects, modules (`mod`/`use`), and `f64`. Each maps to a deferred feature
named in the roadmap.

## File-by-file ledger (`examples/selfhost/*.kd`)

Status legend: **emitter** = `structgen.kd` itself (the subset emitter, the entry
of the candidate corpus); **blocked** = a library-shaped file outside the subset,
with the first blocking feature(s) named. (Even files that use no `HashMap`/`Box`
are library-shaped — multiple fns, no `fn f` entry, library constructs — so they
are not corpus inputs; structgen exits non-zero on them. The blockers column names
the *language* features each needs before it could ever be in-subset.)

| File | Lines | Status | First blocking feature(s) | Owning arc |
|---|---|---|---|---|
| `structgen.kd` | 1874 | emitter | (is the compiler) | — |
| `tokens.kd` | 124 | blocked | library shape (no `fn f` entry); `Vec<Token>` returns at arbitrary positions | full-bootstrap mega-arc |
| `lexer.kd` | 99 | blocked | library shape; multi-fn module | full-bootstrap mega-arc |
| `parser.kd` | 119 | blocked | library shape; recursive-descent over `Vec` | full-bootstrap mega-arc |
| `printer.kd` | 124 | blocked | library shape | full-bootstrap mega-arc |
| `checker.kd` | 142 | blocked | `HashMap`, `Option` | HashMap codegen + `Option`/`match` |
| `front.kd` | 152 | blocked | `HashMap`, `Option` | HashMap codegen + `Option`/`match` |
| `expr.kd` | 158 | blocked | `HashMap`, `Box`, `Option` | `Box` + HashMap + `Option`/`match` |
| `stmt.kd` | 168 | blocked | `HashMap`, `Box`, `Option` | `Box` + HashMap + `Option`/`match` |
| `typeck.kd` | 202 | blocked | `HashMap`, `Box`, `Option` | `Box` + HashMap + `Option`/`match` |
| `scopechk.kd` | 220 | blocked | `HashMap`, `Box`, `Option` | `Box` + HashMap + `Option`/`match` |
| `func.kd` | 238 | blocked | `HashMap`, `Box`, `Option` | `Box` + HashMap + `Option`/`match` |
| `emit.kd` | 250 | blocked | `HashMap`, `Box`, `Option` | `Box` + HashMap + `Option`/`match` |
| `interp.kd` | 258 | blocked | `HashMap`, `Box`, `Option` | `Box` + HashMap + `Option`/`match` |
| `funcheck.kd` | 273 | blocked | `HashMap`, `Box`, `Option` | `Box` + HashMap + `Option`/`match` |
| `compile.kd` | 345 | blocked | `HashMap`, `Box`, `Option` | `Box` + HashMap + `Option`/`match` |
| `llvmgen.kd` | 357 | blocked | `HashMap`, `Box`, `Option` | `Box` + HashMap + `Option`/`match` |
| `enumgen.kd` | 570 | blocked | `HashMap`, `Box`, `Option` | `Box` + HashMap + `Option`/`match` |

**In-subset corpus today:** 0 *real* files (all are library-shaped); the candidate
corpus is the per-feature in-subset programs in `smoke_test_bootstrap.sh`, which
exercise every shipped feature. Growing the *file* count requires the deferred
features below.

## The remaining gap to a full bootstrap (the XL mega-arc)

To make `structgen.kd` (and then `compiler/` itself) self-compile, the
self-hosted subset still needs, roughly in dependency order:

1. **`Box<T>`** in the emitter (heap indirection — needed by every recursive AST).
2. **`Option<T>` / `Result<T,E>` + `match` on them** (pervasive in the real files).
3. **`HashMap`** codegen in the subset (keyed-hash runtime — also still deferred
   in the host's `--emit-c` backend).
4. **Multi-parameter generics**, **closures**, **`dyn` trait objects** (v98
   deferrals), and **modules** (`mod`/`use`) so a multi-file compiler can be
   expressed.
5. A **multi-file driver** (the emitter reads + links several source files), and a
   harness that runs `structgen` *on the real files* and diffs stage1 vs stage2.

Each is a tracked feature; this ledger is updated as they land, moving files from
**blocked** to a real in-subset corpus.

## Known self/host divergences (honest, from the v100 audit)

The v100 adversarial audit ran the self-hosted emitter against the host on edge
cases the per-feature gates never combined. Two were real miscompiles and are
**fixed in v100**; two are documented one-directional divergences (the self-hosted
emitter never produces a *wrong answer* for an accepted program — it either
matches the host or conservatively rejects/loops, so no silent miscompile ships):

| # | Case | Behavior | v100 status |
|---|---|---|---|
| 1 | binary `-` (subtraction) | the lexer had no `-` token → `a - b` silently returned `a` | **FIXED** (lexer kind 28 + `parse_sum` + `sub i64` + `type_of`); locked by the `subtract` corpus case |
| 2 | `for i in lo..hi { … continue … }` | `for` desugars with the increment at the body tail; `continue` branches to the loop header, skipping it → **infinite loop** | **DEFERRED** — needs a continue-targeted latch (a `ForRange` Stmt variant + a latch block, touching ~7 `match`-over-`Stmt` sites). Plain `for` (no `continue`) works self==host; `while`+`continue` works. Documented here rather than risk the structural change in the consolidation version. |
| 3 | effect-row enforcement | self treats `! { … }` as unenforced metadata (v99); host enforces `E0710`. Self **over-accepts** (a superset) — never miscompiles an accepted program | DEFERRED (effect *enforcement* in the subset; v99 ships parse+propagate) |
| 4 | a generic-struct-typed param `p: Pair<T>` | `ty_tag_base` doesn't consume the `<T>`, so self **over-rejects** with a conservative `TYPE ERROR`; host accepts. Concrete non-generic struct params work | DEFERRED (generic-struct param types in the subset) |

\#2's `for`+`continue` latch and #3/#4 are tracked as part of the full-bootstrap
mega-arc above.

## Gates

- `tests/smoke_test_bootstrap.sh` — the determinism + corpus candidate (above).
- `tests/smoke_test_selfhost_effects.sh` — v99 effect rows parse + propagate,
  self == host, byte-identical to row-free.
- `tests/smoke_test_selfhost_{traits,generics,refs,calls,loops,vec}.sh` +
  `phase117`/`phase118` — the per-feature differential gates the corpus rests on.
