# Effects System

kardashev's signature feature is **lightweight effect labels in the
type system**: every function declares the side effects it can produce
as part of its signature, and the compiler tracks them across the call
graph. There are no handlers or continuations (unlike Koka) — effects
are pure type-system information with zero runtime cost.

## Syntax

An effect row attaches to a function's return type:

```rust
fn read_cfg() -> i64 ! { io, alloc } { ... }
fn add(a: i64, b: i64) -> i64          { a + b }   // pure (no row)
```

The grammar: `! '{' label (',' label)* ','? '}'`. An empty row (or no
row at all) means `pure` — the function has no declared effects.

## Built-in labels

| Label    | Meaning                                                    |
|----------|------------------------------------------------------------|
| `alloc`  | Heap allocation                                            |
| `io`     | File / network / stdio / general syscalls                  |
| `panic`  | Unrecoverable failure                                      |
| `async`  | Yields to a scheduler (Phase 6+)                           |
| `unwind` | Stack unwinding for cancellation (distinct from `panic`)   |

Unknown labels in a row are an error unless they match a generic
parameter declared on the same fn — this reserves the namespace for
row-polymorphic effect variables (`fn map<T, U, e>(...) -> ... ! {e}`),
which require first-class function values (Phase 6) before they're
useful.

## Propagation rule

For every fn body, the typechecker collects the union of declared
effects across all calls — direct (`f(x)`), method (`x.foo()`), and
constructor (`Some(7)`, free) — and verifies the union is a subset of
the enclosing fn's declared row. Missing labels are diagnosed at the
function's declaration site:

```
$ kardc bad.kd
type error 2:1: function 'main' uses effect `io` but does not declare
it; add `! { io }` to the signature
```

The `?` operator (Phase 3.4) and `.await` (Phase 6 stub) don't add
effects themselves; their operand fns do, and those propagate.

## Async as an effect

`async fn` implicitly adds `async` to the function's effect row. A
caller still has to opt in:

```rust
async fn fetch(n: i64) -> i64 { n + n }   // ! { async } implicit
fn main() -> i64 ! { async, io } { print(fetch(21).await); 0 }
```

Without `! { async }` on `main`, the compiler reports the same kind of
missing-effect diagnostic.

## Zero runtime cost

Effect rows live entirely in the typechecker. The emitted LLVM IR is
identical whether a function declares `! { io, alloc }` or omits it
(assuming it actually uses those effects). The system is documentation
+ compile-time enforcement, not a runtime ABI.

## Limitations today

- No row-polymorphic effect variables yet (need fn-pointer values).
- Effect-set membership is concrete: `alloc` matches `alloc`, no
  variance between, say, `io` (general) and a more specific `file_io`.
- The five built-ins are hard-coded; a `kardashev::effect Network;`
  declaration form (where users declare their own effects) is a future
  consideration.
