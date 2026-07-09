# kardashev examples

One focused, runnable program per language feature — each file's header
comment says what it demonstrates and (where applicable) the version that
introduced the feature. Run any of them with the toolchain:

```console
$ kard run examples/hello.ks        # build & run
$ kard test examples/hello.ks       # run its `test` blocks
$ kard doc examples/documented.ks   # render its `///` API docs
```

## First programs

| Example | Shows |
|---------|-------|
| [`hello.ks`](hello.ks) | the canonical first program: `comptime` const, `while : (…)`, `defer`, a `test` block |
| [`fib.ks`](fib.ks) | recursion |
| [`collatz.ks`](collatz.ks) | control flow: `while (c) : (cont)`, `if`/`else`, `break` |

## Bindings, numbers & comptime

| Example | Shows |
|---------|-------|
| [`inference.ks`](inference.ks) | type inference for `var`/`const` (v0.121) |
| [`casts.ks`](casts.ks) | integer casts with `@as(T, e)` (v0.137) |
| [`floats.ks`](floats.ks) | 64-bit floating point `f64` (v0.144) |
| [`bitwise.ks`](bitwise.ks) | bitwise & shift operators (v0.132) |
| [`compound_assign.ks`](compound_assign.ks) | `+= -= *= /= %=` (v0.131) |
| [`comptime_vals.ks`](comptime_vals.ks) | comptime **value** parameters — array-size generics (v0.128) |
| [`comptime_builtins.ks`](comptime_builtins.ks) | reflection: `@sizeOf`, `@typeName`, `@This` (v0.136) |

## Control flow

| Example | Shows |
|---------|-------|
| [`for_loops.ks`](for_loops.ks) | `for (xs) \|x\|` and `for (xs, 0..) \|x, i\|` over arrays & slices (v0.133) |
| [`labeled_loops.ks`](labeled_loops.ks) | labeled `break` / `continue` (v0.147) |
| [`switch_ranges.ks`](switch_ranges.ks) | `switch` range labels + multi-label arms (v0.146) |

## Structs, enums & unions

| Example | Shows |
|---------|-------|
| [`point.ks`](point.ks) | structs: literals, field access/assignment, nesting (v0.112) |
| [`counter.ks`](counter.ks) | struct methods + associated functions (v0.113) |
| [`pointer_receiver.ks`](pointer_receiver.ks) | `self: *Self` methods — true in-place mutation (v0.134) |
| [`arrays.ks`](arrays.ks) | fixed-size arrays `[N]T`, bounds-checked (v0.117) |
| [`slices.ks`](slices.ks) | pointers `*T` and slices `[]T`, slicing `a[lo..hi]` (v0.118) |
| [`enums.ks`](enums.ks) | enums + exhaustive `switch` (v0.116) |
| [`enum_values.ks`](enum_values.ks) | explicit enum values, `@intFromEnum` / `@enumFromInt` (v0.143) |
| [`unions.ks`](unions.ks) | tagged unions `union(enum)` + `switch` payload capture (v0.124) |

## Optionals & errors

| Example | Shows |
|---------|-------|
| [`optional.ks`](optional.ks) | `?T`, `null`, `orelse`, `.?` (v0.114) |
| [`captures.ks`](captures.ks) | `if (opt) \|v\|` payload capture + `errdefer` (v0.125) |
| [`errunion.ks`](errunion.ks) | error unions: `!T`, `error.X`, `try`, `catch` (v0.115) |
| [`error_sets.ks`](error_sets.ks) | named error sets `error{ … }`, `Set!T` (v0.139) |
| [`catch_capture.ks`](catch_capture.ks) | the capturing handler `catch \|e\|` (v0.142) |
| [`panic.ks`](panic.ks) | `@panic` and `unreachable` (v0.141) |

## Memory & generics

| Example | Shows |
|---------|-------|
| [`heap.ks`](heap.ks) | the explicit `Allocator`: `c_allocator()`, `alloc`, `free` (v0.119) |
| [`generics.ks`](generics.ks) | generic functions over `comptime T: type`, monomorphised (v0.120) |
| [`multi_typeparam.ks`](multi_typeparam.ks) | generics over more than one type (v0.135) |
| [`generic_structs.ks`](generic_structs.ks) | generic structs via type-returning functions (v0.129) |
| [`generic_direct.ks`](generic_direct.ks) | direct `Name(T)` application in type position — no alias needed (v0.152) |

## Strings & the standard library

| Example | Shows |
|---------|-------|
| [`strings.ks`](strings.ks) | string literals as `[]u8` values (v0.127) |
| [`string_utils.ks`](string_utils.ks) | std string helpers over `[]u8` (v0.149) |
| [`use_std.ks`](use_std.ks) | `@import("std")`: containers + helpers by bare name (v0.145) |
| [`arraylist.ks`](arraylist.ks) | `ArrayList(T)` — the growable list (v0.130) |
| [`hashmap.ks`](hashmap.ks) | `HashMap(V)` — open addressing, tombstones (v0.138) |

## I/O, arguments & tooling

| Example | Shows |
|---------|-------|
| [`io.ks`](io.ks) | `@readFile` and `@readLine` (v0.148) |
| [`write_args.ks`](write_args.ks) | `@writeFile` / `@appendFile` / `@argc` / `@arg` (v0.158) |
| [`tested.ks`](tested.ks) | `kard test --filter` and `kard bench` (v0.150) |
| [`documented.ks`](documented.ks) | `///` doc comments + `kard doc` (v0.140) |
