# kardashev Gen 2 — Language & Toolchain Specification (v0.111.0)

> This is the **single source of truth** for the Gen-2 reboot: a Rust
> implementation of a small systems language built around Zig's philosophy.
> Every compiler module is implemented against this document and the type
> contract in `crates/kardc/src/{span,diag,token,types,ast}.rs`.

## 0. Design laws (Zig-philosophy)

1. **No hidden control flow.** No exceptions, no operator overloading, no
   implicit destructors. The *only* deferred-execution construct is `defer`,
   and it is explicit.
2. **No hidden allocations.** v1 has no heap; when containers arrive they will
   take an explicit allocator. There is never an implicit global allocator.
3. **Compile-time evaluation, not macros.** `comptime` folds expressions at
   compile time. There is no preprocessor and no textual macros.
4. **Tests are first-class.** `test "name" { ... }` blocks live in the source
   and run with `kard test`.
5. **One self-contained toolchain.** A single `kard` binary is compiler, build
   system, test runner and formatter. The build is described in-language in
   `build.ks`.

## 1. Lexical grammar

- **Whitespace** (` `, `\t`, `\r`, `\n`) separates tokens and is otherwise
  ignored.
- **Comments**: `//` to end of line. (No block comments in v1.)
- **Identifiers**: `[A-Za-z_][A-Za-z0-9_]*`. If the spelling is a keyword it
  lexes as `Keyword`, else `Ident`. Type names (`i32`, `bool`, …) are **not**
  keywords — they are identifiers resolved in sema.
- **Keywords**: `pub fn const var return if else while break continue defer
  comptime test true false and or`.
- **Integer literals**: `[0-9]+`, decimal, parsed into `i64`. Out-of-range →
  diagnostic `E0002`. (Hex / `_` separators are a later roadmap item.)
- **String literals**: `"…"` with escapes `\n \t \\ \"`. Used only for test
  names in v1.
- **Operators / punctuation**: `( ) { } [ ] , ; : .` and
  `= == != < <= > >= + - * / % !`. Two-char operators (`== != <= >=`) take
  priority over their one-char prefixes; `!` followed by `=` is `!=`, else `!`.
- The token stream always ends with a single `Eof` token.

## 2. Syntax grammar

```
module      := item*
item        := func | const_decl | test_block
func        := "pub"? "fn" IDENT "(" params? ")" type block
params      := param ("," param)* ","?
param       := IDENT ":" type
const_decl  := "pub"? "const" IDENT ":" type "=" expr ";"
test_block  := "test" STRING block
type        := IDENT                      // resolved to a builtin Type in sema

block       := "{" stmt* "}"
stmt        := let_stmt | assign_stmt | return_stmt | if_stmt | while_stmt
             | "break" ";" | "continue" ";" | defer_stmt | block | expr_stmt
let_stmt    := ("var" | "const") IDENT ":" type "=" expr ";"
assign_stmt := IDENT "=" expr ";"
return_stmt := "return" expr? ";"
if_stmt     := "if" "(" expr ")" block ("else" (if_stmt | block))?
while_stmt  := "while" "(" expr ")" (":" "(" loop_cont ")")? block
loop_cont   := IDENT "=" expr | expr   // a continue-clause statement (no ";")
defer_stmt  := "defer" stmt
expr_stmt   := expr ";"

expr        := or_expr
or_expr     := and_expr ("or" and_expr)*
and_expr    := cmp_expr ("and" cmp_expr)*
cmp_expr    := add_expr (("=="|"!="|"<"|"<="|">"|">=") add_expr)*
add_expr    := mul_expr (("+"|"-") mul_expr)*
mul_expr    := unary  (("*"|"/"|"%") unary)*
unary       := ("-" | "!") unary | comptime_expr
comptime_expr := "comptime" primary | primary
primary     := INT | "true" | "false" | IDENT | call | "(" expr ")"
call        := IDENT "(" args? ")"
args        := expr ("," expr)* ","?
```

Notes:
- Parentheses around `if`/`while` conditions are **required** (Zig style).
- `if`/`while` bodies are always braced blocks — no single-statement bodies.
- Comparison operators are left-associative; chaining (`a < b < c`) parses but
  is a type error (`<` yields `bool`, and `bool < int` fails in sema).
- `comptime` binds a single primary; wrap compound expressions in parens:
  `comptime (2 + 3)`.

## 3. Types & semantic rules (`sema`)

Builtin types: `i8 i16 i32 i64 u8 u16 u32 u64 usize bool void`.

`sema::check(&Module) -> Result<(), Vec<Diagnostic>>` validates, in one pass
with a scope stack:

- **Name resolution.** Every `IDENT` used as a value resolves to a parameter,
  an in-scope local, or a top-level `const`. Every `call` callee resolves to a
  user `fn` or a builtin (`print`, `expect`). Unknown name → `E0100`.
- **No shadowing of builtins.** Defining a `fn` named `print` or `expect` →
  `E0101`.
- **Types.**
  - Integer literals are polymorphic and adopt the expected integer type at
    their use site; with no expectation they default to `i64`.
  - Binary arithmetic (`+ - * / %`) requires both operands the same integer
    type; result is that type.
  - Comparisons (`== != < <= > >=`) require both operands the same type
    (int or bool); result is `bool`.
  - `and`/`or` require both operands `bool`; result `bool`.
  - Unary `-` requires a signed integer; `!` requires `bool`.
  - `if`/`while` conditions must be `bool`.
  - Assignment target must be a `var` local (not a `const`, not a param);
    RHS type must match the declared type. Type mismatch → `E0110`.
  - `var`/`const` initializer type must match the declared type.
  - `return e` type must match the function return type; `return;` only in a
    `void` function.
- **`break`/`continue`** are only valid inside a `while` body → else `E0120`.
- **`comptime e`** and **top-level `const` initializers** must be
  const-evaluable via `const_eval::eval` over the top-level consts defined
  *earlier* in source order. Non-constant → `E0130`. A `const` referencing a
  later/undefined const → `E0131`.
- **Builtins.**
  - `print(x)` — `x` must be an integer type; returns `void`; valid anywhere.
  - `expect(c)` — `c` must be `bool`; returns `void`; valid **only inside a
    `test` block** → else `E0140`.
- **Program entry.** Whether a `main` is required is decided by the driver, not
  sema (a file may be a library). The driver requires `fn main` for
  `build`/`run`; `main` must return `void`, `i32` or `i64`.

`const_eval::eval` supports: integer & bool literals, references to known
consts, unary `- !`, and all binary operators, with the same type rules. It
returns `ConstVal::{Int,Bool}` or a diagnostic.

## 4. C backend (`emit_c`)

`emit_c::emit(&Module, EmitMode) -> String` lowers a **validated** module to
portable C11. Determinism matters: identical input → byte-identical output.

### 4.1 Prelude & naming

Every emitted file begins with:

```c
#include <stdint.h>
#include <stdbool.h>
#include <stdio.h>
static void kd_print(long long v) { printf("%lld\n", v); }
```

- **All user identifiers** (functions, params, locals, consts) are prefixed
  with `kd_` in the output. This sidesteps every C keyword/identifier clash.
  So source `main` → `kd_main`, local `x` → `kd_x`, const `MAX` → `kd_MAX`.
- Emission order: prelude, top-level consts, **forward declarations** of every
  function, function definitions, then the generated C `main`.

### 4.2 Top-level consts

Emit each as `static const <cty> kd_<name> = <literal>;` where the literal is
produced by evaluating the initializer with `const_eval` over the consts
emitted so far (C does **not** treat `const` variables as constant expressions,
so the value must be folded to a literal). Bool literals emit as `true`/`false`.

### 4.3 Statements & expressions

Straightforward 1:1 lowering. `var`/`const` → `<cty> kd_<name> = <expr>;`
(local `const` may also be `const <cty>`). `assign` → `kd_<name> = <expr>;`.
`if`/`while` map to C `if`/`while`; a `while (c) : (cont) {…}` lowers the
continue-expression to run at the end of each iteration **and** before each
`continue` (see defer rules). `print(x)` → `kd_print((long long)(<x>))`.
Expression operators use `BinOp::c_op` / unary spellings. Parenthesize
sub-expressions to preserve precedence.

### 4.4 `defer` lowering — the careful part

Maintain a stack of scopes during emission; each scope holds its registered
`defer` statement bodies in registration order, plus a flag marking whether it
is a **loop-body** scope. A `defer S;` does **not** emit `S` immediately — it
pushes `S` onto the current scope. Deferred statements run in **LIFO** order at
every exit edge:

- **Fall-through** off the end of a block: emit that scope's defers in reverse
  registration order, then pop the scope.
- **`return e`:** if any defer is active *and* the function is non-void,
  evaluate the return value into a temporary first
  (`<ret_cty> __kd_ret = (<e>);`), then flush **all** scopes from innermost to
  the function scope (each in reverse registration order), then `return
  __kd_ret;`. For void, or when no defer is active, flush (if any) then emit
  `return;`/`return (<e>);` directly.
- **`break` / `continue`:** flush scopes from innermost up to **and including**
  the nearest loop-body scope (reverse order each), then emit C
  `break;`/`continue;`. For a `while (c) : (cont)`, the continue-expression is
  emitted *after* those defers and *before* the C `continue;`.

The same return-flush path lowers `expect(c)` failures in test mode (see 4.5).

### 4.5 Emit modes

- **`EmitMode::Program`:** assume a `kd_main` exists. Emit
  ```c
  int main(int argc, char **argv) { (void)argc; (void)argv; <wire>; }
  ```
  where `<wire>` is `return (int) kd_main();` if `main` returns an integer, or
  `kd_main(); return 0;` if it returns `void`.
- **`EmitMode::Test`:** each `test "name"` block becomes
  `static int kd_test_<idx>(void) { <body>; return 0; }`, where `expect(c)`
  lowers to `if (!(<c>)) { <flush active defers>; return 1; }`. The C `main`
  runs every test, prints `ok: <name>` / `FAIL: <name>` to stderr and a final
  `<passed>/<total> tests passed` line, and returns the failure count as the
  process exit code (0 = all passed). In test mode no user `main` is wired.

## 5. Native driver (`backend`)

- `cc_build(c_src, out, opts)` — write `c_src` to a temp `.c`, invoke the
  system C compiler (`$CC`, else `cc`, `clang`, `gcc` — first found) as
  `<cc> <-O0|-O2> -std=c11 -o <out> <tmp.c>`; return its stderr on non-zero
  exit. The optimization level comes from `BuildOptions::opt` and defaults to
  `-O2` (`kard build`, `bench` and cross-compiles); `kard run`/`test` build
  unoptimized dev binaries at `-O0` for fast iteration, and their `--release`
  flag restores `-O2`.
- `cc_build_and_run(c_src, args, opt)` — build to a temp executable at `opt`,
  exec it with `args`, return the child exit code.

The lex→parse→sema→emit pipeline is `kardc::compile_to_c`; `backend` only does
cc + process execution.

## 6. CLI (`cli`) — the `kard` binary

```
kard build [FILE] [-o OUT] [-target TRIPLE]   # compile to a native executable
kard run   [FILE] [-- ARGS...]                 # build to temp, run, propagate exit code
kard test  [FILE]                              # build+run the test harness
kard fmt   FILE [--check | -w]                 # canonical formatting
kard init  [NAME]                              # scaffold a new project
kard version                                   # print the version (also --version, -V)
kard help                                      # usage (also --help, -h, no args)
```

