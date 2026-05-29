# Compiler Architecture

The kardashev compiler is a single C++ binary (`kardc`) that walks a
source file through a fixed sequence of passes before handing the
result to LLVM. The same front end backs three other entry points:
`kard-lsp` (the language server), `kardfmt` (the source formatter), and
the `kard` shell wrapper that drives `kard build` / `kard run` over a
`kard.toml` project.

```
   .kd source
       |
       v
   +-------+
   |  lex  |  compiler/src/lexer.cpp
   +---+---+
       |  tokens
       v
   +-------+
   | parse |  compiler/src/parser.cpp
   +---+---+
       |  ast::Program (with `mod` entries)
       v
   +---------+
   | resolve |  compiler/src/main.cpp — prepend prelude, read sibling
   +---+-----+  `.kd` for `mod foo;`, flat-merge
       |  merged ast::Program
       v
   +-----------+
   | typecheck |  compiler/src/typecheck.cpp
   +---+-------+  HM unification + generic instantiation + trait/assoc-type
       |          resolution + effect inference
       |  TypeCheckResult (exprTypes, schemas, methodResolutions, ...)
       v
   +--------+
   | borrow |  compiler/src/borrow_check.cpp
   +---+----+  affine ownership + NLL borrow tracking
       |  BorrowCheckResult (errors)
       v
   +---------+
   | codegen |  compiler/src/codegen.cpp  (~8.8 K lines, the largest file)
   +---+-----+  LLVM IR + lazily-emitted builtins + Drop glue
       |  llvm::Module
       v
       |  O2 PassBuilder pipeline (finish())
       v
  +-------+              +-----+
  |  JIT  |  <-- or -->  | AOT |
  +-------+              +-----+
   ORC v2 LLJIT          TargetMachine -> .o
                         + synthesized C `int main(argc,argv)`
                         + clang link  -> exe
```

## Source layout

| File | Role |
|------|------|
| `compiler/src/lexer.cpp` | tokenizer |
| `compiler/src/parser.cpp` | recursive-descent + Pratt parser |
| `compiler/src/types.cpp` | `Type` representation, unification helpers |
| `compiler/src/typecheck.cpp` | HM typechecker, trait/effect/assoc-type resolution, stdlib schemas |
| `compiler/src/pattern_match.cpp` | Maranget decision-tree compiler for `match` |
| `compiler/src/borrow_check.cpp` | NLL ownership / borrow checker |
| `compiler/src/codegen.cpp` | LLVM IR emission + builtin runtime + opt pipeline (largest file) |
| `compiler/src/main.cpp` | driver: prelude, module resolver, REPL, JIT, AOT, `--test` runner |
| `compiler/src/fmt_main.cpp` | `kardfmt` entry point |
| `compiler/src/lsp_main.cpp` | `kard-lsp` LSP server over stdio |
| `compiler/src/ast_print.cpp` | AST/IR dump helpers used by `--emit-llvm` and the formatter |

## Per-pass notes

### Lexer

Single-pass with line/column tracking. Two-char operators (`==`, `<=`,
`->`, `=>`, `::`, `..`, `..=`, ...) are matched before single-char ones
to keep the grammar unambiguous. Recognises the keyword and operator set
described in [`language-reference.md`](language-reference.md).

### Parser

Recursive descent for items and statements, Pratt precedence climbing
for expressions, with a unary layer (`-x`, `!x`) binding tighter than the
binary operators. The grammar is hand-written in
[`parser.cpp`](../compiler/src/parser.cpp) with no generated tooling. It
parses the full surface language: generics, traits (including
`trait Name<T>` and associated `type Item;`), `where` clauses, `impl`
blocks (trait and inherent), effect rows (`! { io, alloc, e }`),
closures, `match`, `if`/`else if` (note: `if` is an expression and the
`else` is mandatory), `while` / `loop` / `for`, arrays `[T; N]`, tuples
`(A, B)`, `const` items, and `extern "C"` declarations.

### Prelude + module resolver

Both live in [`main.cpp`](../compiler/src/main.cpp), between parsing and
typechecking.

- **Prelude** (`applyPrelude`): the root source is *prepended* with
  declarations the user has not already supplied — `Option<T>`,
  `Result<T, E>`, the generic `Iterator<T>` trait + its `impl` for the
  built-in `Range`, and the `Option`/`Result`/iterator combinators
  (`map`/`filter`/`fold`, etc.). The inclusion is by-mention (a grep over
  the source), so a program that declares its own `Option` or `Iterator`
  suppresses the corresponding prelude piece rather than colliding with
  it. Prepended combinators then lower like any other generic kardashev
  function.
