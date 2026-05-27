# Standard Library

What ships with `kardc` today. Everything here is built into the
compiler — no external crate / module needs to be imported.

## Prelude (auto-included)

```rust
enum Option<T> { Some(T), None }
enum Result<T, E> { Ok(T), Err(E) }
```

Available in every program kardc compiles. Tests that bypass the
driver and call `kardashev::parse` directly don't see the prelude;
they have to declare these themselves (the existing typecheck tests
work this way).

## I/O

### `print(n: i64) -> i64 ! { io }`

Writes one i64 + newline to stdout via libc's `printf("%lld\n", n)`.
Returns 0. Requires the caller to declare `io` in its effect row.

```rust
fn main() -> i64 ! { io } {
    print(42);       // -> writes "42\n"
    print(0 - 7);    // -> writes "-7\n"
    0
}
```

## Vec

A growable buffer of `i64` elements. Backed by libc's `malloc` /
`realloc` with capacity-doubling growth (0 → 4 → 8 → ...).

### `vec_new() -> Vec ! { alloc }`

Returns an empty Vec with `cap == 0` (the first `vec_push` allocates
the initial 4-slot backing buffer).

### `vec_push(v: &mut Vec, x: i64) -> i64 ! { alloc }`

Appends `x`. Reallocates if `len == cap`. Returns 0.

### `vec_get(v: &Vec, i: i64) -> i64`

Returns the i-th element. **No bounds check yet** — reading past `len`
is undefined behaviour. Add bounds-checked accessors when `panic`
support lands.

### `vec_len(v: &Vec) -> i64`

Returns the current element count.

### Example

```rust
fn sum_from(v: &Vec, i: i64) -> i64 {
    if i < vec_len(v) { vec_get(v, i) + sum_from(v, i + 1) }
    else              { 0 }
}
fn main() -> i64 ! { alloc, io } {
    let v = vec_new();
    vec_push(&mut v, 10);
    vec_push(&mut v, 20);
    vec_push(&mut v, 30);
    print(vec_len(&v));     // 3
    sum_from(&v, 0)         // 60
}
```

## What's missing

- `Vec<T>` for arbitrary `T` — today the element type is fixed at
  `i64`. Each new element type needs a separate runtime function pair
  until the codegen learns to specialize the heap layout per `T`.
- `String` — needs a byte type (kardashev only has `i64` / `bool`
  today) before it can wrap a heap-allocated `Vec<u8>`.
- Drop / destructors — Vec's backing buffer leaks when the binding
  goes out of scope. Fine for programs that run-to-completion, not
  fine for daemons.
- `Option::unwrap_or` / `Result::map` / etc. — these would all be
  user-callable methods, blocked on the same first-class function
  values that `! {e}` row polymorphism wants.
