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

## 7. `build.ks` (v1 minimal form)

```
build {
    name = "hello";
    root = "src/main.ks";
}
```

`build_system::parse_build_kd` extracts `name` and `root`. The full imperative
build graph (steps, dependencies, install targets — Zig's `build.zig` model) is
a roadmap item.

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
