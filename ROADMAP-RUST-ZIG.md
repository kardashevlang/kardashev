# kardashev Gen 2 — Roadmap (Rust implementation, Zig philosophy)

> **A complete change of direction.** Generation 1 (v0.1.0 – v0.110.0) was a
> C++/LLVM compiler for a Rust-flavoured language with an affine borrow checker
> and effect system, built with Bazel. It shipped 110 roadmap versions and is
> preserved in git history and GitHub releases.
>
> **Generation 2 is a ground-up reset.** The compiler is reimplemented in
> **Rust** (every implementation file is `.rs`, zero external crates), and the
> language is **redesigned around Zig's philosophy**: no hidden control flow,
> no hidden allocations, `comptime` instead of macros, explicit `defer`,
> first-class tests, and a single self-contained `kard` toolchain whose build
> system is written in the language itself.

The semantics of each version are specified in `SPEC.md`; this file is the plan.

> **Status (v0.179.0):** Arc 1 (v0.112–v0.123), Arc 2 (v0.124–v0.130), Arc 3
> (v0.131–v0.140) and Arc 4 (v0.141–v0.150) are ✅ complete, as are the
> numbered Arc-5 versions v0.151–v0.158. The self-hosting arc is underway:
> stages shipped as v0.159–v0.179 have the self-hosted emitter reproducing
> the Rust emitter's C byte-for-byte on 365/384 of the conformance corpus
> (Program/Test mode). Details per version in [CHANGELOG.md](CHANGELOG.md).

## Generation policy

Pre-1.0, each completed roadmap version is a MINOR bump. The Gen-2 reboot opens
at **v0.111.0** (continuing the repo's tag line to avoid collisions while
clearly marking the reset). 1.0 remains reserved for a language-surface
stability commitment.

---

## ✅ v0.111.0 — Reboot: the procedural core + self-contained toolchain (THIS VERSION)

The foundation everything else builds on.

**Language**
- Functions (`pub fn name(p: T) R { … }`, Zig-style return type), recursion.
- Fixed-width integer types (`i8…u64`, `usize`), `bool`, `void`.
- `var` / `const` locals and top-level `const` (comptime-evaluated).
- Arithmetic / comparison / logical operators with no overloading.
- `if`/`else`, `while` (incl. `while (c) : (cont)`), `break`, `continue`,
  `return`.
- **`defer`** with correct LIFO flushing across fall-through, `return`,
  `break` and `continue` — a Zig signature feature.
- **`comptime`** expression folding + a const evaluator.
- **Built-in `test "name" { … }`** blocks with the `expect` builtin.
- `print` builtin for integer output (minimal runtime, explicit).

**Toolchain (single `kard` binary)**
- `kard build/run/test/fmt/init/version/help`.
- C backend: `lex → parse → sema → emit C → cc → native binary`.
- `build.ks` minimal declarative form; `kard init` scaffolding.
- Diagnostics with filename, line/column and a source caret.

**Engineering**
- Rust workspace, zero dependencies, `cargo test` (unit + compile-and-run e2e).
- CI on Ubuntu + macOS via cargo (replacing Bazel/LLVM).

---

## Planned

### v0.112.0 — Aggregates: `struct` (data) ✅
`const Name = struct { … };` declarations, struct literals (`Name{ .f = e }`),
field access (`a.b.c`), field assignment, struct-valued params/returns/locals,
nested structs. Emitted as C structs (by value). Methods / associated functions
are split out to v0.113 to keep each version complete and well-tested.

### v0.113.0 — Struct methods + associated functions ✅
Functions declared in a `struct` block; `Type.func(…)` and the `instance.method(…)`
call sugar (self-prepend), chained calls. Lowered to `kd_<Struct>_<method>(self, …)`.

### v0.114.0 — Optionals: `?T`, `null`, `orelse`, `.?` ✅
Null-safety the Zig way; lowered as a tagged `{ bool has; T val; }` value in C
with `T → ?T` coercion. `if (x) |v|` payload capture is deferred to a later
increment.

### v0.115.0 — Error unions: `!T`, `error.X`, `try`, `catch` ✅
Errors as values (implicit global error set), explicit propagation. Lowered as
`{ i32 err; T val; }` in C. Deferred: `errdefer`, `catch |e|` capture, named
error sets, `try` in nested expression positions.

