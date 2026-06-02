# The Kardashev Language Specification (draft)

Status: **draft / pre-1.0**. This document is the normative reference for the
kardashev language as implemented by `kardc` at this version. It describes the
language that the compiler **actually accepts and runs today** — where a feature
is partial, that is stated. Every normative clause has a stable `[K-xxx]` id so
tests and tooling can link to it. The grammar below is exercised by
`tests/smoke_test_grammar_conformance.sh` (representative programs parse;
ill-formed programs are rejected).

This draft covers the syntactic surface + the load-bearing semantic clauses
(panic, integer overflow, the C ABI, the effect system, ownership). Full
operational semantics, a machine-checkable EBNF-conformance generator, and the
6/6 formal-methods guarantees (totality, refinement types, mechanized soundness)
are tracked in `ROADMAP-1.0-AND-BEYOND.md` (v45 spec, v47–v50 beyond).

## 1. Lexical structure

```ebnf
ident      = (letter | "_") , { letter | digit | "_" } ;
int_lit    = digit , { digit | "_" } , [ int_suffix ] ;        (* 5, 0xFF, 5i32 *)
float_lit  = digit , { digit } , "." , digit , { digit } , [ exp ] ;
string_lit = '"' , { str_char | escape } , '"' ;               (* \n \t \\ \" \u{..} *)
char_lit   = "'" , ( char | escape ) , "'" ;                   (* one Unicode scalar *)
bool_lit   = "true" | "false" ;
doc_comment= "///" , { any_until_eol } ;                        (* attaches to next decl *)
line_comment = "//" , { any_until_eol } ;
```

Contextual keywords (lexed as identifiers, meaningful only in position):
`async`, `await`, `effect`, `handle`, `with`, `perform`, `unsafe`, `use`, `type`,
`macro_rules`. Reserved: `fn let if else return struct enum match trait impl for
while loop break continue mod pub true false extern const as`.

## 2. Items (top level)

```ebnf
program   = { item } ;
item      = [ doc_comment ] , [ attribute ] , [ "pub" [ pub_restrict ] ] ,
            ( fn_decl | struct_decl | enum_decl | trait_decl | impl_decl
            | const_decl | type_alias | use_decl | mod_decl | extern_block
            | macro_rules ) ;
attribute = "#" , "[" , ( "derive" , "(" , ident_list , ")"
                        | "cfg" , "(" , cfg_pred , ")"
                        | ident , [ "(" , { token } , ")" ] ) , "]" ;
fn_decl   = [ "async" ] , [ "const" ] , "fn" , ident , [ generics ] ,
            "(" , [ params ] , ")" , [ "->" , type ] , [ effect_row ] ,
            ( block | ";" ) ;
struct_decl = "struct" , ident , [ generics ] , "{" , [ field_list ] , "}" ;
enum_decl   = "enum" , ident , [ generics ] , "{" , { variant , [","] } , "}" ;
trait_decl  = "trait" , ident , [ generics ] , [ ":" , bound_list ] ,
              "{" , { trait_member } , "}" ;
impl_decl   = "impl" , [ generics ] , [ "!" ] , type , [ "for" , type ] ,
              "{" , { fn_decl } , "}" ;
const_decl  = "const" , ident , ":" , type , "=" , expr , ";" ;
generics    = "<" , generic_param , { "," , generic_param } , ">" ;
effect_row  = "!" , "{" , [ ident , { "," , ident } ] , "}" ;     (* see §6 *)
```

## 3. Types

```ebnf
type = ident , [ "<" , type_args , ">" ]            (* named / generic *)
     | "&" , [ "mut" ] , type                        (* reference *)
     | "*" , ( "const" | "mut" ) , type              (* raw pointer (unsafe) *)
     | "dyn" , type                                  (* trait object, see §5 *)
     | "(" , [ type , { "," , type } ] , ")"         (* tuple / unit *)
     | "[" , type , ";" , const_expr , "]"           (* fixed array *)
     | "fn" , "(" , [ type_list ] , ")" , [ "->" , type ] , [ effect_row ] ;
```

Built-in scalar types: `i8 i16 i32 i64 u8 u16 u32 u64` (the integer tower),
`f32 f64`, `bool`, `char` (a Unicode scalar, NOT an integer — `[K-char]`),
`String`, the unit type `()`.

## 4. Expressions, patterns, statements

```ebnf
expr  = literal | ident | call | method_call | field | index | tuple_field
      | binary | unary | if | match | block | while | loop | for | closure
      | struct_lit | array_lit | tuple_lit | cast | try | ref | range
      | turbofish_call | macro_invoke ;
binary = expr , bin_op , expr ;     (* + - * / % & | ^ << >> && || == != < <= > >= *)
unary  = ( "-" | "!" | "*" | "&" | "~" ) , expr ;
match  = "match" , expr , "{" , { pattern , "=>" , expr , "," } , "}" ;
         (* match GUARDS `pat if cond =>` are roadmapped, not yet implemented *)
pattern= literal_pat | ident_pat | "_" | enum_pat | struct_pat | tuple_pat
       | slice_pat | ( pattern , "|" , pattern ) ;
closure= "|" , [ closure_params ] , "|" , expr | "||" , expr ;
cast   = expr , "as" , type ;
try    = expr , "?" ;
turbofish_call = ident , "::" , "<" , type_args , ">" , "(" , [ args ] , ")" ;
macro_invoke   = ident , "!" , ( "(" {token} ")" | "[" {token} "]" | "{" {token} "}" ) ;
stmt   = let_stmt | assign_stmt | expr_stmt | return_stmt ;
let_stmt    = "let" , [ "mut" ] , pattern , [ ":" , type ] , [ "=" , expr ] , ";" ;
assign_stmt = place , "=" , expr , ";" ;       (* place = ident | field | index | *deref *)
```