- **Module resolver**: for each `mod NAME;`, reads `<srcDir>/NAME.kd`,
  parses it, and flat-merges its top-level declarations into the program
  (recursive + cycle-safe via a visited-path set; no namespacing —
  bare-name references resolve across modules, `pub` gates path-qualified
  `foo::bar` references). The `kard` wrapper's `kard.toml` local-path
  dependency resolution works by staging each dependency's library `.kd`
  as a `mod`-resolvable sibling of the entry point, so this same
  flat-merge picks it up.

### Typechecker

The core semantic pass. `TypeChecker::check()` orchestrates:

- register struct / enum / trait schemas (opaque first, then resolved),
  including a trait's type parameters (`trait Name<T>`) and associated
  type names (`type Item;`);
- register trait/`impl` bindings and function schemas, allocating fresh
  generic type variables and effect rows (`where` clauses are desugared
  to inline bounds);
- body-check every function: parameters bound to schema variables,
  expressions typed by Hindley-Milner unification, `match` arms checked
  against the scrutinee's ADT;
- effect inference: union each callee's effect row into the caller's and
  verify it is a subset of the declared row. Effect rows are
  row-polymorphic (a row variable `e` makes a function effect-polymorphic
  over its callbacks), so a pure caller invoking an `io` function is
  rejected at the definition site, at zero runtime cost.

`instantiate(t, subst)` clones a generic schema with fresh variables per
call site; `pattern_match::compileDecisionTree` builds the Maranget
decision tree that codegen lowers `match` through. The typechecker is
also the home of the built-in stdlib *schemas* (`print`, `vec_*`,
`hashmap_*`, the file-I/O and string builtins, ...), registered at the
top of `check()` so user code can call them unqualified; it records a
`usesFileIo` flag so codegen emits the file-I/O runtime only when the
program actually references it.

### Borrow checker

Two passes over each function body:

- **Pass 1** assigns sequential positions to AST nodes and records, per
  binding, the highest position any `IdentExpr` / `RefExpr` references it
  (its last use).
- **Pass 2** walks in the same order maintaining the active-loan set.
  Each `let r = &x` records a loan expiring at `r`'s last use, so borrows
  die at the borrower's last use (non-lexical lifetimes). The aliasing
  rule (shared XOR mutable, neither permitted across a move) is checked at
  every borrow and move site. A `&mut` is passed by move and is not
  auto-reborrowed when threaded through recursive calls.

### Codegen

A single LLVM module per program. Highlights:

- Built-ins are emitted *into the module*. The always-on core
  (`print`, `print_str`, string helpers, the `Vec`/`String`/`HashMap`
  scaffolding) is declared up front, while per-type collection
  operations are emitted lazily on first use via `getOrEmit*` helpers —
  e.g. `getOrEmitVecOp(op, T)` and `getOrEmitHashMapOp(op, K, V)` emit a
  monomorphic specialization per element/key/value type, with a
  `DataLayout`-sized stride. The file-I/O runtime (which calls libc
  `fopen`/`fseek`/...) is emitted only when `usesFileIo` is set, so
  I/O-free programs stay byte-identical to before.
- **Monomorphization**: monomorphic functions emit eagerly; generic
  functions are discovered along the way and queued as
  `Instance { fnName, typeArgs }` records on a worklist drained at the end
  of `run()`. Generic struct/enum instances get a distinct LLVM struct
  type per `(name, typeArgs)` tuple.
- **Trait dispatch**: static calls route through `methodResolutions_`
  (Concrete vs BoundedGeneric) to the right impl's mangled function name.
  `dyn Trait` is a `{data, vtable}` fat pointer with per-impl vtable
  globals and thunks; object-safety is enforced.
- **Closures** lower to a heap env-struct plus a uniform fat-pointer
  function value carrying an effect row; `FnMut` / capture-by-reference
  closures store a pointer to the captured slot.
- **`?`** lowers to an inline tag-check that rebuilds the enclosing
  function's `Err(...)` shape and early-returns — no intermediate match
  desugaring.
- **Drop / RAII**: every owning local gets a per-local **drop flag** (an
  `i1` alloca). `emitDropGlue` recursively frees a value
  (`Vec`/`String`/`HashMap`/`Box`, aggregates that transitively own one,
  and any type with an `impl Drop`); `getOrEmitDropThunk` wraps it as a
  uniform `void(i8*)` thunk for cleanup-stack entries. Drops run at scope
  exit in reverse declaration order, each guarded by its flag so a
  conditionally-moved value drops exactly once (no double-free / UAF).
  Moved or returned values are not dropped.