### v0.116.0 — Enums & exhaustive `switch` ✅
Plain `enum` + `switch` with exhaustiveness checking (every variant or `else`;
`else` required for integers) — no hidden fall-through. Lowered to C `enum` +
`switch`. Tagged unions (`union(enum)`) + payload capture are a later item.

The original "arrays + slices + pointers + Allocator" version is split into
focused, fully-tested releases (quality over breadth):

### v0.117.0 — Fixed-size arrays `[N]T` ✅
Array types, array literals `[N]T{ … }`, indexing `a[i]` (read + write,
runtime-bounds-checked, panic on OOB), `a.len`, value semantics. Lowered to a
by-value C struct wrapper with a bounds-checked accessor.

### v0.118.0 — Pointers `*T` & slices `[]T` ✅
`&place`, `p.*`, `p.* = e`; slices as `{ ptr, len }` views with `a[i]` / `.len`
and slicing `a[lo..hi]`, bounds-checked. Raw (no lifetime checking).

### v0.119.0 — The **Allocator** interface + heap ✅
The explicit `Allocator` value (`c_allocator()`) + `alloc(a, T, n) -> []T` /
`free(a, s)` builtins; heap allocation takes an allocator parameter — no global
allocator. (Error-returning alloc, custom allocators, comptime-generic alloc:
later.)

### v0.120.0 — `comptime` generics (generic functions) ✅
Generic functions `fn f(comptime T: type, …)`, **monomorphised** (one C function
per concrete type argument), with transitive instantiation and type-parameter
forwarding. Generic structs / type-returning functions (`fn List(comptime T:
type) type`) and comptime *value* params are a later item.

### v0.121.0 — Type inference for `var`/`const` ✅
The `: T` annotation on a binding is optional — inferred from the initializer
(local `var`/`const` and top-level `const`). Inferred types are concrete (no
implicit conversions). A standard prelude/std is a later item.

### v0.122.0 — The build graph (`build.ks`) ✅
A `build.ks` describing a graph of one or more named executable targets
(`exe "name" { root = ".."; }`), with CLI target selection (`kard build/run/test
[TARGET]`; `build` with no name builds all). The full imperative `build.zig`
model (a kardashev program with a `build(*Builder)` entry point, step
dependencies and install artifacts) remains a future item.

### v0.123.0 — Cross-compilation (the mechanism) ✅
`kard build -target <triple>` (via clang `--target=`), `-c`/`--emit obj` object
output, and `kard targets`. The host triple builds + runs out of the box.
**Honest limitation:** because the runtime uses libc, foreign-target builds need
that target's C headers/sysroot installed — **bundling cross sysroots** (Zig's
"cross-compile anything out of the box") is the headline remaining work, now
tracked under *Beyond*. The compiler-side mechanism is complete.

---

🏁 **Arc 1 — the numbered roadmap v0.112 – v0.123 — is complete.**

## Arc 2 — completing the language surface (v0.124 – v0.130)

Promotes the tractable, high-value items out of *Beyond* into numbered versions
that finish the Zig-philosophy language surface. Same discipline; XL platform
items stay in *Beyond*.

### v0.124.0 — Tagged unions `union(enum)` + `switch` capture ✅
`const Shape = union(enum) { circle: i64, rect: Point };`; `switch` arms capture
the payload `.circle => |r| …`. Lowered to a tagged C struct
`{ int32_t tag; union { … } data; }`. Builds on the enum/switch machinery.

### v0.125.0 — Payload captures: `if (opt) |v|` + `errdefer` ✅
`if (x) |v| { … } else { … }` unwraps an optional, binding the value; `errdefer`
joins the LIFO flush but runs only on error-return edges (`try` propagation /
`return error.X`). `catch |e|` (the capturing error handler) is deferred to a
later version — the non-capturing `expr catch default` (§12) remains.

### v0.126.0 — Multi-file modules (`@import`) ✅
`@import("util.ks");` (a top-level import) — the compiler resolves, lexes,
parses and **flattens** the transitively-imported files into one program
(relative paths, dedup, cycle detection `E0292`, global-unique names `E0293`).
v0.126 is `#include`-style: bare-name access, `pub` not yet enforced across
modules, no `m.member` qualified access (all deferred to a later namespacing
pass).