Notes: every `if` used as a value requires an `else` (`[K-if-else]`). `match`
arms with a block body are comma-separated (`[K-match-comma]`). A `*p = v`
deref-assignment writes through `&mut T` (safe) or `*mut T` (in `unsafe`).

## 5. Traits, generics, object safety

Traits support default methods, supertraits (`trait Ord: Eq`), blanket impls,
associated types/consts, and GATs (concrete-Self). Generic functions are
monomorphized; explicit type arguments use turbofish `f::<T>(x)`.

`[K-obj-safe]` A trait may be used as `dyn Trait` only if it is **object-safe**:
every method has a `self` receiver, no method returns `Self` by value, and no
method takes a `Self`-by-value (non-receiver) parameter. A `dyn` use of a
non-object-safe trait is a compile error naming the offending member.

## 6. The effect system (normative)

`[K-effects]` Every function has an effect row `! { e1, e2, … }` (empty = pure).
A function's body may only perform effects in its declared row (the
**effect-subset rule**); calling an effectful function attributes its effects to
the caller. Built-in effects include `io`, `alloc`, `panic`, `async`, `share`.
User effects are declared with `effect E { fn op(..) -> R; }` and discharged by
`handle { body } with E { op(p) => .. }`; `perform E::op(x)` invokes the
installed handler (tail-resumptive). `catch` clears `panic`; `block_on` strips
`async`. Effect subtyping: a pure function coerces where an effectful one is
expected (`[K-effect-sub]`).

## 7. Ownership & memory (normative)

`[K-own]` kardashev is affine: each non-`Copy` value has one owner; binding or
passing it by value **moves** it (the source is then unusable). `Copy` scalars
(the integer tower, `bool`, `char`, `f32/f64`, raw pointers, atomics) are copied.
`[K-borrow]` `&T` (shared) and `&mut T` (exclusive) borrows are checked: no
`&mut` aliasing, with two-phase borrows for `f(&mut v, g(&v))`. The current
borrow checker is an NLL-lite position-counting analysis; full region inference
is roadmapped. `[K-escape]` a function may not return a value that contains a
reference rooted in this call frame: a returned reference (directly, or wrapped
in a struct / tuple / enum / array, through `if`/`match`/`loop`/block control
flow, a method receiver, or a call) must root in a by-reference parameter or a
global, never a local, a by-value parameter, or a temporary. This is sound and
conservative (no lifetime variables yet, so a return whose reference roots in
*some* reference parameter is accepted; precise multi-parameter lifetimes and
reference-stores into out-parameters are roadmapped). `[K-drop]` values are
dropped (RAII) in reverse declaration order at scope exit; user `Drop` impls are
not yet supported.

`[K-mem-model]` There is no garbage collector. Heap memory (`Box`, `Vec`,
`String`, `HashMap`, `Arc`) is freed deterministically at end of ownership. The
one documented safe-subset leak is an `Arc`/`Rc` reference cycle.

## 8. Panic, overflow & the C ABI (normative load-bearing clauses)

`[K-panic]` A `panic(msg)` (or an out-of-bounds index, a failed `?` with no
`From`, an arithmetic trap) aborts the current thread after running pending
`Drop`s on the unwound frames; `catch(f)` recovers a panic within `f` (via
`setjmp`/`longjmp`) and clears the `panic` effect. **A panic must not cross an
`extern "C"` boundary** (the FFI-unwind contract — `[K-ffi-unwind]`); doing so is
undefined and a future lint will reject it at compile time.

`[K-overflow]` The default integer-overflow policy is **two's-complement
wrapping** (`-fwrapv` semantics): `i64::MAX + 1` wraps to `i64::MIN`. Explicit
checked/wrapping operators (`checked_add → Option`, `wrapping_add → i64`) are
provided; a selectable trap mode is roadmapped.

`[K-abi]` `extern "C"` functions use the platform C ABI. Scalar arguments
(the integer tower, `f32/f64`, `bool`), `*const T`/`*mut T` raw pointers, and
`&String` (as a C pointer) are supported across the boundary; `repr(C)`
struct-by-value and C callbacks are roadmapped. C symbol names are unmangled.
`[K-repr]` Aggregate layout follows the host target data layout (set before LLVM
optimization), so `repr(C)` field offsets match C.

## 9. Conformance

`tests/smoke_test_grammar_conformance.sh` checks a curated corpus: a
representative program for each grammar production in §2–§4 compiles, and an
ill-formed corpus is rejected with a diagnostic (never a crash). A
machine-generated ≥2000-program EBNF-conformance suite + ≥95% coverage is the
remaining v45 spec gate (ROADMAP-1.0-AND-BEYOND.md).
