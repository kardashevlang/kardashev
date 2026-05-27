# kardashev Language Reference

Surface syntax + semantics for what kardashev compiles today. The
roadmap in [`../README.md`](../README.md) tracks what's still on the
runway; this document only covers what works.

## Lexical structure

| Token              | Notes                                          |
|--------------------|------------------------------------------------|
| Identifier         | `[A-Za-z_][A-Za-z0-9_]*`                       |
| Integer literal    | `[0-9]+`                                       |
| Operators          | `+ - * / < <= > >= == != = -> => ? !`          |
| Punctuation        | `( ) { } , ; : :: . _ &`                       |
| Keywords           | `fn let if else return struct enum match`      |
|                    | `trait impl for mod pub`                        |

`async`, `await`, and `mut` are reserved by convention but stay as
plain Identifiers so they can appear inside effect rows / generic
parameter names. The parser disambiguates them positionally.

Comments are `// to end of line`. No block comments yet.

## Top-level grammar

```
program        := decl*
decl           := mod_decl | (pub?) (fn_decl | struct_decl | enum_decl | trait_decl | impl_decl)
mod_decl       := 'mod' Ident ';'
fn_decl        := ('async')? 'fn' Ident generic_params? '(' params? ')' '->' type_ref effect_row? block_expr
generic_params := '<' generic_param (',' generic_param)* ','? '>'
generic_param  := Ident (':' Ident)?              -- optional single-trait bound
struct_decl    := 'struct' Ident generic_params? '{' field_decl (',' field_decl)* ','? '}'
enum_decl      := 'enum' Ident generic_params? '{' variant (',' variant)* ','? '}'
trait_decl     := 'trait' Ident '{' method_sig* '}'
method_sig     := 'fn' Ident '(' params? ')' '->' type_ref effect_row? ';'
impl_decl      := 'impl' Ident 'for' type_ref '{' fn_decl* '}'
effect_row     := '!' '{' (effect_label (',' effect_label)* ','?)? '}'
type_ref       := ref_prefix? path type_args?
path           := Ident ('::' Ident)*
type_args      := '<' type_ref (',' type_ref)* ','? '>'
ref_prefix     := '&' 'mut'? -- '&' or '&mut'
```

## Expressions

Operator precedence (low to high):

1. Comparisons: `< <= > >= == !=`
2. Additive: `+ -`
3. Multiplicative: `* /`

Postfix operators (left-associative):

- `.field` — struct field access (auto-derefs through `&T` / `&mut T`)
- `.method(args)` — trait method dispatch
- `?` — Result-style early return (Phase 3.4)
- `.await` — async suspend point (Phase 6 stub — pure pass-through today)

Primary expressions:

- Integer literal: `42`
- Identifier reference: `x`, `foo::bar`
- Function / constructor call: `f(a, b)`, `Some(7)`, `Point { x: 3, y: 4 }`
- Block: `{ stmts; tail_expr }`
- `if cond { then } else { else }`
- `match e { pat => body, ... }`
- `&expr`, `&mut expr` — borrow

Statements:

- `let name = expr;`
- `return expr;`
- `expr;` (discarded)

## Types

Primitive: `i64`, `bool`, `()` (unit, only internally — not user-named).

Compound:
- `struct Foo { field: T, ... }` — value-typed, fields accessed via `.`
- `enum E { Variant, Variant(T), ... }` — tagged union
- `Foo<T>`, `E<T, U>` — generic instantiations
- `&T`, `&mut T` — references (Phase 2.4b/c)

Built-in: `Vec` (a growable `i64` buffer with malloc-backed heap storage).

Built-in prelude (auto-included by `kardc`):

```
enum Option<T> { Some(T), None }
enum Result<T, E> { Ok(T), Err(E) }
```

## Built-in functions

| Signature                                           | Notes                          |
|-----------------------------------------------------|--------------------------------|
| `print(n: i64) -> i64 ! { io }`                     | writes one i64 + newline       |
| `vec_new() -> Vec ! { alloc }`                      | empty growable buffer          |
| `vec_push(v: &mut Vec, x: i64) -> i64 ! { alloc }`  | append (may realloc)           |
| `vec_get(v: &Vec, i: i64) -> i64`                   | index, no bounds check yet     |
| `vec_len(v: &Vec) -> i64`                           | element count                  |

## Modules

`mod foo;` at the top of a `.kd` file pulls in `foo.kd` from the same
directory and merges its declarations into the program. Resolution is
recursive (modules can declare their own `mod` lines) and cycle-safe.

`pub` is accepted as a visibility marker on top-level decls but is
currently a no-op (Phase 7.1 flat-merges everything). Path-qualified
references (`math::double(p.x)`) parse and collapse to the last
segment for the same reason.

## See also

- [Effects system](effects.md) — `! { io, alloc }` semantics
- [Architecture](architecture.md) — compiler pipeline
