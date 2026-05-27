# Compiler Architecture

The kardashev compiler is a single C++ binary (`kardc`) that walks a
source file through six passes before handing the result to LLVM:

```
   .kd source
       |
       v
   +------+
   | lex  |  compiler/src/lexer.cpp
   +---+--+
       |  tokens
       v
   +-------+
   | parse |  compiler/src/parser.cpp
   +---+---+
       |  ast::Program (with `mod` entries)
       v
   +--------+
   | resolve|  inline; reads sibling `.kd` files for `mod foo;`
   +---+----+
       |  merged ast::Program (prelude + modules flat-merged)
       v
   +----------+
   | typecheck|  compiler/src/typecheck.cpp
   +---+------+  HM unification + generic instantiation + effect inference
       |  TypeCheckResult (exprTypes, schemas, methodResolutions, ...)
       v
   +----------+
   | borrow   |  compiler/src/borrow_check.cpp
   +---+------+  affine ownership + NLL borrow tracking
       |  BorrowCheckResult (errors)
       v
   +--------+
   | codegen|  compiler/src/codegen.cpp
   +---+----+  LLVM IR emission + builtin runtime + O2 opt pipeline
       |  llvm::Module
       v
  +-------+       +-----+
  |  JIT  | <-- or --> | AOT |
  +-------+       +-----+
   ORC v2          TargetMachine -> .o
                   clang link    -> exe
```

## Per-pass notes

### Lexer

Single-pass, line/column tracking, stops after ~20 errors. Recognises
the operators / keywords listed in
[`language-reference.md`](language-reference.md). Two-char operators
(`==`, `<=`, `->`, `=>`, `::`, ...) are lexed before single-char to
keep the grammar unambiguous.

### Parser

Recursive descent for top-level + statements, Pratt precedence
climbing for binary expressions. The grammar is small enough to fit
in [`compiler/src/parser.cpp`](../compiler/src/parser.cpp) with no
generated tools.

### Module resolver

Lives in [`compiler/src/main.cpp`](../compiler/src/main.cpp) — sits
between the parser and typechecker. For every `mod NAME;` declaration,
reads `<srcDir>/NAME.kd`, parses it, and merges its top-level decls
into the calling program. Recursive + cycle-safe via a visited-path
set. The Phase 7.1 merge is flat (no namespacing).

### Typechecker

The biggest single pass. Built around:

- `TypeChecker::check()` orchestrating the per-pass pipeline:
  - 1a/1b: register struct / enum / trait schemas (opaque, then resolved)
  - 1c/1d/1e: register trait + impl bindings, fn schemas (with
    generic var allocation + effect rows)
  - 2: body-check every fn (params bound to schema Vars, expressions
    typed via Hindley-Milner unification)
  - 3: effect inference (union callee-effects per fn, verify ⊆
    declared)
- `pattern_match::compileDecisionTree` for the Maranget algorithm
  driving codegen of `match`.
- `instantiate(t, subst)` cloning a generic schema's signature with
  fresh Vars per call site.

The typechecker also doubles as the home of every built-in stdlib
schema (`print`, `vec_new`, `vec_push`, ...) — they're registered at
the very top of `check()` so user code can call them unqualified.

### Borrow checker

Two-pass over each fn body:

- Pass 1 assigns sequential positions to AST nodes and records, per
  binding declaration, the highest position any IdentExpr / RefExpr
  references it.
- Pass 2 walks in the same order maintaining `activeLoans_`. Each
  `let r = &x` records a loan with `expirePos = r.lastUsePos`, so
  borrows die at the borrower's last use (non-lexical lifetimes). The
  aliasing rule (shared XOR mutable, with neither permitted across a
  move) checks at every borrow / move site.

### Codegen

A single LLVM module per program. Highlights:

- `declareBuiltins()` emits the LLVM-level implementations of the
  built-in stdlib (`print`, `vec_new`, `vec_push`, `vec_get`,
  `vec_len`), plus extern decls for `printf` / `malloc` / `realloc`.
- Monomorphic functions emit eagerly; generic fns lazily as
  `Instance { fnName, typeArgs }` records on a worklist drained at the
  end of `run()`.
- Generic struct / enum instances declare distinct LLVM struct types
  per `(name, typeArgs)` tuple via `getOrDeclareStructInstance`.
- Trait method calls route through `methodResolutions_` (Concrete vs
  BoundedGeneric) to find the right impl's mangled function name.
- `?` lowers to inline tag-check + Err-propagation that rebuilds the
  enclosing fn's `Err(...)` shape — no intermediate match desugar.
- `finish()` runs LLVM's per-module O2 pipeline before returning,
  unless verifyModule already flagged an error.

### JIT vs AOT

- JIT (default `kardc <file.kd>`): ORC v2 LLJIT compiles the module
  on-the-fly and looks up `main`, calling it as a function pointer.
- AOT (`kardc -o <out> <file.kd>`): TargetMachine writes a PIC object
  file; a C-compatible `int main()` wrapper is synthesized that calls
  the user's `__kd_main()` and returns the i64 result truncated to a
  process exit code; the host's `clang` links against libc.

The two modes share the same LLVM module — the only differences are
the final consumer (JIT runner vs object writer) and the C-main
wrapper that AOT prepends.

## Build system

- Bazel + LLVM module-extension (autodetects via `llvm-config`) is the
  canonical build. `bazel build //...` reproduces the CI matrix on a
  developer machine.
- A `Makefile.local` shim builds the same tree against the system
  LLVM + clang when Bazel isn't available; smoke tests work identically.

## Test suite

```
$ make -f Makefile.local test
All lexer tests passed (19 cases)
All parser tests passed (56 cases)
All typecheck tests passed (101 cases)
All pattern_match tests passed (33 cases)
All borrow_check tests passed (31 cases)
All codegen tests passed (47 cases)
PASS: smoke test JIT fib(10)
PASS: smoke test AOT fib(10)
PASS: smoke test mod foo;
PASS: smoke test print()
PASS: smoke test Vec
```

287 unit tests + 5 smoke tests. CI runs the same suite under Bazel on
ubuntu-latest + macos-latest on every push.