- With no `FILE`, `build`/`run`/`test` read `./build.ks` for the `root` source
  and (for `build`) the output `name`.
- `-target` is accepted and, for v1, passed through to the C compiler's
  `-target` flag where supported; the full cross-compilation matrix is a
  roadmap item, documented honestly.
- Compile diagnostics are rendered with `diag::render_all` (filename + line/col
  + caret) to stderr; the process exits non-zero.
- `fmt --check` exits non-zero if the file is not already canonical; `-w`
  rewrites in place; otherwise canonical source is printed to stdout.
- v1 formats from the AST, which carries no comment trivia, so **comments are
  not yet preserved** by `fmt` (the code is reproduced faithfully and the
  result is idempotent). Comment-preserving formatting is a roadmap item.

## 7. `build.ks` — the build graph (v0.122)

A `build.ks` describes a **build graph** of one or more named executable
targets. Two forms are accepted:

```
// Single-target sugar (legacy):
build {
    name = "hello";
    root = "src/main.ks";
}

// Multi-target:
build {
    exe "app"  { root = "src/main.ks"; }
    exe "tool" { root = "src/tool.ks"; }
}
```

`build_system::parse_build_kd` returns a `BuildSpec { targets: Vec<Target> }`
(`Target { name, root }`); `BuildSpec::select(name)` finds a target by name (or
the sole target when unnamed). It is a tiny self-contained recursive parser
(not the language lexer/parser), tolerant of whitespace and `//` comments; any
malformed/missing field yields `E0300`.

CLI (`build`/`run`/`test` with no `FILE`): read `./build.ks`; a positional
**TARGET** name selects one target; with no name and a single target, that
target is used; with no name and multiple targets, `build` builds **all** of
them (and `run`/`test` require a target name → error otherwise). A positional
argument ending in `.ks` is treated as a direct FILE, not a target name.

The **full imperative** build graph (Zig's `build.zig` model — a kardashev
program with a `build(*Builder)` entry point, step dependencies and install
artifacts) remains a future item.

## 8. Honest deferrals (tracked in ROADMAP-RUST-ZIG.md)

Optionals `?T`, error unions `!T` + `try`/`catch`/`errdefer`, struct **methods /
associated functions** (struct *data* lands in v0.112 — see §9), enums,
tagged unions, arrays/slices/pointers, the allocator interface and an
allocator-based stdlib, generics via `comptime T: type`, type inference for
`var`/`const`, the full imperative `build.ks`, the real cross-compilation
matrix, comment-preserving `fmt`, and re-self-hosting. None of these are stubbed
— they are absent and scheduled.

## 9. Structs (v0.112) — data aggregates

A struct is a named, by-value product type. v0.112 ships struct **data**;
methods / associated functions are v0.113.

### 9.1 Syntax (grammar additions)

```
item        := func | const_decl | test_block | struct_decl
struct_decl := "pub"? "const" IDENT "=" "struct" "{" field ("," field)* ","? "}" ";"
             | "pub"? "const" IDENT "=" "struct" "{" "}" ";"        // empty struct
field       := IDENT ":" type

primary     := ... | struct_lit
struct_lit  := IDENT "{" (field_init ("," field_init)* ","?)? "}"
field_init  := "." IDENT "=" expr
postfix     := primary ("." IDENT)*          // field access chains
```

- `const Point = struct { x: i32, y: i32 };` — a struct declaration. It is a
  top-level item (parsed when a `const`'s `=` is followed by the `struct`
  keyword; otherwise `const` is the ordinary value binding of §2).
- `Point{ .x = 1, .y = 2 }` — a struct literal. Every declared field must be
  initialised exactly once; order is free.
- `p.x` — field access (`postfix`). Chains: `a.b.c`.
- `p.x = e;` — field assignment (`Stmt::FieldAssign`); the place may be a chain
  `a.b.c`. Simple `name = e;` remains `Stmt::Assign`.

The `IDENT {` struct-literal form is unambiguous: bare blocks never start an
expression, and `if`/`while` conditions are parenthesised, so a `{` following an
identifier in expression position always opens a struct literal.

### 9.2 AST additions (`ast.rs`)

```
Item::Struct(StructDecl)
StructDecl { is_pub: bool, name: String, fields: Vec<FieldDecl>, span }
FieldDecl  { name: String, ty: TypeExpr, span }
Expr::StructLit { name: String, fields: Vec<FieldInit>, span }
FieldInit  { name: String, value: Expr, span }
Expr::Field { base: Box<Expr>, field: String, span }
Stmt::FieldAssign { place: Expr /* a Field */, value: Expr, span }
```

### 9.3 Types (`types.rs`)

`Type` gains `Struct(u32)` (an id into the `StructTable`; stays `Copy`; two
struct types are equal iff same id). `Type::name()` returns `"struct"` for a
struct (sema formats real names via the table); `Type::c_name()` is
`unreachable!()` for `Struct` — emit resolves struct C names through the table.

```
StructInfo  { name: String, fields: Vec<(String, Type)> }
StructTable { /* id <-> name, field lists */ }
  ::new(), ::intern(name)->u32, ::id_of(name)->Option<u32>,
  ::get(id)->&StructInfo, ::set_fields(id, fields),
  ::c_name(id)->String  // "kd_struct_<Name>",  ::iter() in declaration order
```

### 9.4 Semantics (`sema`) — new signature

`sema::check(&Module) -> Result<StructTable, Vec<Diagnostic>>`. In a pre-pass,
collect struct declarations in source order, intern ids, then resolve each
field's type (a field type name resolves via `Type::from_name` or a *previously
declared* struct — a forward/cyclic struct reference is `E0160`; an unknown type
is `E0161`; a duplicate field name is `E0162`). Then check bodies:

- **Struct literal** `Name{…}`: `Name` must be a struct (`E0163` otherwise);
  every field present exactly once with a matching type, none missing/extra/
  duplicated (`E0164`); result type `Struct(id)`.
- **Field access** `e.f`: `e` must be a struct (`E0165`); `f` must be a field
  (`E0166`); result is the field's type.
- **`FieldAssign`** `place = e`: `place` must be a field-access chain rooted in
  an assignable `var` (not a `const`/param) (`E0167`); `e`'s type must match the
  field type (`E0110`).
- Struct-typed params/locals/returns are allowed; assignment/return type checks
  compare struct ids. `==`/`!=` on structs is `E0168`. `print`/`expect` reject
  struct arguments (still int/bool).

### 9.5 Backend (`emit_c`) — new signature

`emit_c::emit(&Module, &StructTable, mode) -> String`. Emit, after the prelude
and before function forward-decls, one C typedef per struct **in declaration
order**:

```c
typedef struct { <cty(f.ty)> kd_<f.name>; ... } kd_struct_<Name>;
```

(An empty struct gets a `char _unused;` member so it is valid C.) Lowerings:
field access `e.f` → `(<e>).kd_<f>`; struct literal → `((kd_struct_<Name>){
.kd_<f> = <e>, ... })` (C99 compound literal); `FieldAssign place = e;` →
`(<place>) = (<e>);`. Struct-typed locals/params/returns use the typedef'd type
(`cty` maps `Struct(id)` → `structs.c_name(id)`). C passes/returns structs by
value, matching the language semantics. Output stays deterministic.

## 10. Struct methods & associated functions (v0.113)

Functions may be declared inside a `struct` body, after the fields:

```
const Counter = struct {
    n: i32,
    pub fn get(self: Counter) i32 { return self.n; }
    pub fn bumped(self: Counter, by: i32) Counter { return Counter{ .n = self.n + by }; }
    pub fn zero() Counter { return Counter{ .n = 0 }; }   // associated (no self)
};
```

- **Grammar:** the struct body is `(field ",")* (func)*` (fields first, then
  `pub? fn …` items). `StructDecl` gains `methods: Vec<Func>`.
- A function whose **first parameter is named `self`** is a *method*; otherwise
  it is an *associated function*.
- **Call:** `receiver.method(args)` parses to `Expr::MethodCall{receiver,
  method, args}` (the postfix `.name` followed by `(`). sema resolves:
  - receiver is a **struct value** → method call: look up `method` in that
    struct's functions (must be a method); the receiver becomes `self`; the
    remaining params bind `args`.
  - receiver is an **`Ident` naming a struct type** → associated call: look up
    `method`; bind `args` to *all* its params (so `Counter.get(c)` is the
    explicit-self form, `Counter.zero()` the static form).
  - Diagnostics: unknown method `E0170`; arity mismatch `E0171`; calling a
    method statically without the self arg, or an assoc fn on a value,
    `E0172`; method/arg type mismatch reuses `E0110`.
- **Lowering:** each struct function emits a free C function
  `kd_<Struct>_<method>(<params>)` (the `self` param is an ordinary by-value
  `kd_struct_<Struct>` parameter). A `MethodCall` emits
  `kd_<Struct>_<method>(<receiver-as-self-if-method>, <args>)`. Method bodies
  reuse all existing statement/expr/`defer` lowering. Forward-declare struct
  functions alongside ordinary functions.

## 11. Optionals (v0.114)

`?T` makes nullability explicit and checked. v0.114: `T` is a primitive or a
struct; no nesting (`??T`).

### 11.1 Syntax & AST
- Type `?T`: `TypeExpr.optional = true` (the `?` precedes the type name in
  params/returns/`var`/`const`/struct fields). Resolves to
  `Type::Optional(StructTable::intern_optional(inner))`.
- `null` — `Expr::Null`. Its type is `?T` taken from the expected type at its
  position; a `null` with no expected optional type is `E0180`.
- `x orelse y` — `Expr::Orelse{lhs,rhs}`: `lhs` must be `?T` (else `E0181`),
  `rhs` must be `T`; result `T`.
- `x.?` — `Expr::Unwrap{expr}`: `expr` must be `?T` (else `E0182`); result `T`;
  **panics (stderr message + exit 101) if null**. Lexes as `Dot` then
  `Question`.

### 11.2 Coercion
A value of type `T` **coerces implicitly to `?T`** at positions with a known
expected optional type: `var`/`const` initializers with a `?T` annotation,
assignment to a `?T` place, `return` in a `?T` function, a call argument whose
param is `?T`, and a struct field-init whose field is `?T`. `null` also takes
the expected `?T`. (This widening is explicit in intent and has no hidden
control flow.) Type mismatches are `E0110`.

### 11.3 Backend (`emit_c`)
For each interned optional (`StructTable::optionals()`), after the struct
typedefs emit:
```c
typedef struct { bool has; <inner cty> val; } kd_opt_<tag>;          // tag = type_mangle
static inline <inner cty> kd_opt_<tag>_orelse(kd_opt_<tag> o, <inner cty> d) { return o.has ? o.val : d; }
static inline <inner cty> kd_opt_<tag>_unwrap(kd_opt_<tag> o) { if (!o.has) { fputs("panic: unwrapped a null optional\n", stderr); exit(101); } return o.val; }
```
(Add `<stdlib.h>` to the prelude for `exit`.) Lowerings: `null` (expected `?T`)
→ `((kd_opt_<tag>){ .has = false })`; a coerced `T` value → `((kd_opt_<tag>){
.has = true, .val = <e> })`; `x orelse y` → `kd_opt_<tag>_orelse(<x>, <y>)`
(`y` is evaluated eagerly in v0.114); `x.?` → `kd_opt_<tag>_unwrap(<x>)`. `cty`
maps `Type::Optional(id)` → `StructTable::optional_c_name(id)`.

