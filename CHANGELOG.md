# Changelog

All notable changes to kardashev are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## Versioning

kardashev uses [Semantic Versioning](https://semver.org). It is **pre-1.0**, so
each completed **roadmap** is a `MINOR` bump (Roadmap v9 → `0.9.0`, v10 →
`0.10.0`) and bug-fix releases bump `PATCH`. Per SemVer's 0.x rule anything may
change between minors until 1.0. `1.0.0` is reserved for a language-surface
**stability commitment**; after it the language evolves via opt-in **editions**
(the Rust model) rather than `MAJOR` bumps. From `0.111.0` on, the version lives
in `Cargo.toml` and `crates/kardc/src/lib.rs` (`VERSION`, reported by
`kard version`), and here.

`0.9.0` is the first tagged release; the entries below `0.9.0` document the
pre-tag roadmap history (Phases 0–56), each of which shipped fully green (6 unit
suites + the smoke aggregate, JIT **and** AOT).

## [0.144.0] — Floating point `f64`

### Added
- **`f64`** — the first non-integer scalar (C `double`): literals (`3.14`),
  arithmetic `+ - * /` and comparison, `print`, and `[]f64`/`[N]f64` arrays &
  slices. `Type::F64`, `Expr::Float`, `TokenKind::Float`; the lexer reads
  `digits.digits` (a `.` not before a digit stays `..`/field access).
- **`@as`** extends to numeric casts — `@as(f64, n)` (int→float) and `@as(i32,
  x)` (float→int, truncating).
- 900 unit + 42 e2e tests; `examples/floats.ks`.

### Limitations (honest, v0.144)
No implicit int↔float mixing (cast with `@as`); no `%` on `f64`; float `const`s
are deferred (floats are runtime-only — `var x: f64 = 3.14;` works, `const`
doesn't).

## [0.143.0] — Enum explicit values + conversions

### Added
- **Explicit enum values**: `const Color = enum { Red = 1, Green, Blue = 10 };`
  — a variant with `= N` takes value `N`; a value-less variant auto-increments
  from the previous (first is 0). `EnumVariant{ name, value }`;
  `EnumInfo.values`; the C `enum` carries the values, so literals / `switch`
  stay value-based.
- **`@intFromEnum(e)`** → `i64` (the variant's value) and **`@enumFromInt(E,
  n)`** → `E` — integer round-trips for stable enum representations.
- 870 unit + 41 e2e tests; `examples/enum_values.ks`.

## [0.142.0] — `catch |e|` capture

### Added
- **`expr catch |e| default`** — the capturing error handler (deferred from
  v0.125): if `expr` (an `!T`) is ok it yields the payload, else it binds the
  error **code** (`i32`) to `e` and evaluates `default` (a `T`) **only on the
  error path**, so the handler can react to which error occurred.
- `Expr::Catch.capture`; lowered by hoisting like `try` (a temp + an `if` on
  `.err`). The non-capturing `expr catch default` (§12) is unchanged.
- 846 unit + 40 e2e tests; `examples/catch_capture.ks`.

## [0.141.0] — `@panic` + `unreachable`

First version of **Arc 4** (toward a practical 1.0: safety, floats, std).

### Added
- **`@panic(msg)`** — write the `[]u8` `msg` to stderr and `exit(101)`.
  **`unreachable`** — trap (exit 101) if reached. Both **diverge** and adopt the
  expected type, so they stand in any value position (e.g. a total `switch`'s
  `else => { unreachable; }`).
- `Expr::Unreachable` + the `unreachable` keyword; `@panic` via `Expr::Builtin`.
  `_Noreturn` C helpers `kd_panic`/`kd_unreachable`. 833 unit + 39 e2e tests;
  `examples/panic.ks`.

## [0.140.0] — Doc comments + `kard doc`

The capstone of **Arc 3** (v0.131–v0.140 complete).

### Added
- **`kard doc FILE`** — renders a file's `pub` items and their `///` doc
  comments as Markdown: signatures (`fn add(a: i32, b: i32) i32`, `struct Vec2`,
  `error set LookupErr`, `const X: T`) are reconstructed from the AST, and the
  contiguous `///` lines directly above each item are associated by source
  position. Non-`pub` items are omitted. CI smoke-tests it.
- `///` is a doc-comment convention (an ordinary ignored comment to the
  compiler); no AST/parser change. 807 unit + 38 e2e tests;
  `examples/documented.ks`.

## [0.139.0] — Named error sets

### Added
- **Named error sets**: `const FileErr = error{ NotFound, Denied };`
  (`Item::ErrorSet`) and error unions typed over them — `FileErr!T` — alongside
  the implicit global `!T`. `TypeExpr.error_set`.
- **Membership checking**: a `return error.X;` (or `var x: Set!T = error.X;`)
  must name a member of the set (`E0330`); an undeclared set or a duplicate
  member is `E0331`. A global `!T` accepts any error name (unchanged).
- At runtime `Set!T` lowers identically to `!T` (the set is a compile-time
  constraint), so `try`/`catch` are unchanged. 805 unit + 38 e2e tests;
  `examples/error_sets.ks`.

## [0.138.0] — `HashMap(V)` std container

### Added
- **`HashMap(V)`** — a generic open-addressing hash map on the `Allocator`
  (`put`/`get`/`has`/`remove`/`len`, with grow-and-rehash at 0.75 load and
  tombstones for `remove`), written entirely in the language —
  `examples/hashmap.ks`. The second allocator-based std container.

### Fixed (generic-struct methods, lifting two v0.130 limitations)
- A generic-struct method body may now **reference top-level `const`s and free
  functions**: method bodies are checked in a new post-Pass-2 phase (their
  signatures are still registered earlier so call sites resolve).
- A generic-struct method may **call `Self.assoc(…)`** (an associated
  constructor like `Self.with_cap`): the backend now resolves a `Self` receiver
  through the active substitution.
- 779 unit + 37 e2e tests.

## [0.137.0] — Integer casts `@as(T, e)`

### Added
- **`@as(T, e)`** — casts an integer value `e` to integer type `T` (extends the
  §32 `@`-builtin machinery), lowering to a C cast `((T)(e))`. Bridges the strict
  integer types (e.g. an `i32` key → a `usize` index), used inline anywhere
  (`arr[@as(usize, k)]`). Diagnostics `E0321` (non-integer target/value).
- This unblocks mixed-integer code and a real `HashMap` (verified end-to-end).
- 779 unit + 36 e2e tests; `examples/casts.ks`.

## [0.136.0] — comptime reflection builtins

### Added
- **`@sizeOf(T)`** → `usize` (lowers to C `sizeof`) and **`@typeName(T)`** →
  `[]u8` (the type's source name). Both are substitution-aware, so they work on
  a generic type parameter (`@sizeOf(T)` inside a generic body). `Expr::Builtin`.
- **`@This()`** — the enclosing struct type, parsed in type position and
  desugared to `Self`; `Self`/`@This()` are now bound in **plain** struct methods
  too (not only generic structs), e.g. `fn m(self: *@This())` in a `const Point
  = struct { … }`.
- Diagnostics `E0320` (unknown / mis-arity `@`-builtin). 779 unit + 35 e2e tests;
  `examples/comptime_builtins.ks`.

## [0.135.0] — Multiple type parameters

### Added
- **Type-constructors with more than one type parameter**: `fn Pair(comptime A:
  type, comptime B: type) type { return struct { … }; }`, instantiated via a
  type alias (`const E = Pair(i32, i64);`), monomorphised on the argument tuple
  (order matters — `Pair(i32,i64)` ≠ `Pair(i64,i32)`). Fields and methods
  substitute all type parameters + `Self`. (Single-parameter constructors are
  unchanged; generic *functions* already accepted N comptime params.)
- `StructInstance.args: Vec<Type>`; arg-count mismatch is `E0311`, a non-type
  comptime parameter in a type-constructor is `E0310`.
- 752 unit + 34 e2e tests; `examples/multi_typeparam.ks`.

## [0.134.0] — Pointer-receiver methods (true mutation)

### Added
- **Pointer-receiver methods**: `fn m(self: *Point, …)` / `fn m(self: *Self, …)`
  mutate the receiver in place. The call site **auto-refs** (`c.inc()` passes
  `&c`; the receiver must be an addressable lvalue) and field access
  **auto-derefs** (`self.field`). A value receiver (`self: Point`) still copies.
- Field read/assign (and compound assign) on **any** `*Struct` value writes
  through the pointer (`p.field = e`). Enables a mutating `ArrayList`/`Stack`
  `push` on a generic struct.
- No new syntax/contract — `*Self`/`*Point` already parse; pure sema + emit.
- 741 unit + 33 e2e tests; `examples/pointer_receiver.ks`.

## [0.133.0] — `for` loops over arrays & slices

### Added
- **`for (xs) |x| { … }`** iterates the elements of an array (`[N]T`) or slice
  (`[]T`); `x` binds each element by value. **`for (xs, 0..) |x, i| { … }`** also
  binds a 0-based `usize` index. Lowered to an indexed `while` (a loop-body
  scope), so `break`/`continue` behave — and `continue` still advances the index.
- `for`/`Kw::For`; `Stmt::For{ iter, elem, index, body }`. The iterable is
  evaluated once. Capture-count must match the `, 0..` form.
- 719 unit + 31 e2e tests; `examples/for_loops.ks`.

## [0.132.0] — Bitwise & shift operators

### Added
- **`& | ^ << >> ~`** on integers, with C-like precedence
  (`| < ^ < & < == < relational < shift < + < *`). Infix `&`/`|` are bitwise;
  prefix `&` stays address-of and `|x|` stays a capture (disambiguated by
  position). All fold in `const` expressions (`const MASK = (1 << 8) - 1;`).
- Lexer `^`/`~`/`<<`/`>>`; `BinOp::{BitAnd,BitOr,BitXor,Shl,Shr}` +
  `UnOp::BitNot`. Integer operands required.
- 696 unit + 30 e2e tests; `examples/bitwise.ks`.

### Deferred (honest)
Bitwise compound assignments (`|= &= ^= <<= >>=`) and hex/binary integer
literals are later work.

## [0.131.0] — Compound assignment operators

First version of **Arc 3** (toward 1.0: ergonomics, mutation, richer generics).

### Added
- **`+= -= *= /= %=`** on any assignable place (`x`, `s.f`, `a[i]`): `place =
  place op rhs`, with the place evaluated **once** (an index compound reads `i`
  a single time), valid in a `while` continue-clause too.
- Lexer `+=`/`-=`/`*=`/`/=`/`%=`; `Stmt::Assign`/`Stmt::FieldAssign` carry
  `op: Option<BinOp>`. Integer operands required (the binop type rule).
- 667 unit + 29 e2e tests; `examples/compound_assign.ks`.

## [0.130.0] — Generic-struct methods + `ArrayList(T)`

The final piece of the numbered roadmap (**v0.112–v0.130 complete**).

### Added
- **Generic-struct methods**: a type-constructor's `struct { … }` may declare
  methods that use `Self` (the instantiated struct) and the type parameter
  (`Expr::StructType.methods`). Each method is monomorphised per instantiation
  and registered/emitted like a struct method (`kd_<Struct>_<method>`); the
  backend emits them by iterating `StructTable::struct_instances`.
- **`ArrayList(T)`** — a generic growable list on the `Allocator`
  (`init`/`append`/`get`/`len`/`deinit`, grows by alloc+copy+free), shipped as
  `examples/arraylist.ks` — the first allocator-based std container.
- `alloc(a, T, n)` now resolves `T` through the active substitution (works
  inside a generic body); associated calls resolve type-alias receivers
  (`IntList.init(a)`). 636 unit + 28 e2e tests.

### Limitations (honest, v0.130)
Value-semantics `self` (no true pointer receivers); one type parameter; `Self`
only (no `@This()`).

## [0.129.0] — Generic structs (type-returning functions)

### Added
- **Type-constructors**: `fn Name(comptime T: type) type { return struct { …
  }; }` — a function returning a `type`, monomorphised per type argument
  (`Expr::StructType` is the anonymous struct-type value).
- **Type aliases**: `const IP = Pair(i32);` instantiates a type-constructor and
  binds the result (a monomorphised struct, memoised) as a type usable in
  signatures, struct literals, and field access. Aliases are shared to the
  backend via the `StructTable`.
- Diagnostics `E0310`/`E0311`. 617 unit + 26 e2e tests;
  `examples/generic_structs.ks`.

### Limitations (honest, v0.129)
One type parameter, fields-only struct (no methods inside a generic struct), and
no direct `Name(T)` / `Name(T){…}` in type/literal position (use a `const`
alias) — all later work.

## [0.128.0] — `comptime` value parameters

### Added
- **`comptime n: usize`** value parameters — a function is monomorphised per
  distinct value, extending the v0.120 generics machinery. `n` may appear as an
  **array-size** (`[n]T`) and as a value in the body.
- Array sizes are now `ArraySize::{Lit(n), Param(name)}`; generic instantiations
  key on `ComptimeArg::{Type, Value}` (a value arg mangles to its digits, e.g.
  `kd_dot__3`). A non-constant value argument is `E0251`/`E0253`.
- 590 unit + 25 e2e tests; `examples/comptime_vals.ks`.

## [0.127.0] — Strings (`[]u8` literals)

### Added
- **String literals** are now **values** of type `[]u8` (a slice over static
  bytes) — `Expr::StrLit`. Reuses the slice machinery, so `.len`, indexing
  `s[i]` (a `u8`) and sub-slicing `s[lo..hi]` all work, no new type.
- **`print`** now accepts a string (`[]u8`) as well as an integer — it writes
  the bytes followed by a newline (`fwrite` + `fputc`).
- 571 unit + 24 e2e tests; `examples/strings.ks`.

## [0.126.0] — Multi-file modules (`@import`)

### Added
- **`@import("path.ks");`** — a top-level import. A new `modules::resolve`
  flattener lexes/parses the root and every transitively-imported file
  (relative paths, file dedup, cycle detection) and concatenates them into one
  program, fed to the existing `sema`/`emit_c`. `compile_program(path)` drives
  it; `kard build`/`run`/`test` now compile from a path.
- Lexer `@`/`At`; `Item::Import`. Diagnostics `E0290`–`E0294` (residual import,
  not-found, cycle, duplicate name, imported-file error). CI smoke-tests a
  two-file program.
- 545 unit + 23 e2e tests.

### Limitations (honest, v0.126)
`#include`-style flatten: bare-name access (no `m.member`), `pub` not enforced
across modules, no package/std resolver — all deferred to a later namespacing
pass.

## [0.125.0] — Payload captures: `if (opt) |v|` + `errdefer`

### Added
- **Optional `if` capture**: `if (opt) |v| { … } else { … }` evaluates the
  optional once, binds the unwrapped value `v` on the present branch, and runs
  `else` on null.
- **`errdefer`**: registers cleanup that runs (LIFO, alongside `defer`) only on
  **error-return** edges — a `try` propagation or a `return error.X` — and not
  on success or normal exit. The defer machinery now tags each entry and flushes
  errdefers only on error paths.
- `Stmt::If.capture`, `Stmt::ErrDefer`; lexer `errdefer`. Diagnostics `E0280`.
- 535 unit + 23 e2e tests; `examples/captures.ks`.

### Deferred
- `catch |e| { … }` (the capturing error handler); the non-capturing
  `expr catch default` remains.

## [0.124.0] — Tagged unions `union(enum)` + `switch` capture

First version of **Arc 2** (completing the language surface).

### Added
- **Tagged unions**: `pub? const Name = union(enum) { v: T, … };`. Construction
  reuses struct-literal syntax (`Name{ .v = e }`, exactly one variant).
- **`switch` payload capture**: `.v => |x| { … }` binds the active variant's
  payload (`x`) in the arm; exhaustiveness checked as for enums.
- Type system: `Type::Union(id)` + a union table; lexer `union` keyword and `|`.
  Lowered to a tagged C struct `{ int32_t tag; union { … } data; }` emitted in
  dependency order. Diagnostics `E0270`–`E0272`.
- 519 unit + 22 e2e tests; `examples/unions.ks`.

## [0.123.0] — Cross-compilation

Completes the numbered Gen-2 roadmap (v0.112 – v0.123).

### Added
- **`kard build FILE -target <TRIPLE>`** cross-compiles via clang's
  `--target=`; **`-c` / `--emit obj`** emits an object file (skipping the link
  step); **`kard targets`** lists common triples.
- `backend::BuildOptions { target, object_only }`, threaded into `cc_build`.

### Honest limitation
Because the runtime uses libc (`<stdio.h>`/`<stdlib.h>`/`<stdint.h>`), foreign
targets need that target's C headers/sysroot installed — even for `-c`. The
**host triple builds and runs out of the box** (and multi-arch SDKs like macOS
x86_64 ↔ arm64 work); other triples require the target toolchain. **Bundling
cross sysroots** (Zig's "cross-compile anything out of the box") is the headline
remaining work — the `-target`/`-c`/`targets` *mechanism* is complete, the
bundled sysroots are not yet. CI smoke-tests `kard targets` and a `-target`
host-triple build end to end.

- 485 unit + 21 e2e tests.

## [0.122.0] — The build graph (`build.ks`)

### Added
- `build.ks` now describes a **build graph of named executable targets**:
  `build { exe "app" { root = "src/app.ks"; } exe "tool" { root = ".."; } }`.
  The legacy single-target `build { name = ..; root = ..; }` sugar still works.
- CLI **target selection**: `kard build/run/test [TARGET]` selects a target by
  name; with a single target the name is optional; `build` with no name builds
  **all** targets. A positional ending in `.ks` is still a direct file.
- `BuildSpec { targets: Vec<Target> }` + `BuildSpec::select`. CI smoke-tests a
  two-target project end to end.
- 473 unit + 21 e2e tests.

### Deferred
- The full imperative `build.zig` model (a kardashev program with a
  `build(*Builder)` entry point, step dependencies, install artifacts).

## [0.121.0] — Type inference for `var`/`const`

### Added
- The `: T` annotation on a binding is now **optional**: `var x = expr;` /
  `const x = expr;` (and top-level `const X = expr;`) infer the type from the
  initializer. `Stmt::Let.ty` / `ConstDecl.ty` became `Option<TypeExpr>`.
- Inferred types are **concrete** (no implicit conversions): `var i = 0;` is
  `i64`. A value with no context-free type (bare `null`, `error.X`, `.Variant`)
  requires an annotation → `E0260`.
- 458 unit + 21 e2e tests; `examples/inference.ks`.

## [0.120.0] — `comptime` generics (generic functions)

Zig's metaprogramming model: compile-time type parameters + monomorphisation.

### Added
- **Generic functions** `fn f(comptime T: type, …)`. A function with a
  `comptime IDENT: type` parameter is generic; its runtime parameters, return
  type and body may use the type parameter as a type (including `?T`, `[]T`,
  `[N]T`, `*T`, `!T`).
- **Monomorphisation**: each distinct type argument emits its own specialised C
  function (`kd_max__int32_t`, …) — no runtime dispatch. Supports **transitive
  instantiation** and **type-parameter forwarding** (`max(T, …)` inside another
  generic). Type arguments are passed positionally: `max(i32, a, b)`.
- `Param.is_comptime`; a `StructTable` instantiation registry; substitution
  threaded through sema and the backend. Diagnostics `E0250`–`E0252`.
- 423 unit + 20 e2e tests; `examples/generics.ks`.

### Deferred
- Generic structs / type-returning functions, comptime *value* parameters,
  comptime control flow, and `anytype`.

## [0.119.0] — The Allocator interface + heap

Zig's law — no global allocator; heap memory is requested from an `Allocator`
that is passed explicitly.

### Added
- **`Allocator`** type and three builtins: **`c_allocator()`** (the malloc/free
  allocator), **`alloc(a, T, n) -> []T`** (heap-allocate a slice of `n`
  elements; the type argument is an identifier; panics on OOM), and
  **`free(a, s)`**.
- Lowered with no new AST: each slice type gains a `_alloc` heap helper;
  `Allocator` is a small C struct. Diagnostics `E0241`/`E0242` (+ `E0101` guards
  the builtin names).
- 400 unit + 19 e2e tests; `examples/heap.ks`.

### Deferred
- Error-returning `alloc` (`![]T`), custom allocators / a vtable interface,
  `realloc`, aligned allocation, and comptime-generic `alloc`.

## [0.118.0] — Pointers `*T` & slices `[]T`

### Added
- **Pointers `*T`**: `&place` (address-of an lvalue), `p.*` (dereference), and
  `p.* = e` (assign through a pointer). Raw — no lifetime checking.
- **Slices `[]T`**: `{ ptr, len }` views created by slicing an array
  `a[lo..hi]` (aliasing the backing storage); `s[i]` (bounds-checked), `s[i] =
  e`, and `s.len`.
- Type system: `Type::Ptr(id)` / `Type::Slice(id)` + tables; lexer `&` and `..`.
  Pointers lower to C `T*`; slices to a `{ T* ptr; uintptr_t len; }` struct with
  a bounds-checked accessor (emitted in dependency order); slice/array bounds
  violations panic with exit 101. Diagnostics `E0230`–`E0232`.
- 380 unit + 18 e2e tests; `examples/slices.ks`.

## [0.117.0] — Fixed-size arrays `[N]T`

### Added
- **`[N]T`** fixed-size arrays with **value semantics** (copied on assign /
  pass / return). Array literals `[N]T{ e0, … }`, indexing `a[i]` (read and
  write), and `a.len`.
- **Runtime bounds checking**: an out-of-range index panics with exit 101.
- Type system: `Type::Array(id)` + an array table; lowered to a by-value C
  struct wrapper `{ T data[N]; }` with a bounds-checked accessor, emitted in
  dependency order. Diagnostics `E0220`–`E0224`.
- 321 unit + 16 e2e tests; `examples/arrays.ks`.

The original "arrays + slices + pointers + Allocator" roadmap item is split
into focused releases (v0.118 pointers & slices, v0.119 the Allocator + heap).

## [0.116.0] — Enums & exhaustive `switch`

### Added
- **Plain enums**: `pub? const Name = enum { A, B, C };`. Values written
  `Name.Variant` (qualified) or `.Variant` (inferred from context).
- **`switch`** with **exhaustiveness checking**: an enum switch must cover every
  variant or carry an `else`; an integer switch requires `else`. Multi-label
  arms (`.A, .B => { … }`). No hidden fall-through.
- Type system: `Type::Enum(id)` + an enum table; lexer `enum`/`switch` and the
  `=>` token. Lowered to a C `enum` typedef + a C `switch`. Diagnostics
  `E0210`–`E0215`.
- 282 unit + 14 e2e tests; `examples/enums.ks`.

### Deferred
- Tagged unions (`union(enum)`) and payload capture.

## [0.115.0] — Error unions (`!T`, `error.X`, `try`, `catch`)

Errors as values, the Zig way — with an implicit global error set.

### Added
- **`!T`** error-union types; **`error.Name`** error values; **`try expr`**
  (propagates the error out of the enclosing `!U` function; statement-level in
  v0.115); **`expr catch default`** (fall back to `default` on error). Implicit
  `T → !T` / `error.X → !T` coercion at typed positions.
- Type system: `Type::ErrorUnion(id)` + an interned payload table + a global
  error-name registry; lexer/keywords `try`/`catch`/`error` and the `!T` type
  prefix. Lowered to a tagged C struct `{ int32_t err; T val; }` with a
  per-union `_catch` helper; composite C typedefs still emitted in dependency
  order. Diagnostics `E0190`–`E0193`.
- 243 unit + 13 e2e tests; `examples/errunion.ks`.

### Deferred
- `errdefer`, `catch |e|` capture, named error sets `error{ … }`, and `try` in
  nested (non-statement) expression positions.

## [0.114.0] — Optionals (`?T`, `null`, `orelse`, `.?`)

Explicit, checked nullability — the Zig way.

### Added
- **`?T`** optional types (inner: a primitive or struct), with implicit `T → ?T`
  coercion at typed positions (initializers, assignment, return, args, fields).
- **`null`** (the empty optional), **`x orelse default`** (unwrap-or-default),
  and **`x.?`** (force-unwrap; panics with exit 101 if null).
- Type system: `Type::Optional(id)` + an interned optional-inner table; lexer
  `?`/`orelse`/`null`. Lowered to a tagged C struct `{ bool has; T val; }` with
  per-optional `_orelse`/`_unwrap` helpers. Composite C typedefs are now emitted
  in **dependency order**. Diagnostics `E0180`–`E0182`.
- 204 unit + 12 e2e tests; `examples/optional.ks`.

### Deferred
- `if (opt) |v| { … }` payload capture (a later increment).

## [0.113.0] — Struct methods & associated functions

Completes structs: functions declared inside a `struct` body.

### Added
- **Methods** — a function whose first parameter is `self` is callable as
  `instance.method(args)` (self is prepended); **associated functions** (no
  `self`) are callable as `Type.func(args)`. The explicit-self form
  `Type.method(instance, args)` is also accepted, and method calls **chain**
  (`c.bumped(1).bumped(2)`).
- AST: `StructDecl.methods: Vec<Func>` and `Expr::MethodCall`. Lowered to free C
  functions `kd_<Struct>_<method>(self, …)`. Diagnostics `E0170`–`E0172`.
- 168 unit + 10 e2e tests; `examples/counter.ks`.

## [0.112.0] — Structs (data aggregates)

The first roadmap version of the Gen-2 arc: **structs**, the foundational
product type. Data only — methods / associated functions are v0.113, kept
separate so each version ships complete and well-tested.

### Added
- **Struct declarations** (Zig syntax): `pub? const Name = struct { x: i32, y:
  i32 };`, including empty structs and nested structs.
- **Struct literals**: `Name{ .x = 1, .y = 2 }` — every declared field
  initialised exactly once, order-free.
- **Field access** `a.b.c` and **field assignment** `a.b.c = e;`.
- **Struct-valued** parameters, returns and locals — passed/returned **by
  value** (lowered to C structs and C99 compound literals).
- Type system: `Type::Struct(id)` + a `StructTable` (built by `sema`, consumed
  by `emit_c`); `sema::check` now returns the table.
- Diagnostics `E0160`–`E0168` for struct misuse (forward/cyclic field
  reference, unknown field type, duplicate field, non-struct literal/access,
  missing/extra field, immutable-place field assignment, struct equality).
- Formatter, 36 new unit tests, 2 end-to-end struct tests, and
  `examples/point.ks`.

## [0.111.1] — Source extension `.kd` → `.ks` ("Kardashev Scale")

### Changed
- The canonical source extension is now **`.ks`** — for "**K**ardashev
  **S**cale", the scale the project is named after. The build manifest is
  `build.ks` (Zig-style: the build is written in the language). `kard init`
  scaffolds `src/main.ks` + `build.ks`; `kard build`/`run`/`test` default to
  `build.ks`; examples are `examples/*.ks`.
- Added `.gitattributes` mapping `*.ks linguist-language=Zig` so GitHub labels
  and highlights kardashev sources with Zig's (closest) grammar rather than
  **KerboScript**, which owns `.ks` in GitHub Linguist by default.

## [0.111.0] — Generation 2: ground-up Rust rewrite, Zig-philosophy reboot

**A complete change of direction.** Generations 1 (`0.1.0`–`0.110.0`) was a
C++/LLVM compiler for a Rust-flavoured language with an affine borrow checker
and effect system, built with Bazel. It is preserved in git history and
releases. **Generation 2 is a ground-up reset**: the compiler is reimplemented
in **Rust** (every implementation file is `.rs`, zero external crates) and the
language is redesigned around **Zig's philosophy** — no hidden control flow, no
hidden allocations, `comptime` instead of macros, explicit `defer`, first-class
tests, and a single self-contained `kard` toolchain whose build is written in
the language itself. See `SPEC.md` and `ROADMAP-RUST-ZIG.md`.

### Added
- **New compiler in Rust** (`crates/kardc/`): `lexer`, `parser`, `sema` +
  `const_eval`, `emit_c`, `backend`, `cli`, `build_system`, `scaffold`, `fmt`,
  over the shared `ast`/`types`/`token`/`span`/`diag` contract. Pipeline:
  `source → lex → parse → sema → emit C → cc → native binary`.
- **Language v1 (the procedural core):** functions with Zig-style return types
  and recursion; fixed-width integers `i8…u64`, `usize`, `bool`, `void`;
  `var`/`const` bindings and comptime-evaluated top-level `const`;
  arithmetic/comparison/logical operators with no overloading; `if`/`else`,
  `while` (incl. `while (c) : (cont)`), `break`, `continue`, `return`;
  **`defer`** with correct LIFO flushing across fall-through, `return`, `break`
  and `continue`; **`comptime`** expression folding; built-in **`test`** blocks
  with `expect`; and a `print` builtin.
- **The `kard` toolchain:** `build`, `run`, `test`, `fmt`, `init`, `version`,
  `help`. `build.kd` minimal declarative build manifest; `kard init`
  scaffolding; diagnostics with filename, line/column and a source caret.
- **Tests:** 101 unit tests + 7 end-to-end compile-and-run tests (108 total),
  green on Ubuntu and macOS via `cargo test`.

### Changed
- CI now builds and tests with **cargo** (replacing Bazel + LLVM); a toolchain
  smoke step scaffolds, builds, runs and tests a project end-to-end.

### Removed
- The entire Generation-1 C++/LLVM/Bazel codebase: `compiler/` (C++), `bazel/`,
  `BUILD.bazel`/`MODULE.bazel*`/`.bazelrc`/`.bazelversion`, `Makefile.local`, the
  `kard` shell driver, the Gen-1 `tests/`, `examples/`, `docs/`, `bench/` and
  roadmaps. (All recoverable from git history and the `v0.110.0` release.)

### Deferred (honestly; tracked in `ROADMAP-RUST-ZIG.md`)
Optionals `?T`, error unions `!T`/`try`/`catch`/`errdefer`, structs, enums,
slices/arrays/pointers, the allocator interface + stdlib, comptime generics,
type inference, the full imperative `build.kd`, the real cross-compilation
matrix, comment-preserving `fmt`, and re-self-hosting. None are stubbed —
absent and scheduled.

## [0.110.0] — Bound-satisfaction diagnostics + LSP code actions (closes ARC D; completes the v101–v110 arc)

The final version of the v101–v110 production-depth arc.

### Added
- **Trait-bound-satisfaction diagnostics** (`typecheck.cpp`): a missing trait `impl`
  now emits a clear, actionable **E0277** — it names the bound (**the trait bound
  `X: Trait` is not satisfied**), suggests the fix (**add `impl Trait for X`**), and
  lists the types that DO provide the method. A direct missing-method call gets a
  correct caret on the call.
- **LSP `textDocument/codeAction`** (`lsp_main.cpp`): the server advertises
  `codeActionProvider` and offers, for each bound diagnostic, a **quick-fix** whose
  `WorkspaceEdit` inserts an `impl` skeleton at the end of the file (parsed straight
  out of the v110 diagnostic — so the diagnostic and the fix compose).
- **`tests/smoke_test_bound_diag.sh`** (4 checks) and
  **`tests/smoke_test_lsp_codeaction.sh`** (3 checks).

### Deferred (honest)
- A generic CALL site whose type-param is bound to a concrete type lacking the impl
  still surfaces at codegen (not typecheck) — a deeper monomorphization-time check;
  the `#[derive]` diagnostic's caret still points into the synthesized prelude region
  (message names the type correctly); the inserted `impl` stub is an empty block
  (auto-generating the method signatures is future work).

## [0.109.0] — expect_* panic asserts + kard bench (opens ARC D)

Scope corrected by live probing (research workflow `wg1nxd1fu`):
`assert!`/`assert_eq!`/`assert_ne!` already exist (the v37 effect-free `test_*`
convention — a failed assert `return`s 1). v109 adds two additive capabilities
without regressing that convention.

### Added
- **`expect!` / `expect_eq!` / `expect_ne!`** — the Rust-semantics PANIC form of
  assert, usable anywhere (not just `test_*`). On failure they
  `panic(format!("…  left: {:?}\n right: {:?}", l, r))` — aborting with **exit 101**
  and a Debug-formatted message. Built on the existing `panic` + `format!` + the `Eq`
  trait's `.eq()` + `{:?}` Debug, so they generalize to any `T: Eq + Debug` (i64, bool,
  String, Option, Result, …). The panic form forces the caller to declare
  `! { alloc, panic }` — which is why the effect-free return-1 asserts stay the test default.
- **`kard bench` / `kardc --bench`** — discovers `bench_*() -> i64` fns (mirroring
  `--test`), JIT-runs each, times it in the C++ host with `std::chrono`, and prints
  `bench <name> ... <ms> ms (result=<r>)`; `--filter` narrows. The bench returns a
  deterministic checksum so gates assert the result, never wall-time. A `kard bench`
  wrapper case + `--help`/usage lines added.
- **`tests/smoke_test_assert_v109.sh`** (5 checks incl. the return-1 regression guard,
  JIT+AOT abort, String operands) and **`tests/smoke_test_kard_bench.sh`** (5 checks:
  discovery + result correctness, count, no-bench→error, `--filter`, wrapper).

### Deferred (honest)
- The effect-free return-1 reporter stays i64-only (use `expect_*` or `assert!(a.eq(&b))`
  for non-i64 in tests); a panic-catching test runner; advisory wall-time regression
  thresholds + statistical sampling (needs a sub-ms timer); `--format=json` for bench;
  non-i64 bench return types.

## [0.108.0] — Self-hosted Box heap indirection (closes ARC C)

The self-hosted LLVM-IR compiler (`examples/selfhost/structgen.kd`) gains a real
`Box<i64>` — heap indirection, the next bootstrap rung after enums/match. Research
workflow `w8pn39kbb` probed the live host; implemented + verified in-session
(self == host **and** AddressSanitizer-clean).

### Added
- **`::` token** in the lexer (byte 58 twice → kind 29) — structgen had none.
- **`Box::new(e)`** recognized by name in the parser (like `vec_new`/`Just`),
  lowering to `call ptr @malloc(i64 8)` + `store i64 <e>`; value is a `ptr` (tag 600).
- **prefix-`*` deref** in `parse_factor` (kind 11 at a factor start — distinct from
  infix multiply), lowering to `load i64, ptr`.
- **Drop**: a `let mut` Box is freed once at the single fn exit (`load ptr` + `call
  void @free`) — sound (no early return; `check_fn` rejects a return tag ≥ 200, so a
  Box can never escape). A `want_box` runtime-family flag emits the libc malloc/free
  declares for a Box-only program (prior gates stay byte-identical).
- **`tests/smoke_test_selfhost_box.sh`** — differential self == host + ASan-clean gate
  (10 checks: R0 byte-identity, malloc/store/load/free IR shape, 2-malloc/2-free
  balance, box-in-helper-fn, `Box::new(bool)` + `*<i64>` negatives).

### Deferred (honest)
- `Box<i64>` only (no Box of struct/String/bool/generic-`T`); read-only deref (host has
  no deref-assign); no returning a Box / no Box-typed params (a Box stays a within-fn
  `let mut` local); no nested Box-of-Box; only the FINAL value of a reassigned `let mut`
  box is freed; a plain immutable `let p = Box::new(..)` lowers but isn't freed (no slot).

## [0.107.0] — Self-hosted enum + match (opens ARC C)

The self-hosted LLVM-IR compiler (`examples/selfhost/structgen.kd`) gains a real
generic enum + `match` lowering — the next bootstrap rung. Research workflow
`w9sa01eh6` probed the live host to fix the target IR shape; implemented + verified
in-session.

### Added
- **`Opt<T> { Just(T), Nope }` in the self-hosted subset** — recognized by name
  (`Opt`/`Just`/`Nope`/`match`) exactly as the `str_*`/`vec_*` builtins are; the
  declaration is genuinely parsed + consumed (`skip_enum_decls`). An enum value is a
  tagged struct `{ i64 tag, i64 payload }` (type tag 500), passed by value like a
  struct; `o: Opt<i64>` params resolve to it.
- **Constructors** `Just(x)` → `insertvalue {0, x}`, `Nope` → `{1, undef}` (mirrors
  the struct-literal lowering).
- **`match o { Just(b) => .., Nope => .. }`** — a new `Expr` variant lowered as
  `extractvalue` (tag + payload) + the Just binder bound to the payload in a child
  env + `select` on `tag == 0`. The `select` form mirrors the If-as-value lowering, so
  match/if expressions emit no branches (single basic block) and compose, even nested.
- **`tests/smoke_test_selfhost_enum_match.sh`** — differential self == host gate (12
  checks: R0 byte-identity, just/nope paths, both-arms, arm-order independence,
  binder-in-expr, inferred let-bound enum, mismatched-arm + wrong-payload negatives).

### Deferred (honest)
- Arbitrary enum/variant names + >2 variants (the recognizer is keyed to the fixed
  `Opt` shape — like v94's single-`T` generics start); single i64 payload only
  (multi-payload / non-i64 / struct payloads); `match &o` (scrutinee must be owned);
  side-effecting arms (would need branch+phi instead of `select`); `let` type
  annotations (a separate pre-existing structgen limitation — bindings infer the type).

## [0.106.0] — Codegen: tail-call + bounds-elision locked (closes ARC B)

**Lock-only** (the v95 pattern): live probing proved the default `-O2` build
already (a) lowers self-tail-recursion to a loop/closed form (so deep recursion
doesn't blow the stack) and (b) elides monotone bounds checks where sound — so
v106 ships **no codegen change** (one would be a no-op stub / regression risk) and
instead pins the wins with a permanent gate.

### Added
- **`tests/smoke_test_codegen_tco.sh`** — deterministic, target-aware, zero
  wall-time: BLOCKING structural IR-greps (0 surviving `call @sum` at -O2; monotone
  array loop 0 bounds checks; `vec_get` loop 0 range/sign/panic checks) + a runtime
  no-overflow + correctness proof (`sum(1_000_000,0)` exit ≠ 139 and ==
  `500000500000`) + loop correctness oracle; vectorization x86-64-enforce /
  arm64-soft. Complements the v95 perf-lock + v90 vector-lock.

### Deferred (honest)
- TCO at explicit `-O0` (a deliberate opt-out); a `become`/`musttail` language
  *guarantee*; general/mutual tail-call-elimination guarantee; the `vec_get`
  null-data branch (correctness-neutral, off the benchmark surface — `vec_get_ref`
  already vectorizes); LTO / cross-module inlining (XL).

## [0.105.0] — Generic Eq/Hash for Option/Result (opens ARC B)

Prelude-only blanket impls (verified post-v101 resolver).

### Added
- **`impl<T: Eq> Eq for Option<T>`** / **`impl<T: Hash> Hash for Option<T>`** and
  the **`Result<T,E>`** pair. Structural eq; derive-convention hash (per-variant
  seed `527+ordinal`, fold payload `*31`) so equal values hash equal.
- This makes Option/Result usable in `==`, as `Vec`/`Box` elements, and — the
  headline — **`#[derive(Eq, Hash)]` on a struct with an `Option`/`Result` field
  now resolves**, so that concrete struct keys a `HashMap` (verified round-trip
  across distinct allocations — eq+hash commute end-to-end).
- **`tests/smoke_test_composite_eq_hash.sh`** — JIT==AOT: Option/Result eq,
  hash-commute, derive'd-struct-with-Option-field HashMap key, Option Vec membership.

### Deferred (honest, probe-confirmed)
- **Tuple `Eq`/`Hash`** — a tuple is not a registrable impl head
  (`impl Eq for (T,U)` → "impl for unsupported type"); the composite-key path is a
  nominal `#[derive(Eq,Hash)]` struct, **not** `HashMap<(K1,K2),V>`.
- **A generic type *directly* as a HashMap/HashSet key** (`HashSet<Option<T>>`,
  `HashMap<Pair<T>,V>`) — blocked at codegen (the eager-emit pass skips
  monomorphized generic-impl methods, so the key machinery's bare-name hash/eq
  lookup misses); a codegen-dispatch fix, its own version. Concrete derive'd
  struct keys work.
- `char` Eq/Hash (no `char_to_int` builtin); `Ord`/`cmp` for these; arity > 4.

## [0.104.0] — Slice utilities (closes ARC A: stdlib depth)

Prelude-only. Slices were first-class but had only scalar get/len/set builtins.

### Added
- **`slice_to_vec<T: Clone>(s: &[T]) -> Vec<T>`** — owned deep-copy (i64 + struct/
  String).
- **`SliceIter<T> { s: &[T], pos }` + `slice_iter`** — a borrowing `Iterator<T>`
  holding `&[T]` directly, chains into the v101 `g*` adaptor tower
  (`slice_iter(&v[1..4]).gmap(...).collect()`).
- **`slice_chunks` / `slice_windows`** → `Vec<&[T]>` zero-copy views. They take
  `&Vec<T>` (not `&[T]`): re-slicing a `&[T]` is rejected, and the views must root
  in a ref-param to stay sound (the escape checker doesn't track refs nested in a
  `Vec`, so `Vec<&[local]>` would be UB).
- **`slice_contains` / `slice_index_of`** `<T: Eq>` — linear search.
- **`tests/smoke_test_slice_methods.sh`** — JIT==AOT (no `--emit-c` leg): to_vec
  independence, iter chaining the v101 tower, chunks `[3,3,3,1]`, windows,
  contains/index_of, non-Copy String to_vec.

### Deferred (honest)
- Mutable-slice iteration (`for x in &mut s`), `split_at`/`first`/`last` wrappers,
  slice utilities in `--emit-c` (non-scalar `Vec<&[T]>`), `chunks_exact`/`rchunks`.

## [0.103.0] — Sort/search: quicksort + binary_search + partition

Prelude-only stdlib algorithms. The only sort was an O(n²) insertion sort.

### Added / Changed
- **`sort<T: Ord>`** upgraded in place from insertion sort to **quicksort**
  (median-of-three pivot + insertion-sort cutoff ≤12) — **same signature + `! {}`
  effect row** (a recursive `qsort` helper over the effect-free `vec_swap`/
  `vec_get_ref`). Median-of-three bounds depth to O(log n) on sorted/reverse
  adversarial input. Drops O(n²) → O(n log n) average.
- **`sort_by<T>(v, cmp: Fn(&T,&T)->i64)`** — caller-comparator quicksort,
  *iterative* (a closure is move-only so it can't recurse), `! { alloc }`.
- **`binary_search<T: Ord>` / `binary_search_by<T>`** → `Option<i64>`, `! {}`.
- **`partition<T>(v, pred: Fn(&T)->bool) -> i64`** — in-place, returns the pivot
  index (count satisfying), `! {}`.
- **`tests/smoke_test_sort_search.sh`** — deterministic seeded-RNG gate (1000-elem
  sortedness oracle, adversarial sorted/reverse complete+correct, binary_search
  present/absent, sort_by, partition, non-Copy String sort).

### Note
- Quicksort is **not stable** (the old insertion sort was). Every in-tree sort
  consumer uses a *total* comparator, so observable order is unchanged; a
  `sort_stable` merge-sort variant is a documented follow-on if ever needed.

## [0.102.0] — Recursive container `Debug` (`{:?}`)

`Debug` had impls only for scalars + `String`, so `println!("{:?}", v)` over a
`Vec`/`HashMap`/`Option`/… was impossible. v102 adds recursive container `Debug`.
Live probing confirmed the v101 generic-impl resolver makes every blanket impl
Just Work, so this is a **prelude-only** change (no codegen).

### Added (prelude blanket impls over each element's `fmt_debug`)
- `Vec<T>` → `[a, b, c]`; `Option<T>` → `Some(x)`/`None`; `Result<T,E>` →
  `Ok(x)`/`Err(e)`.
- `BTreeMap<K,V>` → `{k: v, …}` and `BTreeSet<T>` → `{a, …}` — **ordered /
  deterministic** (direct index walks; no `K: Ord`/`Clone` bound).
- `HashMap<K: Hash+Eq+Clone+Debug, V: Debug>` (bounds mandatory; bucket order is
  non-deterministic so only single-entry is gated) and `VecDeque<T>`.
- String elements are quoted + escaped (reusing v27 `str_escape`). `Box<T>` Debug
  works via deref. All under the `trait Debug` opt-out gate (no collision/bloat).
- **The headline DX win:** `#[derive(Debug)]` over a struct/enum with
  `Vec`/`Option`/… fields now recurses (`Widget { id: 7, tags: ["a", "b"],
  parent: Some(3) }`).
- **`tests/smoke_test_debug_recursive.sh`** — deterministic JIT==AOT over all of
  the above + a scalar-derive regression lock.

### Deferred (honest)
- Tuple `Debug` and `&[T]` slice Debug → v104; format-spec dispatch
  (`{:x}`/`{:04d}`) → its own version; `{:#?}` pretty-printing → a follow-on.

## [0.101.0] — Element-generic iterator adaptors

Opens the **ROADMAP-v101-v110** "production depth" arc. The lazy iterator adaptor
tower (v61/v78) was `i64`-only because a generic impl could not bind a generic
param as the trait's type-arg. Ground-truth probing corrected the roadmap premise
(the "nested-adaptor PHI crash" was a red herring — no codegen change was needed):
the real block was a typecheck error `unknown type: T` on the impl header.

### Added
- **Generic-impl resolution fix** (`bindTraitParamsForImpl`): an impl's own
  generic params that a trait type-arg references (the `T` in
  `impl<I: Iterator<T>, T> Iterator<T> for GTake<I,T>`) are now seeded into the
  resolution env. Restricted to *referenced* params so the `i64` tower (trait arg
  `i64`, no param names) allocates **zero** fresh Vars and stays **byte-identical**
  (a naive fix shifts the global Var-ID counter and renames phantom-mangled
  symbols → IR drift; verified avoided by an empty `--emit-llvm` diff).
- **An element-generic prelude adaptor tower** under `g*` names — `gvec_iter`,
  `gtake`, `gskip`, `gmap`, `gfilter` — that fuses lazily over **any** element type
  (i64, structs, owned `String`), nests arbitrarily deep, and drains via the
  already-generic `iter_collect`. `gmap` takes `fn(&T)->U` (by reference) so a
  struct/String element passes by pointer (the by-value fat-pointer ABI would
  mismatch the indirect call). The existing i64 `iter_*` tower is **frozen** (its
  struct mangles are byte-identity-locked; the generic tower is a sibling).
- **`tests/smoke_test_iter_generic.sh`** — i64/struct/String chains JIT==AOT, a
  3-deep nested struct chain (IR-grep: `%Option__Pair` + distinct
  `%GTake__GFilter__…` instantiations, no PHI crash), and a use-gated lock (an
  i64-only program emits no `g*` symbols — the tower is monomorphize-on-use).

### Deferred (honest)
- **Unannotated** element inference (`let t = gtake(...)`) — the annotated forms
  (`let t: GTake<…> = …`, `let o: Vec<Pair> = iter_collect(…)`) work and are the
  supported idiom; bound-driven inference is a follow-on version.
- **Element-generic `zip`/`enumerate`** — their element is a *computed* pair
  (`TwoTup<T,U>`) only in the impl trait-arg, so `iter_collect` cannot infer it
  without bound-output inference; the i64 `iter_zip`/`iter_enumerate` remain.

## [0.100.0] — Arc close: codegen audit (2 real bugs fixed) + the 1.0 ledger

The final version of the v91–v100 arc. A 4-agent adversarial audit of every
lowering path the arc touched **found real bugs the per-feature gates missed** —
exactly the point of the audit — and v100 fixes them, hardens the bootstrap
candidate, and ships the honest 1.0-readiness ledger.

### Fixed
- **Packed-field misaligned write (host codegen).** A write to a misaligned field
  of a `#[repr(packed)]` struct emitted an over-aligned `store … align 8` to a
  1-aligned address — IR-level UB (latent on x86-64, **SIGBUS** on strict-alignment
  targets, exploitable by LLVM's alignment passes). Now codegen flags a
  packed-field place (`lastPlacePacked_` in `emitPlaceAddr`) and emits `align 1` at
  the three `emitAssign` store sites. The read path was already correct.
  *(Known limitation, matches Rust: a store through a `*mut T` taken from a packed
  field still loses packedness — UB in Rust too; not claimed fixed.)*
- **Binary `-` silently dropped (self-hosted emitter).** `examples/selfhost/structgen.kd`
  had no `-` token, so `a - b` returned `a` (a silent wrong answer). Fixed at 4
  sites: lexer (kind 28), `parse_sum`, `type_of` (arithmetic result), codegen
  (`sub i64`).

### Added
- **`docs/road-to-1.0.md`** — the measured 1.0-readiness ledger (perf / tooling /
  stdlib / platform / self-hosting), each row tagged shipped/measured-gap/mega-arc
  and **cross-checked against a named in-tree test**. No blanket "1.0-ready" claim.
- **`ROADMAP-v91-v100.md` → "v101 and beyond"** — the forward stub naming the XL
  mega-arcs (full bootstrap, register-ABI struct-by-value FFI, WASM/Windows
  backends, package registry, mechanized-spec capstone) with honest sizing.
- **`tests/smoke_test_packed_write.sh`** — packed write `align 1`, non-packed
  control `align 8`, runtime round-trip, + a `&mut [u32]` `align 4` audit-lock.
- **`tests/smoke_test_v100_close.sh`** — composes the packed-write fix + the
  hardened bootstrap corpus + the v99 effects gate + the v95 perf lock + the v97
  repr-packed lock + the v90 vector lock + a ledger doc-vs-reality cross-check.

### Changed
- `docs/bootstrap-status.md` gains a "Known self/host divergences" section (the 2
  fixes + 2 honest deferrals). `tests/smoke_test_bootstrap.sh` corpus grows to 11
  (adds the now-correct `-`).

### Deferred (honest)
- The audit's other two self/host divergences: `for`+`continue` (infinite loop —
  needs a continue-targeted latch / `ForRange` variant; deferred rather than risk
  a new-Stmt-variant change in the consolidation version) and effect-enforcement /
  generic-struct-param parse in the subset. Recorded per-case in
  `docs/bootstrap-status.md`.
- The road to 1.0 itself — the XL mega-arcs, sized in the v101+ stub.

## [0.99.0] — Self-hosting: effect rows + an honest bootstrap candidate + ledger

The self-hosted compiler gains opt-in **effect rows**, and the arc gets its honest
**bootstrap accounting**. Empirical probing confirmed: effect rows genuinely
failed in the subset (no `!` token), determinism already holds, and structgen
genuinely cannot self-compile (it is a single-`fn f` subset emitter — feeding it a
real `examples/selfhost/*.kd` segfaults). So v99 ships the genuine increment, not
a faked self-compile.

### Added (self-hosted emitter)
- **Opt-in effect rows** `! { alloc }` / `! { io }` / `! { io, alloc }` — the lexer
  gained `!` (token kind 27); `parse_fn`/`parse_impl_method` consume an optional row
  after the return type; a new `effects: i64` bitset field on the `Fn` record
  **propagates** it (1 = alloc, 2 = io). Codegen ignores it, so a row-free fn emits
  **byte-identical** IR (matching the host's opt-in default). Before v99, any
  effectful program emitted `; TYPE ERROR`.

### Added (bootstrap accounting)
- **`docs/bootstrap-status.md`** — the honest, file-by-file ledger of all 18
  `examples/selfhost/*.kd`: in-subset vs blocked, the first blocking feature for
  each (`Box`/`Option`/`HashMap`/library-shape), and the explicit remaining gap to
  a full bootstrap. Turns the XL bootstrap into a tracked contract.
- **`tests/smoke_test_bootstrap.sh`** — the bootstrap fixed-point **candidate**,
  named honestly: **NOT** a self-compile (impossible on a subset emitter), but the
  two bootstrap-necessary properties that hold — **determinism/idempotence** (a
  fixed program → byte-identical IR across runs) and **corpus self-application** (a
  10-program corpus, one per shipped self-hosting feature, each deterministic AND
  `self == host`).
- **`tests/smoke_test_selfhost_effects.sh`** — effect rows parse + run `self ==
  host`, a row vs row-free byte-identity assertion, and a no-false-`TYPE ERROR`
  regression guard.

### Deferred (honest, named per-file in `docs/bootstrap-status.md`)
- The **full-tree fixed-point** (structgen compiling the real library-shaped files
  / its own source) — blocked by `Box<T>`, `Option`/`Result` + `match`, `HashMap`
  codegen, multi-param generics, closures, `dyn`, and modules.
- **Effect enforcement** in the subset (v99 ships parse + propagate, not strict
  checking — matching the host's opt-in default).

## [0.98.0] — Self-hosting: static trait dispatch in the self-hosted emitter

The self-hosted compiler (`examples/selfhost/structgen.kd`) gains **static
(monomorphized) trait dispatch** — the v94-generics pattern extended from generic
*functions* to trait *methods*. Of the three coupled candidates (modules,
closures, trait dispatch), ground-truth probing picked the one genuine capability
increment that fits the existing machinery (struct-tag registry + direct-call
lowering + mangled-name monomorphization) and avoids a half-feature.

### Added (self-hosted emitter)
- **`trait Name { fn m(&self, …) -> R ; }`** — method signatures (no default
  bodies).
- **`impl Name for Widget { fn m(&self, …) -> R { … } }`** — each impl method is
  registered as an ordinary function under a mangled symbol `Widget_m` and emitted
  by the existing `emit_fn` loop.
- **`recv.method(args)`** — a new `MethodCall` `Expr` variant, disambiguated from
  field access by a `(`-lookahead in `parse_post`.
- **Static dispatch** — typecheck + lower resolve the receiver's concrete struct
  type to the mangled `Struct_method` and emit a **direct** `call <ret>
  @Struct_method(ptr %recv, <args>)`, passing the receiver by reference as
  `&self`. No vtable, no fat pointer, no `dyn` — it reuses the direct-call path.
- **`tests/smoke_test_selfhost_traits.sh`** — 10 differential self==host
  assertions (byte-identity for trait-free programs, single impl, method-with-arg,
  two impls of one trait for two types, a method calling another method on `self`,
  and a no-such-impl negative → self-hosted `TYPE ERROR`).

### Deferred (honest, with evidence)
- **`dyn Trait` vtable dispatch** — the emitter has zero indirect-call machinery
  (every `Call` lowers to a direct `call @name`); vtables need `{data,vtable}` fat
  pointers + per-(trait,type) vtable structs + slot-load indirect calls (~400-500
  lines). Its own arc.
- **Closures `|x| …`** — need an env-struct + heap env-alloc + hoisted
  `__closure_N` + the same fat-ptr/indirect-call ABI the emitter lacks (shares the
  `dyn` prerequisite).
- **`mod`/`use`/`pub`** — the emitter is a single-source-string compiler with a
  flat global registry, so modules would lower to *nothing*; real value needs a
  multi-file bootstrap arc.
- Default method bodies, supertraits, associated types/consts, generic/`dyn`-safe
  traits — each an independent increment atop the static core.

## [0.97.0] — Binary-format systems: repr(packed) + endianness + volatile

The "parse-a-packet-header / touch-a-device-register / read-a-binary-file"
version. Ground-truth probing corrected the plan: **raw pointers and enforced
`unsafe` blocks already exist** (so volatile is cheap and must be `unsafe`-gated),
and **`reverse_bytes` was hardcoded to i64** (so endianness on sized ints needed
width-aware lowering, not a bswap alias).

### Added
- **`#[repr(packed)]`** — a struct with no inter-field padding (LLVM packed
  struct), mirroring the v88 `#[repr(C)]` infrastructure end-to-end. `size_of!`
  shrinks to the sum of field sizes; unaligned field load/store stay correct.
  (`{u8, u64}` is 9 bytes, not 16; a `{u8, u64, u8}` header round-trips JIT==AOT.)
- **Width-aware endianness intrinsics** — `swap_bytes`, `to_le`, `to_be`,
  `from_le`, `from_be`, typed `T -> T` (preserving the argument's width and
  signedness). Lowered via `llvm.bswap` at the argument's *actual* width
  (`swap_bytes(0x1122u16) == 0x2211`, not the i64-bswap bug), with target
  endianness read from the module DataLayout. `reverse_bytes` stays as the v70
  i64 alias.
- **`volatile_load(p: *const/*mut T) -> T`** and **`volatile_store(p: *mut T, v)`**
  — `setVolatile(true)` (the optimizer may not elide, reorder, or duplicate the
  access), with the load width taken from the typechecked pointee. **Requires an
  `unsafe` block** (reusing the existing `unsafeDepth_` enforcement, exactly like
  `ptr_write`). The `--emit-llvm` IR shows `load volatile` / `store volatile`.
- **`smoke_test_repr_packed.sh`** — packed no-padding `size_of!` + byte
  round-trip; the width-aware `swap_bytes` case (which fails with the old i64
  bswap); `to_le`/`to_be`/`from_le`/`from_be` round-trips at u16/u32/u64; volatile
  round-trip + the `unsafe`-rejection + a target-independent `volatile`-keyword IR
  grep; three C-backend refusals; and `repr(transparent)` still rejected.

### Changed
- `#[repr(packed)]` is no longer rejected at parse (the v88 message was "only
  `repr(C)` is supported" → now "only `repr(C)` and `repr(packed)` are
  supported"). `smoke_test_repr_c_ffi.sh`'s `neg-repr-packed` case was repointed
  to `repr(transparent)` (still rejected) + a positive `repr(packed)`-compiles
  case.
- The C backend (`--emit-c`) **refuses** packed structs (layout-sensitive) and
  the endianness/volatile builtins (no in-subset C runtime) — never a silent
  miscompile.

### Deferred (honest)
- **Bit-fields** (`field: uN : W`) — a genuine **L** feature (a parallel
  single-backing-integer struct representation special-cased across struct-literal
  emit, the three field-access/lvalue paths, field-assign read-modify-write,
  struct-body declaration and `size_of!`, plus a borrow ban on `&`-of-a-sub-byte
  field). `#[repr(packed)]` already covers byte-granular packet/register access;
  bit-fields are the sub-byte refinement. Designed; in-source note at
  `parser.cpp` `parseParam`; tracked in ROADMAP-v91-v100. Not rushed (no
  half-feature).
- `#[repr(transparent)]` / `#[repr(align(N))]` (the rest of the repr family);
  big-endian *target* codegen (the bswap branch is correct but untested with no
  big-endian backend).

## [0.96.0] — Coherence: a stable E0119 + generalized negative impls

Ground-truth probing found **three of the four planned CORE items were already
met** by the shipped compiler — overlapping blanket impls were already rejected,
concrete-beats-blanket already dispatched to the concrete impl, and a duplicate
concrete impl was already a clean error. v96 therefore re-scopes to the genuine
gaps: a **stable error code** on the existing coherence diagnostic, and
**generalizing negative impls** beyond `Send`/`Sync`.

### Added
- **`E0119`** — a stable error code (with `kardc --explain E0119`) for the
  conflicting-trait-implementation / coherence diagnostics that previously had
  none. Classifies the `conflicting implementations`, `conflicting \`impl\``,
  `duplicate impl of marker`, and `duplicate negative impl` messages; ordered
  ahead of the broad `E0308` fallback.
- **Generalized negative impls** — `impl !Tr for X {}` now works for **any
  declared trait**, not just the `Send`/`Sync` markers (lifting the v31
  restriction). A negative impl opts `X` out of a blanket `impl<T> Tr for T`:
  the existing `expandBlanketImpls` `impld` set already seeds `"X/Tr"` from the
  negative impl, so the blanket never synthesizes `impl Tr for X`, and a later
  `X{}.tr_method()` fails to resolve. The trait must be declared and the impl
  method-less (the latter enforced at parse time).
- **`smoke_test_coherence.sh`** — an 11-case gate. A true overlap errors (now
  `E0119`); **concrete-beats-blanket compiles and the binary exits 111 not 222**
  (the #1 false-positive guard, dispatch asserted by running the binary); the
  blanket applies without the opt-out (exit 7); `impl !Greet for H {}` makes
  `H{}.g()` fail to resolve; `impl Tr` + `impl !Tr` and a duplicate `!Tr`
  conflict; a negative impl of an unknown trait / with a method body is rejected;
  **`#[derive(Clone)]` over a `Vec` field deep-copies (exit 7) and
  `#[derive(Debug)]` formats (exit 0)** — the highest-risk derive regression,
  locked by running the binaries; and `--explain E0119` prints.

### Changed
- The coherence pass tracks positive and negative impls in separate sets so a
  positive `impl Tr` plus a negative `impl !Tr` for the same type (in either
  order), and a duplicate `!Tr`, are reported as `E0119` conflicts — while a
  negative impl never falsely reads as a second positive.
- The negative-impl gate accepts any declared trait (was: hard error "negative
  impls are only allowed for the marker traits `Send` and `Sync`"). The two
  tests that asserted the old message (`typecheck_test.cpp`,
  `smoke_test_phase167.sh`) were repointed to the unknown-trait rejection.

### Deferred (honest)
- **Orphan rule** — documented in-source as deliberately **not enforced**: it has
  no soundness value in a single-crate language (every impl shares one prelude; a
  foreign-trait+foreign-type impl can only conflict — already caught — or be a
  benign extension), so enforcing it would forbid working code while catching
  nothing new. Revisit at the package-ecosystem mega-arc.
- **Call-site bound-satisfaction checking** — an unsatisfied generic bound still
  surfaces as `E0277 no impl provides method` at the resolution site rather than a
  dedicated "T does not implement Tr" message; a proper checker needs a full
  bound-satisfaction subsystem (its own version).
- RFC-1023 covered-types lattice, `default fn` specialization, cross-crate
  coherence, assoc-type-projection disjointness — all pre-deferred; none regressed.

## [0.95.0] — Codegen perf: a permanent perf-regression gate (parity is already at 1.00× C)

A ground-truth measurement found the roadmap's "~1.2× fib gap" was **stale**:
`fib(40)` and the 200M `loop` are at **1.00× C** today (`@fib`'s asm is
byte-identical to clang's; `@main` in `loop` has 0 allocas + 16 vector ops at -O2
— the v51 TargetMachine/TTI fix already neutralized the old alloca-heavy
lowering). So v95 ships **no codegen change** (one would be a no-op stub) and
instead installs the version's actual unaddressed risk: a perf-regression gate.

### Added
- **`smoke_test_perf_regression.sh`** — a CI-robust gate that LOCKS the measured
  parity invariants so a future PassBuilder/codegen refactor can't silently
  regress perf:
  - **BLOCKING** deterministic structural IR-greps (identical on x86-64 + arm64,
    zero wall-time): `@fib` has 0 allocas at -O2; `@main` (loop) has 0 allocas;
    the loop auto-vectorizes (arch-aware: x86-64 strict, arm64 soft).
  - **ADVISORY** wall-time sanity: generous (≤ 2.0× = gross regression only),
    best-of-5, x86-64-only, fully skippable — can never flake CI. The tight 1.00×
    numbers live in BENCHMARKS.md, never asserted in CI.
- Complements (does not duplicate) the v65 codegen-perf and v90 vector-lock gates.

### Deferred (honest)
- LTO / cross-module inlining, true tail-call elimination, escape-to-stack for
  closure envs (all XL / their own version). The fib gap is irreducible below
  1.00× without them — and it is already at 1.00×.

## [0.94.0] — Self-hosting: monomorphic generics (`fn id<T>`, `struct Pair<T>`)

The self-hosted LLVM-IR compiler (`examples/selfhost/structgen.kd`) gains the
feature the *real* compiler uses pervasively — type parameters — via
**monomorphic specialization** (one specialized copy per concrete type at a call
site, deduped by mangled name, mirroring the host's `emittedInstances_`).

### Added (in the self-hosted emitter)
- **Single type-parameter generics**: `fn id<T>(x: T) -> T`, `struct Pair<T> { a: T, b: T }`
  — `<T>` parsing, tag `-1` for the unbound `T`, a monomorphization registry,
  `mangle` / `specialize_*` helpers, and `Call`/`SLit` routing to the
  per-concrete-type instance (`T` inferred from the first generic-typed argument).
- Use-gated so non-generic programs emit **byte-identical** IR (the prior gates).

### Notes
- Gate: `smoke_test_selfhost_generics.sh` — differential self == host on a generic
  fn specialized at i64 + at a struct, a generic struct build+sum, a generic call
  in a loop, two-types-dedup, and an ill-typed-generic-call negative.

### Deferred (honest, evidence-based)
- **Element-generic host iterator adaptors** (the planned second half): empirically
  the typecheck fix is a one-liner that unblocks a *single-level* element-generic
  impl, but **nested adaptors** (`Take<I>` over `Iterator<T>`) crash codegen with a
  PHI type mismatch (`T` unresolved through the transitive bound — real L work),
  and even the one-liner risks the 10+ shipped i64-adaptor tests. The i64 tower
  stays as-is; element-generic iterators move to a later line.
- Generic trait dispatch (vtables) → v98; const-generics / multi-param `<A,B>` in
  the self-hosted subset.

## [0.93.0] — Write-capable `&mut [T]` slices + variadic-C FFI + C-backend slice-from-array

The highest-leverage practical-systems gap: mutation-through-slice existed in no
backend. v93 makes `&mut [T]` write-capable end-to-end and folds in two adjacent
FFI / C-backend unlocks.

### Added
- **`slice_set(&mut [T], i, v)`** and **`slice_get_mut(&mut [T], i) -> &mut T`** —
  in-place writes through a slice (LLVM: `slice_set` = GEP + `store`,
  `slice_get_mut` = the `slice_get_ref` GEP; the existing deref-assign means
  `*slice_get_mut(s, i) = v` worked with no new code). C backend lowers the same
  over `struct kdslice`.
- `&mut [T]` as a distinct write-capable slice (a `Type.sliceIsMut` flag, checked
  at the write-builtin call site so a shared `&[T]` is rejected — `unify` ignores
  it, giving the `&mut [T] → &[T]` coercion for free).
- **`&mut v[a..b]` / `&mut arr[a..b]`** construction, and **slice-from-array**
  (`&arr[a..b]` over a stack `[T; N]`) in the type-checker, LLVM, and C backend
  (the v89/v90 deferral).
- **Variadic-C FFI**: `extern "C" fn printf(fmt: &String, ...) -> i32` — a `...`
  marker + `isVarArg` `FunctionType` with C default-argument promotion on the
  trailing args.

### Notes
- Gate: `smoke_test_slice_mut.sh` — in-place sort over `&mut [i64]` / `slice_set`
  fill / array-slice read+write / `&mut[T]→&[T]` each **JIT == AOT == C**;
  `*slice_get_mut = v` + variadic `printf` JIT == AOT; and two soundness negatives
  (E0502 aliasing read across `slice_set`; `slice_set` on a shared `&[T]`).
- Borrow-check reuses the v26 two-phase + v89/v90 array/slice exclusivity rules.

### Deferred (honest)
- Variadic + `*slice_get_mut = v` deref-assign in the C backend (`--emit-c`
  refuses extern fns / non-variable assignment places — LLVM/JIT/AOT full).
  Non-scalar `&mut [String]` in C (LLVM full). Mutable-slice *iteration*
  (`for x in &mut s`) → v94. Register-ABI struct-by-value FFI → XL mega-arc.

## [0.92.0] — Self-hosting: growable `Vec<i64>` + owned strings

Builds on v91's CFG. The self-hosted LLVM-IR compiler
(`examples/selfhost/structgen.kd`) gains the one heap data structure every
compiler phase needs — a growable `Vec<i64>` and owned (heap-allocated) strings —
emitted into its **self-contained** IR (clang links libc).

### Added (in the self-hosted emitter)
- **`Vec<i64>`** (type-tag 4 → `{ ptr, i64, i64 }`): `vec_new` / `vec_push` /
  `vec_get` / `vec_len` / `vec_set`, plus **growable `str_concat`** (owned
  `cap > 0` strings reusing the String layout).
- A **use-gated runtime preamble**: libc `declare`s (`malloc`/`realloc`/`free`/
  `memcpy`) + LLVM `define`s for `@kdvec_*` / `@kdstr_*`, emitted **only when a
  Vec/owned-String is actually used** — so the v84–v91 gates stay byte-identical.
- **Drop-free-at-exit** for non-escaping owned locals (one `free` at the function
  exit block — enabled by v91's real exit block).
- Two enabling fixes: `&mut <mutable-local>` now passes the local's actual
  `alloca` slot (not a load+re-alloca copy), and a new `ExprStmt` so a bare
  `vec_push(...);` statement parses.

### Notes
- Gate: `smoke_test_selfhost_vec.sh` — differential self == host on vec build+sum,
  `for`-push + `vec_len`, growable `str_concat`, a tokenizer capstone, grow
  boundaries, negatives, and a 100k-push `MALLOC_CHECK_=3` + RSS-flat leak check.

### Deferred (honest)
- `vec_set` is self-only-tested (no host counterpart). String drop-on-*reassign*
  leaks the prior buffer (bounded, freed at exit; true drop needs liveness).
  `Vec<T>` for non-scalar `T`, nested `Vec`, `HashMap` → v94+ (need generics).

## [0.91.0] — Self-hosting: real control flow (mutable locals + `while`/`for` CFG)

Opens the v91–v100 arc (practical systems + self-hosting completeness). The
self-hosted LLVM-IR compiler (`examples/selfhost/structgen.kd`) was *branch-free*
(every `if` → `select`, every binding immutable, one basic block). v91 rewrites
it to be **block-terminator-aware** — the architectural unlock every later
self-hosting increment (Vec, real lexers, the compiler's own phase loops) depends
on.

### Added (in the self-hosted emitter)
- **Mutable locals**: `let mut x = e` lowers to `alloca` / `store`; a use `load`s;
  `x = e` stores. Immutable `let` keeps the original SSA-value path verbatim, so
  the v84–v86 gates stay **byte-identical**.
- **`while` loops** as a real CFG: `loop.header` / `loop.body` / `loop.exit` basic
  blocks with `br i1`, a "current-block-terminated" cursor enforcing exactly one
  terminator per block.
- **`for i in lo .. hi { … }`** — desugared in the self-hosted parser to the
  `let mut` + `while` form. New lexer tokens `..` and `<=`.
- **`break` / `continue`** → `br` to a loop-target stack (nested loops supported).
- Self-hosted type-checking: a `let mut`'s type is fixed; assignments must match;
  `break`/`continue` outside a loop is rejected.

### Notes
- Gate: `smoke_test_selfhost_loops.sh` — differential self == host on while-sum
  (55), while/for factorial (120), break-early, continue-skip, an iterative-fib
  mutable accumulator, nested loops, and a break-outside-loop negative;
  phase115–118 + refs + calls stay byte-identical. Correctness-first: `alloca` +
  `-O2` mem2reg reclaims the SSA (no hand-emitted `phi`).

### Deferred (honest)
- Labeled break, hand-emitted minimal `phi` networks, `match`-as-decision-tree
  CFG (the `select`-chain stays). Self-hosted `Vec` → v92 (needs this foundation).

## [0.90.0] — Closing pass: read-only slices in the C backend + vectorization lock

The final version of the v81–v90 arc. A grounded survey corrected two premises:
"mutable slices" don't exist in *any* backend (mutation-through-slice is rejected
even in LLVM), and vectorization is *already* complete across JIT/AOT/`--emit-llvm`
(not JIT-only). So v90 ships the honest, real, testable cuts.

### Added
- **Read-only slices in the C backend** (`--emit-c` previously refused all slices):
  `&[T]` → `struct kdslice { int64_t* ptr; int64_t len; }` (mirrors the LLVM
  `{ i8*, i64 }` slice), with bounds-checked `slice_len` / `slice_get` /
  `slice_get_ref` and `&v[a..b]` creation over a scalar `Vec`. Scalar-element only
  (`&[i64]` / `&[bool]`); a non-scalar slice (`&[String]`) is cleanly refused
  (LLVM keeps it).
- **`smoke_test_v90_close.sh`** — slice read / subrange / `get_ref` each
  **JIT == AOT == C backend**, the non-scalar-slice C refusal, and a vectorization
  regression lock (IR-grep for vector ops) so the v51 TargetTransformInfo fix can't
  silently regress in a future PassBuilder refactor.

### Notes
- Vectorization was verified already-correct across all emit paths (16 vector ops
  in `--emit-llvm bench/loop.kd`; 17 SIMD instructions in the AOT binary) — v90
  *locks it in* rather than fixing it.

### Deferred (honest, no stubs) — a documented v91 line
- A user-replaceable **`GlobalAlloc`** allocator: L/XL (~63 hardcoded
  malloc/realloc/free sites + free-glue routing) and not CI-safely-observable
  without fragile LD_PRELOAD; the 63-site inventory is captured for v91.
- Genuine **slice mutation** (`slice_set` / `slice_get_mut`) — exists in no backend
  today (needs typecheck + borrow-check + both backends).
- Slice-from-fixed-array in the C backend; the <5-LOC array-layout-helper trim.

## [0.89.0] — Stack arrays `[T; N]`: C-backend parity + differential gate

A ground-truth survey confirmed fixed-size arrays `[T; N]` are **already fully
runtime-first-class in the LLVM backend** (alloca-backed, const-generic `N`,
bounds-checked indexing + OOB panic, by-value params/returns, in-place `a[i] = x`,
array-of-struct, per-element Drop — all JIT==AOT). So v89 closes the one genuine
gap and locks the surface with the first end-to-end differential gate.

### Added
- **C-backend array support** (`--emit-c`, previously refused all arrays): `[T; N]`
  lowers to a first-class wrapper `struct kdarr_<elem>_<N> { <elem> data[N]; }`
  (the v75 tuple pattern), with array literals (`[a, b, c]` / `[v; N]`),
  **bounds-checked** `a[i]` reads and `a[i] = x` stores (panic + `exit 101` with
  the same message as LLVM), and by-value param/return/copy.
- **`smoke_test_stack_array.sh`** — a triple-differential gate (JIT == AOT ==
  C backend): histogram, in-place bubble sort, array-of-struct, by-value
  param+return, value-copy independence, OOB-panic parity, and the non-Copy
  refusal.

### Deferred (honest, no stubs)
- Non-Copy array **elements** in the C backend (`[String; N]` / `[Vec<_>; N]`)
  need C-backend per-element Drop glue — cleanly **refused** (LLVM keeps full
  non-Copy arrays). Symbolic / side-effecting `[v; N]` repeat counts and nested
  array-of-tuple in the C backend → v90 / follow-on.

## [0.88.0] — `#[repr(C)]` struct layout + struct FFI by pointer

Builds on v87's sized-int FFI widths. A grounded survey proved that full struct
**by-value** FFI is a verified miscompile risk (clang lowers `int sum(struct
Point{int x,y})` to `i32 @sum(i64)` — register-classified, not an LLVM aggregate
param), so it is honestly deferred to the by-value-ABI / WASM+Windows mega-arc.
v88 ships the portable, fully real-C-tested cut.

### Added
- **`#[repr(C)]`** attribute on a struct — a guaranteed C layout (declaration
  field order + host alignment via the already-set datalayout). Stored on
  `StructDecl`/`Type` (`reprC`). `repr(packed)` / `repr(transparent)` /
  `repr(align(N))` are **rejected**, not silently ignored.
- **Struct FFI by pointer**: an `extern "C"` signature may pass/return a
  `#[repr(C)]` struct as `&T` / `&mut T`. A pointer to a **non-repr(C)** user
  struct is rejected (no layout guarantee); a struct **by value** is rejected
  with an actionable "pass `&T`" message.
- **signext/zeroext** on narrow (i8/i16) `extern "C"` params + returns (the v87
  deferral) — a C `unsigned char` / `signed char` / `short` boundary is now
  value-correct (255 stays 255, not −1).
- **`kardc --emit-obj <file.o>`** — emit a native object (no link) so a build or
  test can link it with a C object for real FFI interop.

### Notes
- Gate: `smoke_test_repr_c_ffi.sh` links a real clang-compiled C helper
  (`int point_sum(const struct Point*)`, `point_scale`, narrow-int `low_byte`/
  `neg_sc`) against `kardc --emit-obj` output — the kardc-built repr(C) struct is
  read/written by C (exit 70), and narrow-int values round-trip correctly. Plus
  IR-shape (`{ i32, i32 }`, by-pointer declaration, `zeroext`/`signext`) and three
  negatives (non-repr(C) pointer, by-value, `repr(packed)`). Skips with pass if
  clang is absent.

### Deferred (honest, no stubs)
- Struct **by-value** params + **`sret`** struct returns → the by-value-ABI /
  WASM+Windows mega-arc (needs the per-platform System V eightbyte register
  classifier, ~2000 lines). Rejected with a clear message, not stubbed.
- `repr(packed)` / `repr(align(N))` / `repr(transparent)` → a future repr-family
  follow-on (rejected now, so no silent misbehavior).

## [0.87.0] — Sized integers across all surfaces (Arc C begins)

Opens Arc C (practical systems gaps). A ground-truth survey found sized integers
(i8/i16/i32/i64, u8/u16/u32/u64) and f32 were **already runtime-first-class** in
the LLVM backend (shipped in v11): distinct widths, signedness-correct
arithmetic (`sdiv`/`udiv`, `ashr`/`lshr`, `slt`/`ult`), all casts, literal
suffixes, and rejection of implicit widening. So v87 surfaces them across the
boundaries that still assumed i64, and locks the semantics with a real gate.

### Changed
- **Extern `"C"` FFI boundary** (`cAbiType`): each sized int now maps to its real
  C width (`u8` → `i8` = C `unsigned char`, `u32` → `i32` = C `unsigned int`, …)
  instead of collapsing to a 64-bit word. (`i32` keeps its historical
  i64-sugar; `abs(0 - 7) == 7` is preserved.) This is the **v88 repr(C)-by-value
  prerequisite** — `extern "C" fn fw(a: u8, b: u16, c: u32, d: u64)` now declares
  `i8 @fw(i8, i16, i32, i64)`.

### Added
- **`smoke_test_sized_runtime.sh`** — the end-to-end runtime differential gate
  the v11 work never had: unsigned overflow-wrap, signed-vs-unsigned
  division/remainder/shift/compare, cast round-trips (trunc/sext/zext/fp), a
  sized struct field read at **-O2** (datalayout-before-opt guard), a sized array
  element, f32 arithmetic, the FFI all-width declaration shape, a mixed-width
  negative, and the C-backend's clean refusal — each JIT == AOT.

### Deferred (honest, no stubs)
- The **C backend** (`--emit-c`) continues to cleanly **refuse** sized ints:
  faithful support would need a width-cast after *every* op, because C integer
  promotion computes `uint8_t + uint8_t` in `int` and would silently diverge from
  LLVM's wrap-at-width. Refusing is sound, not a stub.
- **`print`/`print_f64` arg-widening** (so a sized int prints without `as i64`) →
  v89 stdlib formatting. The sound idiom today is the explicit `print(x as i64)`.
- **`signext`/`zeroext` narrow-arg ABI attributes** (need a real C-function test
  harness to verify end-to-end) → v88 FFI hardening.
- Per-element-type `Vec<u8>` runtime → later.

## [0.86.0] — Self-hosting: user function calls + read-only strings

Continues the self-hosting completeness arc. The self-hosted LLVM-IR compiler
(`examples/selfhost/structgen.kd`) gains multi-function programs and string
literals — and delivers the strings that v0.85.0 resequenced here.

### Added
- **User function calls.** A multi-fn registry: every top-level `fn` is parsed,
  type-checked against the registry, and emitted. A new `Call(name, args)` AST
  node lowers to `call <rty> @name(...)` using the *callee's* parameter types.
  `find_entry` keeps the fn named `f` as the differential-gate entry so the host
  wrapper (`fn main() { f(a, b) }`) still compiles.
- **Read-only strings.** A `"..."` lexer token (kind 24) → `StrLit(start, len)`.
  Each literal emits one private `@.str.<offset>` constant into a new module
  **preamble** buffer (globals precede the function defines), and lowers to the
  host's borrowed-String aggregate `{ ptr, i64, cap=0 }`. The `str_len(&s)`
  builtin lowers to `getelementptr` field 1 + `load`.
- A multi-function **capstone** differential program (calls + strings + struct +
  ref).

### Fixed
- A latent `is_alpha` bug in the self-hosted lexer: the `_` (95) case was dead
  code (95 fell into the `A`–`Z` branch and returned 0), so identifiers with
  underscores never lexed. No prior test used underscores, so it never surfaced
  until `str_len`.

### Notes
- All-i64 structs stay **byte-identical** (a callless/stringless program emits an
  empty preamble, so output still begins `define`), so phase117/118 + v85-refs
  hold.
- Gate: `smoke_test_selfhost_calls.sh` — byte-identity guard, capstone IR-shape +
  exit, seven differential cases (capstone ×2, one-arg, three-arg, nested calls,
  `str_len` hello/empty), and two negatives (unknown callee, arity mismatch);
  each self-hosted exit == host exit.

### Deferred (honest, no stubs)
- `while`/`for`-loop CFG + mutable locals + assignment, and scalar `Vec<i64>` +
  growable strings, move into the **XL real-bootstrap mega-arc** — they require a
  block-terminator/CFG rework plus an alloca-backed mutable-local model, an
  architectural change to the branch-free emitter, and self-contained runtime
  emission. v87–v90 remain the committed **Arc C — practical systems gaps**.

## [0.85.0] — Self-hosting: by-reference values (`&T`)

Continues the self-hosting completeness arc. The self-hosted LLVM-IR compiler
(`examples/selfhost/structgen.kd`) gains by-reference values — the survey's
"gate to everything" increment.

### Added
- **`&` lexer token** (kind 23) and an `Expr::Ref` node.
- **Reference types**: a `&T` carries type-tag `200 + base` (so `&i64`=201,
  `&Struct#idx`=300+idx); `ty_llvm` lowers any reference to an opaque `ptr` —
  exactly what the host emits for `&T`.
- **`&e` address-of**: materializes its operand into a stack slot
  (`alloca` + `store`) and yields the pointer.
- **Field access through a reference**: for a `&Struct` operand the backend
  `load`s the aggregate, then `extractvalue`s the field.

### Changed
- The self-hosted type-checker **rejects returning a reference** (`rt >= 200`) — a
  returned `&local` would dangle. This single rule is *provably sufficient* in
  this subset (no ref fields, no ref-of-ref, no stored refs), so a borrow can only
  flow downward into a call and die at end of statement — no NLL needed.

### Notes
- All-i64 structs stay **byte-identical** (`{ i64, i64 }`), so the phase117/118
  demo greps still hold.
- Gate: `smoke_test_selfhost_refs.sh` — byte-identity guard, ref-IR-shape, four
  differential cases (ref-field-sum, ref-field-in-if, ref-three-field,
  ref-nested-struct), and a negative return-ref rejection — each self-hosted exit
  == host exit. Tested via an in-fn `let r = &p`, so the helper keeps an
  `(i64, i64)` signature and the host differential wrapper works.

### Deferred / resequenced (honest)
- Read-only **strings** (the planned second half of v85) move to **v86**, not
  stubbed: they need call-expression parsing (for `str_len(s)`) and module-level
  global accumulation (for `@.str`) — both of which v86 builds anyway (loops +
  Vec + calls), so strings ride on v86 at roughly half the code.
- `&mut`, returned references, and NLL remain out of scope (by design).

## [0.84.0] — Self-hosting: heterogeneous struct fields + multi-payload enums

Opens the self-hosting completeness arc (v84–v86). The self-hosted LLVM-IR
compilers (`examples/selfhost/structgen.kd`, `enumgen.kd`) lowered their data
types as **all-i64**; they now carry real per-field / per-payload type
information, the lowest-risk highest-ROI self-hosting unlock.

### Added
- **Heterogeneous struct fields** (`structgen.kd`): `SDef.fields` stores typed
  fields (`Param{name, ty}`) instead of bare names. `parse_structs` reads the
  field type token via `ty_tag`; `ty_llvm` emits each field's real LLVM type
  recursively, so a nested struct lowers to `{ i64, { i64, i64 } }`; `type_of`
  and `lower` (`SLit`/`Field`) carry the field's declared type. Bool and
  nested-struct fields now compile.
- **Multi-payload enum variants** (`enumgen.kd`): a variant carries `1..N`
  payloads. `EDef.variants: Vec<VDef{name, arity}>`, `ECon` holds
  `Vec<Box<Expr>>`, and `Arm.binds: Vec<String>`. The layout widens to
  `{ i64 tag, i64 p0, …, i64 p<maxArity-1> }` (a narrower variant leaves trailing
  slots `undef`), with multi-`insertvalue` construction, multi-`extractvalue`
  destructuring, and positional binding in `match`.

### Notes
- All-i64 structs and single-payload enums stay **byte-identical** (`{ i64, i64 }`),
  so the existing demo IR greps still hold.
- Gates (extended, both differential self-hosted-exit == host-exit): `smoke_test_phase117.sh`
  (nested-struct, bool-field) and `smoke_test_phase118.sh` (two-payload,
  mixed-arity widest-second, three-payload). Exceeds the planned cap of 2 payloads.

### Deferred (honest)
- **Payloadless / nullary variants** (`None`) — they need paren-less
  match/construct syntax the toy self-hosted parser does not yet have.
- String / Vec struct fields (need the heap — v85+).

## [0.83.0] — Collapse the effect surface + docs

Closes the effects-simplification arc (v81–v83): a smaller, clearer surface.

### Changed
- The niche **`div`** (may-not-terminate) effect label is now an **extension**
  label — recognized in an explicit `! { … }` row only under
  `--effects=extended` (it had zero real uses). The default recognized surface
  is `io` / `alloc` / `panic` / `async` / `unwind` / `share`.
- **`share`** (the concurrency / thread-boundary effect) stays a recognized
  core-adjacent label — it is auto-inferred by `thread_spawn` / channel ops and
  widely declared, so gating it would be churn without simplification.
- **`docs/effects.md`** rewritten around the v81 opt-in model: effects are an
  optional typed side-channel; use `Result` + `?` + ownership for everyday
  errors; reach for a row to *prove* purity / IO-freedom (esp. with
  `#[codegen(no_*)]`). Documents the `--effects=opt-in|strict|extended` modes.

### Added
- **`kardc --explain effects`** — a single consolidated guide to the effect
  system (opt-in model, modes, when to use rows, Result-for-errors), replacing
  cross-referencing the scattered E0710 / E0711 / E0712 entries.

### Notes
- Gate: `smoke_test_effects_surface.sh` (`! { div }` rejected by default /
  accepted under `--effects=extended`; share concurrency still type-checks;
  `--explain effects` prints the guide). Full `make test` green.

### Deferred (honest)
- Gating `share` (load-bearing for concurrency inference + Send/Sync tests) and
  a prelude effect-row trim pass — both are churn-heavy with little real
  simplification gain; the opt-in model (v81) already removes the *requirement*.

## [0.82.0] — Result + ownership as the error story

Continues the opt-in-effects arc by making `Result` + `?` + ownership the
*primary* error/resource story.

### Added
- **`fn main() -> Result<T, E>`** entrypoint. Codegen synthesizes an i64
  exit-code wrapper (`Ok` → 0, `Err` → 1) as the real `main`; the AOT binary
  uses it as the process exit code, the JIT prints it. Done in IR (not by
  decoding a struct return through an `int64_t(*)()` pointer), so both backends
  see a plain integer entry. Combined with v81's opt-in `?`, this gives the
  idiomatic `let v = step()?; Ok(v)` top-level error flow.
- **`#[allow(missing_effect)]`** attribute — suppresses the undeclared-effect
  error for one fn even under `--effects=strict`, so a codebase can run strict
  mode with surgical opt-outs (`FnDecl.allowMissingEffect`, consulted in
  `checkEffects`).
- **`result_flatten`** (`Result<Result<T,U>,U> → Result<T,U>`) and
  **`option_flatten`** (`Option<Option<T>> → Option<T>`) — the monadic join,
  rounding out the (already large, v79) combinator vocabulary.

### Notes
- `?` already works in a no-row `Result`-returning fn (v81 opt-in) — verified.
- Gate: `smoke_test_result_main.sh` (main→Result exit codes, `#[allow]` under
  strict, flatten combinators).
- The C backend (`--emit-c`) refuses a `main() -> Result` entry cleanly (LLVM
  backend only).

### Deferred (honest)
- A `-W effect-unchecked` migration lint (needs the typechecker to expose
  inferred effects) and a custom `Error` trait hierarchy / backtraces.

## [0.81.0] — Effects are opt-in

Begins the v81–v90 "practical-systems-language" arc. The headline: effects are
no longer mandatory — they become an **opt-in** discipline, centring the
everyday language on `Result` + ownership.

### Changed
- A function with **no** `! { … }` effect row is now **unchecked**: it may
  perform any effect (e.g. `fn greet() -> i64 { print(42) }` compiles).
- A function with an **explicit** row (including `! { }`, an asserted-pure) is
  still **strictly checked** — it must declare every effect it performs. This
  keeps the change **fully backward-compatible**: all ~235 existing tests and
  the ~192 prelude rows are explicit, so they behave exactly as before.
- The inferred effect set is still computed and **propagated to callers**, so an
  *annotated* caller of an un-annotated effectful fn still sees the real effects.

### Notes
- `--effects=strict` restores the pre-v81 rule (an absent row means
  asserted-pure). `--effects=opt-in` is the default.
- `#[codegen(no_alloc/no_panic/no_io)]` contracts and the user-defined-effect
  exhaustiveness check (`perform E::op` reaching `main` unhandled) are
  **unchanged** — they are soundness/codegen properties, not style rules.
- Implementation: `FnDecl.sawEffectRow` (threaded from the parser's
  `sawEffectRow_`); `checkEffects` gates the undeclared-effect loop on it.
- Gate: `smoke_test_effects_optin.sh` (7 cases). The existing `smoke_test_effects*`
  suite still passes unchanged.

### Deferred (honest)
- `#[allow(missing_effect)]` per-fn attribute and a migration lint (v82); the
  broader Result-centric error ergonomics (v82) and effect-surface trim (v83).

## [0.80.0] — Diagnostics depth (multi-char spans, fix-its, JSON)

The final entry of the v67–v80 roadmap arc — diagnostics depth.

### Added
- **Multi-char span underlines**: a diagnostic now underlines the whole
  offending token (`^~~~~`) instead of a single caret, by scanning the source
  line from the caret column over an identifier/number/string run (operators /
  punctuation keep a lone `^`).
- **Inline fix-it `help:` lines**: common error codes carry a short, actionable
  hint shown under the snippet — e.g. E0384 → "declare the binding as
  `let mut …`", E0004 → "add the missing arms, or a catch-all `_ => …` arm",
  E0001/E0308/E0425/E0711. (The long form remains under `kardc --explain`.)
- **`--error-format=json`**: emits each diagnostic as a JSON object, one per line
  (NDJSON), with `severity` / `kind` / `code` / `message` / `file` / `line` /
  `column` / `endColumn` (half-open) / `help` — for IDE and CI tooling.

### Notes
- LSP **rename** across a file's references (the 4th item of the planned v80
  surface) already shipped earlier (`smoke_test_lsp_edit`/`rich`) and is
  unchanged here.
- Gate: `smoke_test_diag_depth.sh` (12 checks: underline width, help lines, JSON
  shape + `jq` field extraction, lone-caret for punctuation).

### Deferred (honest)
- AST-precise spans for type/borrow errors (the underline uses a source-line
  token scan from the caret, which covers the common identifier/literal case);
  cross-file LSP rename; LSP-protocol incremental edits.

## [0.79.0] — Generic Option/Result combinators

### Changed
- **Generalized** the previously `i64`-only combinators to be fully generic
  (mirroring the already-generic `result_is_err`/`ok`/`err`/`map_err`):
  `option_map`, `option_and_then`, `option_unwrap_or`, `option_is_some`,
  `option_ok_or`, `result_map`, `result_unwrap_or`, `result_is_ok`. Existing
  `i64` callers are unaffected (they instantiate at `i64`).

### Added
- New combinators, all pure-prelude `match` over the enum (closures
  effect-polymorphic via `! { e }`):
  - **Option**: `option_is_none`, `option_map_or`, `option_or`,
    `option_or_else`, `option_ok_or_else`.
  - **Result**: `result_and_then`, `result_unwrap_or_else`, `result_map_or`,
    `result_or`, `result_or_else`.

### Notes
- The `?` operator (Result, with `From`-based error conversion) and the `Error`
  trait already shipped in v0.50.0 (Phase 190); this version re-verifies them in
  the gate.
- Gate: `smoke_test_combinators.sh` (4 JIT==AOT groups: Option, Result,
  type-changing `ok_or`/`result_ok`/`map_err`, and `?` + `Error`).

### Deferred (honest)
- The `?` operator for **Option** (the type-checker requires `Ok`/`Err`
  variants — a focused typecheck+codegen follow-on). Calling a generic
  combinator with a bare `None` / `Err(x)` whose other type parameter is
  unconstrained needs a type annotation (a general generic-inference limit, not
  specific to these combinators).

## [0.78.0] — Lazy iterator adaptors (map / filter / fold / peekable)

### Added
- **`iter_map`** (`Map<I>`): applies a `fn(i64) -> i64` (or a capturing closure)
  to each element on demand.
- **`iter_filter`** (`Filter<I>`): yields only elements matching a
  `fn(i64) -> bool`, pulling/discarding non-matches inside `next`.
- **`iter_fold`**: the eager (terminal) reduction — walks the iterator to
  exhaustion threading an accumulator of any type `A`, with an
  **effect-polymorphic** folding fn (`! { e }` propagated to the result).
- **`iter_peekable`** (`Peekable<I>`): one element of lookahead — `peek()`
  returns the next element without consuming it; `next()` returns the cached
  element if present, else pulls fresh.
- All extend the v61 lazy `Iterator` tower (Take/Skip/Chain/Zip/Enumerate) and
  fuse: `iter_take(iter_filter(iter_map(0..100, …), …), 3)` runs in a single
  pass with O(1) extra memory; only a terminal `iter_collect` / `iter_fold`
  consumes. Pure-prelude — no codegen or type-check changes.

### Notes
- The mapper / predicate is stored as a struct **fn-field** and invoked via
  `(self.f)(v)` (a closure is stored as the same fat pointer). `Peekable` keeps
  its lookahead in **scalar** fields (`peeked` / `has_val` / `pval`) to avoid
  moving a non-Copy `Option` out of `self`.
- Gate: `smoke_test_lazy_iter.sh` (6 JIT==AOT cases incl. map→filter→take
  fusion, a capturing closure, and peek/next interleaving).

### Deferred (honest)
- **Element-generic** adaptors (`impl<T> Iterator<T> for Map<T, I>`) remain
  blocked by the impl resolver's "unknown type: T" limit (a v61 deferral), so
  these are `i64`-specialized like the rest of the tower.

## [0.77.0] — Stdlib container convenience ops

### Added
- **Vec**: `vec_is_empty`, `vec_first` / `vec_last` (→ `Option<T>`),
  `vec_clear`, `vec_truncate`, `vec_extend` (append another `Vec`).
- **HashMap**: `hashmap_is_empty`, `hashmap_get_or` (value or a default),
  `hashmap_clear`.
- **HashSet**: `hashset_is_empty`, `hashset_clear`.
- All are **pure-prelude** functions over the existing container intrinsics —
  no codegen or type-check changes. Generic where the element/value type allows
  (`vec_first<T: Clone>`, `hashmap_get_or<K: Hash+Eq+Clone, V: Clone>`).

### Notes
- The mutating ops (`*_clear`, `vec_truncate`) read the length into a **local
  counter** rather than re-reading the `&mut` container in the `while`
  condition — re-reading a `&mut` place while mutating it in the body trips the
  borrow checker (E0499); `*_clear` for the maps iterates a **snapshot** of the
  keys/items. `vec_extend` loops over the other (`&`) Vec.
- Gate: `smoke_test_container_ops.sh` (5 JIT==AOT groups incl. a String-keyed
  HashMap and an empty-Vec case).

### Deferred (honest)
- `vec_dedup` (in-place remove-while-iterate hits the same borrow limit),
  `vec_sort` / `vec_binary_search` (need an intrinsic), and HashSet algebra
  (`union` / `intersection` / `difference`).

## [0.76.0] — Parameter destructuring

### Added
- **Tuple-pattern parameters**: `fn dist((x, y): (i64, i64)) -> i64 { x + y }`
  (also with a `_` element, e.g. `(a, _): (i64, i64)`), on free functions and
  impl methods.
- **Wildcard parameters**: `fn ignore(_: i64, y: i64) -> i64 { y }`.
- Implemented as a **parser desugar** with zero type-check/codegen changes: a
  pattern param becomes a fresh synthetic param (`__patN` / `__wildN`) plus a
  `let (a, b) = __patN;` prepended to the function body, reusing the existing
  tuple-destructuring `let`. Multiple pattern params and 3+-element tuples work.

### Notes
- Gate: `smoke_test_param_destructure.sh` (6 JIT==AOT cases incl. impl method +
  3-tuple, + a C-backend refusal check).
- The C backend (`--emit-c`) handles **wildcard** params (no destructuring), but
  **refuses** tuple-pattern params cleanly (the desugar produces a
  tuple-destructuring `let`, which the C backend doesn't yet support) — never
  miscompiled.

### Deferred (honest)
- Nested tuple patterns in params (`((a, b), c)`), struct-pattern params
  (`Point { x, y }`), and tuple-destructuring in the C backend.

## [0.75.0] — C backend: tuple types

### Added
- The `--emit-c` C-source backend now supports **tuples**: `(a, b)` literals,
  `.N` field access, and tuples as fn **parameters**, **return types**, and
  **locals** (including **nested** tuples like `((i64, i64), i64)` and tuples
  behind a **reference** `&(i64, i64)`). A tuple `(T0, T1, …)` lowers to an
  anonymous C struct `struct kdtup_<elems> { T0 _0; T1 _1; … };`; distinct
  shapes are deduped and emitted in dependency order (nested before outer).
- Differentially gated: every test program's LLVM-AOT exit code equals the
  emitted-C exit code (`smoke_test_c_tuples.sh`, 6 positive + 4 refusal cases).

### Notes
- Tuple **elements** are restricted to scalars (`i64`/`bool`) and nested tuples
  of those. Tuples in **struct fields / enum payloads / top-level consts**,
  **tuple-destructuring `let`**, and tuples with **non-scalar elements**
  (String/Vec/struct) are **refused with a clear error** — never miscompiled.

### Deferred (honest)
- Tuples in struct fields / enum payloads / consts (an emission-ordering item);
  tuple destructuring in the C backend; tuples with heap-owning elements (need
  Drop-aware lowering).

## [0.74.0] — Single-level dyn trait upcasting

### Added
- **dyn trait upcasting** (single-level): a `&dyn Sub` / `Box<dyn Sub>` is now
  usable where a `&dyn Super` / `Box<dyn Super>` is expected, when `Super` is a
  **direct supertrait** of `Sub` (`trait Sub: Super { … }`). The object's data
  pointer is preserved, so the concrete impl is still dispatched correctly for
  both super- and sub-trait methods.
- Implementation: each subtrait's vtable now embeds one pointer slot per direct
  supertrait (placed **after** the method slots, so existing dyn dispatch — which
  only indexes the method slots — is unchanged). The upcast loads that pointer
  and rebuilds the fat pointer. Type-check adds a coercion rule (`coerceOrUnify`)
  that records the upcast; codegen swaps the vtable via `makeDynUpcast`.
- Multi-level upcasting works by **chaining** single steps (`Cee → Bee → Aee`),
  since each supertrait vtable likewise embeds its own supertrait pointers.

### Notes
- Gate: `smoke_test_dyn_upcast.sh` (6 cases incl. data-preservation across a
  distinct impl, `Box<dyn>` upcast, two-step chain, plain-`dyn` regression, and
  a rejected one-step grandparent), JIT==AOT.
- A **direct** grandparent upcast in a single step (`&dyn Cee` → `&dyn Aee`
  where `Aee` is not a *direct* supertrait of `Cee`) is rejected with a clear
  type error — chain through the intermediate trait instead.

### Deferred (honest)
- **Turbofish on method calls** (`v.method::<T>()`) — the other half of the
  planned v74 — is **deferred**: it would bind method-level generic parameters,
  but those are not yet fully supported (trait-method `MethodSig` has no generic
  params, and inherent generic methods `fn m<T>(&self)` currently fail at
  codegen). Adding turbofish first requires completing generic-method codegen, a
  separate arc; shipping turbofish alone would be a no-op veneer.
- One-step transitive (non-direct) upcasting.

## [0.73.0] — Associated constants, completed (Rust-style access)

### Added
- **Bare-path associated-const access** `Type::CONST` (no parens). Trait
  associated consts (`trait B { const MAX: i64; }` / `impl B for G { const MAX:
  i64 = 42; }`) and the `Type::CONST()` call form already worked (v25); v73 adds
  the Rust spelling `Type::CONST` as a value — previously the qualifier was
  dropped and it errored as an unknown identifier.
- **`Self`-qualified resolution** inside impl / default methods: `Self::CONST`,
  `Self::CONST()`, and `Self::method()` (a sibling associated function) now
  resolve through the concrete implementing type. The `Self` qualifier is mapped
  to that type's name during type-checking; codegen is unchanged (it already
  reads the resolved mangled target).
- Bare-path access also flows through to generic `T::CONST` (bounded type param)
  via the existing generic-static-call path.

### Notes
- Implemented in the parser (a bare `Type::seg` path now desugars to a zero-arg
  call of the no-self associated item, keeping the qualifier) + a small
  type-check `Self`→concrete-type mapping. `Type::CONST` ≡ `Type::CONST()`.
- Gate: `smoke_test_assoc_const.sh` (5 cases incl. bool/f64 consts + enum-variant
  regression, JIT==AOT).
- **Already shipped (verified, no change needed):** associated consts in
  traits/impls, impl coverage checking, and `where`-clauses on functions, impl
  blocks, and impl methods all work today.

### Deferred (honest)
- `where`-clauses on **type aliases** — type aliases don't yet take generic
  params (`type Alias<T> = …`), which that feature depends on; deferred as a
  focused follow-on.

## [0.72.0] — f64 transcendental math library

### Added
- A full **f64 math library** (the existing `f64_sqrt`/`floor`/`ceil`/`abs`
  grow to 25 functions), all pure `(f64…) -> f64`:
  - **trig**: `f64_sin`, `f64_cos`, `f64_tan`, `f64_asin`, `f64_acos`,
    `f64_atan`, `f64_atan2`
  - **exp / log**: `f64_exp`, `f64_exp2`, `f64_ln`, `f64_log2`, `f64_log10`
  - **power / roots**: `f64_pow`, `f64_cbrt`, `f64_hypot`
  - **misc**: `f64_copysign`, `f64_fmod`, `f64_min`, `f64_max`, `f64_trunc`,
    `f64_round`
- Functions with a portable LLVM float intrinsic (sin/cos/exp/log family,
  pow/copysign/min/max/trunc/round) lower to that intrinsic; the rest
  (tan/asin/acos/atan/cbrt/atan2/hypot/fmod) forward to the corresponding
  **libm** symbol. Both resolve in the **JIT** (process symbol table) and the
  **AOT** link (`-lm`, already present) — verified end-to-end.

### Notes
- Gate: `smoke_test_f64_math.sh` (5 value groups + 2 reject cases, JIT==AOT),
  results checked via `as i64` truncation to stay stable across libm
  implementations (Linux/macOS).
- The C backend (`--emit-c`) refuses f64 (out of its i64/bool subset), so these
  are JIT/AOT only — unchanged from prior f64 support.

## [0.71.0] — Format specs (`{:width}`, alignment, fill, radix)

### Added
- **Format specs** in `format!` / `print!` / `println!`:
  - **width / fill / alignment** — `{:5}`, `{:<5}` (left), `{:>5}` (right),
    `{:^5}` (center), a custom fill char (`{:*^7}`, `{:-^5}`), and the `0`
    zero-pad flag (`{:05}`). Width counts **characters**, not bytes.
  - **radix types** — `{:x}`, `{:X}`, `{:b}`, `{:o}`. Built from the raw
    two's-complement bit pattern (via the v70 `leading_zeros` intrinsic), so
    negatives format exactly like Rust (`{:x}` of `-1` → `ffffffffffffffff`,
    `{:o}` of `-1` → `1777777777777777777777`).
  - Specs compose: `{:08b}`, `{:08x}` zero-pad a radix conversion.
- Implementation is **pure parser desugaring + prelude** — the `parseFormatMacro`
  hole scanner now parses a small spec grammar and emits `str_pad_left/right/
  center` + `int_to_binary/octal/hex_lower/hex_upper` calls (all new prelude
  functions). No codegen or typecheck changes.

### Notes
- Default alignment (no `<`/`>`/`^`) is **right** (pad-left), matching the
  common numeric case; strings left-align with an explicit `{:<w}`.
- Unsupported specs (precision `.N`, sign `+`, unknown type chars,
  named/positional `{0}`) are **rejected with a clear parse error**, never
  silently mis-formatted.
- Gate: `smoke_test_fmt_specs.sh` (11 cases, JIT==AOT).

### Deferred (honest)
- Precision `{:.N}` (float rounding / string truncation), the `+` sign flag,
  the `#` alternate form (`0x`/`0b` prefixes), and named/positional arguments.

## [0.70.0] — Saturating arithmetic + bit-manipulation intrinsics

### Added
- **Saturating integer arithmetic** on `i64`: `saturating_add`,
  `saturating_sub`, `saturating_mul`. On signed overflow these **clamp** to
  `i64::MIN` / `i64::MAX` in the correct direction (vs the v33 `checked_*` ops,
  which return `Option<i64>`, and `wrapping_*`, which wrap). Lowered as the same
  overflow detection followed by a `select` to the boundary.
- **Bit-manipulation intrinsics** on `i64`, lowered to LLVM intrinsics:
  - `count_ones` (popcount), `count_zeros` (`64 - popcount`),
  - `leading_zeros` (ctlz), `trailing_zeros` (cttz) — both return `64` for an
    all-zero input (non-poison form),
  - `reverse_bytes` (bswap),
  - `rotate_left(x, n)` / `rotate_right(x, n)` (funnel shift; `n` taken modulo
    the 64-bit width, matching Rust).

### Notes
- All v70 builtins are JIT==AOT differentially gated
  (`smoke_test_satbits.sh`, 7 cases).
- The C backend (`--emit-c`) cleanly **refuses** these (out of its scalar
  subset — no miscompile), consistent with the other intrinsic builtins.

## [0.69.0] — Integer range patterns (`0..10 =>`)

### Added
- **Range patterns** in match arms: `lo..hi =>` (exclusive) and `lo..=hi =>`
  (inclusive), for integer scrutinees. Implemented as **sugar over v68 guards** —
  a range arm binds the scrutinee to a fresh name and produces the guard
  `(v >= lo) && (v < hi)` (or `<= hi`), reusing the suffix-tree fall-through and
  guard-aware exhaustiveness. So range arms chain correctly (`0..10 / 10..20 /
  _`), combine with explicit `if` guards, and a range arm does **not** count
  toward coverage — a range-only match is non-exhaustive (E0004), needing a `_`.
- New `@` token (lexer) for the reserved `name @ pattern` syntax.

### Deferred (honest)
- **`@`-bindings** (`name @ pattern`): the `@` token + AST node exist, but
  binding a whole value through decision-tree specialization is a focused
  follow-on — for now `name @ pattern` is **rejected with a clear message**
  (bind in the arm body instead) rather than mis-bound.
- Nested range patterns (`Some(0..10)`) and char ranges; range patterns don't
  participate in integer-domain exhaustiveness (a full-range still needs `_`).

## [0.68.0] — Match guards (`pat if cond =>`)

### Added
- **Match guards**: an arm may carry a guard, `pat if cond => body`. The arm
  fires only when the pattern matches **and** the guard (a `bool`, checked in the
  arm's pattern-binding scope) is true; on guard-false, control **falls through
  to the next arm** (not the wildcard). The guard's effects flow into the match.
- Verified genuinely **missing** before this version (the parser rejected `if`
  after a pattern) despite the v26/Phase-141 project record claiming guards —
  this is the real implementation.
- **Guard-aware exhaustiveness**: a guarded arm does **not** count toward
  coverage, so `match n { x if x>5 => 1 }` is correctly non-exhaustive (E0004),
  while `match o { Some(n) if n>5 => 1, Some(n) => 2, None => 3 }` is exhaustive.
- Implemented via per-guarded-arm **suffix decision trees** (`compileDecisionTree`
  gained a `firstArm` parameter): codegen tests the guard at the arm's leaf and,
  on false, emits the decision tree of the remaining arms — chaining correctly
  across multiple guards. Threaded through parser, typecheck, pattern_match,
  codegen, and ast_clone.

### Deferred (honest)
- A **by-value** guarded arm that binds a **non-Copy** payload is rejected (the
  suffix tree would re-extract it — a double-move); use `match &x` (borrows
  re-extract safely) or move the check into the body. The C backend (`--emit-c`)
  refuses guarded matches (outside its subset) rather than miscompiling.

## [0.67.0] — Codebase optimization & efficiency (audit-driven)

Opens the **ROADMAP-v67-v80** arc (workflow-designed + fact-checked: several
first-draft versions were dropped/narrowed because their premise was already
shipped). This version applies the v54–v66 adversarial audit's findings.

### Changed (no behavior change)
- Added a **`makeRuntimeFn(name, ret, params)`** helper in codegen.cpp that
  factors the repeated runtime-builtin skeleton (`FunctionType::get` +
  `Function::Create(ExternalLinkage)` + an `entry` block + `declaredFns_`
  registration) the audit flagged as its top factoring opportunity, and routed
  the representative single-block builtins (`monotonic_millis`,
  `rng_seed_global`, `__assert_report`) through it. **Byte-identical IR** —
  verified behavior-preserving by the existing builtin smoke tests + a new gate.
- `smoke_test_loc_audit.sh` gate: asserts the helper is present and adopted
  (≥4 sites) and that the converted builtins stay behavior-preserving
  (instant/rng/assert, JIT==AOT) — so the boilerplate cannot silently re-grow.

### Honest finding
The 7-reviewer audit concluded the codebase is **already ~90% tight** — there is
no egregious waste to cut, only ~6–10% factorable-but-largely-defensible
boilerplate. Accordingly this is a focused, small optimization, not a large
rewrite.

### Deferred (honest, with rationale)
- Routing the remaining multi-block builtins through `makeRuntimeFn` (mechanical
  follow-on; ~20 LOC; each needs interleaved arg-naming edits).
- A shared `tests/lib/harness.sh` for the per-script KARDC-finder/`diff_run`
  preamble — **intentionally kept per-script**: each smoke test stays
  self-contained / standalone-runnable, and a sourced lib adds Bazel-runfiles
  path coupling that can't be validated outside the CI sandbox.
- ROADMAP↔CHANGELOG narrative overlap is **intentional** (forward plan vs.
  release notes — different audiences), not waste.

## [0.66.0] — Test infrastructure: borrow fuzzer + sanitizer sweep + property harness

Three reusable, seeded, deterministic test rigs — **pure test infrastructure, no
compiler changes** — that the prior soundness/codegen work earns.

### Added
- **`smoke_test_fuzz_borrow.sh`** — a borrow-checker differential fuzzer. A
  seeded generator emits 120 programs from 14 hand-classified templates (shared
  & mutable refs, reborrows, ref returns rooted in ref params, field/tuple access
  through refs, match-through-`&T`, two-phase borrows, closure captures, plus the
  UNSOUND duals: use-after-move, two `&mut`, `&mut` while `&` live, return-ref-to-
  local, assign-to-immutable). Each carries a SOUND/UNSOUND **oracle**: every
  sound program must compile, every unsound one must be rejected — zero false
  pos/neg, with each unsound template's canonical instance hand-verified to be
  rejected (no silent false-negative blessing a hole).
- **`smoke_test_asan_ubsan_c_backend.sh`** — sweeps 12 in-subset C-backend
  programs (`--emit-c`: struct / enum+match / ref / for / while / String / Vec /
  closure / generic / recursion / bool) under `-fsanitize=address,undefined` and
  asserts each is clean, then feeds the **same** flags 3 known-UB C programs
  (heap overflow, use-after-free, signed overflow) and asserts each is caught —
  proving the sanitizers are live. Skips gracefully without clang/ASan.
- **`smoke_test_property_harness.sh`** — 16 prelude/stdlib invariants (Vec
  push/len/get/sum/pop/reverse/swap/remove, String concat/repeat/contains/
  starts/ends/index_of, Option `unwrap_or`, the lazy iterator tower, arithmetic
  round-trip), each checked over 50 seeded random inputs, asserting **JIT == AOT**.

### Deferred (honest)
- TSan concurrency fuzzing; a 2000+-case grammar-conformance corpus; whole-program
  type+effect interaction fuzzing. Also noted: `iter_collect` over a `Take<Range>`
  hit a codegen "unsupported type" edge in the harness (worked via `.next()`
  draining) — a v61 lazy-tower follow-on.

## [0.65.0] — Codegen perf: param-reg lowering + inline hints

### Added
- **`#[codegen(param_regs)]`** — a Copy-**scalar** by-value parameter that is
  never address-taken in the body is bound to its SSA argument directly, skipping
  the entry `alloca`+`store`. Observable at `-O0` (baseline fib has 1 param
  alloca, the annotated fib has 0); at `-O2` `mem2reg` already promotes it, so
  this is parity there, not a new win. Safety: assigning to a param is already a
  type error (immutable), and the address-taken walk is conservative (any
  unanalyzable node keeps the alloca), so a param bound this way is read-only.
  Excludes async fns (the SSA value wouldn't survive a suspension).
- **`#[codegen(inline)]`** — sets LLVM `InlineHint`; a **small, non-recursive**
  fn at `-O2` also gets `AlwaysInline` (a recursive fn keeps `InlineHint` only).
- Both parse alongside `no_alloc`/`no_panic`/`no_io`; opt-in, no default change.

### Performance (advisory)
The documented ~1.2× **fib** gap is dominated by recursive call overhead. With
`mem2reg` already SSA-ing param allocas at `-O2`, `param_regs` yields a
below-noise change there (measured: `fib(32) -O2` annotated ≈ baseline); the
real lever is inlining. **Closing the gap is incremental — 1.0× is not
guaranteed.** (The 2.2× *loop* gap was already closed in v0.51.0.)

### Deferred (honest)
- Bounds-check elision for loop-invariant (non-literal) indices;
  `#[codegen(vectorized)]` + verification; whole-program LTO/PGO.

## [0.64.0] — Diagnostics depth: more error codes + value-printing asserts

### Added / Changed
- **Expanded the error-code table** from 8 to 20 codes, and made `classifyError`
  a **deterministic priority-ordered** classifier (table sorted most-specific
  first). New codes: borrow/lifetime — `E0597` (does-not-live-long-enough /
  dangling return), `E0499` (mut-borrow-twice), `E0502` (shared/mut conflict),
  `E0505` (move out of borrowed); `E0004` (non-exhaustive match); effects —
  `E0710` (effect not declared), `E0711` (effect escapes `main`), `E0712`
  (unknown/duplicate effect); `E0720` (codegen-quality contract violated),
  `E0721` (totality), `E0080` (const-eval failed). Also classified the
  previously-uncoded `let`-binding type mismatch (now `E0308`).
- **`kardc --explain Exxxx`** automatically covers every new code with a curated
  multi-line explanation (it iterates the table).
- **Value-printing asserts** — `assert_eq!` / `assert_ne!` now bind their
  operands to temporaries (single evaluation) and, on failure, print the actual
  `left=…`/`right=…` values via a `Display`-bound reporter **before** returning
  the non-zero test code (previously they silently returned 1). Operands must be
  `Display` (mirrors Rust's `Debug` requirement).

### Deferred (honest)
- **Multi-character spans** (`^^^^` underlines covering the full offending
  subexpression) — the heaviest sub-feature; split to a later **v64.x**. Also:
  cross-function breadcrumb context, structured JSON diagnostics, fix-it hints.

## [0.63.0] — Stdlib I/O depth: buffered reader + file metadata

### Added
- **Buffered line reading** — `struct BufReader` (owns a `FILE*` + persistent
  `getline` scratch) with `buf_reader_new(&String) -> Result<BufReader, IoError>`
  and `buf_read_line(&mut BufReader) -> Option<String>` (`\n`-stripped lines,
  `None` at EOF). A `Drop` impl `fclose()`s the handle and `free()`s the scratch,
  so a dropped reader is leak-free (verified RSS-flat over 100k open/read/drop
  cycles). Built on a portable `getline`.
- **File metadata** — `struct Metadata { size, is_dir, is_file, mtime }` +
  `fs_metadata(&String) -> Result<Metadata, IoError>` over a single `stat()`,
  plus `fs_is_dir` / `fs_is_file` wrappers. The builtin returns size/mode/mtime
  as `i64` out-params (read at `#if`-guarded `struct stat` offsets — Linux and
  Darwin); the prelude derives `is_dir`/`is_file` from the `S_IFMT` bits, so no
  bool/struct field is touched from codegen.
- Both reuse the existing `IoError`/`Result`/`io_error_cat` scaffolding; the
  builtins operate on **primitive types only** (i64 handles / `&mut i64` /
  `&mut String`) so they never name the prelude structs, and are emitted only
  when referenced (the file-I/O runtime gate).

### Deferred (honest)
- `BufWriter`, seek/random-access, directory listing/walk, permissions/chmod,
  symlink resolution, mtime-based incremental-build wiring.

## [0.62.0] — Stdlib runtime: monotonic clock, env vars, seeded global RNG

### Added
- **Monotonic clock** — `struct Instant { ms: i64 }` + `instant_now()`,
  `instant_elapsed_millis(&Instant)`, and `instant_duration_since(&Instant,
  &Instant) -> Duration`, over a new `monotonic_millis()` builtin
  (`clock_gettime(CLOCK_MONOTONIC)`, ms resolution).
- **Environment variables** — `env_var(&String) -> Option<String>` (an **owned**
  copy on a hit) over a `env_var_into` builtin (`getenv`), and
  `env_var_set(&String, &String) -> i64` (`setenv`, overwrite).
- **Seeded process-global RNG** — `rand_global() -> i64` over a 64-bit LCG in two
  internal globals, **lazily seeded from `KARDASHEV_SEED`** (else a fixed
  default) on first use; `rng_seed_global(seed)` to set it explicitly; and a
  `--fuzz-seed N` CLI flag that exports `KARDASHEV_SEED` for the JIT run. The
  same seed reproduces an identical sequence (JIT == AOT); a different seed
  differs.
- Each builtin is a thin libc wrapper lowered in codegen (idempotent
  `getOrInsertFunction`), emitted **only** when referenced
  (`usesRuntimeExtras`), so clock-/env-/RNG-free programs carry none of it.

### Deferred (honest)
- Process/subprocess control (spawn/exec); wall-clock/system-time formatting;
  cryptographically-secure RNG. (Buffered I/O + file metadata → v63.)

## [0.61.0] — Lazy iterator adaptor tower

### Added
- **Lazy iterator adaptors** `iter_take` / `iter_skip` / `iter_chain` /
  `iter_zip` / `iter_enumerate`, backed by stateful adaptor **structs**
  (`Take<I>`, `Skip<I>`, `Chain<A,B>`, `Zip<A,B>`, `Enumerate<I>`) that each
  `impl Iterator` and pull **one element at a time** from the wrapped iterator.
  A chain like `iter_take(iter_skip(range, 20), 5)` **fuses into a single pass**
  with O(1) extra memory and no intermediate `Vec` — only a terminal
  `iter_collect` allocates. A `take(skip(range(50_000_000), …), 5)` completes in
  ~10 ms (an eager adaptor would materialize a ~400 MB Vec). Plus a
  `vec_iter_i64` Vec→iterator bridge so `Vec<i64>`s feed the tower (ranges
  already `impl Iterator`). Pure-prelude Kardashev over the existing generic
  monomorphization — **no codegen changes**.
  CI-gated by `smoke_test_iter_lazy.sh` (7 cases: take∘skip, zip, enumerate,
  chain, vec-bridge, collect, and the 50M-range allocation-discipline proxy),
  with `smoke_test_iter.sh` staying green.

### Deferred (honest)
- **Element-generic adaptors.** The tower's element type is `i64` (and
  `(i64,i64)` for zip/enumerate). A fully element-generic tower needs
  `impl<T> Iterator<T> for Adaptor<T>` — a generic parameter as the trait's
  type argument — which the impl resolver rejects today (`unknown type: T`,
  because the impl's generic params aren't in scope when the trait-ref's type
  args resolve). Tracked for a later version.
- **The eager `vec_take` / `vec_skip` / … remain** unchanged (direct Vec→Vec)
  rather than being rewritten in terms of the lazy tower — the rewrite was
  deferred to avoid churn; both coexist.
- `fold` / `scan` / `flat_map` / `peekable` and `DoubleEndedIterator`; C-backend
  lowering of the lazy tower (outside the emit-c subset).

## [0.60.0] — Type & effect checker depth

### Fixed
- **Effect-row-variable / fn-typed-param name collision (soundness).** An
  effect-polymorphic higher-order **free** function whose fn-typed parameter
  shares a name with a top-level function was mis-charged that function's
  effects. The prelude `option_map(o, f: fn(i64)->i64 ! {e}) -> Option<i64> ! {e}`
  calls its parameter `f`; if a program also defined `fn f ! {io}` (or any other
  effect — `f`/`g` are extremely common names), the per-site effect set for the
  indirect call to the *parameter* came out empty and `collectEffects` fell back
  to the **top-level** `fn f`, so the program failed to compile with a spurious
  *"function 'option_map' uses effect `io` but does not declare it"*. The fix
  records such calls as **indirect** (callee resolved to a local binding) and
  never consults a same-named top-level schema for them — a local binding shadows
  any global. Effect propagation through the row var itself is unchanged (a pure
  caller of `option_map` with an `io` mapper is still correctly rejected).
  CI-gated by `smoke_test_effect_param_collision.sh` (5 accept JIT==AOT incl. a
  user higher-order fn + the `option_map` polymorphism path, 1 reject).

### Added
- **Type-inference depth regression suite** (`smoke_test_infer_depth.sh`, 12
  programs). The v60 roadmap entry targeted match-arm + nested-closure inference;
  investigation found the HM engine already handles these comprehensively (arm
  payloads, two-level enum nesting, closure params/returns, closures in
  lets/HOFs/captures, generic mappers, tuple destructuring, if-as-value). Rather
  than fabricate new inference code, this version **locks that behavior in** so a
  future refactor cannot silently weaken it. Each program leans on inference for
  a binding/param/return type that is never written down; differential JIT==AOT.

### Deferred (honest)
- **Never-type (`!`) / divergence typing.** A block whose tail is `panic(..)` /
  `break` / `continue` is still typed by the tail expression's nominal type
  (`panic` returns `i64`), not a bottom type that unifies with any branch. This
  blocks `let x = if c { v } else { panic(..) }` when the branches differ and is
  the remaining blocker for `let ... else { <diverge> }`. Tracked for a later
  version; out of scope for the targeted checker-soundness fix here.

## [0.59.0] — Ergonomics: struct-update spread

### Added
- **Struct-update syntax `S { x: 10, ..base }`** — fields not given explicitly are
  taken from `base`. This version supports a **Copy base** (a struct whose fields
  are all Copy — scalars / arrays / tuples): codegen byte-copies the base and
  overwrites the explicit fields (the base is consumed, per kardashev's struct
  move semantics). The base must be a value of the **same** struct. Implemented
  via a new `StructLitExpr.spread` field threaded through the parser, typecheck
  (`validateStructLitFields`: explicit fields ∪ base cover all; same-struct +
  all-Copy checks), codegen (`emitStructLit`: `ExtractValue` the missing fields),
  and the borrow/effects/clone walks.
  CI-gated by `smoke_test_struct_update.sh` (4 accept JIT==AOT + 2 reject:
  wrong-type base, move-field struct).

### Deferred (honest)
- **Move-field spread** (a base with heap fields) — needs partial-move-from-base
  + drop of the overwritten base fields; rejected cleanly for now.
- **Parameter destructuring** (`fn f(P { x, y }: P)`) — the roadmap's other half;
  it touches the pervasive `Param` struct and fn-entry codegen (higher blast
  radius). Struct-update spread is the self-contained, higher-leverage half;
  param-destructure is a follow-on.

## [0.58.0] — Ergonomics: `if let` / `while let`

### Added
- **`if let PAT = e { … } else { … }`** and **`while let PAT = e { … }`** —
  pattern-binding conditionals, desugared at **parse time** to the existing
  `match` lowering (no new typecheck or codegen):
  - `if let PAT = e { A } else { B }` → `match e { PAT => A, _ => B }` (a missing
    `else` is a unit else);
  - `while let PAT = e { BODY }` → `loop { match e { PAT => BODY, _ => break } }`
    (the scrutinee is re-evaluated each iteration; a non-match breaks the loop).

CI-gated by `smoke_test_if_let.sh` (7 cases, JIT==AOT: some/none/no-else/binding-
use for `if let`; drain / empty / accumulate for `while let`).

### Deferred (honest)
- **`let … else`** is **not** shipped. The desugar (a `match` whose `_` arm
  diverges) is sound, but a diverging `else` block that ends in `panic(..)` types
  as `()` rather than bottom — kardashev has no *never* type yet — so the else
  arm fails to unify with the bound value; and a `_ => return` arm trips a
  separate pre-existing effect-inference quirk with effect-polymorphic prelude
  functions. Both need a never-type / divergence-typing pass first (the roadmap
  flagged let-else as "the one non-trivial bit"). Tracked as a follow-on.

## [0.57.0] — Reference-returning functions (escape-gated)

### Added
- **Functions may now return references (`-> &T`).** kardashev previously
  blanket-rejected *every* user `-> &T` return ("cannot return a reference, no
  lifetime system yet") — a rule (PR#25) that predated the v0.52.0–v0.54.0 escape
  analysis. That analysis now precisely decides soundness, so the blanket rule is
  lifted (both the free-function and impl-method sites): a returned reference
  rooted in a **by-reference parameter, `&self`, or a global** outlives the call
  and is accepted; one rooted in a **local, a by-value parameter, or a
  temporary** is rejected by the borrow checker as a dangling reference. This
  unblocks accessor / `&self.field` methods (`fn getx(&self) -> &i64 { &self.x }`)
  and pass-through borrows (`fn id(r: &i64) -> &i64 { r }`). Raw-pointer returns
  (`-> *mut T`, unsafe-gated, no lifetime obligation) are likewise permitted.

CI-gated by `smoke_test_ref_returns.sh` (6 accept-and-run: param / `&self.field`
/ `&param.field` / chained pass-through / method receiver; 4 reject: `&local`,
`&temp`, `&by-value-param`, `&local.field`). Unit suites + the existing escape /
borrow tests stay green.

### Rescope note (honest)
- The roadmap's v57 was **Index/Deref operator overloading**. Implementing it
  revealed a hard prerequisite: `Index`/`Deref` methods return `&Self::Output`,
  which the blanket `-> &T` rejection forbade. This release ships that
  prerequisite (reference-returning functions) — a complete, sound capability on
  its own. The Index/Deref **operator sugar** (associated-type `Output` + `[]` /
  `*` dispatch through the existing operator-trait machinery, now expressible) is
  the documented follow-on.

## [0.56.0] — Soundness under concurrency: thread-local effect handlers

### Fixed (concurrency)
- **Two threads installing different handlers for the same effect no longer race
  a shared global.** The per-`(effect,op)` current-handler slot
  (`effectHandlerGlobal`) was a single process-global `InternalLinkage` global, so
  concurrent `handle … with` installs on different threads clobbered each other.
  The handler global is now **thread-local** (`GeneralDynamicTLSModel`) in AOT, so
  each thread reads/writes its own handler slot; the existing `handle`
  save/restore then mutates only the calling thread's storage.
- **JIT keeps process-global handlers** — `thread_local` lowers to
  `__emutls_get_address`, which the ORC JIT cannot resolve (same reason the panic
  stacks are process-global). JIT runs are single-threaded, so there is no race in
  practice. This is selected by a new `forJit` flag threaded from the driver into
  `codegen()` (the JIT execution path sets it; AOT / `--emit-llvm` leave it false).
  Single-thread effect behaviour is unchanged under both backends.

CI-gated by `smoke_test_thread_local_handlers.sh` (**AOT-only**): two threads
install different handlers for one effect and perform it 100 000× concurrently;
each thread's sum proves it saw **only its own** handler (100000 / 200000),
deterministic over 6 runs, MALLOC_CHECK-clean; the emitted IR shows the handler
global is `thread_local`. Existing `smoke_test_phase176.sh` /
`smoke_test_effect_exhaustive.sh` stay green (JIT + AOT).

### Deferred (honest)
- TSan CI gate (needs sanitizer-instrumented codegen). Multi-shot /
  continuation-capturing handlers (handlers stay tail-resumptive). A JIT-mode
  concurrent-handler path (TLS unavailable under ORC).

## [0.55.0] — Correctness: UTF-8-safe string casing + char API + built-in `Drop`

### Fixed (correctness)
- **`str_to_upper` / `str_to_lower` are now UTF-8-safe.** They iterated by *byte*
  and mapped only ASCII 97–122 / 65–90, so `str_to_upper("café")` left the `é`
  un-cased. They now iterate by **char** (`str_char_width_at` +
  `str_decode_char_at`), case-map the codepoint, and re-encode with the existing
  `str_push_char` codec — `str_to_upper("café") == "CAFÉ"`. `char_to_upper` /
  `char_to_lower` were extended from ASCII-only to the **Latin-1 Supplement**
  (à–þ ↔ À–Þ via ±32, with the ÷/×/ÿ↔Ÿ exceptions). Full Unicode case folding
  (Greek/Cyrillic/Latin-Extended, ß→SS) is deferred (needs a Unicode DB).

### Added
- **Char-indexed string helpers:** `str_split_char(&String, char)` (vs the
  existing by-substring `str_split`), `str_get_char(&String, i)` → `char`,
  `str_index_char(&String, char)` → `Option<i64>` (all char-boundary-correct).
- **`Drop` is now a built-in prelude trait** — `impl Drop for T` resolves
  *without* the user re-declaring `trait Drop` (it used to error "unknown trait
  Drop"). The drop glue (user destructor first, then reverse-field drop) has
  existed since Phase 16; this closes only the declaration gap. A user-declared
  `trait Drop` still wins (guarded). Method effect row is `! { io }` (matching the
  established convention); a drop needing other effects can declare its own trait.

CI-gated by `smoke_test_utf8_casing.sh` (the `café` bug case + 8 Latin-1
round-trips + the 3 helpers, JIT==AOT) and `smoke_test_builtin_drop.sh`. Existing
`smoke_test_drop.sh` / `smoke_test_strings.sh` stay green.

### Note
- `vec_reverse` was already in the prelude (the roadmap draft wrongly listed it
  as missing); not re-added.

## [0.54.0] — Soundness: store-into-out-parameter escape (escape-analysis trilogy complete)

### Fixed (memory safety)
- **A frame-local reference can no longer be stored into a place that outlives
  the call.** The v0.52.0 escape analysis guarded function *returns*; storing a
  local reference through a `&mut` out-parameter — `fn leak(out: &mut R) { let x =
  7; out.p = &x; }` — was unchecked, and `out.p` dangled into the freed frame
  after the call. The borrow checker now runs `checkStoreEscape` on every field /
  index / deref assignment: if the target place roots in a **reference
  parameter** (or a global) — i.e. it outlives this frame — and the stored value
  contains a reference rooted in a local, a by-value parameter, or a temporary,
  the store is **rejected** (`cannot store a reference … into a place that
  outlives this function …`). A store into a **local** place is still fine (it
  dies with the frame). Reuses the same `classifyRoot` / `escapesAggregateRef` /
  per-binding-provenance machinery as the return check.
  CI-gated by `smoke_test_field_ref_escape.sh` (6 reject incl. `&local`/`&temp`/
  `&by-val-param`/`&local.field`/`&mut self`/nested-aggregate, + 4 accept). This
  completes the escape-analysis trilogy: v0.52.0 (returns) → v0.53.0 (`&CONST`) →
  v0.54.0 (stores).

### Deferred (honest)
- **Aggregate-const promotion** (the other half of the roadmap's v54 entry) is
  *not* shipped here: promoting `&CONST_ARRAY` / `&CONST_STRUCT` to a stable
  global requires a new AST-initializer → `llvm::Constant` const-lowering path
  that does not exist yet, and the current behaviour is already **sound** —
  in-scope use works, and *returning* an aggregate-const borrow is correctly
  rejected (since v0.52.0). Promotion is a featureful addition, folded into a
  later stdlib version rather than rushed here.
- Stores into a longer-lived location other than a `&mut` parameter/global (e.g.
  through a chain of local reborrows) remain conservatively unanalyzed; full
  region inference is the deferred mega-track.

## [0.53.0] — Soundness + feature: `&CONST` promotion

### Fixed (memory safety) / Added
- **A borrowed scalar `const` is now a stable, returnable reference.** A
  top-level `const` is an inlined immediate with no address, so `&C` used to
  materialize a **frame-local temporary**: reading it in scope worked, but
  *returning* it (wrapped in a struct/tuple/enum) read freed stack — a
  dangling-reference UB orthogonal to, and missed by, the v0.52.0 escape
  analysis (which classified `&const` as a safe global). Codegen now **promotes**
  a borrowed scalar const to a deduplicated internal global, so `&C` is a genuine
  `'static` address: it reads correctly in scope **and** can be safely returned
  (e.g. `fn make() -> R { R { p: &C } }` now returns a live reference, not
  garbage). The escape checker's "this reference outlives the call" signal is
  keyed on the *same* condition (membership in the scalar-const set), so the two
  agree exactly.
- **Latent companion hole closed:** because the escape checker previously treated
  *every* unresolved `&ident` as a safe global, returning `&<nullary-enum>`
  (`&Nil`) or `&<aggregate-const>` — both frame-local temporaries — was also
  wrongly accepted. The classifier now treats a non-(scalar-const) `&ident` as a
  temporary, so those returns are correctly rejected with the
  "does not outlive this function" diagnostic. In-scope use of such borrows still
  works. CI-gated by `smoke_test_const_ref.sh` (6 accept-and-run + 2 reject).

### Known limitations (documented)
- **Aggregate consts are not promoted.** A borrowed array/struct/enum `const`
  remains a frame-local temporary: it works in scope but cannot be returned
  (rejected by the escape check, soundly). Promoting aggregate consts to globals
  (materializing their initializer) is a follow-on.
- Reference *stores* into out-parameters (`out.p = &local`) remain unchecked
  (v0.52.0 limitation, unchanged).

## [0.52.0] — Soundness: escape analysis closes a dangling-reference UB

### Fixed (memory safety)
- **A returned value may no longer carry a reference into freed stack.** The
  borrow checker rejected a top-level `-> &T` return but did **not** look inside
  aggregates, so a function returning a struct/tuple/enum/array that contained a
  reference to a **local** (e.g. `struct R { p: &i64 }  fn f() -> R { let x = 7;
  R { p: &x } }`) compiled clean and read freed memory at runtime — a silent
  dangling-reference UB in a language whose pitch is safety. The checker now runs
  a sound, conservative **escape analysis**: a function whose return type
  transitively contains a reference is rejected unless *every* contained
  reference roots in a **by-reference parameter** (or a global) — which outlives
  the call — never a local, a by-value parameter, or a temporary. It covers
  references wrapped through structs, tuples, enum payloads, **arrays**, and
  nested aggregates, and through `if` / `match` / `loop`-break / block-tail
  control flow, direct `return`s and the function-body tail, **method receivers**
  (`&self`), and **calls** (a `&local` nested in an aggregate argument or behind a
  ref-typed local is caught). A per-binding *provenance* pass lets the common
  `let r = <param-rooted>; … r` shape compile while still rejecting
  `let r = &local; … r`.

  Diagnostic: `cannot return a reference into a value that does not outlive this
  function …`. CI-gated by `smoke_test_escape_analysis.sh` (9 reject + 6
  accept-and-run). Built and validated against a multi-agent workflow: a 72-case
  labelled corpus plus an adversarial pass of 18 attackers — every one of the 29
  confirmed dangling-ref escapes it surfaced is now rejected, with no false
  positive on the accept corpus.

### Known limitations (documented honestly)
- **Inter-procedural precision.** A multi-argument call whose result roots in one
  ref argument but is passed *another* `&local` argument (e.g.
  `pick(&local, real)` where `pick` returns its 2nd arg) is conservatively
  rejected — sound, but a real lifetime system would accept it. Requires
  inter-procedural lifetime analysis (deferred).
- **Stores, not just returns.** Assigning a reference into a longer-lived
  aggregate (`out.p = &local` through an out-parameter) is still unchecked — a
  separate, narrower escape route (deferred).
- **`&CONST` is separately unsound** (orthogonal to this fix): a top-level
  `const` is an inlined immediate with no stable address, so `&C` yields a
  dangling pointer regardless of escape analysis. Tracked as its own issue.
- Slices/`Mutex`/atomics are ref-free Copy handles to this analysis (no `&T`
  field), so a slice viewing a local buffer is out of scope here. Raw pointers
  (`*const`/`*mut`) are `unsafe`-gated and carry no lifetime obligation.

## [0.51.0] — Performance: vectorization + codegen efficiency

A codegen-efficiency pass (no language-surface change). Driven by an
adversarially-verified multi-agent audit of the optimization pipeline, codegen
IR quality, the prelude, and memory layout; every fix below was measured or
correctness-gated, and over-rated audit findings were down-scoped honestly.

### Fixed / improved
- **Auto-vectorization now actually runs (the headline win).** The IR
  optimization `PassBuilder` was constructed **without a `TargetMachine`**, so
  the pipeline had a no-op `TargetTransformInfo` — the loop/SLP vectorizers and
  cost models ran target-blind and declined every vectorization. The host
  `TargetMachine` (created in `setHostDataLayout`) is now kept alive and passed
  to `PassBuilder`, registering real TTI. The `loop` benchmark (a 200M-iteration
  integer reduction) went from **2.2× C → 1.0× C (parity)**; emitted IR now
  contains vectorized loop bodies (0 → 21 vector ops). CPU stays **generic** —
  keeps the optimizer's layout identical to the backend's (guards the
  recursive-enum-read miscompile) and the emitted object portable.
- **Array indexing no longer spills the whole array.** `emitIndex` spilled the
  entire array value (`load [N x T]` + alloca + store) on every element read of
  any non-local array object; SROA does not undo this for large `N`, so an array
  **field** (`g.cells[j]`) read in a loop copied the whole array per access. It
  now takes the place address via `emitPlaceAddr` (used already for `a[i] = x`)
  and GEPs directly; genuine rvalue arrays (`f()[i]`) still spill correctly.
- **HashMap probe: modulo → bitwise AND.** The capacity is always 0 or a power
  of two (starts at 8, doubles), so `h mod cap` is exactly `h & (cap-1)` — the
  home-slot and all five probe-wrap sites now use a single AND instead of a
  hardware `idiv` (~20–40 cyc, not pipelined). Bit-identical results; all
  HashMap/HashSet smoke tests green.
- **Prelude scans now early-exit.** `vec_contains`/`vec_index_of`/`vec_any`/
  `vec_all`/`vec_find` and `str_index_of`/`str_starts_with`/`str_ends_with` were
  scanning the whole input after the answer was decided; they now `break` (or use
  a short-circuit loop guard) — O(n) → O(k) on an early hit. The `str_*`
  boundary cases (prefix/suffix longer than the string) are preserved without an
  out-of-bounds read and pinned by `smoke_test_prelude_earlyexit.sh`.

### Deferred / honest limitations
- Host-CPU/`-march=native` codegen (would unlock AVX-width vectors) is **not**
  enabled: it requires folding the CPU+features into the content-addressed AOT
  cache key (else an AVX object is served to an older CPU → SIGILL) and breaks
  cross-machine artifact portability — a future opt-in `-Ctarget-cpu=native`.
- Backend `CodeGenOptLevel` propagation to `emitObject` (a `-O0` compile-speed
  win, negligible runtime effect) and the quadratic macro-expansion rewrite
  (correctness-sensitive frame-stack refactor) are deferred.

## [0.50.0] — Roadmap v50 "6/6 BEYOND IV: statically-verified exhaustive effect handling" (partial)

### Added
- **Statically-verified exhaustive effect handling** — a user-defined
  (algebraic) effect must be discharged by a `handle … with E { … }` before it
  reaches the program entry point `main`. Performing an effect with no installed
  handler is undefined — at runtime it silently no-ops / returns garbage — and
  was previously accepted. Now the compiler reuses the (transitively sound)
  effect set: if `main`'s inferred effects still contain a **user** effect (a
  builtin effect — `io`/`alloc`/`panic`/… — legitimately reaches `main`), that
  effect escapes unhandled and the program is **rejected**, pinpointing the
  operation (`effect \`E\` is performed but never handled before reaching
  \`main\` (first performed as \`E::op\`)`). A `handle` that discharges the
  effect makes the program compile and run. CI-gated by
  `smoke_test_effect_exhaustive.sh` (accept: direct/callee/nested/deep-chain
  handled, JIT==AOT; reject: direct/callee/partial/deep-chain escape; plus a
  12-effect deep-nest robustness pair — all handled accepts, outermost-missing
  rejects).

  This closes a real soundness gap in the v32 algebraic-effects feature and is
  the tractable core of the v50 type-system capstone.

### Deferred / honest limitations
- The rest of v50's 6/6 work is XL/research-grade and largely needs tooling this
  environment cannot host (ROADMAP, v50): the mechanized soundness proof of the
  exhaustive-handling property (and of progress/preservation, drop-soundness, HM,
  NLL, effect-row subtyping) in a proof assistant checked in CI; effect
  exhaustiveness for thread-entry and `#[test]` roots (only synchronous `main`
  reachability is checked here); sub-100ms self-verifying incremental
  compilation; the unified query-backed IDE server; in-tree record-replay
  time-travel debugging; the mechanized test-linked spec; and the
  differentially-verified reproducible self-hosting bootstrap.

## [0.49.0] — Roadmap v49 "6/6 BEYOND III: compile-time reflection" (partial)

### Added
- **Compile-time reflection intrinsics** — `field_count!(S)`, `variant_count!(E)`,
  `size_of!(T)` reflect to an `i64` constant, and `type_name!(T)` to a `String`,
  all resolved at compile time against the program's static type information:
  - `field_count!` / `variant_count!` are computed by the typechecker from the
    resolved struct fields / enum variants (a wrong-kind type is rejected:
    `field_count!` requires a struct, `variant_count!` an enum);
  - `type_name!` yields the type's canonical display name;
  - `size_of!` is computed in codegen from the lowered type's **real LLVM
    DataLayout alloc size** (so `size_of!(i64)==8`, a `{i64,i64,i64}` struct
    `==24` — alignment-correct, not an approximation).

  Reflection results compose in ordinary expressions and are emitted as plain
  constants (zero runtime cost). This is the tractable core of the v49
  "typed AST-reflection API" — the unifying metaprogramming primitive. CI-gated
  by `smoke_test_reflection.sh` (13 cases, **JIT==AOT** differential + negatives).

### Deferred / honest limitations
- The rest of v49's 6/6 work remains XL/research-grade (ROADMAP, v49): the full
  field-iterating `TypeInfo` API (`for f in fields!(T)`), procedural macros as
  in-language `meta fn`s (quote/unquote) that build on reflection, the
  `--meta-audit` differential+soundness gate for all expansions, the
  deterministic record-replay + exhaustive-interleaving concurrency
  model-checker, the machine-checked memory model / verified scheduler, and
  refinement/dependent-lite types via a bundled SMT solver.

## [0.48.0] — Roadmap v48 "6/6 BEYOND II: per-function codegen-quality contracts" (partial)

### Added
- **Per-function codegen-quality contracts** — `#[codegen(no_alloc)]`,
  `#[codegen(no_panic)]`, and `#[codegen(no_io)]` are statically-verified
  guarantees about the emitted code. A contract is checked against the function's
  **transitively sound** effect set: if the function — or anything it calls —
  performs the forbidden effect (`alloc` / `panic` / `io`), compilation **fails**
  with a diagnostic naming the function and the violating effect. Contracts
  compose on one fn (`#[codegen(no_alloc, no_panic, no_io)]`), run in normal
  `kardc` (no special flag), and are CI-gated by `smoke_test_codegen_contracts.sh`
  (each contract proven to bite by a negative test, including a transitive-callee
  case; a `catch`-discharged panic correctly satisfies `no_panic`). This lets a
  hot path or a `no_std`/embedded function promise it never touches the heap, the
  panic runtime, or I/O — checked, not hoped. (A beyond-parity capability; the
  acceptance gate for the v48 "per-function codegen-quality contracts" phase.)

### Deferred / honest limitations
- The rest of v48's 6/6 work remains XL/research-grade (ROADMAP, v48): the
  `vectorized` codegen contract (needs vector-IR inspection post-lowering),
  async-as-an-effect-handler runtime unification, zero-allocation fusing iterator
  pipelines, derive-based serde + schema-evolution checking, machine-verified
  Big-O collection contracts, the deterministic replayable (`--sim`) executor,
  the always-on dual-bound perf-regression gate, universal C-backend reach across
  Windows/non-LLVM arches, and static deadlock-freedom. The contracts shipped
  here cover the three most useful effect-based guarantees and reuse the existing
  (sound) effect system rather than approximating it.

## [0.47.0] — Roadmap v47 "6/6 BEYOND I: verified safety + totality" (partial)

### Added
- **Totality via `#[total]`** — a checked termination assertion. A sound
  conservative call-graph analysis accepts a fn only if it (and every fn it
  transitively calls) is loop-free and the reachable call graph is acyclic (no
  recursion, incl. mutual). `for`-over-a-range is bounded and fine. A
  non-terminating-or-recursive fn declared `#[total]` is rejected, naming the
  cause. (A 6/6 beyond-parity capability — few production languages check
  termination.)

### Deferred / honest limitations
- The rest of v47's 6/6 work remains research-grade (ROADMAP, v47): a first-class
  `! { div }` divergence effect row + a halting oracle over fuzzed inputs, the
  Miri-style UB interpreter gating `unsafe` in CI, and continuous adversarial
  memory-safety fuzzing through a triple oracle. The current checker is
  conservative (a terminating `while` is still rejected under `#[total]`).

## [0.46.0] — Roadmap v46 "Tooling + conformance + stability + security → 1.0 surface" (partial)

### Added / fixed
- **Parser DoS fix** — deeply-nested adversarial input (`(((…`, `[[[…`,
  `Vec<Vec<…>>`, `&&&…`, `----…`, `!!!!…`) stack-overflowed the recursive-descent
  parser; now bounded by a recursion-depth guard reporting a clean diagnostic.
- **Compiler-hardening fuzzer** (`smoke_test_compiler_fuzz.sh`) — 266 adversarial
  inputs (curated deep-nesting/malformed + random token soup) through the
  front-end, asserting ZERO crashes (a signal exit fails CI).
- **SECURITY.md** — coordinated security-response policy (private reporting,
  ack/triage/fix SLA, embargo) + two-surface threat model.

### Deferred / honest limitations
- The rest of v46 remains (ROADMAP, v46): LSP semantic tokens / inlay hints /
  code actions + workspace rename, the DWARF debugger (needs a gdb/lldb env) +
  the `--emit-c -g` floor, doctests + hosted docs site, the conformance
  pass-rate gate, the SemVer/MSRV stability checker, and the ≥100k-input nightly
  fuzz_compiler.

## [0.45.0] — Roadmap v45 "Ecosystem foundation: registry, toolchain, spec" (partial)

### Added
- **Normative language spec** (`docs/SPEC.md`) — the EBNF grammar + load-bearing
  normative clauses (effects, ownership/borrow/drop, panic, overflow, C ABI,
  object-safety) with stable `[K-xxx]` ids, describing the language as `kardc`
  accepts it today. Grounded by `smoke_test_grammar_conformance.sh` (20
  well-formed programs compile, 8 ill-formed rejected). Honest: it corrected a
  wrong assumption — match guards `pat if cond =>` are roadmapped, not yet
  implemented (only or-patterns landed); the grammar reflects that.

### Deferred / honest limitations
- The rest of v45 is ecosystem infra this sandbox can't host/verify (ROADMAP,
  v45): the hosted package registry + `kard publish`, the `kardup` toolchain
  manager, the manifest resolver/lockfile + MSRV enforcement, the
  >=2000-program EBNF-conformance generator, and the salsa-style query engine.

## [0.44.0] — Roadmap v44 "Backends & platforms: perf, cross, WASM, Windows" (partial)

### Added
- **Application-scale benchmark suite** — `primes` (trial division) + `matmul`
  (64×64 flat-array int matmul) added to the output-gated bench harness (vs
  `clang -O2`). Honest finding: kardashev is **~1.07× C on `primes`** (inside
  the 1.1× parity target) and ~1.0× on `collatz`; the ~2.2× figure is specific
  to the trivial `loop` micro-bench. `matmul` is correctness-only (clang
  constant-folds the deterministic result). See BENCHMARKS.md.

### Deferred / honest limitations
- The rest of v44 remains (ROADMAP, v44): alloca-free-counter / signed-div
  strength-reduction codegen, LTO/PGO, cross-compilation + per-target std, and
  the **WASM / Windows / freestanding backends** — each needs a wasmtime / wine
  / qemu environment to differentially verify, which this sandbox lacks. The
  hard application-perf ≤1.1× CI gate needs a stable bench machine.

## [0.43.0] — Roadmap v43 "Metaprogramming parity + regex + typed/multishot effects" (partial)

### Added
- **Built-in helper macros** — `stringify!` (tokens -> String), `concat!`
  (join literals -> String), `count!` (arg count -> i64), and `cfg!(pred)`
  (-> bool of the #[cfg] predicate against `--cfg` flags). Parser-desugared at
  compile time, like format!/println!.

### Deferred / honest limitations
- The rest of v43 remains (ROADMAP, v43): macro hygiene (gensym/syntax
  contexts), nested repetition + metavar-after-repetition, span-accurate macro
  diagnostics, full comptime (const trait dispatch / const collections), a
  linear-time regex engine, and the async-effects 6/6 beyond work (typed effect
  rows end-to-end + multi-shot resumptions).

## [0.42.0] — Roadmap v42 "Stdlib depth I" (partial)

### Added
- **`Duration`** — a milliseconds time span with operator-overloaded arithmetic
  (v37 Add/Sub traits), Ord comparison, and conversions
  (from_millis/from_secs/as_millis/as_secs). Deterministic + unit-testable.

### Deferred / honest limitations
- The rest of "Stdlib depth I" remains (ROADMAP, v42): balanced-tree BTreeMap
  (vs the current sorted-Vec), leak-free interior-Drop HashMap, lazy iterator
  adaptors, buffered I/O + stdin/files/env (Phase 189), a real monotonic clock
  (timespec FFI), networking, and the observability facade — each runtime-heavy
  or needing global state / a clock FFI.

## [0.41.0] — Roadmap v41 "Memory safety, parity complete + unsafe surface" (partial)

### Added
- **Deref-assignment `*p = v`** — write through a `&mut T` (safe) or a `*mut T`
  raw pointer (`unsafe`), plus `*box = v`. Retires the long-standing
  "deref-assign unsupported language-wide" gap. Writing through `&T`, or a raw
  `*p=v` outside `unsafe`, is a clear error.
- **`copy_nonoverlapping(src: *const T, dst: *mut T, n: i64)`** — a memcpy of n
  ELEMENTS between raw pointers (unsafe; pointee-type checked; element stride
  from the host DataLayout).

### Deferred / honest limitations
- Lifetime params + **real region inference** (the XL NLL rearchitecture — the
  sound NLL-lite position-counting check stays) and reducing the
  intentional-leak allowlist to only the Arc/Rc-cycle fixtures (recursive
  Future-drop) remain (ROADMAP-1.0-AND-BEYOND.md, v41). The Miri-gate + formal
  proof are the 6/6 work (v47/v50).

## [0.40.0] — Roadmap v40 "Parallel executor & structured concurrency" (partial)

The concurrency capstone. Its headline — the multi-threaded work-stealing
executor (the deferred Phase 174) — is genuinely XL and environment-bound
(its "race-free / deterministic-over-200-runs" gates need a ThreadSanitizer CI
job + a macOS kqueue environment). This release ships the tractable,
locally-verifiable structured-concurrency primitive.

### Added
- **Cooperative cancellation token** — `cancel_token_new()` is a shared
  Send+Sync `AtomicBool` flag (Copy handle → passing it by value to a worker
  thread shares the same cell). `cancel(t)` (from any thread) requests
  cancellation; cooperating workers/loops poll `is_cancelled(t)` and stop.
  Works single-threaded (deterministic) and across real OS threads (a worker
  shares the token, observes main's cancel, finishes, join returns). This is
  the primitive structured cancellation builds on.

### Deferred / honest limitations
- The v40 headline remains (tracked in ROADMAP-1.0-AND-BEYOND.md, v40): the
  multi-threaded work-stealing executor (deferred Phase 174), borrow-capturing
  scoped threads, blocking multi-channel select via a shared waker (retiring
  poll-backoff), and async scope + recursive cancel-drop. These need a
  core-runtime rewrite plus a TSan CI job + a macOS kqueue environment to
  verify — none exercisable in this sandbox; the executor stays single-threaded.

## [0.39.0] — Roadmap v39 "FFI maturity, no_std & async parity I" (partial)

The systems-language unblocker version. Most of its phases are large or
environment-bound (several were deferred in v31/v33); this ships the tractable,
locally-verifiable FFI slice.

### Added
- **Raw-pointer arithmetic + write** (retires part of the Phase 177 deferral) —
  `ptr_offset(p: *const/*mut T, n: i64)` advances a raw pointer by n ELEMENTS
  (GEP by pointee type), and `ptr_write(p: *mut T, v: T)` stores through a
  `*mut` (raw write, since `*p = v` deref-assign is unsupported language-wide).
  Both are unchecked and require `unsafe`; with the existing `*p` raw deref,
  raw pointers now support read + write + arithmetic. A C-style "sum an array
  via a moving pointer" loop works.

### Deferred / honest limitations
- The rest of v39 remains (tracked in ROADMAP-1.0-AND-BEYOND.md, v39):
  `repr(C)` struct-by-value + C callbacks across `extern "C"`, a `kard bindgen`
  header importer, `no_std`/freestanding + pluggable `GlobalAlloc`, generic
  `thread_join<T>`, the kqueue/poll cross-platform reactor, blocking multi-wait
  select + async Mutex/RwLock, recursive Future-drop, and HRTB
  (`for<'a>`)/let-generalization. Several need core-runtime rewrites or a
  macOS/embedded environment to verify.

## [0.38.0] — Roadmap v38 "The type system, completed I (lifetime spine)" (partial)

The load-bearing type-system version. Its headline pieces are genuinely
multi-month type theory; this release ships the tractable, verifiable core and
honestly defers the rest.

### Added
- **Object-safety (dyn-safety) completeness** — `dyn Trait` now enforces the
  full classic rules: in addition to rejecting static (no-`self`) methods
  (Phase 11), a method that RETURNS `Self` by value or takes a `Self`-by-value
  (non-receiver) PARAMETER makes the trait non-object-safe, with a diagnostic
  naming the offending method/parameter. Object-safe traits dispatch correctly
  through `&dyn`; `&Self` / `Self::Assoc` returns and params stay fine.

### Deferred / honest limitations
- The rest of v38 is NOT in this release — it is multi-month type-theory work:
  **named lifetimes + region inference (NLL)** (an XL borrow-checker
  rearchitecture; kardashev keeps its sound NLL-lite position-counting check
  meanwhile), **full GATs** (bounded-Self / generic-param projection),
  **variance inference**, and **where-clauses on associated-type projections**
  + **supertrait `dyn` upcast**. Tracked in ROADMAP-1.0-AND-BEYOND.md (v38).

## [0.37.0] — Roadmap v37 "Foundations & unblockers" (post-1.0-roadmap, batch 1)

First batch of the **Road to 1.0 and Beyond** (ROADMAP-1.0-AND-BEYOND.md) —
the cheap, dependency-free wins other phases stand on. Each is real + tested
(differential JIT vs AOT or runner-verified).

### Added
- **Full operator-trait surface** — operator overloading (Phase 184's
  Add/Sub/Mul/Div) extended to the binary `%` (Rem) + bitwise/shift family
  (BitAnd/BitOr/BitXor/Shl/Shr) and the UNARY operators Neg (`-x`) and Not
  (`!x`). New unary-operator machinery (`unaryOpMethod`) mirrors the binary
  path; primitives keep their built-in ops; a missing impl is a clear error.
- **Turbofish** — explicit generic type arguments on calls (`id::<i64>(x)`,
  `pair::<i64, bool>(a, b)`); the type checker binds the callee's generic
  params positionally (constraining inference, which still works when
  omitted). Too-many / conflicting args diagnose. Unblocks where-clauses /
  GATs work where inference is insufficient.
- **Real test framework** — `assert!` / `assert_eq!` / `assert_ne!` prelude
  macros (over the Phase 182 macro engine), plus `kardc --test --filter
  <substr>` and `--test --format=json`; exit 0 iff all pass.

### Deferred / honest limitations
- The remaining v37 items are NOT in this release: the **sanitizer + TSan CI
  gates** need kardc to emit sanitizer-instrumented code (real codegen work,
  not just CI YAML); the **panic strategy** (panic=unwind/abort + catch_unwind
  + FFI unwind-boundary), **complete-borrow-check** (mut-2nd-arg / field
  reborrows), a selectable **overflow-trap policy**, and the
  **benchmark-regression harness** are each L/threading-heavy. Tracked in
  ROADMAP-1.0-AND-BEYOND.md (v37). Operator Index/Deref/custom-Output also
  remain (need lvalue/autoderef/assoc-type support).

## [0.36.0] — Roadmap v36 "tooling & compiler performance" (Phases 192, 194, 196)

Theme: developer-facing tooling and a concrete codegen-performance win. Each
shipped phase is independently verifiable (LSP over stdio JSON-RPC, Markdown
output, IR inspection).

### Added
- **LSP `textDocument/documentSymbol`** (Phase 192) — the file outline: the
  server advertises `documentSymbolProvider` and returns the user's top-level
  `fn` / `struct` / `enum` decls with their LSP `SymbolKind` and position
  (parsed raw — no prelude noise). Editors get outline / breadcrumbs / go-to-
  symbol.
- **`kardc --doc`** (Phase 194) — generates Markdown API documentation from a
  file's top-level declarations and their `///` doc comments: rendered
  signatures (visibility, generics, parameter types, return type), struct
  fields, and enum variants. Prelude items are excluded.
- **Bounds-check elision** (Phase 196) — when an array index is a compile-time
  constant provably in `[0, len)`, codegen emits no runtime bounds check (no
  compare / branch / panic block); a runtime index keeps its check and an
  out-of-range constant is still caught. A concrete step toward closing the
  codegen-performance gap.

### Deferred / honest limitations
- **Phase 193 (debugger story — validated gdb/lldb + pretty-printers +
  backtraces)** and **Phase 195 (incremental compilation — query caching)** are
  NOT in this release: 193 needs a gdb/lldb environment to validate (not
  deterministically testable in this sandbox), and 195 is a large query-engine
  rearchitecture (the content-addressed AOT cache already covers whole-program
  reuse). Tracked in ROADMAP.md.
- doc-gen emits the structured Markdown; a hosted docs site + executable
  doctests are future work. The remaining LSP features (semantic tokens, code
  actions, inlay hints) and broader perf work (regalloc, inlining, LICM, LTO)
  remain.

## [0.35.0] — Roadmap v35 "stdlib depth: collections, iterators, errors & random" (Phases 187-191)

Theme: broaden the standard library — ordered collections, a fuller iterator
surface, an error-handling ecosystem, and a seeded PRNG. Almost all of it is
written in kardashev itself (in the prelude, over the `Vec` primitive and the
existing traits), demonstrating the language is now expressive enough to grow
its own stdlib. Every phase is differentially gated (JIT vs AOT).

### Added
- **Ordered collections + a deque** (Phase 187) — `VecDeque<T>` (a two-stack
  double-ended queue, O(1) amortized at both ends; pops return `Option<T>`),
  `BTreeMap<K: Ord, V>` (an ordered map kept as parallel sorted Vecs, binary
  search, ascending-key iteration — the property HashMap lacks; works for i64
  and String keys), and `BTreeSet<T: Ord>` (ordered set, dedup on insert). All
  over the `Vec` primitive and the existing `Ord` trait — no new builtins.
- **Iterator-adaptor / reducer completeness** (Phase 188) — `vec_take` /
  `vec_skip` / `vec_chain` / `vec_zip` (-> `Vec<(A,B)>`) / `vec_enumerate`, the
  reducers `vec_sum` / `vec_any` / `vec_all` / `vec_find` / `vec_min` /
  `vec_max`, and `iter_collect<T, I: Iterator<T>>` which drains ANY value
  implementing the `Iterator` trait (e.g. a Range) into a Vec — the lazy→eager
  bridge.
- **Error-handling ecosystem** (Phase 190) — an `Error` trait
  (`fn message(&self) -> String`); generic `result_is_err` / `result_ok`
  (`-> Option<T>`) / `result_err` (`-> Option<E>`) / `result_map_err`; and
  **`?`-with-`From`**: a `?` on a `Result<_, E1>` inside a fn returning
  `Result<_, E2>` now converts the error via `E2::from(e1)` when an
  `impl From<E1> for E2` exists, instead of being a hard type error
  (a same-type `?` is unchanged; a mismatch with no `From` impl is a clear
  diagnostic).
- **Seeded PRNG** (Phase 191) — `Rng`, a deterministic 64-bit LCG
  (`rng_new` / `rng_next` / `rng_below` / `rng_range` / `rng_bool`) plus a
  Fisher-Yates `vec_shuffle<T>`. Seeded ⇒ reproducible ⇒ unit-testable, and
  identical under JIT and AOT.

### Deferred / honest limitations
- **Phase 189 (buffered I/O, stdin streams, file seek, full process/env)** is
  NOT in this release: it is runtime/FFI-heavy and largely non-deterministic to
  test in CI. Tracked in ROADMAP.md.
- These collections are eager and Vec-backed: `BTreeMap`/`BTreeSet` are sorted
  vectors (O(log n) lookup, O(n) insert), not balanced trees; the iterator
  adaptors are eager (materialized Vecs), not Rust's lazy adaptor structs
  (`iter_collect` is the lazy→eager bridge). Reference-returning helpers
  (`get` / `key_at`) return owned values via a `Clone` bound (no lifetime
  system). `?`-with-`From` supports one `From` impl per error type. Wall-clock
  time and (de)serialization (serde-like) remain future work.

## [0.34.0] — Roadmap v34 "metaprogramming: macros, derive & comptime" (Phases 182-186)

Theme: give the language the tools to abstract over syntax and shift work to
compile time — declarative macros, user-defined derives, operator overloading,
richer `const fn` evaluation, and conditional compilation. Every phase is
differentially gated (JIT vs AOT).

### Added
- **Declarative `macro_rules!` macros** (Phase 182) — a real token-level macro
  system. `macro_rules! name { (matcher) => { body }; … }` defines rules, and
  `name!( … )` / `name![ … ]` / `name!{ … }` invocations are rewritten into the
  first matching rule's body before parsing, so a macro can expand in
  expression, statement, OR item position. Supports multiple rules (selected by
  shape), fragment metavariables (`$x:expr | ident | literal | ty | pat | tt |
  …`), one level of repetition `$( … )sep* / + / ?` in both matcher and body,
  and recursion (a variadic `sum!` reduces one element per re-invocation). A new
  `$` token carries metavariables. The built-in format macros (`format!` /
  `println!` / `print!`) are untouched and compose with user macros.
- **User-defined `#[derive(...)]`** (Phase 183) — a library author writes a
  custom derive as a `macro_rules! derive_Foo` whose matcher destructures the
  item (e.g. `struct $name { $($f:ident : $t:ty),* }`) and whose body emits an
  `impl`; `#[derive(Foo)]` then synthesizes the expansion automatically. User
  and built-in derives (Clone/Eq/Debug/…) compose on the same attribute. The
  macro matcher is now recursive over delimiter groups, which also enables
  map-literal-style macros (`m!{ k => v, … }`).
- **Operator overloading** (Phase 184) — a user type opts into `+` / `-` / `*`
  / `/` by implementing the prelude `Add` / `Sub` / `Mul` / `Div` trait
  (`fn add(self, rhs: Self) -> Self`); the binary operator desugars to the
  method. Operator traits are pure (effect-free), so an `impl` body is pure too.
- **Richer comptime / `const fn`** (Phase 185) — a `const fn` can now use the
  imperative `let mut … ; while … { … }` style with variable reassignment and
  early `return`, all evaluated at compile time (iterative factorial /
  fibonacci, running sums — usable as `const` values and array lengths). A
  non-terminating const loop fails against the global step budget instead of
  hanging the compiler.
- **`#[cfg(...)]` conditional compilation** (Phase 186) — items can be gated on
  build flags set with `--cfg NAME` / `--cfg key=value`. Predicates: a bare
  flag, `not(…)`, `all(…)`, `any(…)`, and `key = "value"`. A disabled item is
  dropped during parsing (before type checking, so it may even reference
  undefined types). Active flags fold into the AOT cache key.

### Deferred / honest limitations
- **Macro hygiene** is not implemented — expansions are unhygienic (avoid
  capturing identifiers); nested repetitions and a metavariable in the matcher
  *after* a repetition are rejected with a clear error (never miscompiled).
- **`#[cfg]` from a `kard.toml` `[features]` table** — the `--cfg` mechanism is
  the engine; auto-feeding it from a manifest section is a thin follow-on.
- Operator overloading is homogeneous (`Self`-typed `rhs` and result); `Index` /
  `Deref` / `Neg` and heterogeneous / custom-`Output` operators are deferred.

## [0.33.0] — Roadmap v33 "systems-grade: FFI, `unsafe` & overflow control" (Phases 177-181)

Theme: the systems-programmer escape hatch. Raw pointers + `unsafe`, a more
mature C FFI surface, and explicit integer-overflow control. Every phase is
differentially gated (JIT vs AOT); the FFI phase is verified against real
libm/libc.

### Added
- **Raw pointers + `unsafe` blocks** (Phase 177) — `*const T` / `*mut T` raw
  pointers (NOT borrow-checked, nullable, lowering to the same opaque pointer as
  `&T`; a `&T` never unifies with a `*const T`) and `unsafe { … }` blocks. Create
  a raw pointer from a reference (`&x as *const T`, safe), dereference-READ it
  inside `unsafe` (a deref outside is a type error), and cast reference↔rawptr
  (no-op) / rawptr↔integer-address (`ptrtoint` / `inttoptr`). `effect` / `handle`
  / `with` / `perform` / `unsafe` are contextual keywords, so existing
  identifiers (a task `handle`, …) keep working.
- **FFI maturity — scalars + pointers** (Phase 178) — `extern "C"` signatures,
  which were limited to i32/i64/bool/&String, now also accept f64 / f32 (C
  double / float), the full integer width tower (i8..i64 / u8..u64), and (via
  Phase 177) `*const T` / `*mut T` as a C pointer. This covers the bulk of real C
  interop — libm math and the pointer-taking libc/buffer APIs — verified end to
  end against real `sqrt`/`pow`/`memset`/`memcpy`/`abs`.
- **Overflow-checked + wrapping arithmetic** (Phase 181) — the integer-overflow
  policy is documented (the DEFAULT is 2's-complement WRAP, `-fwrapv`) and joined
  by explicit opt-in operators: `checked_add/sub/mul/div(a, b) -> Option<i64>`
  (`None` on signed overflow / div-by-zero / `INT_MIN / -1`) and
  `wrapping_add/sub/mul(a, b) -> i64`. Overflow is detected with portable
  sign-bit identities / a 128-bit widen-and-compare (no version-fragile
  `*.with.overflow` intrinsics).

### Deferred / honest limitations
- **Phase 179 (`no_std` / freestanding + a pluggable allocator)** and **Phase
  180 (inline asm + SIMD intrinsics)** are NOT in this release. A pluggable
  global allocator means rerouting the core libc malloc/free/realloc path (a
  risky change to every heap type), full `no_std` conflicts with the
  libc-dependent prelude runtime (print/String/Vec), and inline asm / SIMD are
  platform-specific and not portably verifiable here. Tracked in ROADMAP.md as
  future systems work (alongside Phase 174's multi-threaded executor).
- Raw-pointer WRITE (`*p = v`) needs deref-assignment (unsupported
  language-wide); pointer ARITHMETIC; struct-by-value / callbacks / bindgen
  across `extern "C"` (the harder FFI-maturity pieces) — all deferred.

## [0.32.0] — Roadmap v32 "async & effects, matured (differentiator II)" (Phases 172-176)

Theme: take the two features that most distinguish kardashev — its async runtime
and its zero-cost effect system — from "they exist" to "they compose." Future
combinators + a type-safe task API, async cancellation/timeouts, effect
subtyping, and the research-frontier headline: user-defined **algebraic effects
with handlers**. Every phase is differentially gated (JIT vs AOT) and the
heap/leak-sensitive ones under `MALLOC_CHECK_`.

### Added
- **Future combinators + a type-safe task API** (Phase 172) — four
  compiler-synthesized combinator futures: `future_map<T,U>(Future<T>,
  fn(T)->U)` , `future_and_then<T,U>(Future<T>, fn(T)->Future<U>)` (monadic
  bind), `future_join2<A,B>(Future<A>, Future<B>) -> Future<(A,B)>` (wait-all),
  and `future_select<A,B>(…) -> Future<Either<A,B>>` (wait-any, drops the loser)
  — plus a new prelude `enum Either<A,B> { Left, Right }`. The combinators thread
  the continuation's effects to the call site via the existing effect-row var
  (`future_map` of a pure closure is pure; of an `io` closure is `io`). And
  **`JoinHandle<T>`**: `spawn` now returns a move-only, result-typed handle that
  `join` consumes — so double-joining (a double free) is a compile error.
- **A pre-existing codegen bug, fixed** (surfaced by Phase 172) — malloc sizes
  were baked with LLVM's default DataLayout (i64 under-aligned to 4) while
  StructGEP offsets lower against the host layout (i64 align 8), so a
  `Poll<multi-payload-enum>` slot was under-allocated by 8 bytes — an 8-byte heap
  overflow on `block_on` of such a future (also hit `block_on(async fn ->
  Result/Option)`). The host DataLayout is now pinned before the codegen walk.
- **async timeouts + cancellation** (Phase 173) — `timeout<T>(Future<T>, ms) ->
  Future<Option<T>>` races a future against an internal `sleep_ms` timer
  (`Some(v)` if it finishes first, `None` on timeout); `task_cancel<T>(
  JoinHandle<T>)` retires + releases a spawned task (and consumes the handle, so
  a cancelled task can't be joined). With `future_join2` these are the
  structured-concurrency primitives.
- **Effect subtyping** (Phase 175) — a function value that performs FEWER effects
  is now usable where one with MORE effects is expected (subsumption): a pure
  `fn()->R` coerces into a `fn()->R ! {io}` parameter. One-way and sound (an
  actual that does more than expected is still rejected); the `! {e}` effect-row
  threading of `vec_map`/`future_map` is unchanged.
- **User-defined effects + effect HANDLERS — algebraic effects** (Phase 176, the
  headline) — `effect E { fn op(a: A) -> R; … }` declares an effect and its
  operations; `perform E::op(args)` invokes the dynamically-current handler and
  RESUMES at the call site with its result; `handle { body } with E { op(p) =>
  hbody, … }` installs handlers for the body's dynamic extent and DISCHARGES `E`
  from the body's effect row (the way `catch` clears `panic`). Handler arms
  desugar to by-reference-capturing closures, so multiple arms share live
  handle-scope state — a `State` effect's `get`/`put` operate on one cell. This
  is the **tail-resumptive / dynamically-scoped subset** (reader, state, logging,
  dependency injection), implemented over a per-(effect,op) current-handler
  global with save/restore. `effect`/`handle`/`with`/`perform` are CONTEXTUAL
  keywords (a variable named `handle` still works).

### Deferred / honest limitations
- **Phase 174 (multi-threaded work-stealing executor + macOS `kqueue`)** is NOT
  in this release. The async executor remains single-threaded (cooperative) with
  an `epoll` reactor on Linux; a parallel work-stealing executor and a `kqueue`
  reactor are substantial, separately-verifiable future work and are tracked in
  ROADMAP.md.
- **Algebraic effects** ship as the tail-resumptive subset only: no non-tail
  resume, no multi-shot resume, no abort-without-unwind, and a `handle` body must
  not `return` through the handle. `future_select`/`timeout`/`task_cancel` drop a
  loser/cancelled task SHALLOWLY — a mid-flight async-fn loser leaks its nested
  in-flight sub-frame (memory-safe; a recursive `Future`-drop is future work).

## [0.31.0] — Roadmap v31 "concurrency, hardened (differentiator I)" (Phases 167-171)

Theme: take the concurrency story from "structural Send + a type-erased i64
`Mutex` + i64-only threads" to a hardened, modern surface — real `Send`/`Sync`
marker traits, RAII lock guards + `RwLock`, real lock-free atomics, channel
`select` + scoped threads, and atomically-refcounted `Arc`/`Weak`. Every phase
is differentially gated (JIT vs AOT, and the concurrent ones by a
deterministic-over-N-runs stress oracle — a lost update / data race fails
flakily, which repeating catches; the RAII/refcount ones also under
`MALLOC_CHECK_`).

### Added
- **Real `Send`/`Sync` marker traits** (Phase 167) — `Send` and `Sync` are now
  declarable zero-method marker traits (in the prelude), auto-derived
  structurally, manually grantable (`impl Send for Opaque {}`), and opt-out-able
  via a new negative-impl syntax `impl !Send for T {}`. A marker oracle consults
  explicit positive/negative impls and otherwise falls through to the
  (byte-identical) structural rule — so the three live enforcement sites
  (`chan_send` value, `mutex_new` cell, by-value `thread_spawn` capture) are now
  overridable. Fixes a latent gap: `char` (a Copy scalar) is now `Send`/`Sync`.
  Zero runtime cost.
- **Type-safe `RwLock<T>` + RAII lock guards** (Phase 168) — a new reader/writer
  lock (`pthread_rwlock_t`-backed, mirrors `Mutex`) plus move-only RAII guards
  `MutexGuard<T>` / `RwLockReadGuard<T>` / `RwLockWriteGuard<T>` that auto-release
  the lock on `Drop` (the scoped-lock pattern, à la C++ `lock_guard` /
  `shared_lock`). `RwLock`'s cell is `Send`-gated like `Mutex`'s.
- **Atomics + CAS + memory orderings** (Phase 169) — `AtomicI64` / `AtomicBool`
  (Copy `Send`+`Sync` handles) with `load`/`store`/`swap`/`fetch_add`/`sub`/`and`
  /`or`/`xor`/`compare_exchange`, lowered to real LLVM `atomicrmw`/`cmpxchg`/
  atomic-load/store/`fence`. The memory ordering is baked into the op name so the
  LLVM `AtomicOrdering` is a compile-time constant; an ergonomic
  `enum Ordering { Relaxed, Acquire, Release, AcqRel, SeqCst }` + `impl` layer
  (prelude) dispatches to the statically-named builtins. (`--emit-c` refuses
  atomics — the LLVM path is the oracle.)
- **Channel `select` + scoped threads** (Phase 170) — `select2`/`select3`/
  `select4(&r0,..)` block (poll-with-backoff) until one of N homogeneous
  `&Receiver<T>` is ready, returning a prelude
  `SelectResult<T> { Ready(idx, value), Closed(idx) }`. Scoped threads: a
  move-only `Scope` (`scope_new` / `scope_spawn(&s, f)`) whose `Drop` JOINS every
  thread it spawned — the roadmap's "all threads join before the scope ends", via
  RAII. (True cross-thread *borrow* capture is deferred — it needs a
  region/lifetime system; workers capture by value as `thread_spawn` does.)
- **`Arc<T>` / `Weak<T>`** (Phase 171) — atomically reference-counted shared
  ownership: a pointer to `{ i64 strong, i64 weak, T value }` with atomic
  refcounts (clone Relaxed, drop Release + an Acquire fence on the last strong;
  value dropped at strong==0, block freed at weak==0). `Weak<T>` is a non-owning,
  upgradable handle (`weak_upgrade -> Option<Arc<T>>` via an atomic CAS loop).
  Unlike `Rc`, `Arc`/`Weak` ARE `Send`+`Sync` when `T` is (`Send`+`Sync`) — the
  answer to "share owned data across threads" without lifetimes. Capturing an
  `Arc` into a thread clones it. Proven atomic by a 4-thread × 50k clone+drop
  stress (final `strong_count == 1`).

### Deferred / honest notes
- **Generic `thread_join<T>`** (the non-i64 thread-result half of Phase 171) is
  deferred to a follow-on: it rewrites the core OS-thread runtime (per-`T`
  control block + trampoline) the whole v31 test surface depends on. OS threads
  still return `i64` (the sound current behavior); `Arc<T>` is the shipped half.
- `select` is poll-with-backoff (a true blocking multi-channel wait needs a
  shared-condvar ABI change). Scoped threads deliver join-before-scope-end but
  not borrow-capture (needs lifetimes).

### Tests
- New smoke targets `smoke_test_phase167`–`171` (JIT-vs-AOT differential; the
  concurrent ones deterministic-over-N-runs + `MALLOC_CHECK_`). Unit:
  `typecheck_test` 311 → 316, `parser_test` 138 → 139.

## [0.30.0] — Roadmap v30 "the C backend, finished II (heap + RAII + generics)" (Phases 162-166)

Theme: take the `--emit-c` C-source backend (v23/v29) from the i64/bool/struct/
enum/ref/control subset all the way to the heap + RAII + the generic surface,
each phase differentially gated against LLVM (and the memory-safety phases ALSO
gated by an AddressSanitizer + LeakSanitizer oracle — a leak/double-free/stack-
use-after-scope signal the exit-code gate can't see).

### Added (C backend, `kardc --emit-c`)
- **`String` + heap strings** (Phase 162) — a faithful C `struct kdstr { char*
  data; int64_t len; int64_t cap; }` runtime (cap==0 = borrowed literal, copy-on-
  write), mirroring the LLVM builtins exactly (string_new, str_len, str_char_at,
  str_push_byte, string_push_str, str_eq, str_substring, int_to_string, the print
  family). Emitted only when the program uses String.
- **scalar-element `Vec`** (Phase 163) — a `struct kdvec` runtime for `Vec<i64>`/
  `Vec<bool>` (push/get/get_ref/len/pop/remove/insert/reverse/swap). Also a
  soundness fix: an unimplemented builtin is now refused instead of emitting an
  undefined-symbol call.
- **`Drop` / RAII** (Phase 164) — frees non-escaping heap-owning locals AND owned
  by-value params at function exit; a binding is dropped only when every use is a
  borrow and the fn has no early return (escaping/uncertain cases leak rather
  than risk a double-free). ASan-verified.
- **closures + fn-pointers** (Phase 165) — a closure → a hoisted `__cl_<n>(void*
  env, args)` over a stack capture env (scalar by-value captures, free vars the
  backend computes itself); a fn value → the fat pointer `struct kdfn<arity>`; a
  top-level fn → a thunk. An escaping fn value (returned closure) or an FnMut
  closure is refused (ASan caught the stack-env dangle).
- **generics** (Phase 166) — a generic fn is monomorphized ONCE at int64_t (every
  scalar shares one C representation); a non-scalar or const-generic
  instantiation is refused (the backend never emits C that fails to compile).

### Deferred (documented follow-ons)
- `HashMap`/`HashSet` (a keyed-hash C runtime); non-scalar `Vec`/generic
  instances (struct/String elements); user `impl Drop`; heap locals in nested
  blocks / on early-return paths.

718 unit cases (6 suites) + the full smoke sweep green, JIT and AOT.

## [0.29.0] — Roadmap v29 "the C backend, finished I (aggregates + control)" (Phases 157–161)

Theme: grow the `--emit-c` C-source backend (v23) from the i64/bool scalar
subset to aggregates + the full control surface, each phase differentially
gated against the LLVM backend (the LLVM-AOT exit code must equal the
emitted-C-compiled-by-the-system-cc exit code).

### Added (C backend, `kardc --emit-c`)
- **Structs** (Phase 157) — typedefs (inner-before-outer order), struct literals
  as C designated-initializer compound literals, field access/assignment, and
  struct-typed lets/params/returns. The backend is now type-aware (a value is
  `int64_t` or `struct <Name>`), not "everything is int64_t".
- **Enums + `match`** (Phase 158) — an enum lowers to a tagged struct
  `struct E { int64_t tag; int64_t p0..; }`; a variant constructor is a compound
  literal; `match` lowers (without the LLVM decision tree) to an if/else chain on
  the tag (enum) or value (int), binding scalar payloads from `.p<i>`.
- **References / borrows** (Phase 159) — `&T`/`&mut T` → C pointers; `&x`,
  `&<temporary>` (a pointer to a C99 block-scoped compound literal), `*r`, and
  `r.field` auto-dereferencing to `(*r).field`; plus unit-returning fns.
- **`for` / `loop`-with-value + multi-file modules** (Phase 160) —
  `for x in a..b` → a C `for`; `loop { … break v; }` → a `while (1)` yielding the
  break value; and `mod foo;` programs are merged (resolveModules on the raw
  source, sans prelude) so the C backend sees every module's fns.
- **A randomized C-vs-LLVM differential oracle** (Phase 161) — generates many
  random programs over the subset (arithmetic, comparisons, `&&`/`||`, nested
  if/else, helper fns, while loops) and asserts LLVM-AOT exit == `--emit-c` exit.

Out-of-subset code (traits/impls/strings/Vec/Drop/closures/generics/async) is
still refused with a clear error — the backend never emits wrong C. A
match-through-reference that binds a payload is a documented follow-on.

718 unit cases (6 suites) + the full smoke sweep green, JIT and AOT.

## [0.28.0] — Roadmap v28 "const-eval & generics, finished" (Phases 152–156)

Theme: finish the const-evaluator and the generics story — aggregate consts,
non-i64 const-generics, deeper inference, GATs, and monomorphization control.

### Added
- **const-eval beyond i64/bool** (Phase 152) — array / tuple / struct / enum
  `const` values, built and projected (`A[i]`, `p.field`, `t.0`) at compile time
  with const bounds-checking, and usable as runtime values (the initializer is
  re-emitted per use, Rust-style).
- **const-generics beyond i64** (Phase 153) — a `const N` parameter may be
  `i64`, `bool`, or `char`; a value-use has the param's type at the right width;
  a binding's type annotation supplies the const arg (expected-type propagation).
- **bidirectional inference** (Phase 154) — struct-literal field values get the
  same coercions a fn argument does (an unannotated `None` infers from the field,
  int literals narrow). Fixed a real mutual-resolution bug: a **generic enum as a
  struct field** (`struct H { m: Option<i64> }` built with `Some`/`None`) used to
  fail to type-check; now resolved via a second field-resolution round.
- **generic associated types (GATs)** (Phase 155) — `type Out<T>;` in a trait,
  `type Out<T> = Pair<T, T>;` in an impl, projected as `Self::Out<i64>` →
  `Pair<i64, i64>` (the concrete-`Self` case), with arity checking.
- **monomorphization control** (Phase 156) — generics are monomorphized on
  demand and deduplicated (each instance emitted once); a concrete impl
  **specializes** (beats) a bounded blanket impl; and `kardc --mono-report`
  prints the monomorphization footprint (code-bloat visibility).

### Deferred (documented follow-ons)
- `char` / `f64` *scalar* consts (the integer evaluator + const-use codegen
  width handle i64/bool today).
- Turbofish (`f::<T>()`) and a GAT projection on a *bounded generic param*
  (`C::Out<i64>`); enum const-generic params.

718 unit cases (6 suites) + the full smoke sweep green, JIT and AOT.

## [0.27.0] — Roadmap v27 "strings, text & formatting" (Phases 147–151)

Theme: make text a first-class, correct part of the language — a real `char`
type, UTF-8 correctness, and a `format!` story with `Display`/`Debug`.

### Added
- **A real `char` type** (Phase 147) — a Unicode scalar, distinct from the
  integer tower (lowers to an i32 codepoint). Char literals `'a'` with escapes
  (`\n \t \r \\ \' \0` and `\u{HEX}`); equality/ordering (no arithmetic); `char
  as <int>` / `<int> as char` casts; char literal patterns in `match`; and real
  UTF-8 char↔string bridges (`char_to_string`, `char_from_u32` validating to
  U+FFFD, `str_push_char`, `print_char`). A Copy scalar.
- **UTF-8 correctness** (Phase 148) — char-aware operations over a String's
  bytes: `str_char_width_at`, `str_decode_char_at`, `str_char_count` (chars vs
  `str_len`'s bytes), `string_chars` (`-> Vec<char>`), `str_is_valid_utf8`.
- **`format!` / `print!` / `println!`** (Phase 149) — built-in formatting forms
  (there is no general macro system yet), recognized in the parser and
  desugared to string-building over `Display::to_string`. `{}` Display holes,
  `{{`/`}}` literal braces, compile-time placeholder/argument-count checking.
- **The `Debug` trait + `{:?}`** (Phase 150) — `fmt_debug(&self) -> String`,
  distinct from Display (a String is quoted + escaped, a char single-quoted).
  Built-in impls for the scalars + String; `#[derive(Debug)]` for structs
  (`Name { f: <dbg>, … }`) and enums (`Variant(<dbg>, …)`), recursing.
- **char classification + string encode helpers** (Phase 151) —
  `char_is_digit`/`_alpha`/`_alnum`/`_whitespace`, `char_to_upper`/`_to_lower`
  (ASCII), and `str_join` / `str_replace` / `str_lines`.

### Fixed
- The literal-discriminated decision-tree matcher + the codegen literal compare
  only handled `Int` columns — extended to `Char` (a char `match` was collapsing
  to the first arm + segfaulting at AOT). The borrow checker's param-type
  reconstruction now knows `char` is a Copy scalar.

### Deferred (documented follow-ons)
- A distinct borrowed `&str` type (folded into the UTF-8 work; `&String` serves
  the borrowed-string role today).
- Grapheme-cluster segmentation (UAX #29) and full Unicode case folding — both
  need the Unicode character database; scalar-level iteration + ASCII case
  mapping are what's provided.
- `{:width}` / alignment / precision format specs (only `{}` and `{:?}` today).

715 unit cases (6 suites) + the full smoke sweep green, JIT and AOT.

## [0.26.0] — Roadmap v26 "patterns, types & borrow-check completeness" (Phases 141–146)

Theme: close the long-standing gaps in pattern matching, the type surface, the
closure model, the borrow checker, and module visibility. Most of the hard
pattern features are lowered in the parser to constructs the Maranget
decision-tree matcher already handles.

### Added
- **Match guards + or-patterns** (Phase 141) — `A | B => e` arms (split into
  per-alternative arms) and `x if cond => e` guards.
- **Struct / tuple patterns** (Phase 142) — destructuring `Point { x, y }` and
  tuples in `match` arms and let-bindings (lowered to a bind + block).
- **Slice patterns** (Phase 143) — `[a, b]`, length dispatch, and a `[first, ..]`
  prefix, desugared to a length-checked `if/else` chain over `slice_len` /
  `slice_get`; `&mut [T]` mutable slices.
- **Type aliases** (Phase 144) — `type Name = Target;`, resolved in both the
  type checker and codegen (and carried across the module merge).
- **`Fn` / `FnMut` / `FnOnce` closure-trait hierarchy** (Phase 145) — every
  closure is classified by how it uses its captures (reads → `Fn`, mutates →
  `FnMut`, moves a capture out → `FnOnce`). A parameter may be spelled
  `Fn(A) -> R` / `FnMut(A) -> R` / `FnOnce(A) -> R`; the checker enforces the
  lattice `Fn < FnMut < FnOnce` at coercion sites. The bound is compile-time
  only (shared fat-pointer ABI), so accepted programs lower identically.
- **Two-phase borrows** (Phase 146) — a `&mut place` taken as a call argument
  (or a `&mut self` receiver) is a *reserved* borrow that does not conflict with
  a `&place` read nested in a sibling argument, so `vec_push(&mut v, vec_len(&v))`
  (the `v.push(v.len())` shape) compiles. Genuine aliasing — `f(&mut v, &v)` as
  direct sibling args, or two `&mut v` in one call — is still rejected.
- **Module visibility** (Phase 146) — `pub(crate)` / `pub(super)` / `pub(self)` /
  `pub(in path)` parse; `pub(self)` is private (path-unreachable), the rest are
  reachable in this crate. Enforced through the existing path-qualified-call
  visibility check.
- **`use` / `pub use` imports** (Phase 146) — `use a::b::c;`, `use a::b as c;`,
  `pub use a::b;`. A plain import is a scope hint; `use ... as` synthesizes a
  forwarder so the alias is callable; importing a private fn is a `use error`.

### Deferred (documented follow-ons)
- Full NLL region inference, implicit `&T`-field reborrows, and the
  mut-second-argument two-phase case (the borrow checker stays position-based
  NLL-lite).
- Cross-crate visibility distinctions (`pub` vs `pub(crate)`) — collapse to
  "reachable within the crate" until a real crate boundary exists (the
  package-ecosystem arc); type/const `use` aliases and generic/async alias
  forwarders.
- Owned (by-move, non-Copy) closure captures / a true runtime `FnOnce` — needs a
  closure-env drop vtable (fat-pointer ABI change).

710 unit cases (6 suites) + the full smoke sweep green, JIT and AOT.

## [0.25.0] — Roadmap v25 "the trait system, finished" (Phases 135–140)

Theme: bring the trait system from MVP to a usable vocabulary — default methods,
inheritance, blanket impls, coherence, associated consts, and the standard
conversion traits. The enabling utility is a new AST deep-clone.

### Added
- **Default trait methods** (Phase 135) — a trait method may carry a `{ body }`;
  impls inherit it unless they override. A `fillTraitDefaults` pass synthesizes
  the method into each impl (deep-cloning the default via the new `ast_clone`),
  so the type checker / codegen treat it like a hand-written method (a default
  may call abstract or other default methods).
- **Supertraits** (Phase 136) — `trait Ord: Eq + …`; a type impl'ing a trait
  must also impl every supertrait (enforced at the impl site), and a subtrait's
  default can call supertrait methods.
- **Blanket impls** (Phase 137) — `impl<T: Bound> Trait for T`, expanded into
  concrete `impl Trait for X` for every user type X satisfying the bound.
- **Coherence / overlap checking** (Phase 138) — two impls of the same trait for
  the same type (explicit, or two overlapping blankets) are rejected; a precise
  per-instantiation key keeps `Pair<i64>` and `Pair<bool>` distinct.
- **Associated consts** (Phase 139) — `const N: T;` in a trait and `const N: T =
  expr;` in an impl, read as `Type::N()` (desugared to a no-self method).
- **`From` / `Into` conversion traits** (Phase 140) — added to the prelude
  (`Target::from(x)` / `x.into()`), generic over the source/target type.

### Internal
- **`ast_clone`** — a deep-clone of expression/statement/pattern subtrees (the
  AST is move-only `unique_ptr`s), reused by the default-method and blanket-impl
  expansion passes.

No new operator sugar (`Deref`/`Index` auto-coercion is v34); `Self::N()` from
within a method and the `From`↔`Into` auto-blanket are documented follow-ons.
704 unit cases (6 suites) + the full smoke sweep green, JIT and AOT.

## [0.24.0] — Roadmap v24 "diagnostics & the developer surface" (Phases 130–134)

Theme: developer experience — the highest-ROI gap on the road to production.
Errors went from one-line `<kind> error N:M: message` (where N indexed into the
~450-line prelude-prepended source, so a user error on line 3 reported "455") to
real, navigable diagnostics, plus a lint pass, error codes, and doc comments.

### Added
- **Rich diagnostics** (Phase 130) — a rustc-style source snippet with a caret
  under the offending column, the **user's own line number + file** (the prelude
  offset is recovered), and positions embedded in messages rewritten to match
  (`moved at 457:18` → `moved at 5:18`).
- **An opt-in lint pass `kardc -W`** (Phase 132) — **unused `let`** bindings (a
  sound complete-AST use-walk: a name used via a closure, fn-pointer call,
  method/builtin call, or match binding does not warn; `_`-prefixed skipped) and
  **unreachable code** after `return`/`break`/`continue`. Non-fatal and opt-in,
  so the existing corpus is unaffected.
- **Error codes + `kardc --explain Exxxx`** (Phase 133) — a curated table tags
  the common diagnostics (`E0308` mismatched types, `E0382` use-of-moved, …)
  rustc-style (`type error[E0308]:`); `--explain` prints an extended explanation.
- **`///` doc comments** (Phase 134) — captured in the AST and surfaced both by
  the formatter (`kardfmt` round-trips them) and in **LSP hover** (rendered as
  prose below the signature).

### Changed
- **Parser panic-mode error recovery** (Phase 131) — after a statement parse
  error the parser resynchronizes to the next `;`/boundary, so it reports one
  diagnostic per real error (two malformed `let`s → 2 errors, not 4) while still
  surfacing the later errors. Recovery only runs on error; valid programs are
  byte-for-byte unaffected.

No language-surface changes; the diagnostic header keeps `<kind> error` + the
message (plus an optional `[Exxxx]`), so message-grepping tooling/tests still
match. 704 unit cases (6 suites) + the full smoke sweep green, JIT and AOT.

## [0.23.0] — Roadmap v23 "a second backend" (Phase 129)

Theme: break the LLVM/Linux monoculture. kardashev gains a **second code
generator** — a C-source backend — chosen first because it is the most
*verifiable* option here (a C toolchain is present), so it can be
differentially gated against the LLVM backend. This release lands the
foundational subset; the backend grows by subset (structs → enums + match →
strings/Vec → Drop) in later phases, with WASM and a Windows target as the
follow-on reach.

### Added
- **`kardc --emit-c` — a C-source backend** (Phase 129). Walks the same
  typechecked AST that the LLVM backend lowers and emits portable C, compiled by
  the system C compiler. The supported SUBSET: `i64`/`bool`, the full operator
  set (arithmetic / comparison / `&&` `||` / bitwise / unary `- ! ~`), `let`
  (incl. `mut` + assignment), `if`/`else` as a value, `while`, blocks, direct
  calls + recursion + mutual recursion (forward prototypes), and top-level
  `const`. Everything maps to `int64_t`, which is sound for the subset under
  `cc -fwrapv` (two's-complement overflow, truncating `/`, dividend-signed `%`,
  arithmetic `>>`, short-circuit `&&`/`||` — all matching kardashev's i64
  semantics). Expression-oriented constructs (block / `if` / `while` as a value)
  use GNU statement-expressions `({ ... })`. Anything outside the subset
  (structs / enums / match / strings / `Vec` / closures / `Drop` / references /
  generics / async / `mod` / ...) is **refused with a clear error** — the
  backend never emits wrong C.
- **Differential gating** (`tests/smoke_test_phase129.sh`) — for each of 12
  subset programs (recursion, mutual recursion, `while` + nested `if`,
  early-return, every operator, signed modulo, `const`, `bool`) the LLVM AOT
  exit code equals the emitted-C (`cc -fwrapv`) exit code; an out-of-subset
  struct program is cleanly refused. Skips cleanly when no C compiler is present
  (the LLVM path is unaffected).

The C backend is an `emit_c` library with **no LLVM dependency**; the front-end
(parse → derive → typecheck → borrow-check) is shared with the LLVM path, and
emission re-parses the raw user source so the auto-injected prelude doesn't trip
the subset check. 704 unit cases (6 suites) + the full smoke sweep green.

## [0.22.0] — Roadmap v22 "ergonomics, docs, and platform hygiene" (Phases 124–127)

Theme: two small but long-requested surface ergonomics, an honest docs pass, and
a CI-stability tweak. The second-backend exploration is broken out to **v23** —
a full second code generator is its own roadmap, planned (a differentially-gated
C backend first) rather than rushed.

### Added
- **`||` short-circuit logical-or** (Phase 124) — resolves the long-standing
  collision with the zero-parameter closure `|| body`. Disambiguation is
  positional: `||` is logical-or in infix position (after an operand) and a
  closure at the head of an expression, so the two never alias. `||` binds looser
  than `&&` (`a || b && c` is `a || (b && c)`); the lowering mirrors `&&`'s
  short-circuit (a branch + phi, flipped — lhs true skips the rhs). Pinned by
  `tests/smoke_test_phase124.sh` plus parser-precedence and codegen
  short-circuit unit cases.
- **`&<temporary>`** (Phase 125) — taking a reference to an rvalue (`&A(10)`,
  `&5`, `&Foo { .. }`, `&(a + b)`, a nullary variant `&Nil`) now works: the value
  is materialized into a fresh entry-block slot (one slot reused across loop
  iterations, like a `let`), registered as a droppable temporary dropped at scope
  exit, and its address is the borrow. Previously this was a hard codegen error;
  the documented `let`-first workaround is no longer needed. A droppable
  temporary (`&Text(int_to_string(i))`, an enum owning a heap String) in a 500k
  loop drops exactly once — RSS flat, `MALLOC_CHECK_=3` clean. Pinned by
  `tests/smoke_test_phase125.sh`.

### Changed
- **Language-reference + stdlib docs reconciled with reality** (Phase 126) — `%`,
  `&&`/`||`, `&` of a literal/temporary, and enum-typed struct fields were all
  listed as deliberate limitations but compile today (Phases 33 / 36 / 124 / 125).
  The honesty note, the lexical-structure operator table, the enum-field section,
  the surface-limitations list, and the stale "Roadmap v5" version headers are
  brought in line with the implementation and the test suite. doclint stays green.
- **macOS `codegen_test` flaky-retry residual cut** (Phase 127) — the macOS-arm64
  ORC-JIT teardown abort (~50%/run, confirmed non-deterministic; root cause needs
  macOS-arm64 hardware) goes from 3 to 5 `--flaky_test_attempts`, scoped by regex
  to that one target so a real regression elsewhere is never masked (~12.5% → ~3%
  residual). The test is deterministic on Linux, so a genuine regression still
  fails all attempts.

### Fixed
- **`&` of a unit/void temporary no longer crashes** — `&()`, `&{ }`, and
  `&<unit-returning call>` reach the new materialization path; a void value has
  no storage, so they now report a clean codegen error instead of building an
  invalid `alloca void` (which aborted). Guarded in `emitRefToTemporary`.

No new language surface beyond `||` and `&<temporary>`; one deliberate change —
`&A(10)`-style ref-to-temporary now compiles where it previously errored, so the
`smoke_test_diag` known-bad program is repointed to `&()`. 704 unit cases (6
suites) + the full smoke sweep green on Linux, JIT and AOT.

## [0.21.0] — Roadmap v21 "prove it, and close the gaps" (Phases 120–123)

Theme: turn anecdotes into numbers, fix the real footprint leak, and close the
two most-cited stdlib/MVP gaps. v21 has no new surface syntax — it makes the
existing language honest: measured, leak-free, and less `i64`-shaped.

### Added
- **Benchmark suite** (Phase 120, `bench/` + `BENCHMARKS.md`) — each workload
  written identically in kardashev and C, AOT-compiled (`kardc -O2` / `clang
  -O2`), run best-of-3 with outputs checked equal. Result: kardashev is
  **C-competitive** — `fib` ≈ 1.0×, `collatz` ≈ 1.0×, a tight integer `loop` ≈
  2.2× C. Correctness pinned by `tests/smoke_test_bench.sh`; the ratios are
  committed in `BENCHMARKS.md`. This replaces the old "−O2 default / flat RSS"
  anecdotes with data (and flags the ~2.2× tight-loop gap as a codegen target).
- **`HashMap`/`HashSet` `remove`** (Phase 122) — the one genuinely-missing stdlib
  operation. `hashmap_remove<K,V>(m: &mut HashMap<K,V>, k: K) -> Option<V>`
  (value moved out, key dropped) and `hashset_remove<T>(s: &mut HashSet<T>, k: T)
  -> bool`. Open-addressing deletion uses **backward-shift** (Knuth Algorithm R),
  not tombstones, so `get`/`insert`/`grow` are untouched and the table never
  fills with tombstones (no load-factor or infinite-probe regression). Pinned by
  `tests/smoke_test_hashremove.sh`: head/middle/tail + wrap-around chain
  preservation, a 50-key oracle, and heap-clean String-map remove + a 200k churn
  loop under `MALLOC_CHECK_=3` (RSS-flat).
- **Generic `Mutex<T>` cell** (Phase 123) — `Mutex` was `Mutex<i64>` only; its
  guarded cell is now an arbitrary `T`, so you can guard a struct, `String`,
  `bool`, `Vec`, … including shared across threads (a `Mutex<Counter>`
  read-modify-write under lock lands on the exact total). It is a **phantom-typed
  named `Mutex<T>`**: the value is a bare i64 handle (Copy, captured by value into
  thread closures), but the type carries the cell `T`, so `mutex_get`/`mutex_set`
  are *tied* to it — `T` flows from the handle (no annotation needed) and a
  wrong-`T` get/set is a compile error (closing a heap-overflow/punning hole an
  earlier type-erased draft had — found in adversarial review). `mutex_new`/`get`/
  `set` are specialized per cell type over a `{ pthread_mutex_t, T }` block; `get`
  clones the cell and `set` drops the old value (a `Mutex<String>` over 100k sets
  is RSS-flat). The cell `T` must be **`Send`** and not a shared handle
  (`Rc`/`Sender`/`Receiver`) — enforced at `mutex_new`, so a non-Send value can't
  be smuggled across a thread boundary through the cell. Pinned by
  `tests/smoke_test_mutex_generic.sh` (positive cells + 3 negative soundness
  repros).

### Fixed
- **`spawn` + `join` frame leak** (Phase 121) — the async executor leaked a heap
  frame per spawned task (its task array grew unbounded), because `join` drove +
  read the result but never reclaimed the task (unlike `block_on`, which reaps).
  A naïve reap-after-join is *wrong* — driving one handle also completes sibling
  tasks (the executor interleaves), so an all-done reap frees a sibling's result
  before its own `join` reads it. Fixed with a **per-handle release**
  (`__kd_exec_release(h)`): free only task `h`'s frame+slot, resetting the
  executor only once every task is released. A spawn+join loop is now RSS-flat
  and multi-handle joins return the right distinct results
  (`tests/smoke_test_spawnleak.sh`). *(Measurement also confirmed the previously
  suspected HashMap interior-drop and `block_on`/`await` frame reclaim are
  already clean — only `spawn`/`join` leaked.)*

### Notes
- **Still MVP (documented, not stubbed):** the const-eval scalar set (`i64`/
  `bool`) and the OS-thread return value (`fn() -> i64`; async/await is the
  generic path) remain `i64`-shaped.
- **One source-level break (behavior unchanged):** the `Mutex` handle type is now
  `Mutex<T>` (was a bare `i64`). Programs that let the handle be inferred (`let m
  = mutex_new(0)`) are unaffected, but any that **named** the handle type `i64`
  (e.g. `fn bump(m: i64)`) must spell it `Mutex<i64>`. Runtime behavior is
  identical (the handle is still a Copy i64 at the ABI). No other v21 change
  alters an existing program; the HashMap/async/numeric suites and the 455 unit
  cases (155 codegen + 300 typecheck) pass unchanged.

## [0.20.0] — Roadmap v20 "toward a real bootstrap" (Phases 115–119)

Theme: move the self-hosted compiler past the toy. Through v19 "self-hosting"
meant a mini compiler that lowered a 2-type expression language to an in-process
stack VM (it emitted no real code). v20 makes it emit **real native code**,
proves that code **matches the host compiler**, and extends it to the aggregate
shapes kardashev itself is built from — **structs and enums**.

### Added
- **Real LLVM IR codegen** (Phase 115, `examples/selfhost/llvmgen.kd`) — the
  self-hosted compiler now lowers each `Expr` to SSA-form **textual LLVM IR**
  (`add`/`mul`/`icmp`+`zext`/branch-free `select`) and prints a complete module,
  so `clang out.ll -o prog && ./prog` runs **natively** — a real compilable
  artifact, not an interpreter. Differential-gated against the host.
- **A differential fuzzer over the self-hosted codegen** (Phase 116) — for many
  random valid functions with random args, the self-hosted-emitted IR (clang →
  native) must equal the host compiler's result. The self-hosted backend matches
  the host across random programs.
- **Structs** (Phase 117, `structgen.kd`) — `struct NAME { f: i64, ... }`, struct
  literals, and field access, lowered to first-class LLVM aggregates
  (`insertvalue`/`extractvalue`); every value carries its type. Differential-gated.
- **Enums + `match`** (Phase 118, `enumgen.kd`) — `enum NAME { V(i64), ... }`,
  variant construction, and `match`, with an enum as a tagged pair `{ i64, i64 }`,
  construction → `insertvalue`, and `match` → `extractvalue` + a branch-free
  **select-chain** on the tag (sound because the language is pure). Differential-
  gated across all branches/variants.

### Fixed
- **Adversarial review** (Phase 119) of the three self-hosted compilers (~80
  programs vs the host, IR validity, test honesty) found and fixed: a `match`
  whose arms return enum values lowering its select-chain as `i64` instead of the
  aggregate type (clang-rejected; host accepted); and a latent aggregate-return
  `main` mismatch. Both pinned by regression cases. IR validity and test honesty
  came back clean.

> The "self-hosting" is now well past "toy" — but it is still a **subset** (i64/
> bool + structs + enums, not all of kardashev), so this is **not** yet a true
> bootstrap. See [ROADMAP.md](ROADMAP.md).

## [0.19.0] — Roadmap v19 "hardening III" (Phases 112–114)

Theme: push differential fuzzing into the memory-safety and integer-arithmetic
codegen paths (the bug classes that mattered most), and clean up diagnostics.

### Added
- **A memory-safety fuzzer** (Phase 112) — random but borrow-valid struct
  programs: a struct with K fields, each owning a heap `String` and printing a
  unique id on `Drop`; a random subset of distinct fields is moved into a `Vec`,
  the rest drop at scope exit. Two oracles: heap-clean under `MALLOC_CHECK_=3`
  (a double-free aborts) and every id dropped EXACTLY once. A 1 M-iteration loop
  variant gates on RSS-flatness. 75 programs across 3 seeds are all sound —
  evidence the v17/v18 per-field move/drop machinery holds across varied inputs.
- **A division / modulo / bitwise fuzzer** (Phase 113) — the integer paths the
  arithmetic fuzzer skipped, and a classic miscompile source. Generates random
  `+ - * / % & | ^ << >>` programs with the kardashev source and a C-semantics
  Python reference in lockstep (truncating `sdiv`, dividend-signed `srem`,
  arithmetic `>>`). 200 programs across 4 seeds agree (JIT == AOT == reference) —
  the lowering follows C/Rust semantics, not Python's floor-mod.

### Fixed
- **Clean codegen diagnostics** (Phase 114) — when codegen reports a real error
  it kept emitting placeholder IR, and the module verifier then piled cascading
  "module verification failed" lines on top of the real diagnostic. Codegen now
  returns the real errors directly and skips the verifier when any error was
  already reported; the verifier still runs on the error-free path (catching
  codegen bugs that emit invalid IR without reporting an error).

## [0.18.0] — Roadmap v18 "hardening II" (Phases 108–111)

Theme: close the concrete gaps that dogfooding the self-hosted compiler (v15–v17)
and its adversarial review exposed, and deepen the test surface with differential
fuzzing.

### Fixed
- **Re-initializing a moved-out struct field is legal** (Phase 108) — v17's
  field-level move tracking conservatively rejected `s.a = new` after `s.a` was
  moved out. The borrow checker now clears that field from the root's moved set
  on a `root.field = v` assignment (after the RHS is consumed, so `s.a = f(s.a)`
  still flags the RHS), so the field and struct are usable again. Using a moved
  field *without* re-initializing it is still rejected.
- **A unit-returning async fn no longer crashes the compiler** (Phase 109) — an
  `async fn f(..) ! { .. } { stmt; }` (no `-> T`) SIGTRAP'd the compiler when its
  future was consumed: `block_on` / `.await` / `spawn`+`join` read the `Poll<T>`
  value slot as `T`, and for the unit result `T` maps to LLVM `void`, so a `load
  void` (and a named `void` call) emitted invalid IR. A void result now yields
  the unit placeholder without a load, and the `block_on` call is left unnamed.
  (Found by the v17 adversarial review.)

### Added
- **A differential fuzzer for the codegen path** (Phases 110–111) — generates
  random, always-valid programs and checks three oracles agree: the JIT-printed
  value, the AOT exit code, and a Python reference. Phase 110 covers arithmetic
  (`+ - * ( )` over `i64`); Phase 111 adds `let` bindings, comparisons, and
  `if/else` branch selection. Seeded for reproducibility; 500 programs across the
  two harnesses agree exactly — no miscompile found.

## [0.17.0] — Roadmap v17 "self-hosting, continued — a compiler in kardashev" (Phases 98–107)

Theme: complete the self-hosted compiler — type checker **and** code generator,
every stage written in kardashev — and fix the real compiler bugs that
dogfooding it surfaced. By the end, `examples/selfhost/compile.kd` is a mini
compiler that type-checks a whole function and compiles + runs its body.

### Added
- **A whole-function parser + interpreter** (Phase 98, `func.kd`) — parses a
  complete `fn NAME(PARAMS) -> RET { BODY }` into an `Fn` AST and interprets it
  (scope-check the body against the params, bind args, evaluate). JIT + AOT.
- **A real type checker** (Phase 101, `typeck.kd`) — past scope-checking: the
  self-hosted expression language now has **two** types, `i64` and `bool`.
  `type_of` infers each node's type against a type environment, enforcing
  arithmetic on `int×int→int`, comparison on `int×int→bool`, and an `if`
  condition that is `bool` with equal branch types — propagating a `TErr` tag on
  any mismatch.
- **A whole-function type checker** (Phase 102, `funcheck.kd`) — threads the
  checker through `fn NAME(PARAMS) -> RET { LETS ; RESULT }`: param types, `let`
  typing, and the body's type checked against the declared return type.
- **A code generator + VM** (Phase 103, `emit.kd`) — the back-end shape: lowers
  the `Expr` AST to a flat stack-machine bytecode (`PUSH/LOAD/ADD/MUL/LT/EQ/
  SELECT`) and executes it on an operand stack + register file. Proven correct
  by cross-checking every program against a tree-walking `eval`.
- **CAPSTONE: a self-hosted mini-compiler** (Phase 105, `compile.kd`) — takes a
  whole function, type-checks it, and (only if well-typed) compiles the body —
  now with `let` LOCALS lowered to a `STORE` into a register slot — and executes
  it on argument values. Ill-typed functions are rejected before any codegen.
  lex → parse → type-check → code-generate → execute, every stage in kardashev.

### Fixed
- **Field-move double-free** (Phases 99/100/106) — surfaced by self-hosting.
  Moving a non-Copy struct field by value double-freed. Phase 99 stopped the
  single-move double-free in codegen (clear the root binding's drop flag on a
  field/index partial move); Phase 100 made it leak-free with **per-field drop
  flags** so siblings still drop; Phase 106 closed the remaining **double**-move
  hole in the **borrow checker** with field-level (partial) move tracking
  (`Binding::movedFields`) — a second move of the same field, or a whole-struct
  use after a partial move, is rejected, while moving two distinct fields stays
  legal. (Found by an adversarial review of the field-move work.)
- **Unit-tail-`match` miscompile** (Phase 104) — a `match` (or any value-
  producing expression) in tail position of a unit-returning function emitted
  `ret i64` into a void function (invalid IR). The epilogue now gates `ret` vs
  `ret void` on the function's actual return type. (Found writing `emit.kd`.)
- **Field-assignment leak** (Phase 107) — `s.a = new` overwrote a droppable
  struct field without freeing the old value (RSS ballooned in a reassigning
  loop). Codegen now drops the old field value — guarded by the field's drop
  flag (so a moved-out field isn't double-freed) — before storing.

### CI
- **macOS reliably green** — the two non-deterministic macOS-only flakes
  (`codegen_test`'s arm64 ORC-JIT teardown abort, confirmed by a same-commit
  rerun; `smoke_test_executor`'s timing bounds) are marked `flaky = True` (Bazel
  retries up to 3×). A no-op on Linux, which stays deterministic, so a real
  regression is still caught by the ubuntu job.

## [0.16.0] — Roadmap v16 "self-hosting, continued" (Phases 94–97)

Theme: grow the self-hosted front (v15: lexer + parser + signature checker)
toward a full compiler — the BODY grammar: expressions, statements, scope
checking, and a function-body interpreter, all written in kardashev in
`examples/selfhost/`.

### Added
- **An expression parser + evaluator** (Phase 94, `expr.kd`) — a recursive-descent
  parser builds an `enum Expr` AST (`Num` / `Var` / `Add` / `Mul`, recursive via
  `Box`) for an arithmetic expression with VARIABLE REFERENCES (the step beyond
  `examples/calc`'s variable-free arithmetic), then evaluates it against a
  `HashMap<String, i64>` environment. Proves precedence (`a + b * 2` = 11) and
  parentheses (`(a + b) * 2` = 14).
- **A statement/block parser + evaluator** (Phase 95, `stmt.kd`) — grows the body
  to a block: `let NAME = EXPR ;` bindings + a result expression →
  `Block { lets: Vec<Stmt>, result: Box<Expr> }`, evaluated by running each `let`
  in order (extending the environment) then the result. `let x = a + 1 ; let y =
  x * 2 ; y` with `{ a: 3 }` → 8.
- **A scope/semantic checker** (Phase 96, `scopechk.kd`) — walks the block AST and
  reports UNDEFINED variable references (a `let` RHS checked before its own name
  binds; each `let` extends the scope). `… x + c` with `c` undeclared → 1 error.
- **Capstone: a function-body interpreter** (Phase 97, `interp.kd`) — ties the
  whole pipeline (lex → parse → scope-check → evaluate) into one
  `interpret(body, params, args)`: rejects a body referencing an undefined
  variable (`-1`), else binds the arguments and runs the block. `fn f(x=3, y=4)
  { let sq = x*x; let dbl = y+y; sq + dbl }` → 17.

### Notes
- A self-hosted interpreter for kardashev function bodies, written in the
  language it interprets. Surfaced two ergonomics findings handled in-source
  (candidate later-roadmap polish): a `Box`-AST child is dereferenced in `eval`
  as `&(**child)` (`&**child` doesn't parse), and the parser cursor threads as a
  `&mut Pos` struct cell since there is no `*pos = x` deref-assign of a `&mut
  i64`. All four phases green, JIT **and** AOT; Linux + macOS CI green.

## [0.15.0] — Roadmap v15 "self-hosting" (Phases 88–93)

Theme: the north-star arc toward a bootstrap — grow kardashev until a kardashev
compiler can be written *in* kardashev. v15 delivers a self-hosted compiler
**front-end** (lexer + parser + checker), each phase a real, tested kardashev
program in `examples/selfhost/`. The gating primitives already existed (file I/O
via `fs_read_to_string` → `Result<String, IoError>`; byte string access via
`str_char_at` / `str_push_byte` / `str_substring`; `enum` + `Box` for a recursive
AST; `HashMap` for symbol tables), so the front of the pipeline is expressible
today.

### Added
- **A lexer in kardashev** (Phase 88, `lexer.kd`) — scans a kardashev snippet
  byte-by-byte and groups the bytes into real tokens with correct boundaries
  (identifiers, numbers, the multi-char `->`, punctuation), whitespace skipped.
- **A token-stream lexer** (Phase 89, `tokens.kd`) — produces a `Vec<Token>` with
  each token's KIND and SPAN; the spans reconstruct via `str_substring` to `"fn"`
  / `"->"`, the typed interface a parser consumes.
- **A parser for kardashev syntax** (Phase 90, `parser.kd`) — parses a function
  SIGNATURE into a structured `FnSig { name, params: Vec<Param>, ret }` AST,
  recovering each name/type from the token spans. (Arithmetic-expression parsing
  was already shown by `examples/calc`; this parses the language's own grammar.)
- **An AST printer + round-trip** (Phase 91, `printer.kd`) — reprints the `FnSig`
  AST back to source and checks it is byte-identical, proving the AST losslessly
  captures the surface syntax.
- **A scope/semantic checker** (Phase 92, `checker.kd`) — builds a
  `HashMap<String, String>` symbol table over the AST, resolves a parameter's
  type by name, and rejects a duplicate parameter name.
- **Capstone: the front-end, end to end** (Phase 93, `front.kd`) — one program
  runs the whole front (lex → parse → check → reprint) over a function signature
  and proves it generalizes across a 2-param and a 3-param signature. A
  self-hosted compiler front-end written in the language it compiles.

### Notes
- All six phases green, JIT **and** AOT, deterministic; Linux CI green, macOS CI
  green except a flaky `codegen_test` abort (carried from v14, an arm64-JIT issue
  needing a macOS-arm64 environment). Full self-hosting (the whole compiler,
  incl. codegen) is a multi-roadmap effort the later roadmaps continue.

## [0.14.0] — Roadmap v14 "hardening" (Phases 82–87)

Theme: make the toolchain trustworthy across platforms and inputs — a green
**macOS CI** for the first time, a SIGPIPE-robust test harness, the last known
channel footgun closed as a precise compile error, and a JIT-vs-AOT differential
sweep. The consolidation roadmap after three feature roadmaps (v11–v13) that each
needed a soundness fix at review time.

### Added
- **Portable memory/leak gates** (Phase 82) — the constant-memory leak gates
  (peak-RSS checks that catch drop/refcount leaks) hard-required GNU
  `/usr/bin/time -v`, so on macOS (BSD `time`) they died under `set -euo
  pipefail` — 11 of the 12 long-standing macOS-CI failures. A shared portable
  `peak_rss_kb` (GNU `time -v` **or** BSD `time -l`, else a clean SKIP) keeps the
  gate *running* on both platforms; this took **macOS CI green for the first
  time**. Plus a CI step that dumps any failing test's `test.log` (an Aborted
  test prints nothing with `--test_output=errors`).
- **SIGPIPE-robust smoke harness** (Phases 84–85) — `echo "$big" | grep -q` /
  `awk '…exit'` / `$CMD | head -N` make the producer die with SIGPIPE (exit 141)
  when the consumer closes the pipe early — a load-sensitive flake under
  `set -o pipefail`. Swept ~51 such pipelines across 31 files to here-strings /
  capture-then-process; consumers that read to EOF (`tail`, `wc`, plain `grep`)
  left alone. const.sh went from ~3/5 to 12/12 under load.
- **The channel capture-and-keep footgun is a compile error** (Phase 86) — a
  `Sender` captured into a closure is owned by the closure's heap env, which
  never drops its captures, so the only way it is ever dropped (and the channel
  closes) is being MOVED out of the closure. The typechecker now rejects a
  captured `Sender` with no bare (by-value) use anywhere in the body — exactly
  the send-only-never-moved case that leaks and hangs a `recv`-until-`None`
  consumer. The rule is *precise* (a bare use is the only way a non-Copy Sender
  leaves an env, so sound code always has one): zero false positives across the
  whole v13 channel suite.
- **JIT-vs-AOT differential sweep** (Phase 87) — one test runs all 9 single-file
  capstones (calc, checksum, csvstats, json, kdlex, matrix, parstats, rpn,
  wordfreq) through both backends and asserts they agree. The ORC-JIT prints
  `main`'s `i64` return as a trailing line while the clang-linked AOT exits with
  it (& 255), so AOT stdout must equal JIT stdout minus that line and the line
  mod 256 must equal the AOT exit code. One place any future codegen change must
  keep green — verified 9/9 agree, on Linux **and** macOS-arm64.

### Fixed
- **jmp_buf alignment + size** (Phase 83) — the catch-stack `_setjmp` jmp_buf was
  a 1-aligned `[256 x i8]` byte array (the entry struct 264 bytes, so entries
  past the first landed at non-16 offsets). Now a generously-sized, 16-byte
  aligned `[32 x i128]` (512 bytes) cell — correct defensive hardening for any
  platform. (It did not clear the remaining macOS-arm64 `codegen_test` flake — an
  arm64-JIT-execution issue that is ASan/UBSan-clean on Linux and needs a
  macOS-arm64 environment to diagnose; tracked, not papered over.)

### Notes
- Tested green on a cleared clean build: 6 unit suites + the smoke aggregate
  (incl. the new differential + v13-review footgun checks), JIT **and** AOT.
  **Linux CI green; macOS CI green except a flaky `codegen_test` abort** (the
  9-capstone differential passing on macOS-arm64 confirms the *generated code*
  agrees across backends there — the flake is in the unit-test harness).

## [0.13.0] — Roadmap v13 "concurrency" (Phases 75–81)

Theme: make concurrency SAFE BY CONSTRUCTION — typed channels that move data
between threads, with thread-safety enforced *through the effect system* (the
language's differentiator). Designed via a 3-proposal / 3-judge multi-agent
panel (MVP-first won, grafting the structural Send/Sync rule + an `Rc` negative
witness). A pre-merge adversarial multi-agent review (3 reviewers, ~600 stress
runs) then found a use-after-free in the borrow-returning builtins and two
channel-lifecycle defects the green suite had missed — all fixed in Phase 81
(see Fixed); the Send/`share` soundness surface it hammered came back clean.

### Added
- The **`share` effect** (Phase 75) — the concurrency effect that makes
  thread-safety a CHECKED property rather than a library convention.
  `thread_spawn` now carries `share`, so a fn that spawns must declare
  `! { share }`. Because `share` is a built-in effect it rides the existing
  effect-SUBSET rule: a trait method declared without `share` can NEVER have an
  impl that spawns, so concurrent work can't be smuggled past a pure-looking
  `<T: Task>` / `&dyn Task` interface (the super-effecting impl is rejected).
  This is the value-crossing *control* half; the value-*safety* half (only
  `Send` data crosses) lands with channels in Phase 77.
- **Typed MPSC channels** (Phase 76) — `channel() -> (Sender<i64>,
  Receiver<i64>)`, `chan_send` / `chan_recv` (→ `Option`, `None` once closed
  AND drained) / `chan_close`. The runtime is an unbounded linked-list queue
  guarded by a **pthread mutex + condition variable** (`chan_recv` blocks on
  the condvar while the channel is empty and open). A producer thread sending
  1..=100 and the main thread draining sums to exactly 5050, deterministic
  across runs, JIT and AOT. `Sender`/`Receiver` are named generic structs that
  lower to an i64 handle into the channel block; a `Sender` (multi-producer,
  `Send`) crosses into a worker thread, while a `Receiver` is the
  single-consumer endpoint and is **not** `Send` (moving one into a thread is a
  compile error). *(Phase 81 made the endpoints refcounted, move-only owners so
  the block is reclaimed and the channel closes on the last sender — see
  Fixed; `chan_send`/`chan_recv` now borrow the endpoint, `sender_clone` makes
  a second producer.)*
- **Generic channels + the `Send` rule** (Phase 77) — `channel<T>` now MOVES a
  real `T` across threads (the queue node carries a `T`-sized cell, specialized
  per `T`), so an owned `String` or `Vec<i64>` is sent from one thread and
  received on another with ownership transferring sender → node → receiver,
  freed exactly once (no clone, no double-free). The structural **`Send`**
  predicate (`isSend`) gets teeth at `chan_send`: scalars / `String` / owning
  aggregates / the channel `Sender` are `Send`, while a `&T` borrow, the
  `Receiver`, and (Phase 78) `Rc` are not — sending a non-`Send` value on a
  channel is a compile error, so no borrow can dangle across a thread.
- **`Rc<T>`** (Phase 78) — a non-atomic reference-counted shared owner
  (`rc_new` / `rc_clone` / `rc_get` / `rc_strong_count`), a pointer to a heap
  `{ i64 strong, T value }`. The strong count tracks clones; the shared value
  and the block are dropped EXACTLY once when the last `Rc` drops (verified
  drop-once over a `Drop`-counted inner; 200k clone+drop pairs stay flat). It
  is the **legible non-`Send` witness**: its refcount is non-atomic, so an
  `Rc` may not cross a thread boundary (sending one on a channel is a compile
  error that names `Rc`) — the contrast to sharing safely via a `Mutex`.
- **The parallelism payoff + `chan_try_recv`** (Phase 79) — the v13 primitives
  compose into real fork-join parallelism: split 0..N across W worker threads,
  each summing its range and sending the partial on a SHARED `Sender`
  (multi-producer), with the main thread gathering the W partials over the
  MPSC channel (W producers → 1 consumer) and folding — deterministic, JIT and
  AOT. Plus `chan_try_recv` — a non-blocking receive (`Some` if ready, `None`
  if momentarily empty, never blocks on the condvar) for poll loops.
- **Capstone** `examples/parstats` (Phase 80) — "concurrency, applied": a
  parallel map-reduce, safe by construction. The series
  `data(i) = (i*7+13) mod 1000` over `0..10000` is split across 4 worker
  threads; each reduces its chunk to a `Stats` struct and SENDS it on a shared
  MPSC channel; the main thread gathers + merges into the global stats —
  deterministic and checked against the sequential answer (sum 4995000,
  count 10000, min 0, max 999). Exercises the whole v13 line at once:
  `thread_spawn` (`share`), channels moving a `Stats` struct across threads,
  the `Send` rule, fork-join, and the v12 `i64_min`/`i64_max` helpers.
- **Refcounted, move-only channel endpoints** (Phase 81, from the review) —
  the channel block now carries a mutex-guarded live-**sender** count and a
  live-**endpoint** count, and `Sender`/`Receiver` are move-only owners (not
  Copy) with drop glue. `chan_send`/`chan_recv`/`chan_try_recv` BORROW the
  endpoint (`&Sender` / `&Receiver`), so a single owner still sends/recvs in a
  loop; `sender_clone(&Sender) -> Sender` makes an additional producer, and
  capturing a `Sender` into a thread clones it automatically (each thread gets
  its own refcounted handle, dropped by the worker's by-value param). This is
  the Rust ownership model: the channel **closes when the last `Sender` drops**
  (so one producer finishing can't end the stream for the others), and the
  block — plus any queued nodes and undrained droppable payloads — is **drained
  and freed when the last endpoint drops**. `chan_close(Sender)` now consumes
  the sender (an explicit "this producer is done").

### Fixed
*(All found by the v13 pre-merge adversarial review; pinned by
`tests/smoke_test_v13_review.sh`.)*
- **Use-after-free via a borrow-returning builtin (BLOCKER).** `rc_get(&a)` and
  `vec_get_ref(&v, i)` return a `&T` that aliases the owner, but the borrow was
  not tracked against it — so `let r = rc_get(&a); consume(a); *r` compiled and
  read freed memory. The borrow checker now ties such a `let`-bound borrow to
  the owner (exactly like `let r = &a;`), so moving or dropping the owner while
  the borrow is live is a borrow error. (Closes the same hole on the
  stale-`vec_get_ref`-after-`vec_push` path.)
- **Unbounded channel leak (MAJOR).** Endpoints were Copy handles that nothing
  owned, so every `channel()` leaked its ~172-byte block (plus undrained nodes
  and their payloads) — unbounded in a channel-per-task loop. The refcounted
  move-only endpoints (Phase 81) reclaim the block, drain the queue, and drop
  remaining payloads when the last endpoint drops: RSS is now flat over
  1,000,000 created+drained channels and 200k dropped-with-undrained-`Vec`
  channels. Moving an owned value across a channel still drops it exactly once.
- **Multi-producer `chan_close` data loss (MAJOR).** Close set a single boolean,
  so any one producer closing made `chan_recv` return `None` while other live
  producers were still sending — 84/100 runs lost an entire producer's data.
  Close is now refcounted: the channel ends only when the **last** `Sender` is
  gone, so a producer finishing never abandons another's queued items (2
  producers × 300, one closing early → exactly 600 every run).

## [0.12.0] — Roadmap v12 "real stdlib" (Phases 69–74)

Theme: turn a language you can *compute* in into one you can *get data in and
out of* — parsing, richer collections, string and numeric methods. The second
step toward production use. A pre-merge adversarial multi-agent review fixed a
`parse_int` integer-overflow and a discarded-owned-temporary leak the green
suite had missed (see Fixed).

### Added
- **String → number parsing** (Phase 69): `parse_int(&String) -> Option<i64>`
  and `parse_f64(&String) -> Option<f64>` — the all-or-nothing parse a real
  stdlib needs (a string that is not *wholly* a valid number, including one
  with leading/trailing junk or whitespace, is `None`). Built on low-level
  `str_parse_i64` / `str_parse_f64` out-param primitives (C `strtoll`/`strtod`
  over a transient stack buffer, with strict full-consume + no-leading-
  whitespace validation). Plus `int_to_hex(i64) -> String` (lowercase hex, the
  two's-complement pattern for a negative). Reading data no longer needs a
  hand-rolled digit loop.
- **Vec mutation + query** (Phase 70): `vec_pop` / `vec_remove` / `vec_insert`
  / `vec_reverse` (built-ins) and `vec_contains` / `vec_index_of`
  (`Eq`-bounded prelude scans; index −1 when absent). `vec_pop` and
  `vec_remove` MOVE the element out (the length is decremented so the Vec no
  longer owns that slot — no clone, no double-free, the dual of the cloning
  `vec_get`), so they are sound for a non-Copy element type (`Vec<String>`).
  `vec_insert` grows when full and clamps its index to `[0, len]`.
- **HashMap / HashSet enumeration + membership** (Phase 71):
  `hashmap_contains(&HashMap, &K) -> bool` and `hashmap_values(&HashMap) ->
  Vec<V>` (`Eq`+`Clone`-bounded prelude scans over `hashmap_get_ref` /
  `hashmap_keys`, deep-cloning the values), plus `hashset_items(&HashSet) ->
  Vec<T>` — the first way to enumerate a `HashSet` (a codegen built-in
  delegating to the backing map's keys). `hashmap_remove` / `hashset_remove`
  are a deliberate deferral (open-addressing deletion needs tombstone-aware
  get/insert).
- **String methods** (Phase 72): `str_starts_with` / `str_ends_with` /
  `str_contains` / `str_index_of` (pure reads, substring index or −1) and
  `str_to_upper` / `str_to_lower` / `str_concat` / `str_repeat` (fresh heap
  Strings). All kardashev prelude functions over `str_char_at` / `str_len` /
  `str_push_byte` — high-level string manipulation without a manual char loop.
- **Numeric + math helpers** (Phase 73): integer `i64_abs` / `i64_min` /
  `i64_max` / `i64_pow` (prelude) and the f64 math `f64_sqrt` / `f64_floor` /
  `f64_ceil` / `f64_abs` (built-ins lowering to LLVM float intrinsics; the AOT
  link now pulls in `-lm`), plus more Option/Result inspectors
  (`option_is_some`, `option_ok_or`, `result_is_ok`). A real-number program no
  longer needs its own FFI declaration of libm.
- **Capstone** `examples/csvstats` (Phase 74) — "the real stdlib, applied": a
  CSV statistics aggregator that READS data (the thing v11 could not do),
  grouping `category,value` rows and reporting per-category count + sum + the
  running global max in sorted order. Exercises the whole v12 line at once —
  `parse_int` (with an `Option`-driven skip of a malformed row), `str_split`,
  HashMap aggregation, `i64_max`, `sort`, and `int_to_string` + `str_concat`
  formatting.

### Fixed
- A pre-merge adversarial multi-agent review found + fixed two MAJORs the green
  smoke suite had missed — both pinned by `tests/smoke_test_v12_review.sh`:
  - `parse_int` of a value PAST the `i64` range returned a silently-clamped
    `Some(i64::MAX/MIN)` instead of `None` (C `strtoll`'s `ERANGE` was
    unchecked). It now clears `errno` and rejects on `ERANGE`; `i64::MAX` /
    `i64::MIN` themselves still parse. (`parse_f64` keeps `strtod`'s
    overflow-to-`inf` — a valid `f64` parse, like Rust.)
  - a DISCARDED owned temporary leaked: a value moved out by
    `vec_remove(&mut v, 0);` (or any call result like `int_to_string(n);`) used
    as an expression-STATEMENT was never dropped, orphaning its heap. The
    codegen now drops a discarded droppable call-result via an entry-block temp
    — exactly once (the drop / dropleaks / soundness suites confirm no
    double-free).

## [0.11.0] — Roadmap v11 "real machine integers" (Phases 63–68)

Theme: the **numeric tower** — make kardashev practical by giving it real
machine integers (sized + unsigned + f32, `as` casts, bit ops, defined overflow)
instead of i64-only. The first step toward production use. A pre-merge
adversarial multi-agent review hardened a const-evaluation width/sign cluster
(including an invalid-IR blocker) plus two parser/lexer bugs the green suite had
missed (see Fixed).

### Added
- Sized SIGNED machine integers `i8` / `i16` / `i32` (Phase 63) — `i64` stays
  the default. The `Int` type carries a bit width + signedness; codegen lowers
  to the matching LLVM width (`i32 @add(i32, i32)`, not i64). The lattice is
  NON-coercive: no implicit widening (`i32` + `i64` is a type error — `as`
  bridges, Phase 65), and an out-of-range literal for a narrow width is a
  compile error. An unsuffixed literal is i64 by default and narrows to a
  concrete width in context (`let x: i32 = 5`); the type system carries zero
  literal churn (all v10 i64 programs are byte-for-byte unchanged).
- Integer-literal **width suffixes** and **radix prefixes** (Phase 64). A
  suffixed literal `5i32` *is* an `i32` with no annotation (it does not narrow,
  it has that concrete type), so `add(5i32, 3i32)` type-checks against an `i32`
  parameter directly; an out-of-range suffixed literal (`200i8`) is a compile
  error. Hexadecimal `0xFF` and binary `0b1010` literals parse to their value
  (default `i64`), compose with a suffix (`0xFFi32`), and work in `match`
  patterns (`0xFF => …`). Unsigned suffixes (`u8`..`u64`) are parsed and
  rejected with a clear "arrives in a later phase" diagnostic until Phase 66
  lands unsigned integers — never silently mis-typed.
- The **`as` cast operator** (Phase 65) — the only bridge across the
  non-coercive lattice. `operand as Type` converts between any two numeric
  types (an int of any width/signedness, or `f64`): integer widen (`sext`),
  narrow (`trunc`), and `int`↔`f64` (`sitofp` / `fptosi`, truncating toward
  zero), lowered to the width/signedness-correct LLVM cast. A cast is the only
  way to add an `i32` to an `i64` (`a as i64 + b`). `as` binds tighter than
  every binary operator but looser than a prefix unary (`-x as i32` is
  `(-x) as i32`, `a as i32 * 2` is `(a as i32) * 2`) and chains left-to-right
  (`x as i32 as i64`). An `int`→`int` cast is const-foldable and wraps with
  two's-complement semantics (`300 as i8` == 44) identically at compile time
  and run time. Casting from/to a non-numeric type (a struct, `bool`, String,
  reference) is a compile error.
- **Unsigned integers** `u8` / `u16` / `u32` / `u64` and the integer **bitwise
  operators** `& | ^ << >> ~` (Phase 66). Each unsigned type is a distinct
  non-coercive type (`u32` ≠ `i32`; `as` bridges), and codegen lowers its
  division, remainder, ordering comparison, and right-shift to the UNSIGNED
  opcode (`udiv` / `urem` / `icmp u…` / `lshr`) — a signed right-shift stays
  arithmetic (`ashr`). A `u64` literal past `i64::MAX` (e.g. the FNV-1a offset
  basis `0xcbf29ce484222325`) parses, and a wrapping `u64` multiply yields the
  textbook hash. Bitwise operators work on any integer width/signedness, fold
  in const expressions, and are rejected on `f64`. The `&` and `|` tokens are
  position-disambiguated (prefix `&` is still a borrow, a primary `|…|` is
  still a closure; infix they are bitwise-and / bitwise-or), and `<<` / `>>`
  are parsed by token adjacency so nested generics `Vec<Vec<T>>` stay
  unambiguous. Operator precedence now matches Rust: `&&` < comparison < `|` <
  `^` < `&` < shift < `+ -` < `* / %`.
- The **`f32`** single-precision float and **defined overflow semantics**
  (Phase 67). `f32` is a real type lowering to LLVM `float` (`f64` stays the
  default `double`); it is a distinct non-coercive type (`f32` ≠ `f64`), so an
  `as` cast bridges them with `fpext` (`f32`→`f64`) / `fptrunc` (`f64`→`f32`),
  an unsuffixed float literal is `f64` by default and narrows to `f32` in
  context, and `1.5f32` pins the width. Integer overflow is now DEFINED as
  two's-complement **wrapping** at every width (`127i8 + 1 == -128`,
  `255u8 + 1 == 0`), identically at compile and run time. Negative narrow-int
  literals narrow in context — `let x: i8 = -128` (i8::MIN) is valid even
  though `+128` would not fit, while `let x: u8 = -1` is a compile error.
- **Capstone** `examples/checksum` (Phase 68) — "the numeric tower, applied":
  three textbook algorithms written in kardashev, each checked against its
  known answer. **FNV-1a** (64-bit) uses a `u64` offset basis past `i64::MAX`
  (`0xcbf29ce484222325`) and a wrapping `u64` multiply; **CRC-32** (IEEE) uses
  a `u32` with a logical `>>`, the bitwise ops, and a branchless mask built by
  wrapping subtraction (`0 - (crc & 1)`); a **binary parser** assembles `u16`
  / `u32` from raw `u8` bytes with shifts and casts in both byte orders. Each
  routine is generic over its input length with a const-generic `[u8; N]`,
  integrating the v10 const-generic line with the whole v11 numeric tower —
  none of it is expressible in an i64-only language.

### Fixed
- A pre-merge adversarial multi-agent review hardened a cluster the green smoke
  suite had missed — every one with a verified repro, now pinned by
  `tests/smoke_test_v11_review.sh`:
  - **(blocker)** a narrow / unsigned `const` flowed into a narrow slot as a
    64-bit immediate — invalid LLVM IR (`call i32 @id(i64 7)`) / verifier
    crash. Codegen now emits a folded const at the const-reference's resolved
    int width.
  - a sized / unsigned `const`'s folded value disagreed with the same
    expression at run time — an unsigned `>>` folded as an arithmetic shift, a
    narrow result was not wrapped to its width (`100i8 + 100i8` → 200 at const
    time vs −56 at run time), and `1i32 << 31` silently held 2147483648 in an
    `i32`. The const evaluator now wraps every result to its expression-type
    width (two's-complement), so an unsigned `>>` is logical and every sized
    const folds identically to run time.
  - a plain-literal narrow / unsigned `const` (`const C: i32 = 100`) was
    rejected though the identical `let` was accepted — `const` now narrows its
    initializer like any other coercion site.
  - `expr as Type << ..` / `expr as Type < ..` was a parse error — the cast's
    target type greedily consumed the `<` / `<<` as a generic-argument list.
    A cast now parses only a bare (numeric) target, leaving the operator for
    the expression parser.
  - an integer/float width suffix was absorbed in tuple-index position
    (`t.0i32` silently became `t.0`) — the suffix is no longer taken after a
    `.`.

## [0.10.0] — Roadmap v10 "sized and sound at compile time" (Phases 57–62)

Theme: **sized and sound at compile time** — const-generic type params + the
effect system's last soundness floor. A pre-merge adversarial multi-agent review
hardened 5 blockers + 5 majors the green smoke suite had missed (see Fixed).

### Added
- Const-generic parameters parse and bind: `const N: i64` (mixed with type
  params), a symbolic `[i64; N]` array length, and the `let (a, b): (T, T) = ..`
  tuple-pattern annotation (Phase 57 — declaration shell only).
- Monomorphization over a const VALUE (Phase 58): `Mat<3>` and `Mat<5>` become
  DISTINCT LLVM struct types (`{ [3 x i64] }` vs `{ [5 x i64] }`, mangled
  `Mat__c3` / `Mat__c5`), incl. nested `Matrix<R, C>` over `[[i64; C]; R]`. The
  const value substitutes the symbolic array length; a struct literal infers
  each `const N` from the dimensions of the field that carries it
  (`Mat { data: [1,2,3] }` is a `Mat<3>`). Type/const argument slot mismatches,
  a const-value dimension mismatch, and negative const args are compile errors.
- Const-generic FUNCTIONS + compile-time dimension unification (Phase 59):
  `fn dot<const N>(a: [i64; N], b: [i64; N]) -> i64` infers N from the argument
  array lengths, lets `N` be used as a value in the body, and monomorphizes per
  size (`@dot__c3` over `[3 x i64]` vs `@dot__c2`). A dimension MISMATCH
  (`dot([i64;3], [i64;2])`) and a const param that appears in no argument array
  type are compile errors.

- `RingBuffer<T, const CAP>` (Phase 61): a struct generic over BOTH a type and
  a const param, with element-wise Drop and deep clone over a NON-Copy element.
  Fixed-size arrays `[T; N]` now allow non-Copy elements (String/struct/Vec/Box)
  — clone element-wise, drop element-wise; moving a non-Copy element out by
  index (`let x = a[i]`) is a compile error (clone or borrow instead). Symbolic
  const params flow through generic impls (`impl<T, const CAP> Clone for
  RingBuffer<T, CAP>`) and `derive(Clone)`. Plus closure-param INFERENCE:
  `vec_map(v, |x| *x * 2)` infers `x`'s type from the callee's fn-typed
  parameter — no `|x: &i64|` annotation needed.
- Array-repeat `[value; N]` (Phase 62) — `N` a literal, const item, or a
  const-generic param (a symbolic length).
- **Capstone** `examples/matrix` (Phase 62) — a fixed-size linear-algebra
  library: `Matrix<const R, const C>` carries its shape in the TYPE,
  `transpose() -> Matrix<C, R>` swaps the dims, and a dimension-checked
  `matmul(Matrix<R, K>, Matrix<K, C>) -> Matrix<R, C>` rejects a shape mismatch
  at COMPILE time (the shared inner dim `K` can't be two values). Integrates the
  whole v10 line: monomorphize-over-a-value, dimension unification, symbolic
  const params, non-Copy arrays, and array-repeat.

### Fixed
- A pre-merge adversarial multi-agent review (6 dimensions) hardened **5
  blockers + 5 majors** the green smoke suite had missed — every one with a
  verified repro — now pinned by `tests/smoke_test_v10_review.sh`:
  - a const param not threaded into a NESTED struct/enum field's type-args
    (`Inner<N>` field of `Outer<N>` mangled `Inner__c0` → LLVM-verifier failure);
  - a bare `b.clone()` on a const-generic struct leaving the const arg symbolic
    (mangled `c0`) → result type confusion;
  - `Drop` is no longer EXEMPT from the effect-subset rule — a `dyn Drop`
    dispatch could launder io/alloc through a pure-declared `Drop` trait;
  - a BOUNDED-generic method call (`<T: Trait>` + `t.method()`) attributed ZERO
    effects (vs the trait's declared effects) — the subset rule's actual floor;
  - forwarding a SYMBOLIC array length alongside a concrete one was accepted
    ill-typed (LLVM miscompile) and legitimate symbolic forwarding was wrongly
    rejected;
  - const-generic ENUM variant payloads (`[i64; N]`) were wrongly rejected;
  - a monomorphization name colliding with a user identifier (`g__i64`) silently
    resolved to the user fn — now a clear compile error;
  - assigning to a non-Copy array element `a[i] = x` was wrongly rejected;
  - array-repeat `[v; N]` ignored a local shadowing a const param;
  - a method-level const param leaked an internal mangled name — now a clear
    "declare it on the impl block" diagnostic.

### Changed
- The **effect-subset rule** (Phase 60), the effect system's last soundness
  floor: a trait impl method's effects must be a SUBSET of the trait method's
  declared effects, so a `dyn Trait` / `<T: Trait>` dispatch (which attributes
  the TRAIT's effects) can never under-count what an impl actually does. A
  super-effecting impl is a compile error. `Drop` is exempt (static drop glue,
  never dyn-dispatched). To make the prelude honest, `Eq::eq`,
  `Iterator::next`, `Display::to_string` and `Default::default` now declare
  `! { alloc }` (their container/heap impls allocate); a concrete `for` loop
  still attributes its concrete `next`'s effects, so pure (Range) loops stay
  pure, and `derive(Eq)` annotates `! { alloc }` only when a field's `eq`
  actually allocates (a map-/Vec-/generic-free struct's derived `eq` is pure).

## [0.9.0] — Roadmap v9 "data in motion" (Phases 51–56)

### Added
- `Box<T>` as a first-class impl target + `&*`/`**` deref ergonomics, and
  prelude `Clone`/`Eq` for `Box<T>`.
- Generic associated functions: a bounded `T::method()` (e.g. `T::default()`).
- `Vec` higher-order combinators `vec_map` / `vec_filter` / `vec_fold` over
  closures (effect-polymorphic).
- String tokenizing (`str_split`, `str_trim`) and `hashmap_entries → Vec<(K,V)>`.
- Capstone `examples/wordfreq` — a word-frequency histogram pipeline.

### Fixed
- A pre-merge adversarial review hardened 5 memory-safety / type-soundness
  holes the green smoke suite had missed (by-value container-getter double-free,
  `dyn Trait<T>` argument confusion, move-out-of-`&` via `*r`, `&mut` reborrow
  aliasing, an unjoined `if`-branch move-state) plus dyn/generic effect
  attribution — locked in by `tests/smoke_test_soundness.sh`.

## [0.8.0] — Roadmap v8 "generics, finished" (Phases 45–50)

### Added
- Bounded type params (`K: Hash + Eq`) inside container ops; prelude `Clone`/`Eq`
  trait impls for `HashMap`.
- `Ord` trait + a generic in-place `sort<T: Ord>` (+ `vec_swap`, `&mut → &`
  reborrow).
- `#[derive(Hash, Ord, Default)]` and associated functions (static
  `Type::method()`).
- `dyn Trait<T>` generic trait objects + dispatch through `Vec<Box<dyn …>>`.
- Capstone `examples/json` upgraded to JSON 3.0 — `HashMap<String, Json>`
  objects, fully `#[derive]`d, canonical sorted-key output.

## [0.7.0] — Roadmap v7 "real numbers, real abstraction" (Phases 39–44)

### Added
- `f64` floating point.
- Generic `impl<T: Bound>` blocks; generic `Clone`/`Eq` over containers;
  `#[derive(Clone, Eq, Display)]`.
- Runtime string escapes; the last async-frame leak closed.
- Capstone JSON 2.0 — floats + decoded escapes + derived `Clone`/`Eq`.

## [0.6.0] — Roadmap v6 "make the heap recursive" (Phases 33–38)

### Added
- Sound recursive heap-owning enums (`Box`/`Vec<Self>`/`HashMap<K,Self>`) with
  recursive `Drop` + deep `clone`; read-without-move + `match`-by-reference;
  enum-typed struct fields + non-Copy tuples; `Display` + de-`i64`'d iteration.
- Capstone: a full nested-JSON parser + serializer written in kardashev.

### Fixed
- An `-O1+` miscompile: the optimizer ran without the target datalayout, folding
  multi-field-aggregate reads-through-a-pointer to wrong byte offsets.

## [0.5.0] — Roadmap v5 (Phases 27–32)

### Added
- Stdlib depth (string toolkit, generic `HashMap<K,V>`), file I/O + CLI args,
  `Drop`-leak fixes, and self-written capstones (`examples/calc`,
  `examples/rpn`). Docs + a source-comment truth pass.

## [0.4.0] — Roadmap v4 (Phases 21–26)

### Added
- Generic trait parameters + associated types + `where` clauses; fixed-size
  arrays `[T; N]` + tuples `(A, B)`; compile-time `const` items + const
  evaluation (incl. const-generic array lengths); `extern "C"` FFI; an
  arithmetic-interpreter capstone written in kardashev.

## [0.3.0] — Roadmap v3 (Phases 15–20)

### Added
- Expression & item completeness (bool/unary ops, inherent impls); deterministic
  memory management — `Drop`/RAII with runtime drop flags; real panic + unwinding
  with cleanup; OS threads + `Mutex`; opt-level flags + the `kardc --test` runner.

## [0.2.0] — Roadmap v2 (Phases 9–14)

### Added
- Iteration (loops, ranges, `for`); closures + effect-carrying function types
  (first-class fn values, `FnMut` captures); `dyn Trait` dynamic dispatch; a
  growable stdlib (`String`, `HashMap`, `&[T]` slices, `map`/`filter`/`fold`
  combinators, `Option`/`Result` combinators); the source formatter (`kardfmt`)
  and richer LSP.

## [0.1.0] — Roadmap v1 (Phases 0–8)

### Added
- The MVP and foundation: the full pipeline (lexer → parser → Hindley-Milner
  type inference → LLVM IR → ORC JIT + AOT); ownership + non-lexical-lifetime
  borrow checking; ADTs + pattern matching; traits + generics + monomorphization;
  `Result` + the `?` operator; **effect labels** in signatures (the signature
  feature) with effect-row polymorphism; a minimal stdlib (`Option`/`Result`/
  `Vec`/`String`) + AOT pipeline; `async`/`await` + a single-thread executor;
  the module system + `kard` CLI + `rules_kardashev` Bazel rules; `-O0..-O3`
  pass pipelines + the `kard-lsp` language server.
