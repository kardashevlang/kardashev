# Effects System

kardashev's signature feature is **lightweight effect labels in the
type system**: every function declares the side effects it can produce
as part of its signature, and the compiler tracks them across the call
graph. There are no handlers or continuations (unlike Koka) — effects
are pure type-system information with **zero runtime cost**. The
emitted LLVM IR is byte-for-byte identical whether a function declares
`! { io, alloc }` or omits the row entirely; the row exists only to be
checked.

## Syntax

An effect row attaches after a function's return type, introduced by
`!`:

```rust
fn read_cfg() -> i64 ! { io, alloc } { ... }
fn add(a: i64, b: i64) -> i64 { a + b }   // pure — no row
```

The grammar is `! '{' label (',' label)* ','? '}'`. An empty row
(`! {}`) or no row at all means **`pure`**: the function declares no
effects. `pure` is therefore the default, not a keyword you write.

## Built-in labels

The five concrete effect labels are built in:

| Label    | Meaning                                                    |
|----------|------------------------------------------------------------|
| `alloc`  | Heap allocation                                            |
| `io`     | File / network / stdio / general syscalls                  |
| `panic`  | Unrecoverable failure (unwinds via `panic(msg)`)           |
| `async`  | Yields to the scheduler                                    |
| `unwind` | Stack unwinding for cancellation (distinct from `panic`)   |

`pure` is the empty row, not a sixth label. The five built-ins are
hard-coded; a user-declared effect form (e.g. `effect Network;`)
remains a future consideration.

Any other identifier in a row is an error **unless** it matches a
generic parameter declared on the same fn — that reservation is what
makes effect rows row-polymorphic (next section).

## Row polymorphism

A function type carries an effect row, and that row can be a *variable*
rather than a fixed set. Writing a generic parameter name inside the
row makes the function **effect-polymorphic**: it inherits whatever
effects its function-valued argument carries.

```rust
fn map<T, U, e>(xs: Vec<T>, f: fn(T) -> U ! {e}) -> Vec<U> ! { e, alloc } {
    let mut out = Vec::with_capacity(xs.len());
    for x in xs { out.push(f(x)); }
    out
}
```

Here `e` is a row variable. `map` is **pure when `f` is pure**, and
propagates exactly the effects `f` introduces — its own declared row
unions `e` with the `alloc` it performs itself. The function-pointer
type `fn(T) -> U ! {e}` spells the effect row of the value it accepts,
so the row flows from the argument's type through to the caller's
obligation.

This is enforced, not cosmetic: a pure caller that passes an `io`
closure to a function expecting a pure one is rejected, and a caller of
`map` with an `io` `f` must itself declare `io`.

## Propagation rule

For every function body, the typechecker collects the **union** of the
declared effect rows of everything it calls — direct calls (`f(x)`),
method calls (`x.foo()`), constructor calls (`Some(7)`, which are
free), function-valued calls (`f(x)` where `f` is a closure or
fn-pointer parameter, contributing that value's row), and built-ins —
and verifies that union is a subset of the enclosing function's
declared row. Anything missing is diagnosed **at the calling
function's definition site** (not at runtime, and not at the call
site's caller):

```
$ kardc bad.kd
type error 2:1: function 'main' uses effect `io` but does not declare
it; add `! { io }` to the signature
```

So a `pure` function that calls an `io` function is a compile error:

```rust
fn raw_read() -> i64 ! { io } { 42 }
fn use_it() -> i64 { raw_read() }   // error: uses `io`, declares none
```

Declaring the row makes it compile, and the effect keeps propagating
outward — every caller up the chain must declare `io` too (or sit
behind an effect-polymorphic boundary):

```rust
fn raw_read() -> i64 ! { io } { 42 }
fn main() -> i64 ! { io } { raw_read() }   // OK
```

The `?` operator and `.await` do not introduce effects of their own;
the functions they operate on do, and those propagate through the
normal union. (`async fn` is the one implicit source — see below.)

## Async as an effect

An `async fn` implicitly adds `async` to its own row, and a caller must
still opt in:

```rust
async fn fetch(n: i64) -> i64 { n + n }       // ! { async } implicit
fn main() -> i64 ! { async, io } { print(fetch(21).await); 0 }
```

Without `! { async }` on `main`, the compiler reports the same
missing-effect diagnostic as any other undeclared effect. (`async` is
a fully real runtime now — a single-threaded executor with `spawn` /
`join` / `block_on` / `sleep_ms` and an epoll reactor on Linux — but
from the effect system's point of view it is just another label that
unions and checks like the rest.)

## `panic` and `catch`

`panic(msg)` carries the `panic` effect: it prints to stderr and
unwinds (setjmp/longjmp), running Drop glue on the way out. So a
function that can panic must declare `panic`, and that propagates to
its callers like any other effect.

`catch(f, recover)` is the boundary: it runs `f`, and if `f` panics it
runs `recover` instead of letting the unwind escape. Because `catch`
contains the panic, **it clears `panic` from the row** — code wrapped
in `catch` does not force its caller to declare `panic`. This is the
effect system's analogue of `Result`-style recovery: a known-recovered
panic is no longer an effect the caller is obligated to acknowledge.

## FFI carries `io`

An `extern "C"` call is opaque to the effect checker — kardashev cannot
see what the foreign function does — so every `extern "C"` call is
treated as carrying `io`. A function that calls into C must therefore
declare `io`:

```rust
extern "C" fn strlen(s: &String) -> i64;
fn name_len() -> i64 ! { io } {            // `io` required: extern call
    let n = strlen(&greeting);
    n
}
```

This is the conservative-but-honest choice: a C function might do
anything, so the boundary is labeled with the broadest concrete effect
rather than silently treated as pure.

## Zero runtime cost

Effect rows live entirely in the typechecker. Programs flow through
lexer → parser → HM typechecker → NLL borrow-checker → **effect
inference** → LLVM IR; the effect pass is a checking pass, not a
lowering pass. The emitted IR for a function is identical whether its
row is `! { io, alloc }` or omitted. There is no runtime effect ABI, no
handler dispatch, no tagging — the system is documentation plus
compile-time enforcement, and nothing survives into the binary.

The LSP surfaces the row where it matters: `kard-lsp` hover shows a
function's signature **including its effect row**.

## Limitations today

- Effect-set membership is concrete: `alloc` matches `alloc`, with no
  subtyping or variance between, say, `io` (general) and a
  hypothetical more-specific `file_io`.
- The five built-in labels are hard-coded; there is no user-defined
  effect-declaration form yet.