emit decides coercion with a `type_of_expr` helper over an environment it
maintains while emitting a function (param + local types) plus the
`StructTable` (struct field types, optional inners) and the module (fn / method
return types, struct-literal names): at a known-expected-`?T` position, if the
source is `null` or its `type_of_expr` is the inner `T`, wrap it; if it is
already `?T`, pass it through.

Deferred to a later increment (honest): `if (opt) |v| { … }` payload capture.

## 12. Error unions (v0.115)

Errors are values. v0.115 uses a single **implicit global error set** (an
`anyerror`-like set built from every `error.Name` mentioned in the program).

### 12.1 Syntax & AST
- Type `!T`: `TypeExpr.error_union = true` (e.g. `fn f() !i32`). Resolves to
  `Type::ErrorUnion(StructTable::intern_error_union(payload))`. (Not combined
  with `?` in v0.115.)
- **`!void`** (a `void` payload) is allowed (v0.156): success carries no
  value. A `!void` function returns success via a bare `return;` or by falling
  off the end of its body (like a `void` function); `return error.X;` is the
  failure form and `return g();` (for `g() !void`) passes the union through.
  A bare `return;` stays an error (`E0110`) for any payload-carrying `!T`.
- `error.Name` — `Expr::ErrorLit{name}`; registers `Name` via
  `StructTable::intern_error` (1-based code; 0 = "no error"). Coerces to any
  `!T`.
- `try expr` — `Expr::Try{expr}`. **v0.115: statement-level only** — allowed as
  the whole value of a `var`/`const` initializer, a `return`, or an expression
  statement; anywhere else is `E0191`. The enclosing function must return some
  `!U` (`E0190`). On error it returns the error from the enclosing function;
  otherwise it yields the payload.
- `expr catch default` — `Expr::Catch{expr,default}` (parses at the lowest
  precedence, beside `orelse`). `expr` must be `!T` (`E0192`); `default` is a
  `T`; result `T`; `default` is evaluated eagerly in v0.115. **Exception
  (v0.156):** over a `!void` operand the handler is necessarily **lazy** — it
  runs as a statement on the error path only (there is no payload value to
  select eagerly, and a `void` handler exists for its effect).

### 12.2 Coercion
`T → !T` (success) and `error.X → !T` (failure) at typed positions (initializer
with `!T` annotation, assignment, return, call arg, struct field). Mismatches
reuse `E0110`.

### 12.3 Backend (`emit_c`)
For each `StructTable::error_unions()`, after the optionals, emit (in dependency
order, see §11.3) :
```c
typedef struct { int32_t err; <payload cty> val; } kd_err_<tag>;
static inline <payload cty> kd_err_<tag>_catch(kd_err_<tag> e, <payload cty> d) { return e.err == 0 ? e.val : d; }
```
Lowerings: `error.Name` (expected `!T`) → `((kd_err_<tag>){ .err = <code> })`; a
coerced `T` → `((kd_err_<tag>){ .err = 0, .val = <e> })`; `expr catch default` →
`kd_err_<tag>_catch(<expr>, <default>)`. **`try expr`** at a statement position
lowers to a hoisted temp + propagation, using the enclosing function's
error-union C type `<RET>`:
```c
kd_err_<tag> __kd_tryN = <expr>;
if (__kd_tryN.err != 0) { <flush active defers>; return (<RET>){ .err = __kd_tryN.err }; }
/* then: bind/return/use __kd_tryN.val */
```
`cty(Type::ErrorUnion(id))` → `StructTable::error_union_c_name(id)`.

A **`!void`** union lowers **payload-less** (v0.156): `typedef struct {
int32_t err; } kd_err_void;` — no `val` field (`void val` is invalid C) and no
`_catch` helper. Its lowerings special-case the missing payload: `try f();`
hoists and propagates as above but unwraps to no value (`((void)0)`,
discarded); `e catch handler` / `e catch |err| handler` hoist `e` once and run
the (void) handler as a statement inside `if (eu.err != 0) { … }` — the
capture binds `int32_t kd_<err>` exactly as in §36; success construction
writes only `{ .err = 0 }` (a bare `return;`, fall-through off the body end,
and a `void` expression coerced to `!void` — the latter via a comma
expression evaluating the source for effect).

### 12.4 Deferred (honest)
`errdefer`, `catch |e|` capture, explicit named error sets `error{ … }`, and
`try` in arbitrary (nested) expression positions.

## 13. Enums & `switch` (v0.116)

Plain (C-like) enums plus an exhaustive `switch`. Tagged-union payloads
(`union(enum)`) and payload capture are a later roadmap item.

### 13.1 Syntax & AST
- `pub? const Name = enum { A, B, C };` → `Item::Enum(EnumDecl{name,variants})`,
  `Type::Enum(id)`. Variants are 0-based.
- Enum values: **`Name.Variant`** (qualified — reuses `Expr::Field` with an
  `Ident(Name)` base; sema recognises the base as an enum type) and
  **`.Variant`** (`Expr::EnumLit`; its enum type comes from context — the
  expected type / the `switch` scrutinee).
- `switch (scrutinee) { arm* else_arm? }` → `Stmt::Switch{scrutinee, arms,
  default}`. An arm is `label ("," label)* "=>" block` (`SwitchArm{labels:
  Vec<Expr>, body: Block}`); the optional final `else => block` is `default`.
  Labels are constant patterns: enum literals (`.V` / `Enum.V`) for an enum
  scrutinee, or integer literals for an integer scrutinee. Arms are separated by
  `,` (a trailing `,` after a `}` block is optional).

### 13.2 Semantics (`sema`)
- Pre-pass: intern enum decls (`StructTable::intern_enum` + `set_enum_variants`);
  duplicate variant name within an enum is an error. Resolve `enum` type names.
- `Enum.V` / `.V`: `V` must be a variant of the (resolved/expected) enum
  (`E0212`); `.V` with no enum context is `E0215`. Result type `Enum(id)`.
- `switch`: the scrutinee is an enum or an integer type (`E0213` otherwise).
  Every label must be a valid pattern for the scrutinee type; an enum label not
  a variant, or a duplicated label, is `E0212`/`E0211`. For an **enum**
  scrutinee, the arms must cover **every** variant exactly once **or** include an
  `else` (`E0210` if neither). For an **integer** scrutinee an `else` is
  required (`E0214`). Each arm body is checked as a block.

## 14. Fixed-size arrays `[N]T` (v0.117)

Value-semantics arrays of a compile-time-constant length. Element type is a
primitive or struct in v0.117 (not itself an array/optional/error-union).

### 14.1 Syntax & AST
- Type `[N]T`: `TypeExpr.array_len = Some(N)`, `name = T`. `N` is a
  non-negative integer literal. Resolves to
  `Type::Array(StructTable::intern_array(elem, N))`.
- Array literal `[N]T{ e0, e1, … }` → `Expr::ArrayLit{elem, elems}` with exactly
  `N` elements, each coercible to `T`. Result `Type::Array(id)`.
- Indexing `a[i]` → `Expr::Index{base, index}` (postfix, composes with
  `.field`/calls). `base` must be an array; `index` an integer; result `T`.
- Index assignment `a[i] = e` reuses `Stmt::FieldAssign` with an `Index` place.
- `a.len` reuses `Expr::Field` with field `len` on an array → a `usize` constant.
- Arrays are **value types** — assignment / parameter / return copy the whole
  array.

### 14.2 Semantics (`sema`)
`a[i]`: `base` must be `Array(id)` (`E0220`); `index` an integer; result is the
element type. ArrayLit: element count must equal `N` (`E0221`); each element
coerces to the element type (`E0110`). `a.len`: only on arrays (else fall back
to struct field rules); type `usize`. Index-assign: the indexed place must be a
mutable `var` array (`E0223`); value coerces to the element type. A negative or
absurd `N` is `E0224`.

### 14.3 Backend (`emit_c`)
Emit arrays among the dependency-ordered type defs (an array depends on its
element type):
```c
typedef struct { <elem cty> data[N]; } kd_arr_<tag>_<N>;
static inline <elem cty> kd_arr_<tag>_<N>_get(kd_arr_<tag>_<N> a, int64_t i) {
    if (i < 0 || (uint64_t)i >= N) { fputs("panic: array index out of bounds\n", stderr); exit(101); }
    return a.data[i];
}
```
Lowerings: `[N]T{ … }` → `((kd_arr_<tag>_<N>){ .data = { e0, e1, … } })`; `a[i]`
(read) → `kd_arr_<tag>_<N>_get(<a>, <i>)`; `a.len` → `((uintptr_t)N)`. An
index-assign `a[i] = e;` lowers to a bounds-checked block:
```c
{ int64_t __kd_idxK = (<i>); if (__kd_idxK < 0 || (uint64_t)__kd_idxK >= N) { fputs("panic: array index out of bounds\n", stderr); exit(101); } (<a>).data[__kd_idxK] = (<e>); }
```
`cty(Type::Array(id))` → `StructTable::array_c_name(id)`.

## 15. Pointers `*T` & slices `[]T` (v0.118)

Raw single pointers and `{ptr, len}` slice views. Lifetimes are the
programmer's responsibility (no borrow checking), as for Zig's raw pointers.

### 15.1 Pointers
- Type `*T`: `TypeExpr.pointer = true` → `Type::Ptr(intern_ptr(T))`.
- `&place` → `Expr::AddrOf{place}`: `place` must be an lvalue (a `var`, a field
  chain, an index, or a deref) (`E0231`); result `*T`.
