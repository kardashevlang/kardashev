# A tour of kardashev

Every feature of the language as of **v0.179.0**, each with a small runnable
program. Save any snippet as `x.ks` and `kard run x.ks`; expected output is
in the trailing comments. The normative rules live in [`SPEC.md`](../SPEC.md);
this tour is the friendly walkthrough.

The design laws behind everything here: **no hidden control flow** (no
exceptions, no operator overloading, no implicit destructors), **no hidden
allocations** (allocating APIs take an explicit `Allocator`), **`comptime`
instead of macros**, and **tests in the source**.

## Programs, functions, `print`

A program is a `.ks` file (or several, via [`@import`](#modules)) with a
`pub fn main` returning `void`, `i32` or `i64` — the return value is the
process exit code. Functions put the return type after the parameter list,
Zig-style. `print` writes one value per line and accepts integers, strings
(`[]u8`) and `f64`.

```rust
fn add(a: i32, b: i32) i32 {
    return a + b;
}

pub fn main() i32 {
    print(add(40, 2));   // 42
    print("hello");      // hello
    return 0;
}
```

## Bindings, integers, inference

`var` is mutable, `const` is not. The `: T` annotation is optional when the
initializer pins a type. Integer types are `i8 i16 i32 i64 u8 u16 u32 u64
usize`; literals are polymorphic (they adopt the expected type, defaulting to
`i64`) but there are **no implicit conversions** between concrete types —
cast explicitly with `@as(T, e)`. Top-level `const`s are comptime-evaluated,
and `comptime (…)` folds any constant expression at compile time.

```rust
const LIMIT: i32 = comptime (5 * 2);   // folded at compile time

pub fn main() i32 {
    var counter = 0;              // inferred i64
    counter += 1;
    const label: []u8 = "total";  // string constants work too
    var wide: i64 = 41;
    var narrow: i32 = @as(i32, wide) + 1;   // explicit cast, no coercion
    print(label);                 // total
    print(counter);               // 1
    print(narrow);                // 42
    print(LIMIT);                 // 10
    return 0;
}
```

Operators are C-like — arithmetic `+ - * / %`, comparisons, logical
`and`/`or`/`!`, bitwise `& | ^ << >> ~`, compound assignment
`+= -= *= /= %=` — with **no overloading**. `f64` is the one float type:
literals like `3.14`, arithmetic and comparison, `@as` to/from integers; no
implicit int↔float mixing, and floats are runtime-only (no float `const`s).

## Control flow: `if`, `while`, `for`

Conditions are parenthesized and must be `bool`; bodies are always braced.
`while` takes an optional `: (continue-expression)` clause that runs after
each iteration *and* on `continue`. `for` iterates arrays and slices, with
an optional 0-based `usize` index.

```rust
pub fn main() i32 {
    var i: i32 = 0;
    var evens: i32 = 0;
    while (i < 10) : (i += 1) {
        if (i % 2 == 1) {
            continue;             // the `: (i += 1)` clause still runs
        }
        evens += 1;
    }
    print(evens);                 // 5

    var data: [4]i32 = [4]i32{ 3, 1, 4, 1 };
    var sum: i32 = 0;
    for (data) |v| {              // element capture (a by-value copy)
        sum += v;
    }
    print(sum);                   // 9

    var weighted: i32 = 0;
    for (data, 0..) |v, idx| {    // element + index
        weighted += v * @as(i32, idx);
    }
    print(weighted);              // 0*3 + 1*1 + 2*4 + 3*1 = 12
    return 0;
}
```

## `switch`

`switch` is exhaustive — cover every enum variant or supply `else` (integers
always need `else`). Arms take single labels, multi-labels (`1, 2, 3 =>`) and
**inclusive** integer ranges (`10..20 =>`); there is no fall-through.

```rust
fn letter_grade(score: i32) []u8 {
    switch (score) {
        90..100 => { return "A"; },   // inclusive: 100 is an A
        80..89  => { return "B"; },
        60..79  => { return "C"; },
        0, 1, 2 => { return "?"; },   // multi-label arm
        else    => { return "F"; },
    }
}

pub fn main() i32 {
    print(letter_grade(100));   // A
    print(letter_grade(83));    // B
    print(letter_grade(1));     // ?
    print(letter_grade(42));    // F
    return 0;
}
```

## Labeled loops

Loops can carry a label; `break :label` / `continue :label` target an
enclosing loop directly.

```rust
pub fn main() i32 {
    var found: i32 = 0 - 1;
    outer: while (found < 0) : (found -= 1) {
        var j: i32 = 0;
        while (j < 100) : (j += 1) {
            if (j * j == 289) {
                found = j;
                break :outer;     // leaves BOTH loops
            }
        }
        break :outer;
    }
    print(found);                 // 17
    return 0;
}
```

## `defer` and `errdefer`

`defer` schedules a statement for scope exit; deferred statements run in
**LIFO** order on every exit edge — fall-through, `return`, `break`,
`continue`. `errdefer` joins the same stack but runs only when the scope
exits via an error return. This is the language's only deferred control
flow, and it is explicit.

```rust
pub fn main() i32 {
    print(1);          // 1
    defer print(4);    // registered first → runs last
    defer print(3);    // registered second → runs first
    print(2);          // 2
    return 0;          // then 3, then 4
}
```

## Structs and methods

Structs are by-value product types. Functions declared inside the struct
block become methods (`instance.method(…)` prepends `self`) or associated
functions (no `self`, called `Type.func(…)`). A **pointer receiver**
(`self: *Self`) mutates the caller's value in place: the call site auto-refs
and field access through a pointer auto-derefs. `Self` (or `@This()`) names
the enclosing struct type.

```rust
const Point = struct {
    x: i32,
    y: i32,
    fn origin() Self {                       // associated function
        return Self{ .x = 0, .y = 0 };
    }
    fn manhattan(self: Point) i32 {          // value receiver: a copy
        return iabs_(self.x) + iabs_(self.y);
    }
    fn move_by(self: *Self, dx: i32, dy: i32) void {   // pointer receiver
        self.x += dx;
        self.y += dy;
    }
};

fn iabs_(x: i32) i32 {
    if (x < 0) { return 0 - x; }
    return x;
}

pub fn main() i32 {
    var p: Point = Point.origin();
    p.move_by(3, 0 - 4);          // mutates p in place
    print(p.x);                   // 3
    print(p.manhattan());         // 7
    return 0;
}
```

## Enums and tagged unions

A plain `enum` is a set of named variants; write values qualified
(`Op.Add`) or inferred (`.Add`). Variants can carry explicit integer values
(`enum { A = 1, B, C = 10 }` — unvalued variants auto-increment), and
`@intFromEnum(e)` / `@enumFromInt(E, n)` convert. A `union(enum)` holds
exactly one typed payload, matched and captured in `switch`:

```rust
const Shape = union(enum) {
    circle: i64,                       // radius
    square: i64,                       // side
};

fn area3(s: Shape) i64 {               // pi ≈ 3 for round numbers
    switch (s) {
        .circle => |r| { return 3 * r * r; },
        .square => |w| { return w * w; },
    }
}

pub fn main() i32 {
    print(area3(Shape{ .circle = 10 }));   // 300
    print(area3(Shape{ .square = 5 }));    // 25
    return 0;
}
```

## Optionals

`?T` makes absence explicit: `null` is the empty value, a `T` coerces to
`?T`, `orelse` unwraps-or-defaults, `.?` force-unwraps (panicking on null),
and `if (opt) |v| { … } else { … }` unwraps with a binding.

```rust
fn sqrt_below_5(target: i32) ?i32 {
    var i: i32 = 0;
    while (i < 5) : (i += 1) {
        if (i * i == target) {
            return i;                    // T coerces to ?T
        }
    }
    return null;
}

pub fn main() i32 {
    print(sqrt_below_5(9) orelse 0 - 1);   // 3
    print(sqrt_below_5(7) orelse 0 - 1);   // -1
    if (sqrt_below_5(16)) |v| {
        print(v);                          // 4
    }
    return 0;
}
```

## Errors as values

A `!T` function either returns a `T` or an error — a named value like
`error.DivByZero`, not an exception. `try e` unwraps or propagates the error
out of the (also `!T`) caller; `e catch default` unwraps or falls back;
`catch |e|` binds the error code. Error sets can be named and membership is
compile-checked (`FileErr!T` may only return errors in `FileErr`). `!void`
works for functions with nothing to return, and `errdefer` runs cleanup only
on the error path.

`try` placement is deliberately restricted (SPEC §8): it must be the whole
value of an initializer, a `return`, or an expression statement — not nested
inside a larger expression.

```rust
const MathErr = error{ DivByZero };

fn checked_div(a: i64, b: i64) MathErr!i64 {
    if (b == 0) {
        return error.DivByZero;
    }
    return a / b;
}

fn average(total: i64, n: i64) !i64 {
    var per = try checked_div(total, n);   // propagates DivByZero
    return per;
}

pub fn main() i32 {
    print(checked_div(20, 4) catch -1);    // 5
    print(average(20, 0) catch -1);        // -1
    // `catch |e|` binds the error's i32 code; the handler's value must
    // match the payload type (i64 here), so cast:
    var code = checked_div(1, 0) catch |e| @as(i64, e);
    print(code);                           // 1 (first interned error name)
    return 0;
}
```

## Arrays, slices, strings

`[N]T` is a fixed-size **value** type with an `[N]T{ … }` literal; `[]T` is
a `{ptr, len}` **view** produced by slicing, `a[lo..hi]` (lo inclusive, hi
**exclusive** — unlike `switch` ranges). Indexing and slicing are
bounds-checked at runtime — out of bounds panics with exit code 101. String
literals are `[]u8` slices over static bytes.

```rust
pub fn main() i32 {
    var data: [6]i32 = [6]i32{ 4, 1, 9, 2, 8, 5 };
    var window: []i32 = data[1..5];     // elements 1,9,2,8
    print(window.len);                  // 4
    print(window[1]);                   // 9
    window[1] = 42;                     // writes through to `data`
    print(data[2]);                     // 42

    var s: []u8 = "kardashev";
    print(s.len);                       // 9
    print(s[0..4]);                     // kard
    return 0;
}
```

## Pointers

`&place` takes an address, `p.*` reads and `p.* = e` writes through it.
Pointers are raw (no lifetime tracking) — this is a systems language;
what the language *does* check (bounds, unwraps, exhaustiveness) it checks
always.

```rust
fn swap(a: *i32, b: *i32) void {
    var t: i32 = a.*;
    a.* = b.*;
    b.* = t;
}

pub fn main() i32 {
    var x: i32 = 3;
    var y: i32 = 7;
    swap(&x, &y);
    print(x);   // 7
    print(y);   // 3
    return 0;
}
```

## Memory: the explicit `Allocator`

There is no global allocator, ever. Obtain one (`c_allocator()` wraps
malloc/free), pass it to everything that allocates, release with `free` —
`defer` makes the cleanup local and visible. Allocation failure panics
(error-returning alloc is an honest deferral, SPEC §16.3).

```rust
pub fn main() i32 {
    var a: Allocator = c_allocator();
    var buf: []i64 = alloc(a, i64, 5);   // a heap []i64
    defer free(a, buf);

    var i: usize = 0;
    while (i < buf.len) : (i += 1) {
        buf[i] = @as(i64, i * i);
    }
    print(buf[4]);                       // 16
    return 0;
}
```

## Generics: `comptime` parameters

A `comptime T: type` parameter makes a function generic; each distinct
argument **monomorphises** a specialised copy at compile time — no runtime
dispatch, no boxing. Parameters can also be comptime *values* (array-size
generics). Type arguments are positional: `max(i32, a, b)`.

```rust
fn max(comptime T: type, a: T, b: T) T {
    if (a > b) { return a; }
    return b;
}

fn sum_fixed(comptime n: usize, xs: [n]i64) i64 {   // comptime VALUE param
    var total: i64 = 0;
    for (xs) |v| { total += v; }
    return total;
}

pub fn main() i32 {
    print(max(i32, 3, 9));                       // 9
    print(max(f64, 2.5, 1.5));                   // 2.5
    print(sum_fixed(3, [3]i64{ 10, 20, 12 }));   // 42
    return 0;
}
```

**Generic structs** are functions returning a `type` — Zig's
type-constructors — usable through an alias or applied directly in type
position, with methods monomorphised per instantiation:

```rust
fn Pair(comptime T: type) type {
    return struct {
        first: T,
        second: T,
        fn flipped(self: Self) Pair(T) {
            return Self{ .first = self.second, .second = self.first };
        }
    };
}

const IntPair = Pair(i32);              // alias form

pub fn main() i32 {
    var p: IntPair = IntPair{ .first = 1, .second = 99 };
    var q: Pair(i32) = p.flipped();     // direct application form (v0.152)
    print(q.first);                     // 99
    return 0;
}
```

Reflection builtins round comptime out: `@sizeOf(T)` (a `usize`),
`@typeName(T)` (a `[]u8`, substitution-aware inside generics) and `@This()`
(the enclosing struct type).

## Runtime safety: `@panic` and `unreachable`

`@panic("msg")` writes to stderr and exits with code 101; `unreachable`
asserts a path off. Both **diverge**, so they stand in any value position —
e.g. a `switch` else-arm that must never be taken. The built-in checks
(index bounds, `.?` on null) panic the same way.

## Modules

`@import("file.ks");` resolves relative to the importing file, deduplicates,
rejects cycles, and **flattens** everything into one program — items are
then visible by bare name, and top-level names must be globally unique
(namespaced access is an honest deferral, SPEC §22.2). `@import("std");`
loads the [standard library](stdlib.md) embedded in the compiler; unused std
code is eliminated, so it costs nothing.

## Tests

`test "name" { … }` blocks live in the source. `expect(cond)` fails the test
when false (it is only callable inside `test` blocks). `kard test file.ks`
builds a harness running every block; `--filter SUBSTR` selects by name;
`kard bench` times each test.

```rust
fn fib(n: i32) i32 {
    if (n < 2) { return n; }
    return fib(n - 1) + fib(n - 2);
}

pub fn main() i32 {
    print(fib(10));   // 55
    return 0;
}

test "fib base cases" {
    expect(fib(0) == 0);
    expect(fib(1) == 1);
}

test "fib recurrence" {
    expect(fib(10) == 55);
}
```

Document public APIs with `///` doc comments and render them with
`kard doc file.ks` — see [getting started](getting-started.md#formatting-and-api-docs).

## I/O and program arguments

The I/O surface is deliberately minimal (SPEC §41/§44), allocator-explicit,
and honest about errors it can't express yet:

| Builtin | Behaviour |
|---------|-----------|
| `@readFile(a, path) []u8` | whole file (empty slice on error) |
| `@readLine(a) []u8` | one stdin line (empty slice on EOF/error) |
| `@writeFile(path, data) bool` | create/truncate-write, `false` on error |
| `@appendFile(path, data) bool` | append, creating if missing |
| `@argc() i64` | argument count, **including** argv[0] |
| `@arg(a, i) []u8` | i-th argument, freshly allocated |

## The builtin inventory

Callable anywhere without imports: `print`, `expect` (tests only),
`alloc`/`free`/`c_allocator`, the `@`-builtins — `@import`, `@as`,
`@sizeOf`, `@typeName`, `@This`, `@intFromEnum`, `@enumFromInt`, `@panic`,
`@readFile`, `@readLine`, `@writeFile`, `@appendFile`, `@argc`, `@arg` —
and the `unreachable` expression.

## What's deliberately absent

No exceptions, no operator overloading, no destructors/RAII, no globals
mutable at runtime, no hidden allocator, no macros. And a set of *planned*
gaps tracked honestly in [SPEC §8](../SPEC.md#8-honest-deferrals-tracked-in-roadmap-rust-zigmd)
— value-yielding blocks, nested `try`, `Name(T){…}` literals, hex literals,
namespaced imports, and friends. Nothing is stubbed: what's absent is
rejected with a diagnostic and scheduled in
[the roadmap](../ROADMAP-RUST-ZIG.md).