(Reordered by tractability / risk; the XL generic-structs work sits late.)

### v0.127.0 — Strings: `[]u8` literals as values ✅
String literals evaluate to `[]u8` slices (over static bytes), with length /
indexing / sub-slicing (via the slice machinery) and `print` for strings.
Reuses slices, so no new type.

### v0.128.0 — `comptime` value params ✅
`comptime n: usize` parameters — array-size generics (`[n]T`) + comptime values,
monomorphised per concrete value, extending v0.120. Array sizes are now
`ArraySize::{Lit, Param}`; instantiations key on `ComptimeArg::{Type, Value}`.

### v0.129.0 — Generic structs / type-returning functions ✅
`fn Pair(comptime T: type) type { return struct { … }; }` — type-constructors,
monomorphised; used via a type-alias `const IP = Pair(i32);` (memoised). Unlocks
generic containers. (Single type param, fields-only struct in v0.129; multiple
params / methods / direct `Name(T)` in type position are later work.)

### v0.130.0 — Generic-struct methods + `ArrayList(T)` ✅
A type-constructor's `struct` may declare **methods** (using `Self` + the type
parameter), monomorphised per instantiation. On that, `ArrayList(T)` — a growable
list on the `Allocator` (`init`/`append`/`get`/`len`/`deinit`, grows by
alloc+copy+free) — ships as `examples/arraylist.ks`, the first allocator-based
std container. **This completes the numbered roadmap v0.112–v0.130.** (Value-
semantics `self`; pointer receivers / multiple type params are later work.)

## Arc 3 (v0.131–v0.140) — toward 1.0: ergonomics, mutation, richer generics

With the language surface complete (Arc 1 + Arc 2), Arc 3 rounds it out toward a
practical 1.0: imperative ergonomics, real in-place mutation, multi-parameter
generics, comptime reflection, named error sets, and a second std container.
Ordered by tractability so momentum stays high; each ships via the standard
cadence (SPEC+contract → workflow → integrate → test → PR → CI both → release).

### v0.131.0 — Compound assignment operators ✅
`+= -= *= /= %=` on any assignable place (`x`, `s.f`, `a[i]`) — `place = place op
rhs`, evaluating the place once (an index compound reads `i` once). `Stmt::Assign`
/ `Stmt::FieldAssign` carry `op: Option<BinOp>`.

### v0.132.0 — Bitwise & shift operators ✅
`& | ^ << >> ~` on integers, C-like precedence; const-folded. Binary `&`/`|`
disambiguate from address-of / capture by position (infix vs prefix / capture
context). (Bitwise compound assignments `|= &= ^= <<= >>=` are later work.)

### v0.133.0 — `for` loops over arrays & slices ✅
`for (xs) |x| { … }` and `for (xs, 0..) |x, i| { … }` — element (and 0-based
`usize` index) capture, lowered to an indexed `while` (so `break`/`continue`
behave, and `continue` still advances the index). Works for `[]T` and `[N]T`.

### v0.134.0 — Pointer-receiver methods (true mutation) ✅
`fn push(self: *Self, …) …` (or `self: *Point`) with auto-ref at the call site
(`list.push(x)` passes `&list`) and auto-deref field access (`self.field`), so
methods mutate the receiver in place — no value-return dance. Field read/assign
on any `*Struct` writes through the pointer. (No contract change — sema + emit.)

### v0.135.0 — Multiple type parameters ✅
Type-constructors with more than one `comptime T: type` (`fn Pair(comptime A:
type, comptime B: type) type`), monomorphised on the tuple of arguments
(argument order matters; single-param unchanged). Generic *functions* already
supported N comptime params (v0.120/v0.128). `StructInstance.args: Vec<Type>`.

### v0.136.0 — comptime reflection builtins ✅
`@sizeOf(T)` → `usize` (C `sizeof`), `@typeName(T)` → `[]u8` (subst-aware, so
both work on a generic type parameter), and `@This()` → the enclosing struct
type (desugars to `Self`, now bound in plain struct methods too). `Expr::Builtin`.

(Tail reordered: implementing `HashMap` revealed there are no integer casts —
`h = key` mixing `i32`/`usize` fails — which blocks real mixed-integer code, so
casts come first and unblock the map.)

