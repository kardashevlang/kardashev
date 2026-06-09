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

- `cc_build(c_src, out)` — write `c_src` to a temp `.c`, invoke the system C
  compiler (`$CC`, else `cc`, `clang`, `gcc` — first found) as
  `<cc> -O2 -std=c11 -o <out> <tmp.c>`; return its stderr on non-zero exit.
- `cc_build_and_run(c_src, args)` — build to a temp executable, exec it with
  `args`, return the child exit code.

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
  `T`; result `T`; `default` is evaluated eagerly in v0.115.

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
to its digits). Non-generic literal-sized arrays are unchanged.

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
Multiple type parameters, comptime-value type-constructor params, and direct
`Name(T)` / `Name(T){…}` in type / literal position (use a `const` type alias) —
all later work. (Methods inside a generic struct land in v0.130.)

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