- The place must **not** be rooted in a `const` binding's own storage
  (`E0233`, v0.156): there are no const pointers, so the `*T` would let the
  `const` be mutated. The rule mirrors assignment (§9.4/§14.2): a hop through
  a pointer (a deref, a `*Struct` field base) or through a **slice** index
  reaches storage the binding does not own, so such places stay addressable
  even under a `const` root; immutable **parameters** also remain addressable
  (the pointer aliases the parameter's local copy).
- `p.*` → `Expr::Deref{expr}`: `expr` must be `*T` (`E0230`); result `T`.
- `p.* = e` reuses `Stmt::FieldAssign` with a `Deref` place; `e` coerces to `T`.
- C: `cty(Ptr) = "<T cty>*"`; `&place` → `(&(<place>))`; `p.*` → `(*(<p>))`;
  deref-assign → `*(<p>) = (<e>);`.

### 15.2 Slices
- Type `[]T`: `TypeExpr.slice = true` → `Type::Slice(intern_slice(T))`. C:
  `typedef struct { <T cty> *ptr; uintptr_t len; } kd_slice_<tag>;`.
- `base[lo..hi]` → `Expr::SliceExpr{base, lo, hi}`: `base` is an **array** (an
  addressable `var`) or a slice (`E0232`); `lo`,`hi` integers; result `[]T`.
  Runtime-checked `0 <= lo <= hi <= len` (panic exit 101 otherwise).
- `s[i]` (`Expr::Index` on a slice) → element `T`, runtime-bounds-checked.
- `s[i] = e` reuses `Stmt::FieldAssign` with an `Index` place on a slice.
- `s.len` (`Expr::Field` `len`) → `usize`.
- C: a slice from an array `a` lowers to `(kd_slice_<tag>){ .ptr = (a).data +
  <lo>, .len = <hi> - <lo> }` after a bounds check on `lo`/`hi`; from a slice,
  `.ptr = (base).ptr + <lo>`. `s[i]` read → a bounds-checked accessor
  `kd_slice_<tag>_get(s, i)` (`if (i<0 || (uint64_t)i>=s.len) panic; return
  s.ptr[i];`). `s[i] = e` → a bounds-checked block writing `(s).ptr[i] = e`.
  `s.len` → `(s).len`. Emit slice typedefs + accessors among the
  dependency-ordered type defs (a slice depends on its element type).

### 15.3 Notes / deferred
Slices are non-owning views — the backing array must outlive the slice (raw, no
lifetime check). Many-item pointers `[*]T` and pointer arithmetic beyond
slicing are deferred.

## 16. The Allocator interface + heap (v0.119)

Zig's law: **no global allocator** — heap memory is requested from an
`Allocator` value that is **passed explicitly**. v0.119 ships the `Allocator`
type and three builtins (no new AST: they are ordinary calls; `alloc`'s type
argument is just an identifier).

- **`Allocator`** — a first-class type (`Type::Allocator`). C:
  `typedef struct { int _unused; } kd_allocator;` (emitted in the prelude).
- **`c_allocator() -> Allocator`** — the `malloc`/`free`-backed allocator. C:
  `((kd_allocator){0})`.
- **`alloc(a: Allocator, T, n: usize) -> []T`** — allocate a slice of `n`
  elements of type `T`. The **second argument is a type** (an identifier naming
  a builtin/struct/enum; sema resolves it; misuse → `E0241`). Result `[]T`
  (interned). Panics (exit 101) on OOM.
- **`free(a: Allocator, s: []T) -> void`** — free a slice previously returned by
  `alloc` (second argument must be a slice → `E0242`).

### 16.1 Semantics (`sema`)
Special-case the three builtins in call checking (alongside `print`/`expect`):
arity/type-check `a` as `Allocator`; for `alloc`, the 2nd arg is an `Ident`
resolved to a type `T`, the 3rd an integer, result `Slice(intern_slice(T))`;
for `free`, the 2nd arg any `Slice`. A user `fn` may not be named
`alloc`/`free`/`c_allocator` (reuse `E0101`).

### 16.2 Backend (`emit_c`)
Emit the `kd_allocator` typedef in the prelude. For each slice type, emit
(beside its typedef/accessor) an allocator helper:
```c
static inline kd_slice_<tag> kd_slice_<tag>_alloc(uintptr_t n) {
    kd_slice_<tag> s; s.ptr = malloc(n * sizeof(<elem cty>));
    if (!s.ptr && n != 0) { fputs("panic: out of memory\n", stderr); exit(101); }
    s.len = n; return s;
}
```
Lower `c_allocator()` → `((kd_allocator){0})`; `alloc(a, T, n)` →
`kd_slice_<tag>_alloc(<n>)` (the `a` argument is accepted but unused in v0.119);
`free(a, s)` → `free((<s>).ptr)`.

### 16.3 Deferred (honest)
Error-returning `alloc` (`![]T`), custom allocators / a vtable interface,
`realloc`, aligned allocation, and comptime-generic `alloc` (the type argument
is a builtin special-case until comptime generics land in v0.120).

## 17. `comptime` generics (v0.120)

Compile-time **type parameters** + **monomorphisation**: a generic function is
specialised into one concrete C function per distinct type argument. v0.120
covers generic *functions* only (no generic structs / type-returning functions
/ comptime value params yet).

### 17.1 Syntax & AST
- A parameter `comptime IDENT: type` is a compile-time type parameter:
  `Param.is_comptime = true`, `ty.name = "type"`. Comptime type parameters must
  precede all runtime parameters. A function with ≥1 such parameter is
  **generic**.
- The generic function's runtime parameter types, return type and body may use
  the type-parameter names as types (including composite forms `?T`, `[]T`,
  `[N]T`, `*T`, `!T`).
- No new call syntax: a call `g(T1, …, Tk, a1, …)` to a generic `g` passes the
  first `k` arguments as **type arguments** (identifiers naming concrete types),
  the rest as values.

### 17.2 Semantics (`sema`)
- A generic function is **not** checked in the normal body pass; it is checked
  per instantiation.
- At a call to generic `g`: the leading args (one per comptime param) must be
  `Ident`s naming concrete types (resolved via the usual rules — `E0251` if
  not); bind `type_param_name → Type` into a substitution; check the remaining
  args against the **substituted** parameter types (with coercion); the result
  is the substituted return type. Record the instantiation
  (`StructTable::intern_instantiation`); if newly added, type-check the instance
  body under the substitution — which may discover further instantiations
  (process transitively via a worklist).
- **Substitution** maps a `TypeExpr` whose `name` is a bound type parameter to
  the bound `Type`, recursively through `?`/`!`/`[N]`/`*`/`[]` forms; otherwise
  normal resolution.
- Diagnostics: a comptime param not of kind `type` → `E0250`; a non-type type
  argument → `E0251`; too few type arguments for a generic call → `E0252`.

### 17.3 Backend (`emit_c`)
- A generic function is **not** emitted directly. For each recorded
  instantiation emit a specialised C function named
  `StructTable::instantiation_c_name` (e.g. `kd_max__int32_t`) with the
  substitution active so every type-parameter use resolves to the concrete
  type. Forward-declare instances alongside ordinary functions.
- A call `g(T1, …, a1, …)` to a generic function lowers to a call of the
  instance's C name with **only the runtime args** (type arguments dropped).
- Instance bodies reuse all existing statement/expression lowering under the
  active substitution (which drives `cty`/`resolve_ty`/`type_of_expr`).

### 17.4 Deferred (honest)
Comptime **value** parameters (`comptime n: usize`), generic structs /
type-returning functions (`fn List(comptime T: type) type`), comptime control
flow, and `anytype`.

## 18. Type inference for `var`/`const` (v0.121)

The type annotation on a binding becomes **optional**: `var x = expr;` /
`const x = expr;` (and top-level `const X = expr;`) infer the type from `expr`.
`Stmt::Let.ty` and `ConstDecl.ty` are now `Option<TypeExpr>`.

### 18.1 Syntax
`("var" | "const") IDENT (":" type)? "=" expr ";"`. A top-level `const IDENT =
expr ;` with no annotation is an inferred value binding **unless** `expr` begins
with `struct`/`enum` (those remain type declarations, §9/§13).

### 18.2 Semantics (`sema`)
- Annotation present → unchanged: check `expr` coerces to the annotated type.
- Annotation absent → the binding's type is the **inferred** type of `expr`
  (checked with no expected type; an integer literal defaults to `i64`). A value
  with no inferable type without context — bare `null`, `error.X`, `.Variant`,
  or an empty array literal — is `E0260` ("cannot infer type; add an
  annotation"). Top-level inferred `const` infers from the comptime value
  (`i64`/`bool`).

### 18.3 Backend (`emit_c`)
For an inferred binding, the C declaration type is `cty(type_of_expr(value))`;
for an annotated binding, unchanged. Everything else (initializer emission,
coercion) is as before.

`kard fmt` omits the `: T` when a binding has no annotation.

## 19. Cross-compilation (v0.123)

`kard build FILE -target <TRIPLE>` cross-compiles, leaning on the C compiler's
cross support (`clang --target=<triple>`). Because kardashev lowers to portable
C, the only target-specific dependency is the C toolchain.

- **`backend::BuildOptions { target: Option<String>, object_only: bool }`** is
  threaded into `cc_build(c_src, out, &opts)`. When `target` is `Some`, the
  backend selects a clang-family compiler and passes `--target=<triple>`; if no
  clang is available it errors (gcc's `-target` is not equivalent), suggesting
  clang or `-c`.
- **`-c` / `--emit obj`** (`object_only`) compiles to an **object file** only,
  skipping the link step. Default OUT for object mode is `FILE` minus `.ks` plus
  `.o`.
- **Honest limitation.** Because the emitted runtime uses the C standard library
  (`<stdio.h>`/`<stdlib.h>`/`<stdint.h>` for `print`/alloc/panic), even
  `-c` cross-compilation needs the **target's C headers** present, and a fully
  *linked* foreign executable needs the target's libc too. So out of the box
  only the **host triple** (and multi-arch SDKs such as macOS x86_64 ↔ arm64)
  build without extra setup; other triples require that target's C
  toolchain/sysroot installed. **Bundling cross sysroots** — Zig's
  "cross-compile anything out of the box" — is the headline future item: the
  `-target`/`-c`/`kard targets` mechanism is in place; the bundled sysroots are
  not yet.
- **`kard targets`** prints a list of common, known-good triples (informational).

CLI: `kard build [FILE] [-o OUT] [-target TRIPLE] [-c | --emit obj]`. `run`/`test`
always build for the host.

### 13.3 Backend (`emit_c`)
Emit each enum among the dependency-ordered type defs (enums have no
dependencies):
```c
typedef enum { kd_enum_<E>_<V0> = 0, kd_enum_<E>_<V1> = 1, ... } kd_enum_<E>;
```
`Enum.V` / `.V` → the enumerator `kd_enum_<E>_<V>`. A `switch` lowers to a C
`switch`:
```c
switch (<scrutinee>) {
  case <label>: { <body> } break;        /* one case per label in an arm */
  ...
  default: { <else body, or nothing> } break;
}
```
`cty(Type::Enum(id))` → `StructTable::enum_c_name(id)`. (Because sema proves
enum switches exhaustive, a `default:` is emitted only when the source has an
`else`.)

## 20. Tagged unions `union(enum)` + `switch` capture (v0.124)

A tagged union holds one of several typed variants, with a `switch` that
captures the active payload. v0.124: every variant carries a payload type
(payload-less cases stay plain enums, §13).

### 20.1 Syntax & AST
- `pub? const Name = union(enum) { v1: T1, v2: T2, ... };` →
  `Item::Union(UnionDecl{ variants: Vec<UnionVariant{name, payload}> })`,
  `Type::Union(id)`.
- **Construction** reuses `Expr::StructLit`: `Name{ .v1 = e }` — exactly **one**
  field, naming a variant; `e` coerces to that variant's payload type. Result
  `Type::Union(id)`.
- **`switch` over a union**: labels are `.variant` (`Expr::EnumLit`) resolved
  against the union's variants; an arm may bind the payload with
  `SwitchArm.capture` (`.v1 => |x| { … }`) — `x` is a local of the variant's
  payload type within the arm. Exhaustiveness: every variant, or an `else`.

### 20.2 Semantics (`sema`)
- Pre-pass: intern union decls (`intern_union` + `set_union_variants`),
  resolving each variant's payload type (duplicate variant → error).
- `Name{ .v = e }` where `Name` is a union: exactly one field that names a
  variant (else `E0270`); unknown variant → `E0271`; `e` coerces to the payload
  type (`E0110`); result `Type::Union(id)`.
- `switch (u)` where `u: Union(id)`: each label `.v` must be a variant
  (`E0271`); exhaustiveness as for enums (`E0210`/`else`); a `capture` binds the
  variant payload type in the arm body; a capture on a non-union switch, or a
  union switch arm lacking a capture where one is needed, is `E0272`. (An enum
  or integer switch with a capture → `E0272`.)
- `type_name(Union(id))` = the union's source name.

### 20.3 Backend (`emit_c`)
Emit, among the dependency-ordered type defs (a union depends on its payload
types):
```c
typedef struct { int32_t tag; union { <T1 cty> kd_<v1>; <T2 cty> kd_<v2>; ... } data; } kd_union_<Name>;
```
Construction `Name{ .v = e }` → `((kd_union_<Name>){ .tag = <idx>, .data = { .kd_<v> = <e> } })`.
A union `switch` lowers to a C `switch` on `(<u>).tag`; each arm `case <idx>: {`
begins (when captured) with `<payload cty> kd_<cap> = (<u>).data.kd_<v>;` then
the arm body, then `break;`. `cty(Type::Union(id))` → `StructTable::union_c_name`.

## 21. Payload captures + `errdefer` (v0.125)

### 21.1 Optional `if` capture
`if (opt) |v| { then } else { els }` — `Stmt::If` with `capture = Some("v")`.
The condition must be an optional `?T` (else `E0280`); `v` binds the unwrapped
`T` inside `then`; `els` runs when the optional is null. A plain `if (cond)`
(no capture) is unchanged (`cond` is `bool`). Lowering (the condition is
evaluated once into a temp):
```c
{ <opt cty> __kd_ifN = (<cond>); if (__kd_ifN.has) { <T cty> kd_v = __kd_ifN.val; <then> } else { <els> } }
```

### 21.2 `errdefer`
`errdefer stmt;` (`Stmt::ErrDefer`) registers `stmt` to run **only on
error-return** paths, in LIFO order, alongside regular `defer`s.

- Each scope keeps its deferred statements **in registration order**, each
  tagged `defer` or `errdefer`. On a **normal** exit (fall-through, `break`,
  `continue`, or a *success* `return`), only the `defer`s run (reverse order).
  On an **error-return** edge, **both** `defer`s and `errdefer`s run (reverse
  registration order, merged).
- Error-return edges are: a **`try` propagation** (`if (__kd_tryN.err != 0) {
  <flush incl. errdefers>; return …; }`) and a **`return error.X`** (an
  `ErrorLit` return). A `return <value>` (success) and normal fall-through flush
  only `defer`s.
- sema checks an `errdefer`'s statement like a `defer`'s; it is accepted in any
  function (it simply never fires in one that never returns an error).

### 21.3 Deferred (honest)
`catch |e| { … }` capture (the block/expression handler binding the error) — the
non-capturing `expr catch default` (§12) stays; the capturing form is a later
version.

## 22. Multi-file modules (`@import`) (v0.126)

A program may span files. `@import("path.ks");` is a top-level **import
declaration** (`Item::Import`, lexed via the new `@`/`At` token).

### 22.1 The flattener (`modules::resolve`)
`modules::resolve(root: &Path) -> Result<ast::Module, Vec<Diagnostic>>`:
- Lex + parse `root`; for each `@import("p")` item, resolve `p` **relative to
  the importing file's directory** and recurse.
- Track visited (canonicalised) paths so a file imported twice is included
  **once**; a missing/unreadable file is `E0291`; an import **cycle** is
  `E0292`.
- Concatenate **every** file's items into one flat `Module`, with the
  `Item::Import`s erased. All top-level item names must be **globally unique**
  across the whole program — a collision is `E0293`.
- The flat module is fed to the existing `sema`/`emit_c` unchanged.
- Sub-file lex/parse errors are rendered against that file's own source and
  returned (the flattener owns each file's text); structural errors
  (`E0291`/`E0292`/`E0293`) carry the path in their message.

### 22.2 v0.126 limitations (honest)
This is a `#include`-style flatten: there is **no `m.member` qualified access**
(items are referenced by bare name), and `pub` is **not yet enforced across
modules** (every flattened top-level item is globally visible). Proper
namespacing, qualified access, and cross-module `pub` enforcement are future
work, as is a package/std path resolver.

### 22.3 CLI
`kard build`/`run`/`test` compile via `compile_program(root_path)` (path-based)
so `@import`s resolve. The string entry `compile_to_c(src)` remains for
single-file compiles and errors (`E0290`) on a residual `@import`.

## 23. Strings — `[]u8` literals (v0.127)

A string literal `"…"` is a **value** of type `[]u8` (`Type::Slice(u8)`) — a
slice over static bytes. (Reuses the slice machinery, so no new type.)

### 23.1 Syntax & semantics
- `Expr::StrLit{value}` (parser: a `Str` token in expression position). Its type
  is `[]u8` (`Type::Slice(intern_slice(U8))`).
- Slice operations apply: `s.len` (byte length, `usize`), `s[i]` → `u8`
  (bounds-checked), `s[lo..hi]` → `[]u8`.
- **`print`** accepts a `[]u8` (a string) in addition to integers: it writes the
  bytes followed by a newline. A `print` of any other type stays an error.

### 23.2 Backend (`emit_c`)
`StrLit("hi")` → `((kd_slice_uint8_t){ .ptr = (uint8_t *)<C string literal>,
.len = <byte length> })`, where the C string literal escapes the bytes
(`\n \t \" \\` and non-printables) and the length is the decoded byte count.
`print(s)` where `s: []u8` → `{ fwrite((s).ptr, 1, (s).len, stdout); fputc('\n',
stdout); }` (the integer `print` path is unchanged). `cty`/`type_of_expr` treat
a `StrLit` as `Type::Slice(u8)`.

## 24. `comptime` value parameters (v0.128)

Extends v0.120 generics: a parameter may be a compile-time **value** —
`comptime n: usize` — and the function is monomorphised per distinct value
(array-size generics).

### 24.1 Syntax & AST
- A `comptime IDENT: <int type>` parameter (`Param.is_comptime` with a non-
  `type` annotation) is a comptime value parameter. A function with any
  comptime parameter (type or value) is generic.
- Array sizes generalise: `TypeExpr.array_len: Option<ArraySize>` where
  `ArraySize::Lit(n)` is the literal form `[3]T` (v0.117) and
  `ArraySize::Param(name)` is `[n]T` (the size is a comptime value parameter).
- Calls pass comptime value arguments positionally (like type args), each a
  comptime-constant integer.

### 24.2 Semantics (`sema`)
- At a generic call, each comptime parameter is bound: a `type` param to a
  `Type` (`ComptimeArg::Type`), a value param to an `i64` obtained by
  const-evaluating the argument (`ComptimeArg::Value`; a non-constant value arg
  is `E0251`/`E0253`). The instantiation key is `Vec<ComptimeArg>`.
- The instance body is checked under both a **type** substitution and a
  **value** substitution: a reference to a value param `n` is a constant of its
  declared type; an `ArraySize::Param(n)` resolves to the bound value (so
  `[n]i32` becomes `[5]i32`). A `[n]T` outside its generic (n unbound) is an
  error.
- `StructTable::intern_array` keys arrays on the resolved `(elem, len)`, so each
  instantiated size makes a distinct array type.

### 24.3 Backend (`emit_c`)
Per instantiation the emitter holds a value substitution (`name → i64`) beside
the type substitution. `ArraySize::Param(n)` resolves to the bound value when
forming the array type; a reference to a value param `n` in the body emits the
literal value; the instance is emitted as `kd_<fn>__<args>` (a value arg mangles
to its digits; a negative value to `m<digits>` — `-` is not a C identifier
character, so `kd_addk__-3` would not compile; v0.178). Non-generic
literal-sized arrays are unchanged.

## 25. Generic structs / type-returning functions (v0.129)

Zig's metaprogramming for *types*: a function may **return a type**, and that
type is a `struct` parameterised by the function's comptime type parameter.

### 25.1 Syntax & AST
- A **type-constructor** is `pub? fn Name(comptime T: type) type { return struct
  { f1: T, f2: …, … }; }` — its return type is the bare `type`, and its body is
  exactly `return <struct-type>;`. The struct body is an `Expr::StructType{
  fields }` (an anonymous struct **type value**, parsed when `struct {` appears
  in expression/return position). v0.129: one comptime type parameter,
  fields-only struct (no methods inside the generic struct).
- **Type aliases**: `const Alias = Name(ConcreteType);` — a top-level `const`
  whose initializer is a call to a type-constructor. (No new syntax; reuses
  `Expr::Call`.)

### 25.2 Semantics (`sema`)
- Identify type-constructors (return type `type`). They are *not* checked as
  ordinary functions.
- A `const Alias = Name(C);` instantiates `Name` at `C`: substitute the type
  parameter throughout the type-constructor's `StructType` fields, **intern a
  struct** named `Name__<typetag>` (memoised — the same `(Name, C)` yields the
  same struct id), and bind `Alias` as a **type alias** to that
  `Type::Struct(id)`.
- A type alias is usable in type position (`var x: Alias`), as a struct-literal
  name (`Alias{ .f = v }`), and for field access (`x.f`) — all resolving to the
  aliased struct via the existing Ident-based machinery.
- Diagnostics: a type-constructor whose body isn't `return struct {…};` →
  `E0310`; instantiating a non-type-constructor, or a type-constructor argument
  that isn't a type → `E0311`; using a type alias as a value → reuse the
  unknown-name/`E0100` path.

### 25.3 Backend (`emit_c`)
Type-constructor functions are **not emitted** (compile-time only, like generic
functions); `Expr::StructType` therefore never reaches the backend. Type-alias
`const`s emit nothing. The monomorphised structs interned by sema are emitted as
ordinary C struct typedefs (in dependency order) and used like any struct.

### 25.4 Deferred (honest)
Multiple type parameters (landed v0.135, §31), comptime-value type-constructor
params, and direct `Name(T)` / `Name(T){…}` in type / literal position — the
type-position and associated-call forms landed in v0.152 (§42); the literal
form `Name(T){…}` remains deferred. (Methods inside a generic struct land in
v0.130.)

## 26. Generic-struct methods + `ArrayList(T)` (v0.130)

The final Arc-2 piece: a type-constructor's `struct` may declare **methods**, so
a generic struct is a real container. This is the foundation of the std
`ArrayList(T)`.

### 26.1 Syntax & AST
- `Expr::StructType` now carries `methods: Vec<Func>`: `fn Name(comptime T:
  type) type { return struct { f: …, fn m(self: Self, …) … { … } }; }`. Methods
  follow the fields, exactly like a named struct's methods (SPEC §10).
- `Self` is a contextual type name available in a type-constructor method's
  signature/body; it denotes the enclosing struct. `*Self` is a pointer to it.
  The type parameter `T` is also in scope.

### 26.2 Semantics (`sema`)
- When a `const Alias = Name(C);` instantiates a type-constructor (SPEC §25.2),
  in addition to the fields the methods are **monomorphised**: each method is
  checked under the substitution `{ <type param> → C, Self → Struct(id) }` and
  registered on the instantiated struct via the existing struct-method table
  (SPEC §10), so `x.m(args)` resolves like any method call. The instance is
  recorded (`StructTable::record_struct_instance(id, Name, C)`) for the backend.
- `Self` (and `*Self`) resolve to the instantiated struct (`Struct(id)`).

### 26.3 Backend (`emit_c`)
For each recorded struct instance, the emitter emits the constructor's methods
under `{ type-param → C, Self → Struct(id) }`, named `kd_<struct-c-name>_<method>`
(reusing the struct-method lowering, SPEC §10.2) — mirroring how generic-function
instantiations are emitted. The type-constructor function itself is still not
emitted.

### 26.4 `ArrayList(T)` (std prelude)
A growable list built on the `Allocator` (SPEC §16): `init`, `append` (grows by
allocating a larger buffer, copying, freeing the old — there is no `realloc`),
`get`, `len`, and `deinit`. Shipped as `examples/arraylist.ks` and exercised by
tests.

### 26.5 Deferred (honest)
Still one type parameter and `Self` only (no `@This()`); multiple type
parameters remain later work.

## 27. Compound assignment (v0.131)

`place op= rhs;` for `op ∈ { +=, -=, *=, /=, %= }` means `place = place op rhs`,
with **the place evaluated once**. The lexer produces `PlusEq`/`MinusEq`/
`StarEq`/`SlashEq`/`PercentEq`; the corresponding `BinOp` is
`Add`/`Sub`/`Mul`/`Div`/`Rem`.

### 27.1 AST & parsing
`Stmt::Assign{ name, op, value }` and `Stmt::FieldAssign{ place, op, value }`
carry `op: Option<BinOp>` — `None` for a plain `=`, `Some(binop)` for a compound
form. The parser, after a place and on seeing a compound-assign token, records
the op; a simple-name target uses `Assign`, a field/index chain uses
`FieldAssign` (as for `=`).

### 27.2 Semantics (`sema`)
A compound assignment requires the place to be assignable (a `var`, as for `=`)
and both the place and `rhs` to be the **same integer type** (the arithmetic
operators require integer operands — `E0132`/the usual binop type rule); the
result type matches the place. A plain `=` is unchanged.

### 27.3 Backend (`emit_c`)
`op = None` is unchanged. For `op = Some(binop)`: a `Stmt::Assign` lowers to
`kd_<name> = kd_<name> <c-op> (<rhs>);` (a var read is free). A
`Stmt::FieldAssign` evaluates the place **once** — reusing the existing index
hoist for `a[i]` (so `a[i] += e` reads `i` once) — then `<place> = <place>
<c-op> (<rhs>);`.

## 28. Bitwise & shift operators (v0.132)

Integer operators: binary `&` `|` `^` `<<` `>>` (`BinOp::BitAnd`/`BitOr`/
`BitXor`/`Shl`/`Shr`) and unary prefix `~` (`UnOp::BitNot`). Tokens: `&` = `Amp`
(prefix → address-of §15, infix → bitand), `|` = `Pipe` (infix → bitor; a
capture `|x|` is a distinct grammar position §11/§21), `^` = `Caret`, `~` =
`Tilde`, `<<` = `Shl`, `>>` = `Shr`.

### 28.1 Precedence (low → high)
`or` < `and` < `|` < `^` < `&` < equality (`== !=`) < relational (`< <= > >=`) <
shift (`<< >>`) < additive (`+ -`) < multiplicative (`* / %`) < unary (`- ! ~`
and prefix `&`/`*`) < postfix. (C-like; `or`/`and` are keywords so `&`/`|` are
unambiguously bitwise.)

### 28.2 Semantics (`sema`)
`& | ^ << >>` require **both operands to be the same integer type** and yield
that type; `~x` requires an integer and yields its type. (A shift's right
operand is also an integer; the result type is the left operand's.) Non-integer
operands are the usual binop type error.

### 28.3 Backend (`emit_c`)
Direct C lowering: `a & b`, `a | b`, `a ^ b`, `a << b`, `a >> b`, `~a` (the
operands keep their C integer types). `const_eval` folds all of them (and `~`)
on integer constants, so `const MASK = (1 << 8) - 1;` works.

### 28.4 Width fidelity on narrow operands (v0.156)
`~x` and `x << n` yield the **operand's** type (§28.2) even where C's integer
promotion would widen the intermediate: for 8/16-bit operands the backend
truncates the result back to the operand's C type (two's-complement, exactly
the `@as` narrowing of §33), so `~(u8 170)` is `85` and `(u8 200) << 1` is
`144` whether stored or consumed directly. 32/64-bit operands never promote.
`>>` and the masking operators cannot exceed the operand width and keep the
bare lowering. (Found by the v0.156 conformance corpus.)

## 29. `for` loops over arrays & slices (v0.133)

`for (iter) |elem| { … }` iterates the elements of an array (`[N]T`) or slice
(`[]T`); `elem` binds each element **by value**. `for (iter, 0..) |elem, index|
{ … }` additionally binds a 0-based `usize` `index`. (`for`/`Kw::For` is a new
keyword; `Stmt::For{ iter, elem, index, body }`.)

### 29.1 Semantics (`sema`)
`iter` must be a `[]T` or `[N]T` (else an error); `elem` is bound (immutable, a
copy) to the element type `T`, and `index` — present iff the `, 0..` index form
was written — to `usize`. The body is checked in a new loop scope with those
bindings (so `break`/`continue` work). A capture-count that disagrees with the
presence of `, 0..` is an error.

### 29.2 Backend (`emit_c`)
Lowered to an indexed `while` (a loop-body scope, so `defer`/`break`/`continue`
behave). The iterable is evaluated **once** into a temp:
```c
{ <iter cty> __kd_for{N} = (<iter>); usize __kd_fi{N} = 0;
  while (__kd_fi{N} < <len>) {
      <T cty> kd_<elem> = <elem-access>;     // by-value copy
      usize kd_<index> = __kd_fi{N};         // only if the index form
      <body>
      __kd_fi{N} += 1; } }
```
where `<len>` is `__kd_forN.len` for a slice / the literal `N` for an array, and
`<elem-access>` is `__kd_forN.ptr[__kd_fiN]` (slice) / `__kd_forN.data[__kd_fiN]`
(array).

## 30. Pointer-receiver methods (true mutation) (v0.134)

A method's `self` parameter may be a **pointer**: `fn bump(self: *Point, …)` (a
named struct) or `fn push(self: *Self, …)` (a generic struct, §26). The method
then mutates the receiver in place. A value receiver (`self: Point`/`self: Self`)
is unchanged (a by-value copy). No new tokens/AST — `*Self`/`*Point` already
parse (§15.1, §26).

### 30.1 Auto-deref field access (`ptr.field`)
For any `*Struct` value `p`, `p.field` reads `(*p).field` and `p.field = e` (and
compound `p.field += e`) writes **through** the pointer. (General, not just for
`self`.) Method calls likewise auto-deref a `*Struct` receiver.

### 30.2 Auto-ref method calls
A method call `obj.method(args)` whose method has a **pointer receiver** passes
`&obj` — so `obj` must be an addressable lvalue (a `var`, field, or index; else
an error, as for `&`), and — exactly as for `&` (§15.1, v0.156) — must not be
rooted in a **`const` binding's** own storage (`E0233`: the call would mutate a
`const`; declare it `var`). A receiver that is **already a pointer** is passed
through with no addressability requirement — whether a `*Struct` local,
parameter, or **call result** (`pick(&a).add(9)`, and chains through a method
returning `*Self`: `a.add(5).add(7)`). A value-receiver call passes `obj` by
value (unchanged). An associated call `Type.method(args)` (no receiver value)
passes its explicit arguments — including an explicit `self: *Self`/`Self` —
unchanged.

### 30.3 Backend (`emit_c`)
A pointer-receiver method is `kd_<Struct>_<m>(<Struct>* self, …)`; inside it
`self.field` lowers to `(*self).kd_field`. The call lowers to
`kd_<Struct>_<m>(&(<obj>), …)`. Field read/assign on any `*Struct` lowers through
`(*p)`. Mutations are real — they update the caller's struct. An **indexed
receiver** — `a[i].m()` / `s[i].m()`, array and slice alike — auto-refs the
bounds-checked `_at` element pointer (§14.3/§15.2), so the mutation lands in
the real element / the slice's backing storage; a value-receiver call on an
element reads a copy via `_get` (unchanged).

## 31. Multiple type parameters (v0.135)

Generic **functions** already accept more than one comptime parameter
(type/value, §17/§24) — monomorphised on the tuple of arguments. v0.135 extends
the same to **type-constructors** (§25): `fn Map(comptime K: type, comptime V:
type) type { return struct { … }; }`.

### 31.1 Semantics (`sema`)
- A type-constructor may declare **one or more** `comptime _: type` parameters
  (all must be `type`); the body is still `return struct { … };`.
- A type alias `const M = Map(K, V);` must pass exactly as many type arguments as
  the constructor has type parameters (else `E0311`). Each is resolved to a
  concrete type; the struct is interned as `<Ctor>__<tag1>_<tag2>…` (memoised on
  the argument tuple) with the substitution `{ K→…, V→…, Self→Struct(id) }`
  applied to its fields and methods (§26).
- `StructInstance` records `args: Vec<Type>`.

### 31.2 Backend (`emit_c`)
For each instance, the emitter builds the substitution by zipping the
constructor's type parameters with `StructInstance.args` (plus `Self`), then
emits the methods exactly as in §26.3.

## 32. comptime reflection builtins (v0.136)

Three `@`-builtins (the `@` token, §22):

### 32.1 `@sizeOf(T)` and `@typeName(T)` (expressions)
`Expr::Builtin{ name, args }` in expression position; the single argument names
a type (an `Ident`, resolved like `alloc`'s type argument §16 — substitution-
aware, so it works inside a generic body).
- `@sizeOf(T)` → `usize`, the size in bytes of `T`. Lowers to C `sizeof(<cty
  T>)`.
- `@typeName(T)` → `[]u8`, the source name of `T`. Lowers to a `[]u8` slice over
  a static string of the name (the §23 string lowering).
An unknown `@name(…)` in expression position is an error. Builtins are not
constant expressions (`const_eval` → `E0130`).

### 32.2 `@This()` (a type)
`@This()` denotes the **enclosing struct type**. It is parsed in *type position*
and desugared to `Self` (the v0.130 self-type). v0.136 also binds `Self` in
**plain** (non-generic) struct method scopes, so `@This()` / `Self` work in any
struct method — e.g. `fn translate(self: *@This(), …)` inside a plain `const
Point = struct { … }`.

## 33. Integer casts — `@as(T, e)` (v0.137)

`@as(T, e)` casts the integer value `e` to integer type `T` — `var i: usize =
@as(usize, key);`. It extends the §32 `Expr::Builtin` machinery (`name == "as"`,
two arguments: a type and a value).

- **sema**: exactly two arguments; the first names an integer type `T` (else
  `E0321`), the second is an integer value `e` (else `E0321`); the result type is
  `T`. (v0.137 is integer↔integer only.)
- **emit**: lowers to a C cast `((<cty T>)(<e>))`.
- Not a constant expression (`const_eval` → `E0130`); it is a runtime cast (of a
  value that is itself constant in C).

This unblocks mixed-integer code (e.g. an `i32` key hashed into a `usize` index),
and with it a real `HashMap`.

## 34. Named error sets (v0.139)

A **named error set** groups a fixed list of error names: `pub? const FileErr =
error{ NotFound, Denied };` (`Item::ErrorSet`). An error union may then be typed
over a named set — `FileErr!T` — alongside the implicit global `!T` (§12).

### 34.1 Syntax & AST
- `const Name = error{ A, B, … };` — parsed at const-value position (like `=
  struct`/`= enum`/`= union`), producing `Item::ErrorSet{ name, members }`.
- `Set!T` in type position: `TypeExpr{ error_union: true, error_set: Some("Set"),
  name: <payload> }`. The parser, after a base type name `Set`, treats a
  following `!` as a named error-union (`Set ! payload`). The prefix `!T`
  (`error_set: None`) is unchanged.

### 34.2 Semantics (`sema`)
- An `Item::ErrorSet` registers the set name and its members; each member is an
  error name in the existing global error-code space (so `error.A` keeps a
  stable code) and is recorded as belonging to the set.
- `Set!T` resolves to the SAME `Type::ErrorUnion(payload)` as `!T` (the runtime
  representation is identical, §34.3); the *set* is a compile-time constraint.
- **Membership**: when the expected type at an error-literal site is an error
  union with a **named** set `Set` (e.g. a `return error.A;` in a `fn … Set!T`,
  or `var x: Set!T = error.A;`), `A` must be a member of `Set` (else an error,
  `E0330`). A global `!T` target accepts any `error.X` (backward compatible).
- An unknown set name in `Set!T`, or a `const … = error{…}` member duplication,
  is reported (`E0331`).

### 34.3 Backend (`emit_c`)
A named error union `Set!T` lowers **identically** to `!T` (the same `{ int32_t
err; <T> val; }`, interned by payload) — the set is purely a sema constraint, so
codegen is unchanged. `Item::ErrorSet` emits nothing (compile-time only).

## 35. `@panic` and `unreachable` (v0.141)

Runtime-safety primitives that **diverge** (never return):

- `@panic(msg)` — `Expr::Builtin{ name: "panic" }` with one `[]u8` argument:
  write `msg` to stderr and `exit(101)` (the panic convention).
- `unreachable` — `Expr::Unreachable` (the `unreachable` keyword): write
  `reached unreachable code` to stderr and `exit(101)` if control reaches it.

### 35.1 Semantics (`sema`)
Both diverge, so in a value position they **adopt the expected type** (they
type-check anywhere a value is expected — e.g. `else => unreachable`,
`x orelse @panic("…")`); with no expected type (a statement) they are `void`.
`@panic`'s argument must be a `[]u8` (else an error); a wrong argument count is
`E0320` (the `@`-builtin arity code).

### 35.2 Backend (`emit_c`)
Two `_Noreturn` runtime helpers in the prelude: `kd_panic(<slice> msg)` (writes
the bytes + newline to stderr, `exit(101)`) and `kd_unreachable(void)` (writes a
fixed message, `exit(101)`). A statement (or arm) lowers to `kd_panic(<msg>);` /
`kd_unreachable();` and **diverges** (suppresses the fall-through). In an
expression position the lowering is `(kd_panic(<msg>), 0)` / `(kd_unreachable(),
0)` — the helper exits, so the trailing `0` is dead (works where an integer is
expected; a non-integer value position is a later refinement).

## 36. `catch |e|` capture (v0.142)

The capturing error handler `expr catch |e| default` (deferred from §21.3): if
`expr` (an `!T`) is ok it yields the payload, otherwise it binds the **error
code** to `e` (an `i32`) and evaluates `default` (a `T`) — so the handler can
react to *which* error occurred. `Expr::Catch` gains `capture: Option<String>`
(`None` = the non-capturing `expr catch default`, §12, unchanged).

### 36.1 Semantics (`sema`)
With a capture, `expr` must be `!T`; `e` is bound (immutable, `i32`) only inside
`default`; `default` must be a `T`; the whole expression has type `T`. Without a
capture, behaviour is unchanged.

### 36.2 Backend (`emit_c`)
The capturing form lowers like `try` (§12.3): the `!T` is hoisted into a temp,
a result temp `<T> __kd_catchN` is declared, then
```c
if (__kd_euN.err != 0) { int32_t kd_<e> = __kd_euN.err; __kd_catchN = <default>; }
else { __kd_catchN = __kd_euN.val; }
```
and the expression yields `__kd_catchN` — so `default` runs only on the error
path, with `e` in scope. The non-capturing form keeps its existing
(eager-`default`) lowering.

## 37. Enum explicit values + conversions (v0.143)

Enum variants may carry an explicit integer value, and convert to/from integers:

- `const Color = enum { Red = 1, Green, Blue = 10 };` — a variant with `= N`
  takes value `N`; a variant without one **auto-increments** from the previous
  (the first defaults to 0), the C rule. `EnumVariant{ name, value:
  Option<i64> }`; `EnumInfo` stores the resolved `values`.
- `@intFromEnum(e)` → `i64`: the variant's integer value.
- `@enumFromInt(E, n)` → `E`: the enum value for integer `n` (no range check in
  v0.143).

### 37.1 Semantics (`sema`)
sema resolves each variant's value (explicit, else previous + 1, else 0) and
stores them via `set_enum_variants(id, names, values)`. `@intFromEnum`'s argument
must be an enum (→ `i64`); `@enumFromInt`'s first argument names an enum type and
the second is an integer (→ that enum). (An explicit value is an integer literal
in v0.143.)

### 37.2 Backend (`emit_c`)
The C `enum` carries the values — `enum kd_enum_Color { kd_enum_Color_Red = 1,
kd_enum_Color_Green = 2, kd_enum_Color_Blue = 10 }` — so enum literals, `switch`
labels and comparisons are value-based automatically. `@intFromEnum(e)` →
`((int64_t)(e))`; `@enumFromInt(E, n)` → `((<enum cty>)(n))`.

## 38. Floating point — `f64` (v0.144)

The first non-integer scalar: `f64` (`Type::F64`, C `double`).

- **Literals**: `3.14` → `Expr::Float` of type `f64` (the lexer makes a `Float`
  token from `digits . digits`; a `.` not followed by a digit stays `..`/field
  access).
- **Arithmetic** `+ - * /` on two `f64` → `f64`; **comparison** `== != < <= > >=`
  on two `f64` → `bool`. There is **no implicit int↔float mixing** — both
  operands must be `f64` (use `@as`). (No `%` on `f64` in v0.144.)
- **`@as`** extends to numeric casts: `@as(f64, n)` (int→float) and `@as(i32, x)`
  (float→int) — `@as`'s target/value may now be any numeric type (int or `f64`),
  lowering to a C cast.
- **`print`** accepts an `f64` (in addition to integers and `[]u8`).
- Floats are **runtime-only** in v0.144: a `const` cannot fold a float
  (`var x: f64 = 3.14;` is fine; `const P = 3.14;` is `E0130`).

### 38.1 Backend (`emit_c`)
`Type::F64` → C `double`. A `Float` literal emits a C double literal (always with
a decimal point so C reads it as `double`). Arithmetic/comparison reuse the
existing binary lowering (the operands are `double`). `print(x: f64)` lowers to a
`double` print helper (`kd_print_f64`, `printf("%g\n", …)`-style). `@as` reuses
the §33 cast lowering (`((double)(e))` / `((int32_t)(e))`).

### 38.x Cross-platform determinism (v0.157)
The driver passes `-ffp-contract=off` to the C compiler (§5): `a * b + c` is
never fused into a single-rounding FMA, so every `f64` computation rounds
identically on every platform (Apple clang contracts by default — the same
program printed a different 17th digit on macOS before this rule).

## 39. `switch` ranges + multi-label arms (v0.146)

A `switch` arm may already list **several labels** (`1, 2, 3 => …`;
`.A, .B => …` — `SwitchArm.labels` is a `Vec`, since §13/§20). v0.146 adds
**inclusive integer-range labels**: `lo..hi => …` matches when the scrutinee is
in `[lo, hi]`. `SwitchArm.ranges: Vec<(i64, i64)>`; bounds are integer literals;
ranges and labels combine in one arm (it matches any label OR any range).

### 39.1 Semantics (`sema`)
A range label is valid only for an **integer** scrutinee (a range on an
enum/union switch is an error). As for any integer `switch`, an `else` arm is
required (ranges do not establish exhaustiveness). A backwards range (`hi < lo`)
matches nothing.

### 39.2 Backend (`emit_c`)
A range label lowers to a GNU C case-range `case <lo> ... <hi>:` (supported by
the `cc`/`clang` backend), beside the ordinary `case <label>:` lines for value
labels. The rest of the `switch` lowering is unchanged.

## 40. Labeled `break` / `continue` (v0.147)

A loop may carry a **label**, and `break`/`continue` may **target** it:

```
outer: while (a) {
    while (b) {
        if (done) { break :outer; }     // leaves BOTH loops
        if (skip) { continue :outer; }  // next iteration of the OUTER loop
    }
}
```

- `Stmt::While`/`Stmt::For` gain `label: Option<String>` (a `name:` before the
  loop keyword). `Stmt::Break`/`Stmt::Continue` gain `target: Option<String>`
  (`None` = innermost loop, unchanged; `Some(name)` = the enclosing loop with
  that label).

### 40.1 Semantics (`sema`)
A labeled `break`/`continue` must name an **enclosing loop's label** (else an
error); an unlabeled one still requires being inside a loop. Track the stack of
enclosing loop labels alongside the existing loop-depth check.

### 40.2 Backend (`emit_c`)
A labeled loop emits a trailing C break-label `__kd_brk_<label>:;` after it and a
continue-label `__kd_cont_<label>:;` at its continue point. `break :L` flushes
`defer`s out to **and including** loop `L`'s scope, then `goto __kd_brk_L;`;
`continue :L` flushes to loop `L`, runs `L`'s continue-clause, then `goto
__kd_cont_L;`. Unlabeled `break`/`continue` are unchanged (innermost loop). The
emitter's loop-body `Scope` records the loop's label so the flush walks to the
right scope.

## 41. stdin / file I/O (v0.148)

Two `@`-builtins for minimal input, both allocating their result on the passed
`Allocator` and returning a `[]u8`:

- `@readFile(a, path)` — read the whole file named by `path` (a `[]u8`) into a
  fresh `[]u8`. On any open/read error it yields an **empty** slice (`len == 0`)
  — there is no `![]u8` to express the error (the optional/error-union
  named-type-only limitation, §11/§12).
- `@readLine(a)` — read one line from stdin (without the trailing newline) into a
  fresh `[]u8`; an empty line or EOF yields a zero-length slice.

### 41.1 Semantics (`sema`)
`Expr::Builtin{ name: "readFile"/"readLine" }`. `@readFile`'s first argument is
an `Allocator` and its second a `[]u8`; `@readLine`'s only argument is an
`Allocator`. Both have type `[]u8`. (Not constant — `const_eval` rejects.)

### 41.2 Backend (`emit_c`)
Prelude `_`/runtime helpers `kd_read_file(kd_allocator, kd_slice_uint8_t) ->
kd_slice_uint8_t` (NUL-copies the path, `fopen`/`fread`s the file, empty slice on
failure) and `kd_read_line(kd_allocator) -> kd_slice_uint8_t` (`getchar` loop to
a `\n`/EOF), both `malloc`-backed (the allocator is the malloc-backed stub,
§16.2; the result is freeable with `free(a, slice)`). Emitted only when used.
`@readFile(a, p)` → `kd_read_file((a), (p))`; `@readLine(a)` → `kd_read_line((a))`.

## 42. Direct generic-type application `Name(T)` (v0.152)

The v0.129 alias requirement falls: a generic type-constructor may be applied
**directly in type position** and as the **receiver of an associated call**,
without declaring a `const` type alias first:

```zig
var l: ArrayList(i32) = ArrayList(i32).init(a);
var m: HashMap(i64) = HashMap(i64).init(a);
fn Stack(comptime T: type) type {
    return struct { items: ArrayList(T) };  // nested application — generic composition
}
```

`const L = ArrayList(i32);` aliases keep working unchanged (§25.2).

### 42.1 Syntax & AST
- In **type position**, a plain base name may be followed by a parenthesised
  type-argument list: `Name(A, B, …)`. `TypeExpr` gains `ctor_args:
  Option<Vec<TypeExpr>>` — `Some(args)` for an application (`name` is the
  constructor), `None` for a plain named type. Each argument is itself a *base*
  type reference: a bare name or a **nested application** (`ArrayList(
  ArrayList(i32))`); the `?`/`!`/`*`/`[]`/`[N]` argument forms are **not**
  accepted (the same restriction as alias arguments, §25.2 — `E0311`-shaped
  recovery applies: the argument parse expects an identifier).
- The application **composes with every prefix form**: `?Name(A)`, `!Name(A)`,
  `Set!Name(A)` (the payload), `*Name(A)`, `[]Name(A)`, `[N]Name(A)`. It is
  never an error *set* (`Set` in `Set!T` is always a plain name) and `@This()`
  never takes arguments. In type position a `(` after a base name is
  unambiguous — no legal type is followed by `(`.
- In **expression position** there is no new syntax: `ArrayList(i32).init(a)`
  already parses as `Expr::MethodCall{ receiver: Expr::Call{ callee:
  "ArrayList", args: [Ident("i32")] }, method: "init", … }`. v0.152 gives that
  shape *meaning* (§42.2). The struct-literal form `Name(T){ .f = v }` stays
  deferred (§42.4).

### 42.2 Semantics (`sema`)
- **Type position**: resolving a `TypeExpr` whose `ctor_args` is `Some(args)`
  (1) requires `name` to be a registered type-constructor — anything else is
  `E0312` (`` `X` is not a generic type ``; an unknown plain name stays the
  existing unknown-type diagnostic); (2) checks arity exactly like an alias
  (`E0311`, same message text); (3) resolves every argument **under the active
  substitution** (so `ArrayList(T)` inside a generic function or another
  type-constructor body instantiates per monomorphisation — nested applications
  recurse); (4) calls the §25.2 instantiation (memoised by the mangled
  `Ctor__<tag>…` name — an application and an alias of the same `(ctor, args)`
  share one struct id), yielding `Type::Struct(id)`; (5) applies the ordinary
  prefix wrapping (`?`/`!`/`*`/`[]`/`[N]`) on top.
- **Associated calls**: `check_method_call` case (b) gains the `Expr::Call`
  receiver: when the callee names a type-constructor, the call's value-argument
  list is resolved as *type* arguments (identifiers or nested applications —
  the §25.2 argument rule, `E0311` otherwise), the instance is instantiated
  (memoised) and the call proceeds as a static call on it. A type-constructor
  application anywhere else in value position is `E0312` (`` a generic type is
  not a value ``).
- **Instantiation ordering**: applications instantiate lazily at resolution
  time; type-constructors are collected in Pass 0d, before signatures (Pass 1)
  and bodies (Pass 2), so applications work in parameter/return types, locals,
  generic-struct fields and method bodies. Instantiation during the
  post-Pass-2 `pending_ctor_methods` drain may **enqueue further pending
  instances** (a method body using `ArrayList(T)` instantiates it), so the
  drain loops until the queue is empty instead of taking it once. Plain
  (non-generic) struct **fields** still resolve in Pass 0b, before Pass 0d —
  so a *plain* struct field of application type stays unsupported, exactly as
  alias-typed plain-struct fields are (§42.4).

### 42.3 Backend (`emit_c`)
The backend never instantiates: every application that survived sema has its
monomorphised struct in the `StructTable`. `resolve_ty` maps an application
`TypeExpr` to that struct by recomputing the §25.2 mangle — each argument
resolves recursively (under the active emit substitution for generic bodies)
and `Ctor__<tag>…` is looked up with `id_of` (the same hand-mirrored naming
contract as `Emitter::cty`, pinned by the e2e suite). `emit_method_call`
resolves an `Expr::Call` receiver the same way, so
`ArrayList(i32).init(a)` lowers to `kd_ArrayList__int32_t_init(a)` exactly like
the alias form. `type_of_expr` does the matching lookup for the call's result
type.

### 42.4 Deferred (honest)
The struct-literal application `Name(T){ .f = v }` (use an alias, or an
associated constructor like `init`); composite-type *arguments*
(`ArrayList([]u8)` — the same named-type-only limitation as aliases, §25.2);
applications as `comptime` type arguments to **generic functions**
(`alloc(a, ArrayList(i32), n)` — generic-fn type args stay bare names, §17);
and application-typed fields in *plain* (non-generic) structs (Pass-0b
ordering, §42.2 — generic-struct fields support them).

## 43. Dead-function elimination (v0.153)

The emitter only emits functions that are **reachable** from the build mode's
roots. Before v0.153, `@import("std")` put every std function into every
program's C (`kd_str_concat` in hello-world); as std grows, that taxes the C
compile of all programs. Functions are now emitted *pay-as-you-go*, like
generic instantiations always were.

### 43.1 Semantics (`emit_c`)
- **Roots**: `EmitMode::Program` → the user's `main`; `EmitMode::Test` → every
  `test` block. (`pub` does not make a function a root: a kardashev build
  produces an executable, not a linkable library — `pub` remains a visibility
  marker for imports, §22.)
- **Liveness**: a worklist walk over the *flattened* module's AST collects,
  from each live body, every `Call{callee}` (marking the free function of that
  name live) and every `MethodCall{method}` (marking methods/associated
  functions **of that name on every struct** live — name-level, deliberately
  receiver-agnostic and over-approximate; precision is a §43.3 deferral).
  Newly-live function bodies join the worklist (transitive closure).
  `@`-builtins are runtime helpers, not module functions (§35/§41), and keep
  their existing usage-driven emission.
- **Always-walked name sources**: bodies that emit regardless of the
  reachability walk contribute their called names regardless too. The body of
  every *generic* function is walked unconditionally (even uninstantiated — a
  deliberate over-approximation needing no instantiation bookkeeping), and a
  type-constructor's methods are walked for every constructor with **at least
  one recorded instance** — exactly the methods the backend emits. A
  never-instantiated constructor emits nothing, so its methods are *not* name
  sources: this is what keeps an `@import`ed-but-unused std container
  pay-as-you-go (`HashMap`'s internal `iabs` use must not keep `kd_iabs` in a
  program that never builds a `HashMap`).
- **What is skipped**: a dead free function and a dead struct method /
  associated function are omitted from BOTH the forward-declaration pass and
  the definition pass (the two passes must agree). Everything else —
  typedefs, enums/unions, generic instantiations, runtime helpers — is
  unchanged by this version.

### 43.2 Observable effect
Generated C for a program that uses no std function contains none; behaviour
(exit code, stdout) is byte-identical for every program. A program whose every
function is used emits byte-identical C to v0.152.

### 43.3 Deferred (honest)
Receiver-precise method liveness (per-struct rather than per-name);
instantiation-level liveness for generics whose only call sites are dead;
struct/enum/union typedef pruning (cheap text, kept for simplicity); and
`const` pruning.

## 44. File output + program arguments (v0.158)

Four `@`-builtins completing §41's minimal input with **output** and **argv**
access — the self-hosting prerequisites (a compiler must write the files it
produces and read its own command line):

- `@writeFile(path, data)` — write `data` (a `[]u8`) as the whole contents of
  the file named by `path` (a `[]u8`), creating it or **truncating** an
  existing one. Yields a `bool`: `true` on success, `false` on any open/write
  error.
- `@appendFile(path, data)` — like `@writeFile` but **appends** to the file
  (still creating it if missing). Same `bool` result.
- `@argc()` — the number of program arguments as an `i64`, **including**
  `argv[0]` (the executable name), so it is always ≥ 1.
- `@arg(a, i)` — the `i`-th program argument (`0 ≤ i < @argc()`) copied into a
  **fresh** `[]u8` allocated on the `Allocator` `a` (the same allocator
  convention as `@readLine`, §41). An out-of-range `i` — negative or
  `≥ @argc()` — yields an **empty** slice (`len == 0`).

Why `bool` and not `!void` for the write builtins: an error union would carry
exactly the same single success/failure bit (there is no error *payload* to
distinguish), while complicating the builtin's signature and its lowering —
and §41's input builtins already established the no-error-channel convention
(empty slice on failure). The `bool` is the honest equivalent.

Why an indexed accessor pair and not a single `args()` returning all
arguments: `[][]u8` is not expressible — a slice's element must be a *named*
type (§15.2) — so the argument list is exposed as a count (`@argc`) plus a
per-index copy (`@arg`).

### 44.1 Semantics (`sema`)
`Expr::Builtin{ name: "writeFile"/"appendFile"/"argc"/"arg" }`.
`@writeFile`/`@appendFile` take exactly two `[]u8` arguments (path, data) and
have type `bool`; a wrong count is `E0320`, a non-`[]u8` path or data is
`E0110`. `@argc` takes no arguments (`E0320` otherwise) and has type `i64`.
`@arg` takes exactly two arguments — an `Allocator` and an integer index
(`E0320` on count, `E0321` on a non-`Allocator` first argument, `E0110` on a
non-integer index; a flexible literal index defaults to `i64`) — and has type
`[]u8`. All four are runtime-only: `const_eval` rejects them in a constant
initializer (`E0130`, the generic `@`-builtin arm).

### 44.2 Backend (`emit_c`)
One shared write helper `kd_write_file(kd_slice_uint8_t path, kd_slice_uint8_t
data, int append) -> int` (NUL-copies the path exactly like `kd_read_file`,
`fopen`s with `"wb"`/`"ab"`, `fwrite`s the data, returns 1 on full success and
0 otherwise) is emitted at the tail of the type-def section (after the
`kd_slice_uint8_t` typedef it takes), gated on actual `@writeFile`/
`@appendFile` use — the §41 pattern. `@writeFile(p, d)` →
`(kd_write_file((p), (d), 0) != 0)`; `@appendFile(p, d)` → the same with `1`.

Argv access is **usage-gated on `@argc`/`@arg`**: only then does the prelude
declare `static int kd_argc_v; static char **kd_argv_v;` and the generated
`main` store its parameters into them (`kd_argc_v = argc; kd_argv_v = argv;`
— in both program `main` and the test-harness `main`). A module using neither
builtin emits a **byte-identical** `main` to v0.157 (`(void)argc;(void)argv;`).
`@argc()` → `((int64_t)kd_argc_v)`. `@arg(a, i)` → `kd_arg((a), (i))`, a
helper (emitted at the type-def tail, gated on `@arg` use specifically, so an
`@argc`-only module gets no unused `static`) that bounds-checks `i`,
`strlen`s `argv[i]` and copies it into a `malloc`-backed slice — empty slice
when out of range. As in §41, the allocator is the malloc-backed stub (§16.2),
so the result is freeable with `free(a, s)`.

### 44.3 Deferred (honest)
No `@deleteFile`/`@renameFile` (a round-trip test cannot clean up after
itself in-language); no error *cause* for a failed write (the `bool` carries
one bit, see above); no byte-exact binary-mode guarantees beyond C's `"wb"`
(`fopen` binary mode covers POSIX + macOS, the supported targets).