- **Panic / unwinding**: a whole-program `programContainsPanic` scan gates
  *all* panic machinery — the setjmp/longjmp cleanup stack, `panic`,
  `catch`, and array-OOB checks. The same per-local drop flag gates both
  the normal and the unwind path, so Drop glue runs during unwinding and
  every value still drops exactly once. Panic-free programs emit zero
  panic machinery.
- **`async`**: `Future<T> = {poll, frame}`; each `async fn` lowers to a
  resumable poll function over a heap frame that `switch`es on a resume
  state and spills locals live across awaits. `.await` genuinely suspends
  (returns `Pending`) and resumes; `spawn`/`join`/`block_on`/`sleep_ms`
  drive a process-global round-robin executor.
- `finish()` runs the LLVM PassBuilder pipeline on the module before
  returning, unless `verifyModule` already flagged an error (see below).

### Optimization pipeline

`finish()` builds a fresh `llvm::PassBuilder` and runs the pipeline named
by the `-O0..-O3` flag (default `-O2`). `-O1/-O2/-O3` run the matching
`buildPerModuleDefaultPipeline(level)`; `-O0` runs
`buildO0DefaultPipeline` (the default per-module builder asserts on O0),
which keeps the alloca-heavy bindings and trivial wrapper calls
un-inlined — so O0 IR is materially larger than O2. The opt level is
folded into the AOT compile-cache key so `-O0` and `-O2` objects never
collide.

### JIT vs AOT

The two modes share the same optimized `llvm::Module`; only the final
consumer differs.

- **JIT** (`kardc <file.kd>`, the REPL, and `--test`): an ORC v2 LLJIT
  compiles the module on the fly, looks up `main` (or each `test_*`), and
  calls it as a function pointer. With no real argv, the
  `__kd_argc`/`__kd_argv` globals default to 0/null.
- **AOT** (`kardc -o <out> <file.kd>`): an LLVM `TargetMachine` writes a
  PIC object file; the driver renames the user's `main` to `__kd_main`
  and synthesizes a C-compatible `int main(int argc, char** argv)` that
  stores argv into the `__kd_argc`/`__kd_argv` globals (so `args()` sees
  the CLI), calls `__kd_main()`, and returns its `i64` (or `bool`) result
  truncated to a process exit code. The host's `clang` then links the
  object against libc (invoked via `llvm::sys::ExecuteAndWait` with an
  argv vector, not a shell string).

## Build system

- **Bazel + `rules_kardashev`** is the canonical build. An LLVM module
  extension autodetects the toolchain via `llvm-config`;
  `bazel build //... && bazel test //...` reproduces the CI matrix on a
  developer machine. `rules_kardashev/defs.bzl` defines the
  `kardashev_library` / `kardashev_binary` rules that let other Bazel
  targets compose kardashev sources.
- **`Makefile.local`** is a thin clang shim (`clang++` + `llvm-config`)
  that builds the same tree against the system LLVM when Bazel isn't
  available; the smoke tests run identically through it.

CI runs on both ubuntu-latest and macos-latest on every push.

**Documented-deferred (never stubbed):** third-party dependency
resolution via the Bazel module registry (Bazel can't run in this build
environment, so it isn't verifiable here — `mod foo;` plus `kard.toml`
local-path deps are what ship) and macOS/kqueue async fd-readiness (the
epoll fd-readiness reactor is Linux-only; timers work cross-platform).

## Test suite

```
$ make -f Makefile.local test
All lexer tests passed (23 cases)
All parser tests passed (128 cases)
All typecheck tests passed (248 cases)
All pattern_match tests passed (33 cases)
All borrow_check tests passed (45 cases)
All codegen tests passed (154 cases)
PASS: smoke test JIT fib(10)
PASS: smoke test AOT fib(10)
...
```

Six C++ unit suites (lexer / parser / typecheck / pattern_match /
borrow_check / codegen) plus 40 shell smoke tests covering JIT and AOT
across the whole feature set — modules, effects, closures, `dyn`,
iterators, containers, generic traits, `where`, drop / drop-leaks, async
runtime, threads, panic, FFI, `const`, strings, hashing, file I/O, the
toolchain, and the capstones. The capstones — `examples/calc/` (a
recursive-descent arithmetic interpreter), `examples/rpn/`,
`examples/json/` (a numeric-object JSON subset), and `examples/kdlex/` (a
kardashev-subset lexer/parser) — are all written in kardashev and
compiled by `kardc`. CI runs the same suite under Bazel on
ubuntu-latest + macos-latest on every push.