### v0.137.0 — Integer casts `@as(T, e)` ✅
A comptime builtin (extends §32's `Expr::Builtin`) that casts an integer value
to another integer type — `@as(usize, key)` — lowering to a C cast `((T)(e))`.
Unblocks mixed-integer code and `HashMap`.

### v0.138.0 — `HashMap(V)` std container ✅
A real open-addressing hash map on the `Allocator` (`put`/`get`/`has`/`remove`/
`len`, grow-and-rehash, tombstones), written in the language itself
(`examples/hashmap.ks`). Implementing it lifted two generic-struct-method
limitations from v0.130: a method may now **reference top-level `const`s and free
functions** (method bodies are checked after Pass 2) and **call `Self.assoc(…)`**
associated constructors.

### v0.139.0 — Named error sets ✅
`const FileErr = error{ NotFound, Denied };`, `FileErr!T`, and **membership
checking** (`return error.X` must be in the set, `E0330`) — named error sets
alongside the implicit global `!T`. `TypeExpr.error_set`; `Item::ErrorSet`. At
runtime `Set!T` ≡ `!T` (the set is a compile-time constraint), so `try`/`catch`
are unchanged.

### v0.140.0 — Doc comments + `kard doc` ✅
`/// …` doc comments (an ignored `//` comment to the compiler) and **`kard doc
FILE`**, which renders a file's `pub` items + their preceding `///` lines as
Markdown (signatures from the AST, doc text associated by source position) — the
DX capstone of Arc 3. **This completes Arc 3 (v0.131–v0.140).**

## Arc 4 (v0.141–v0.150) — toward a practical 1.0: safety, floats, std

With the language surface and two containers in place (Arcs 1–3), Arc 4 adds the
runtime-safety builtins, floating point, richer error/enum ergonomics, and a
real importable standard library — the pieces a 1.0 needs to write everyday
programs. Ordered by tractability; each ships via the standard cadence.

### v0.141.0 — `@panic` + `unreachable` ✅
`@panic(msg: []u8)` (write `msg` to stderr, exit 101) and `unreachable` (trap on
a path the programmer asserts is impossible) — runtime-safety primitives that
**diverge** and adopt the expected type, so they stand in any value position
(e.g. a total `switch`'s `else` arm). `Expr::Unreachable`; `@panic` via
`Expr::Builtin`; `_Noreturn` C helpers.

### v0.142.0 — `catch |e|` capture ✅
The capturing error handler `expr catch |e| default` (deferred from v0.125):
binds the error **code** (`i32`) to `e` and evaluates `default` only on the error
path, lowered by hoisting like `try`. `Expr::Catch.capture`; the non-capturing
`expr catch default` is unchanged.

### v0.143.0 — Enum explicit values + conversions ✅
`enum { A = 1, B, C = 10 }` (a value-less variant auto-increments), `@intFromEnum(e)`
→ `i64` and `@enumFromInt(E, n)` → `E` — stable integer representations and
round-trips. The C enum carries the values, so literals/switch stay value-based.

### v0.144.0 — Floating point `f64` ✅
A 64-bit float type: literals (`3.14`), arithmetic `+ - * /` / comparison, `@as`
to and from integers, `print`, and `[]f64`/`[N]f64` arrays & slices. The first
non-integer scalar. `Type::F64`, `Expr::Float`, `TokenKind::Float`. (No implicit
int↔float mixing; float `const`s are deferred — floats are runtime-only.)

### v0.145.0 — Importable `std` library ✅
`@import("std");` resolves to the standard library **embedded in the compiler**
(`include_str!("std.ks")`) — `ArrayList(T)`, `HashMap(V)`, and `imin`/`imax`/
`iabs` — flattened into the program by bare name. Programs reuse the containers
instead of copying them.

### v0.146.0 — `switch` ranges + multi-label arms ✅
Inclusive integer-range arms `lo..hi =>` (`SwitchArm.ranges`, lowered to GNU C
case-ranges), combinable with value labels in one arm. Multi-label arms
(`1, 2, 3 =>`, `.A, .B =>`) already worked (labels are a `Vec`), so this added
the range form.

### v0.147.0 — Labeled `break` / `continue` ✅
Loops may carry a label (`outer: while (…) { … }`) and `break :outer` /
`continue :outer` target an enclosing loop, lowered with C `goto` (defers flush
to the targeted loop). Value-yielding block expressions (`blk: { break :blk v;
}`) are a larger AST change, deferred.

### v0.148.0 — stdin / file I/O ✅
`@readFile(a, path)` reads a whole file into a `[]u8` and `@readLine(a)` reads one
stdin line — minimal I/O on the `Allocator`, allocating the result. An open/read
error yields an empty slice (no `![]u8` to express it). `@`-builtins + `kd_read_*`
C helpers (emitted only when used).

### v0.149.0 — String utilities ✅
`str_eq`, `str_starts_with`, `str_index_of`, `str_concat` over `[]u8` (on the
`Allocator`), added to the embedded `std` — pure library, written in the
language, no compiler change.

### v0.150.0 — Test filtering + `kard bench` ✅
`kard test [FILE] --filter SUBSTR` runs only tests whose name contains `SUBSTR`,
and `kard bench [FILE]` runs the harness with **per-test wall-clock timing**
(`<name>: <ms> ms`). The harness `main` parses argv (`--filter`/`--bench`) via a
name+fn table. **This completes Arc 4 (v0.141–v0.150).**

### v0.151.0 — Optimization sweep ✅
Behaviour-preserving codebase optimization (generated C byte-identical):
`kard run`/`test` build dev binaries at `-O0` (`--release` restores `-O2`;
build/bench/cross stay `-O2`), plus internal dedup across all stages
(~−600 lines). No language change.

### v0.152.0 — Direct generic-type application `Name(T)` ✅
The v0.129 alias requirement falls: `var l: ArrayList(i32) = ArrayList(i32)
.init(a);` works directly — in every type position (composing with `?`/`!`/
`*`/`[]`/`[N]`), as an associated-call receiver, nested
(`ArrayList(ArrayList(i32))`), and under generic substitution (`ArrayList(T)`
inside another type-constructor — generic composition). An application and an
alias of the same `(ctor, args)` share one memoised struct (SPEC §42). Still
deferred: the literal form `Name(T){…}`, composite arguments
(`ArrayList([]u8)`), applications as generic-fn type arguments, and
application-typed fields in plain structs (Pass-0b ordering).

## Arc 5 — scale: conformance, std breadth, self-hosting (v0.153–…)

The 12th-goal arc: grow the project to **300k meaningful LOC** — never filler;
every line pins behaviour, implements a real library, or moves self-hosting —
while keeping the toolchain fast (optimize/efficiency first where growth would
otherwise tax every user).

### v0.153.0 — Dead-function elimination (reachability-based emit) ✅
`@import("std")` used to emit *every* std function into *every* program's C
(`str_concat` in hello-world). Before std grew, the emitter gained a
reachability pass: only functions reachable from the mode's roots (`main` for
programs, the `test` blocks for the harness) over the call graph — free fns,
struct methods, associated fns; generic instantiations are already
demand-driven — are emitted (SPEC §43). Hello-world with `@import("std")`
dropped from 108 to 34 lines of generated C.

### v0.154.0 — std wave 1: algorithms & data structures ✅
In-language, each with `test` blocks: generic `sort` (quicksort + insertion
below 17), `reverse`, `binary_search`, `Deque(T)`, `BitSet`, `StrBuilder`,
integer parse/format (`parse_i64`, `fmt_i64`, `fmt_u64_hex`), `imin64`/
`imax64`/`clamp64`/`iabs64`/`sign`, `gcd`/`lcm`/`ipow`/`isqrt`,
`div_floor`/`mod_floor`, and a deterministic xorshift64* `Rng` + `shuffle`.
Pure library (no compiler change); std grew 246 → 1,136 in-language lines,
with the `tests/std/` in-language suites driven by `std_suite.rs`.

### v0.155.0 — Conformance suite A (runner + SPEC §1–§21) ✅
`tests/spec/` is born: **311 directive-driven conformance programs**
(`//SPEC:`, `//EXIT:`, `//OUT:`, `//STDIN:`, `//ERR:`) pinning exact behaviour
per SPEC section, with a parallel Rust runner (`spec_suite.rs`, std threads,
`-O0` dev builds from v0.151). The corpus immediately found **4 real bugs**
(all fixed) — including a clang-only zero-length-array lowering bug caught by
macOS CI.

### v0.156.0 — Conformance suite B (SPEC §22–§42 + interaction matrix) ✅
The second half: the corpus grew to **606 programs** across 25 section
directories plus two feature-interaction matrices (optionals×generics,
defer×error-unions, switch×enums×ranges, …) — where compilers actually break.
**5 more real bugs** found and fixed (9 total across the two waves), and
`!void` error unions gained real support along the way. Verified under gcc
**and** clang.

### v0.157.0 — std wave 2: formats & text ✅
JSON (arena-style parse + minified emit), base64 + hex codecs, crc32 (one-shot
+ streaming) / fnv1a32/64 / adler32 / djb2, `split`/`trim`/`join`/`replace`
splitters, glob matching, and `parse_f64`/`fmt_f64`/`parse_u64`/`fmt_u64`/
`fmt_i64_pad` + ASCII case utils. In-language with tests; std grew 1,136 →
3,092 lines, all pay-as-you-go under DCE. Also locked cross-platform float
determinism (`-ffp-contract=off`, SPEC §38.x).

### v0.158.0 — `@writeFile` / `@appendFile` / `@argc` / `@arg` (self-host prerequisites) ✅
Output and argv access — the minimal OS surface a self-hosted compiler needs
(SPEC §44). Shipped as the indexed accessor pair `@argc()` / `@arg(a, i)`
(rather than a single `@args`) because `[][]u8` stays inexpressible (§15.2).

### v0.159.0 – v0.179.0 — Self-hosting stages ✅ (arc in progress)
The compiler is being rewritten in kardashev itself, under `selfhost/`, one
differentially-tested stage per release. Every mirror is compared against the
Rust implementation **over the whole repo corpus** (700+ source files) — the
lexer byte-for-byte on token dumps, the parser on AST dumps, the emitter on
the generated C itself — so a mirror can never silently drift. Files outside
the mirrored subset are explicitly detected and verdict-pinned as SKIPs,
never silently diverged. Along the way the differential harness has caught
**real bugs in the Rust compiler** (11 by v0.178).

| Version | Stage | Milestone |
|---------|------:|-----------|
| v0.159.0 | 1 | `selfhost/lexer.ks` — the full lexer (73 token kinds), byte-identical token dumps over every repo source |
| v0.160.0 | 2 | `selfhost/parser.ks` + `ast.ks` — the parser, AST-dump differential vs the Rust parser |
| v0.161.0 | 3 | `selfhost/emit.ks` — the C emitter opens (scalar subset, byte-identical C) |
| v0.162.0 – v0.166.0 | 4–8 | emitter growth: strings; index writes + allocator builtins; generalized `[]T` + `@as` casts; slicing `s[lo..hi]`; `test` blocks (Test mode) |
| v0.167.0 – v0.172.0 | 9–14 | `@import` resolution (`modres.ks`); fixed arrays `[N]T` + `for`; plain structs; struct methods + associated fns; enums; `switch` + contextual enum literals |
| v0.173.0 – v0.177.0 | 15–19 | optionals `?T`; error unions `!T`; pointers `*T`; labeled loops; `f64` (a full correctly-rounded, shortest-round-trip float-formatting mirror) |
| v0.178.0 – v0.179.0 | 21–22 | generic functions (comptime type + value params, monomorphisation); generic structs (type-constructors, aliases, direct application, instance methods) |

(Stage labels as recorded in the changelog; the sequence has no stage 20.)

As of v0.179.0 the self-hosted emitter reproduces the Rust emitter's C
byte-for-byte on **365/384 of the conformance corpus** (Program/Test mode),
including the `ArrayList`/`HashMap` examples. **Remaining** for a full
mirror: the sema mirror (today sema-invalid inputs are pinned by diagnostic
code), the last emitter subset corners, and the driver — then the compiler
compiling itself. LSP and the package registry follow.

### Beyond (Arc 5+, each multi-session)
Bundled cross-compilation sysroots; the full imperative `build.ks` graph (a
`build(*Builder)` entry point); completing re-self-hosting (in progress —
the stages above); a package registry; an LSP; and a mechanized spec → 1.0
stability commitment.

---

## Working discipline (carried from Gen 1)

Per version: research live behaviour → real, tested implementation → honest
deferrals (never silent stubs) → PR → CI green on **both** Ubuntu and macOS →
merge → tag + GitHub release. Direct pushes to `main` are blocked; work on a
branch.
