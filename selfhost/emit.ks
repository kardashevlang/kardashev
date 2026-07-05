// emit.ks — self-host stages 3–16 (v0.161–v0.174): a C emitter for the
// SCALAR + STRING + HEAP-BUFFER SUBSET (with generalized `[]T` slices,
// `@as` casts, the `s[lo..hi]` slicing view, `test` blocks, fixed arrays
// `[N]T` with array literals and `for` loops, plain data STRUCTS, struct
// METHODS + associated functions, ENUMS, `switch` with contextual `.V`
// literals, OPTIONALS `?T`, ERROR UNIONS `!T`, POINTERS `*T`, and —
// v0.176 — LABELED LOOPS), written in
// kardashev, mirroring `crates/kardc/src/emit_c.rs` decision for decision
// so that — for every subset program — the emitted C is BYTE-IDENTICAL to
// the Rust emitter's output in BOTH `EmitMode::Program` and (v0.166)
// `EmitMode::Test`: the harness of `static int kd_test_<idx>(void)`
// functions, the name/function-pointer tables, the v0.150
// `--filter`/`--bench` driver `main`, the statement-level `expect`
// lowering, and Test-mode liveness (rooted at the test bodies; EVERY
// function live when there are none).
//
// The subset (the "growing subset" of ROADMAP v0.159.0+; v0.161 shipped the
// scalar slice, v0.162 added strings, v0.163 index writes + the allocator
// builtins, v0.164 generalized slices to `[]T` and added `@as`, v0.165 the
// slicing view `s[lo..hi]`, v0.166 `test` blocks + the Test mode, v0.167
// `@import` resolution (in `modres.ks`), v0.168 fixed arrays `[N]T` with
// literal sizes, array literals `[N]T{ … }`, and unlabeled `for`, v0.169
// adds plain data structs — `const Name = struct { f: T, … };`
// declarations, nominal struct types anywhere a type may appear (params,
// returns, annotations, array/slice elements), literals
// `Name{ .f = e, … }`, field reads, and place-assignment CHAINS through
// fields and indexes with the `_at` element-pointer lowering; v0.170
// adds struct METHODS and associated functions — VALUE receivers only
// (`self: Name`; a pointer receiver `self: *T` stays a `type-form`
// skip), each lowering to a free C function `kd_<Struct>_<method>` whose
// `self` is an ordinary by-value parameter; calls in all three forms
// (`v.m(args)`, the explicit-self `Type.m(v, args)`, and the associated
// `Type.f(args)` — the first two identical in C); liveness gains the
// NAME-LEVEL method set (SPEC §43.1: a live method name marks that
// method on EVERY struct; deliberately receiver-agnostic); the intern
// replay gains sema's pass 1b (struct-function signatures — after all
// fn signatures, before const annotations; a `self` receiver's
// annotation is NEVER resolved and interns nothing) and pass 3 walks
// method bodies in the same item loop as fn/test bodies, with `self`
// bound to the ENCLOSING STRUCT regardless of its written annotation;
// v0.171 adds ENUMS — declarations with explicit values and the C
// auto-increment rule (counter = used + 1, wrapping; duplicates advance
// the counter but record nothing), registered in sema's pass 0 BEFORE
// structs and seeded FIRST in the typedef dependency walk
// (`typedef enum { kd_enum_<N>_<V> = <val>, … } kd_enum_<N>;`, every
// enumerator's value explicit); qualified literals `Enum.V` reuse the
// FIELD shape and lower to the C enumerator (checked before the `.len`
// arms, exactly like Rust); enum equality is plain C `==`/`!=`; the
// conversions `@intFromEnum(e)` → `((int64_t)(<e>))` and
// `@enumFromInt(E, n)` → `((kd_enum_E)(<n>))` join `@as` in the builtin
// arm (the type argument never walks). Enum names join struct names as
// nominal types anywhere a type may appear, including `[N]Enum` /
// `[]Enum` (mangle `enum_<N>`); enum-typed STRUCT FIELDS are
// sema-invalid (E0161 — resolve_field_type has no enum arm), a pinned
// language limit. v0.172 adds `switch` — enum/integer scrutinees,
// multi-label arms (`case a:` chains, the LAST label opening the arm's
// brace), GNU `case lo ... hi:` ranges, `else` → `default:`, every arm
// closed `} break;` (no fallthrough), divergence = (else present OR
// enum scrutinee) AND every arm diverges; payload-capture arms (tagged
// unions) stay out (`capture`) — and CONTEXTUAL `.V` literals through
// the expected-type plumbing: `emit_coerced` maps an `.V` against an
// expected enum to its enumerator at let/assign/place-assign/return/
// call-arg/method-arg/struct-field/array-element positions (fn and
// method parameter types are recorded in a flat positional table);
// sema supplies NO context in comparisons — `x == .V` is E0215 both
// ways (the emitter's sibling-context arm is defensive only) — and a
// bare `.V` in a switch LABEL takes the SCRUTINEE's enum. The scan
// mirrors check_switch: scrutinee first; enum-scrutinee labels that
// are `.V`/matching `Enum.V` never check as expressions, every other
// label checks fully (an INTEGER label checks + const-folds); an
// unswitchable scrutinee checks bodies only; arm bodies per arm, else
// last. v0.174 adds ERROR UNIONS `!T` (and named sets `Set!T` — the set
// name is sema's E0330 membership concern; the runtime type is the
// payload's union either way): the GLOBAL 1-based error-code table
// replays sema exactly — error-set members intern in pass 0 (after
// enums, before struct names), then `error.X` literals in BODY-CHECK
// order (the scan's ND_ERRLIT arm); `kd_err_<mangle>` typedefs
// `{ int32_t err; T val; }` + `_catch` (the `!void` variant keeps only
// `err` and SKIPS the helper) seed between optionals and arrays;
// `error.X`/`T` widen through `emit_coerced` (`{ .err = code }` /
// `{ .err = 0, .val = e }`; a `!void` target evaluates the void source
// in a comma-expr); `try` hoists `__kd_try{N}`, early-returns the
// re-wrapped error after an ERRDEFER-INCLUSIVE flush_all, and yields
// `.val` (`((void)0)` for `!void`) — statement positions: let-init
// (payload re-coerced via coerce_str), `return try e;` (NOT an error
// edge), bare `try e;` → `(void)(…);`; `catch` lowers eager through
// `_catch`, capturing through `__kd_eu{N}`/`__kd_catch{N}` with
// `int32_t kd_<e>` bound lazily, and `!void` operands ALWAYS as lazy
// statements; `errdefer` registers error-tagged defers that only
// error edges flush (`return error.X` and try-propagation; plain
// returns/breaks skip them); a `fn … !void` falling off its end
// returns success — at COLUMN 0, the Rust indent quirk, mirrored.
// v0.175 adds POINTERS `*T` (bare pointees only): NO typedef — the C
// spelling is structural `<pointee cty>*`, so pointer ids never reach
// the output and there is NO intern-order concern; the WRITTEN-`*T`
// PRE-PASS registry (fn/method signatures, local/const annotations,
// test bodies — mirroring collect_ptr_types' walk exactly, struct
// FIELDS excluded) backs `resolve_ty` (miss → the index-0 fallback)
// and `type_of(&place)` (miss → UNTYPEABLE — the load-bearing mirror:
// an unregistered `&x` infers to the i64 fallback exactly like Rust);
// `&place` lowers `(&(<lvalue>))` (an index place IS its `_at`
// pointer), `p.*` reads `(*(<p>))` and writes `*(<p>) = (<e>);`
// (compound re-spells); field/method access through `*Struct`
// auto-derefs `(*(<base>)).kd_f`; pointer RECEIVERS take the
// auto-ref/deref matrix (value→&, element→`_at`, chain→&place,
// ptr→pass-through / value-method over ptr → deref). v0.176 adds
// LABELED LOOPS: a `lab: while/for` records its label on the loop
// scope; `break :L` flushes defers out to AND INCLUDING L's scope then
// `goto __kd_brk_L;` (the label sits past the loop's close — past the
// `for`'s OUTER block); `continue :L` flushes likewise then
// `goto __kd_cont_L;` — the cont-label precedes the continue-clause /
// index increment inside the loop tail, which for a LABELED loop is
// emitted even when the body diverged (a deeper goto still targets
// it); unlabeled break/continue are byte-identical to before:
// v0.173 adds OPTIONALS `?T` — over bare subset names only (a
// composite inner `?[]u8`/`??T` is a PARSE error): `kd_opt_<mangle>`
// typedefs + `_orelse`/`_unwrap` helpers seed between structs and
// arrays in the dependency walk (an optional-over-struct/enum visits
// its inner first; a `?T` struct FIELD pulls the optional above the
// struct); `null` and `T` widen through `emit_coerced`
// (`{ .has = false }` / `{ .has = true, .val = e }`, an
// already-optional value passing through); `orelse`/`x.?` lower via
// the helpers (non-optional operands keep the defensive `({e})`
// arms); `if (opt) |v|` hoists into `__kd_if{N}` (a NEW per-fn/test
// counter), tests `.has`, binds `<inner> kd_<v> = .val` in its own
// scope, never diverges. `?T` interns ONLY from written type forms
// (annotations, params, returns, const annotations, struct fields —
// first-intern order); the scan's IF-capture arm binds the payload
// type around the then-block, and orelse/unwrap walk lhs→rhs / inner:
//
//   - types: `i32`, `i64`, `bool`, `void`, `u8`, `usize`, `Allocator` bare
//     names, plus `[]T` slices AND `[N]T` fixed arrays (literal `N` only —
//     a comptime-parameter size stays out) over the five scalar element
//     types (no other `?`/`!`/`*`/`Name(..)` forms);
//   - items: top-level `fn` (non-generic), top-level `const`, and `test`
//     blocks (v0.166 — interned/checked in both modes, emitted only by the
//     Test harness);
//   - statements: `var`/`const` lets, (compound) name-assignment, the
//     (compound) INDEX WRITE `s[i] = e` / `s[i] op= e` — a place-assignment
//     whose place is a DIRECT index (a place whose chain merely passes
//     through an index, like `s[i].f` or `s[i][j]`, stays out; the base
//     may be a slice OR an array, the array bound checked against its
//     CONSTANT length) — `if`/`else if`/`else`, `while` with
//     continue-clause, unlabeled `for` over an array/slice value in both
//     capture forms (`|x|` and `, 0..) |x, i|`, lowered through the
//     `__kd_for{N}` snapshot temp + `__kd_fi{N}` counter with `continue`
//     stepping the counter first — see `emit_for`), unlabeled `break`/
//     `continue`, `defer`, `return`, bare blocks, expression statements;
//   - expressions: integer/bool/STRING literals, names, unary `-`/`!`/`~`,
//     the full binary ladder (arithmetic, comparison, `and`/`or`, bitwise,
//     shifts), free-function calls, `print` (integers and `[]u8` strings),
//     `expect`, `comptime` folds, `@as(T, e)` casts over the subset type
//     names, `.len` on a slice (runtime field) or array (folds to the
//     constant count), the read index `s[i]` (slice getter or array
//     `_get`), array literals `[N]T{ … }` (a C compound literal; the empty
//     one is `{0}`), the slicing view `s[lo..hi]` over slices AND arrays
//     (a `{ptr, len}` view with the bounds check folded into a `_Noreturn`
//     conditional — base/lo/hi re-spliced textually, exactly like the Rust
//     format string; an array base reads `.data` and bounds against the
//     constant length), and the allocator builtins `c_allocator()` /
//     `alloc(a, T, n)` / `free(a, s)`.
//
// v0.164's load-bearing piece: the typedef section carries one
// `kd_slice_<tag>` block per interned slice IN SEMA'S FIRST-INTERN ORDER —
// reproduced here by replaying sema's walk (see the intern-scan section).
// v0.168 doubles it: `kd_arr_<tag>_<N>` blocks (typedef + `_get` + `_at`,
// storage `max(N, 1)`) for every interned ARRAY come first — the Rust
// dependency walk visits arrays before slices — each table in its own
// first-intern order, and the interning replay is now TYPE-AWARE (a
// `for` elem binding or an array-base `s[lo..hi]` interns the element
// type at the point sema resolves it, so the scan carries scopes).
// v0.169 completes the walk: `kd_struct_<Name>` typedefs seed FIRST (id
// order), each node's dependencies — a struct's field types, an
// array's/slice's element — emit before it (post-order, seen-set), so a
// struct with an array field pulls that array's block above itself and
// an array OF structs pulls the struct. Struct declarations replay as
// sema's pass 0a/0b (names, then field types in declaration order —
// interning field slices/arrays BEFORE the signature pass).
//
// Everything else is OUT of the subset. `es_detect` walks the AST in a fixed
// depth-first order and reports the FIRST unsupported construct as a
// `(word, position)` pair; the differential driver prints it as
// `SKIP <word> <pos>` and the Rust test mirrors the same walk, so subset
// membership itself is differentially tested on every corpus file.
//
// Like the Rust emitter, this one works off the plain parsed AST — there is
// no sema here. Emission of a program that sema would REJECT is therefore
// unspecified-but-total: it must never crash or loop, but its output is only
// compared for programs the Rust pipeline validates (the differential test
// carries the explicit list of subset-shaped-but-invalid corpus files).
//
// Mirrored decisions (emit_c.rs / const_eval.rs):
//   - the fixed 10-line prelude + one blank line (`emit_prelude`);
//   - `static const` top-level consts, folded by a `const_eval` mirror in
//     source order (a failing fold SKIPS the const, exactly like the Rust
//     "skip rather than panic" arm), then one blank line if any were emitted;
//   - dead-function elimination (SPEC §43.1): a worklist transitive closure
//     of called names rooted at `main`; forward declarations and definitions
//     both consult the same liveness;
//   - declaration/definition formatting: `<cty> kd_<name>(<params>)`, empty
//     parameter lists spelled `void`, 4-space indentation, one blank line
//     after the forward-declaration block and after every definition;
//   - statement/expression lowering: fully parenthesized operators, `print`
//     → `kd_print((long long)(<e>))`, `expect` in value position →
//     `((void)0)`, compound assignment re-spelling the place on both sides;
//   - `defer` lowering (SPEC §4.4): a scope stack; fall-through flushes the
//     current scope in reverse registration order, `return` flushes all
//     scopes (hoisting a non-void value into `<cty> __kd_ret = (<e>);`
//     first), `break`/`continue` flush to the nearest loop-body scope, and
//     the `while` continue-clause runs after those defers and before the C
//     `continue;`;
//   - local type inference (`type_of_expr` mirror): int literal → `i64`,
//     bool → `bool`, string → `[]u8`, name → the scope stack, unary/binary
//     by operator shape, call → the collected return type, `s.len` →
//     `usize`, `s[i]` → `u8`; an un-inferable initializer falls back to
//     `i64` — including the Rust emitter's own quirks (a top-level const
//     referenced as an initializer infers `i64`, not its own type);
//   - the string machinery (v0.162, SPEC §23.2): the `kd_slice_uint8_t`
//     typedef + `_get`/`_at`/`_alloc` helpers are emitted exactly when the
//     module interns `[]u8` — i.e. writes a `[]u8` type or a string literal
//     anywhere (sema's interning triggers, mirrored by a whole-tree scan);
//     a string literal lowers to a compound literal over `c_string_literal`
//     bytes (escape `\` `"` and `\n`/`\t`/`\r`, hex-escape everything
//     outside printable ASCII, split the literal when a hex escape would
//     absorb a following hex digit); `print(s)` hoists the slice into a
//     fresh `__kd_str{N}` temporary (counter reset per function); `~`/`<<`
//     over a `u8` operand truncate back through `((uint8_t)...)` (§28.2);
//   - `int main(int argc, char **argv){ (void)argc;(void)argv; <wire> }`
//     where `<wire>` is `return (int) kd_main();` for an integer `main`,
//     else `kd_main(); return 0;`.
//
// Known, accepted divergence: the const-fold mirrors Rust's WRAPPING i64
// arithmetic with plain kardashev `i64` ops (plus explicit guards for the
// `i64::MIN / -1`, `i64::MIN % -1` and `-i64::MIN` traps and the shift-amount
// mask `& 63`). A `comptime` overflow therefore folds identically on every
// production target, but is formally implementation-defined here rather than
// two's-complement-guaranteed as in Rust.

@import("ast.ks");
@import("std");

// --- type codes ----------------------------------------------------------------
//
// The mirror of `types.rs::Type` restricted to the subset. `ET_NONE` mirrors
// a `None` from `Type::from_name` / `type_of_expr` (the "no type" outcome);
// it is distinct from `ET_VOID`, which is a real type.

pub const ET_VOID: i64 = 0;
pub const ET_I32: i64 = 1;
pub const ET_I64: i64 = 2;
pub const ET_BOOL: i64 = 3;
pub const ET_NONE: i64 = 4;
pub const ET_U8: i64 = 5;
pub const ET_USIZE: i64 = 6;
pub const ET_ALLOC: i64 = 8;

// Slice types are a code FAMILY (v0.164): `ET_SLICE_BASE + <elem code>`,
// one code per element type. `[]u8` keeps a named constant since the string
// machinery pins it specifically. Fixed arrays (v0.168) are a second
// family: `ET_ARR_BASE + <index into the emitter's interned-array table>`
// (an `(elem, len)` pair cannot pack into a flat code range). Structs
// (v0.169) are a third: `ET_STRUCT_BASE + <struct id>` (declaration order),
// and a slice OVER a struct element moves to its own disjoint band
// `ET_SLICE_STRUCT_BASE + <struct id>` so no arithmetic combination of the
// families can collide (scalar-elem slices keep their v0.164 codes).
pub const ET_SLICE_BASE: i64 = 100;
pub const ET_SLICE_U8: i64 = 105;
pub const ET_ARR_BASE: i64 = 10000;
pub const ET_STRUCT_BASE: i64 = 1000000000;
pub const ET_SLICE_STRUCT_BASE: i64 = 2000000000;
pub const ET_ENUM_BASE: i64 = 3000000000;
pub const ET_SLICE_ENUM_BASE: i64 = 4000000000;
pub const ET_OPT_BASE: i64 = 5000000000;
pub const ET_ERRU_BASE: i64 = 6000000000;
pub const ET_PTR_BASE: i64 = 7000000000;

pub fn et_slice_of(elem: i64) i64 {
    if (elem >= ET_ENUM_BASE) {
        return ET_SLICE_ENUM_BASE + (elem - ET_ENUM_BASE);
    }
    if (elem >= ET_STRUCT_BASE) {
        return ET_SLICE_STRUCT_BASE + (elem - ET_STRUCT_BASE);
    }
    return ET_SLICE_BASE + elem;
}

pub fn et_is_slice(t: i64) bool {
    return (t >= ET_SLICE_BASE and t < ET_ARR_BASE) or (t >= ET_SLICE_STRUCT_BASE and t < ET_ENUM_BASE) or (t >= ET_SLICE_ENUM_BASE and t < ET_OPT_BASE);
}

pub fn et_slice_elem(t: i64) i64 {
    if (t >= ET_SLICE_ENUM_BASE) {
        return ET_ENUM_BASE + (t - ET_SLICE_ENUM_BASE);
    }
    if (t >= ET_SLICE_STRUCT_BASE) {
        return ET_STRUCT_BASE + (t - ET_SLICE_STRUCT_BASE);
    }
    return t - ET_SLICE_BASE;
}

pub fn et_is_arr(t: i64) bool {
    return t >= ET_ARR_BASE and t < ET_STRUCT_BASE;
}

pub fn et_is_struct(t: i64) bool {
    return t >= ET_STRUCT_BASE and t < ET_SLICE_STRUCT_BASE;
}

pub fn et_is_enum(t: i64) bool {
    return t >= ET_ENUM_BASE and t < ET_SLICE_ENUM_BASE;
}

pub fn et_is_opt(t: i64) bool {
    return t >= ET_OPT_BASE and t < ET_ERRU_BASE;
}

pub fn et_is_erru(t: i64) bool {
    return t >= ET_ERRU_BASE and t < ET_PTR_BASE;
}

pub fn et_is_ptr(t: i64) bool {
    return t >= ET_PTR_BASE;
}

/// `StructTable::slice_c_name` over the subset: `kd_slice_<type_mangle(elem)>`
/// where a primitive's mangle is its C spelling. The `void`/`kd_allocator`
/// arms mirror the unreachable unresolved-element cases byte-for-byte.
pub fn et_slice_c_name(t: i64) []u8 {
    var e: i64 = et_slice_elem(t);
    if (e == ET_I32) { return "kd_slice_int32_t"; }
    if (e == ET_I64) { return "kd_slice_int64_t"; }
    if (e == ET_BOOL) { return "kd_slice_bool"; }
    if (e == ET_U8) { return "kd_slice_uint8_t"; }
    if (e == ET_USIZE) { return "kd_slice_uintptr_t"; }
    if (e == ET_ALLOC) { return "kd_slice_kd_allocator"; }
    return "kd_slice_void";
}

/// `Type::from_name` over the subset: the seven bare spellings map to their
/// codes, anything else is `ET_NONE` (the caller decides the fallback,
/// mirroring the two distinct Rust fallbacks: `resolve_ty` → void, `cty` →
/// `int64_t`). `[]u8` is not a name — `resolve_ty`/`cty` map the slice FORM.
pub fn et_from_name(name: []u8) i64 {
    if (str_eq(name, "i32")) { return ET_I32; }
    if (str_eq(name, "i64")) { return ET_I64; }
    if (str_eq(name, "bool")) { return ET_BOOL; }
    if (str_eq(name, "void")) { return ET_VOID; }
    if (str_eq(name, "u8")) { return ET_U8; }
    if (str_eq(name, "usize")) { return ET_USIZE; }
    if (str_eq(name, "Allocator")) { return ET_ALLOC; }
    return ET_NONE;
}

/// `Emitter::cty_of` over the subset: slices through `slice_c_name`,
/// primitives through `Type::c_name`. `ET_NONE` never reaches C spelling
/// through `et_c_name` in a detector-approved program; spell it `int64_t`
/// (the same defensive fallback the Rust `cty` uses for an unresolvable
/// name).
pub fn et_c_name(t: i64) []u8 {
    if (et_is_slice(t)) { return et_slice_c_name(t); }
    if (t == ET_I32) { return "int32_t"; }
    if (t == ET_I64) { return "int64_t"; }
    if (t == ET_BOOL) { return "bool"; }
    if (t == ET_VOID) { return "void"; }
    if (t == ET_U8) { return "uint8_t"; }
    if (t == ET_USIZE) { return "uintptr_t"; }
    if (t == ET_ALLOC) { return "kd_allocator"; }
    return "int64_t";
}

/// `Type::is_int` over the subset (`i32`/`i64`/`u8`/`usize`).
pub fn et_is_int(t: i64) bool {
    return t == ET_I32 or t == ET_I64 or t == ET_U8 or t == ET_USIZE;
}

/// Whether `t` is a subset slice ELEMENT type — the five scalars `[]T` and
/// `alloc(a, T, n)` range over (v0.164).
pub fn et_is_slice_elem(t: i64) bool {
    return t == ET_I32 or t == ET_I64 or t == ET_BOOL or t == ET_U8 or t == ET_USIZE;
}

/// `Emitter::promotes_in_c` over the subset: `u8` is the only sub-32-bit
/// integer here, so a `~`/`<<` over it must truncate back (§28.2).
pub fn et_promotes_in_c(t: i64) bool {
    return t == ET_U8;
}

// --- operator spellings ----------------------------------------------------------
//
// `BinOp::c_op` / `is_bool_result` and the unary spellings, keyed by the
// `OPC_*` / `UOP_*` codes of `ast.ks`.

pub fn es_c_op(op: i64) []u8 {
    if (op == OPC_ADD) { return "+"; }
    if (op == OPC_SUB) { return "-"; }
    if (op == OPC_MUL) { return "*"; }
    if (op == OPC_DIV) { return "/"; }
    if (op == OPC_REM) { return "%"; }
    if (op == OPC_EQ) { return "=="; }
    if (op == OPC_NE) { return "!="; }
    if (op == OPC_LT) { return "<"; }
    if (op == OPC_LE) { return "<="; }
    if (op == OPC_GT) { return ">"; }
    if (op == OPC_GE) { return ">="; }
    if (op == OPC_AND) { return "&&"; }
    if (op == OPC_OR) { return "||"; }
    if (op == OPC_BAND) { return "&"; }
    if (op == OPC_BOR) { return "|"; }
    if (op == OPC_BXOR) { return "^"; }
    if (op == OPC_SHL) { return "<<"; }
    return ">>";
}

pub fn es_is_bool_result(op: i64) bool {
    return (op >= OPC_EQ and op <= OPC_GE) or op == OPC_AND or op == OPC_OR;
}

/// `Emitter::place_chain_has_index`: whether a place expression reaches its
/// target THROUGH an index via value links (an `Index`, or a `Field` whose
/// base does). Decides which place-assignment arm a place takes: a direct
/// `s[i]` place (base index-free) uses the legacy hoisted-`__kd_idx` block;
/// anything else needs the `_at` lowering and stays out of the subset.
pub fn es_chain_has_index(nodes: []Node, n: i32) bool {
    if (n < 0) { return false; }
    var u: usize = @as(usize, n);
    if (nodes[u].kind == ND_INDEX) { return true; }
    if (nodes[u].kind == ND_FIELD) { return es_chain_has_index(nodes, nodes[u].a); }
    return false;
}

// --- subset detection ------------------------------------------------------------
//
// A fixed depth-first walk over the arena, recording the FIRST unsupported
// construct as a `(word, pos)` pair. The walk order is part of the contract:
// items in source order; per function, parameters (flag, then type), return
// type, body; per statement/expression, children in their `a`/`b`/`c` field
// order. `crates/kardc/tests/selfhost_emit.rs` mirrors this walk over the
// Rust AST word for word — the differential compares both the verdict and
// the position.

pub const Det = struct {
    src: []u8,
    nodes: []Node,
    found: bool,
    word: []u8,
    pos: usize,
    // The module's item-chain head (v0.169): a type reference anywhere may
    // name any DECLARED struct (sema pass 0a interns every name before
    // field/signature resolution; E0160 ordering is sema's rejection, not
    // subset membership), so the walk consults the item list by need.
    droot: i32,

    fn init(src: []u8, nodes: []Node, root: i32) Self {
        return Det{ .src = src, .nodes = nodes, .found = false, .word = "", .pos = 0, .droot = root };
    }

    /// Whether `name` names a struct OR an enum declared anywhere in the
    /// module (the named-type set for type positions; sema pass 0/0a
    /// interns every name before any resolution).
    fn is_struct_name(self: *Self, name: []u8) bool {
        var cur: i32 = self.droot;
        while (cur >= 0) {
            var u: usize = @as(usize, cur);
            if (self.nodes[u].kind == ND_STRUCT or self.nodes[u].kind == ND_ENUM) {
                if (str_eq(self.src[self.nodes[u].xoff .. self.nodes[u].xoff + self.nodes[u].xlen], name)) {
                    return true;
                }
            }
            cur = self.nodes[u].next;
        }
        return false;
    }

    /// Record the first finding; later ones are ignored.
    fn hit(self: *Self, word: []u8, pos: usize) void {
        if (self.found) { return; }
        self.found = true;
        self.word = word;
        self.pos = pos;
    }

    /// A type reference: composite forms other than a slice or a
    /// literal-length array are out; `[]T` (v0.164) and `[N]T` (v0.168)
    /// range over the five scalar element types; a bare base name must be
    /// a subset spelling. (`@This()` carries no source name; it reports
    /// `type-name` exactly like the Rust mirror, whose synthesized name
    /// `Self` is not a subset spelling. A `[n]T` comptime-parameter length
    /// is generics territory — `type-form`.)
    fn check_type(self: *Self, n: i32) void {
        if (self.found or n < 0) { return; }
        var u: usize = @as(usize, n);
        var fl: i64 = self.nodes[u].flags;
        var forms: i64 = F_ARRPARAM | F_APP | F_ESETTHIS;
        if ((fl & forms) != 0) {
            self.hit("type-form", self.nodes[u].off);
            return;
        }
        if ((fl & F_OPT) != 0) {
            // `?T` over a bare subset name (v0.173; a composite inner is a
            // PARSE error, so `?` never coexists with the other forms).
            var oname: []u8 = self.src[self.nodes[u].xoff .. self.nodes[u].xoff + self.nodes[u].xlen];
            if (et_from_name(oname) == ET_NONE and !self.is_struct_name(oname)) {
                self.hit("type-name", self.nodes[u].off);
            }
            return;
        }
        if ((fl & F_ERR) != 0) {
            // `!T` / `Set!T` over a bare subset payload name (v0.174; the
            // set name is sema's E0330 membership concern).
            var pname: []u8 = self.src[self.nodes[u].xoff .. self.nodes[u].xoff + self.nodes[u].xlen];
            if (et_from_name(pname) == ET_NONE and !self.is_struct_name(pname)) {
                self.hit("type-name", self.nodes[u].off);
            }
            return;
        }
        if ((fl & F_PTR) != 0) {
            // `*T` over a bare subset pointee name (v0.175; `*` never
            // combines with the other forms in the grammar).
            var ptn: []u8 = self.src[self.nodes[u].xoff .. self.nodes[u].xoff + self.nodes[u].xlen];
            if (et_from_name(ptn) == ET_NONE and !self.is_struct_name(ptn)) {
                self.hit("type-name", self.nodes[u].off);
            }
            return;
        }
        if ((fl & F_THIS) != 0) {
            self.hit("type-name", self.nodes[u].off);
            return;
        }
        var name: []u8 = self.src[self.nodes[u].xoff .. self.nodes[u].xoff + self.nodes[u].xlen];
        if ((fl & F_ARRLIT) != 0 or (fl & F_SLICE) != 0) {
            // `[N]T` / `[]T` over the five scalar element types, or a
            // declared struct element (v0.169).
            if (!et_is_slice_elem(et_from_name(name)) and !self.is_struct_name(name)) {
                self.hit("type-name", self.nodes[u].off);
            }
            return;
        }
        if (et_from_name(name) == ET_NONE and !self.is_struct_name(name)) {
            self.hit("type-name", self.nodes[u].off);
        }
    }

    /// Whether a place chain (FIELD/INDEX steps) bottoms out at a bare
    /// name — the only assignable root in the subset (sema: E0167/E0223
    /// otherwise, but a call/deref ROOT is out of the subset entirely).
    fn place_rooted_in_name(self: *Self, n: i32) bool {
        if (n < 0) { return false; }
        var u: usize = @as(usize, n);
        var k: u8 = self.nodes[u].kind;
        if (k == ND_IDENT) { return true; }
        // A deref step roots a place regardless of its inner expression
        // (sema checks it as an ordinary expr — v0.175).
        if (k == ND_DEREF) { return true; }
        if (k == ND_FIELD or k == ND_INDEX) {
            return self.place_rooted_in_name(self.nodes[u].a);
        }
        return false;
    }

    /// Walk a place chain: bases inward, each index expression where it
    /// sits (the root name itself carries nothing to check).
    fn check_place(self: *Self, n: i32) void {
        if (self.found or n < 0) { return; }
        var u: usize = @as(usize, n);
        var k: u8 = self.nodes[u].kind;
        if (k == ND_INDEX) {
            self.check_place(self.nodes[u].a);
            self.check_expr(self.nodes[u].b);
            return;
        }
        if (k == ND_FIELD) {
            self.check_place(self.nodes[u].a);
            return;
        }
        if (k == ND_DEREF) {
            self.check_expr(self.nodes[u].a);
            return;
        }
    }

    fn check_expr_list(self: *Self, head: i32) void {
        var cur: i32 = head;
        while (cur >= 0) {
            if (self.found) { return; }
            self.check_expr(cur);
            cur = self.nodes[@as(usize, cur)].next;
        }
    }

    fn check_expr(self: *Self, n: i32) void {
        if (self.found or n < 0) { return; }
        var u: usize = @as(usize, n);
        var k: u8 = self.nodes[u].kind;
        var off: usize = self.nodes[u].off;
        if (k == ND_INT or k == ND_BOOL or k == ND_IDENT) { return; }
        if (k == ND_UNARY) {
            self.check_expr(self.nodes[u].a);
            return;
        }
        if (k == ND_BIN) {
            self.check_expr(self.nodes[u].a);
            self.check_expr(self.nodes[u].b);
            return;
        }
        if (k == ND_CALL) {
            var callee: []u8 = self.src[self.nodes[u].xoff .. self.nodes[u].xoff + self.nodes[u].xlen];
            if (str_eq(callee, "alloc")) {
                // `alloc(a, T, n)` is in the subset (v0.163, elements
                // generalized in v0.164) — exactly three arguments with the
                // element type one of the five scalars; any other shape
                // (wrong arity, a non-scalar element) is out.
                var a0: i32 = self.nodes[u].a;
                var a1: i32 = 0 - 1;
                var a2: i32 = 0 - 1;
                var a3: i32 = 0 - 1;
                if (a0 >= 0) { a1 = self.nodes[@as(usize, a0)].next; }
                if (a1 >= 0) { a2 = self.nodes[@as(usize, a1)].next; }
                if (a2 >= 0) { a3 = self.nodes[@as(usize, a2)].next; }
                var shaped: bool = a2 >= 0 and a3 < 0;
                if (shaped) {
                    var eu: usize = @as(usize, a1);
                    if (self.nodes[eu].kind != ND_IDENT) { shaped = false; }
                    if (shaped) {
                        var ename: []u8 = self.src[self.nodes[eu].xoff .. self.nodes[eu].xoff + self.nodes[eu].xlen];
                        if (!et_is_slice_elem(et_from_name(ename))) { shaped = false; }
                    }
                }
                if (!shaped) {
                    self.hit("builtin-call", off);
                    return;
                }
                self.check_expr_list(self.nodes[u].a);
                return;
            }
            // `free(a, s)` and `c_allocator()` are in the subset (v0.163);
            // their arguments are ordinary subset expressions.
            self.check_expr_list(self.nodes[u].a);
            return;
        }
        if (k == ND_COMPTIME) {
            self.check_expr(self.nodes[u].a);
            return;
        }
        if (k == ND_STR) {
            // A string literal is in the subset (v0.162).
            return;
        }
        if (k == ND_FIELD) {
            // Field access is in the subset (v0.169: struct fields; the
            // `.len` special forms since v0.162) — the walk sees only the
            // base, the name resolves in sema/emission.
            self.check_expr(self.nodes[u].a);
            return;
        }
        if (k == ND_INDEX) {
            // A read index `s[i]` is in the subset (v0.162); index WRITES
            // are `ND_PASSIGN` places and stay out.
            self.check_expr(self.nodes[u].a);
            self.check_expr(self.nodes[u].b);
            return;
        }
        if (k == ND_BUILTIN) {
            // `@as(T, e)` is in the subset (v0.164): exactly two arguments,
            // the first an identifier naming a subset type; the VALUE
            // argument is walked. v0.171 adds `@intFromEnum(e)` (exactly
            // one argument, walked) and `@enumFromInt(E, n)` (exactly two,
            // the first an identifier — any name; a non-enum is sema's
            // E0321 — only the VALUE walks). Every other `@`-builtin
            // stays out.
            var bname: []u8 = self.src[self.nodes[u].xoff .. self.nodes[u].xoff + self.nodes[u].xlen];
            if (str_eq(bname, "intFromEnum")) {
                var i0: i32 = self.nodes[u].a;
                if (i0 >= 0 and self.nodes[@as(usize, i0)].next < 0) {
                    self.check_expr(i0);
                    return;
                }
            }
            if (str_eq(bname, "enumFromInt")) {
                var e0: i32 = self.nodes[u].a;
                var e1: i32 = 0 - 1;
                var e2: i32 = 0 - 1;
                if (e0 >= 0) { e1 = self.nodes[@as(usize, e0)].next; }
                if (e1 >= 0) { e2 = self.nodes[@as(usize, e1)].next; }
                if (e0 >= 0 and e1 >= 0 and e2 < 0 and self.nodes[@as(usize, e0)].kind == ND_IDENT) {
                    self.check_expr(e1);
                    return;
                }
            }
            if (str_eq(bname, "as")) {
                var b0: i32 = self.nodes[u].a;
                var b1: i32 = 0 - 1;
                var b2: i32 = 0 - 1;
                if (b0 >= 0) { b1 = self.nodes[@as(usize, b0)].next; }
                if (b1 >= 0) { b2 = self.nodes[@as(usize, b1)].next; }
                var shaped2: bool = b1 >= 0 and b2 < 0;
                if (shaped2 and self.nodes[@as(usize, b0)].kind != ND_IDENT) { shaped2 = false; }
                if (shaped2) {
                    var tname: []u8 = self.src[self.nodes[@as(usize, b0)].xoff .. self.nodes[@as(usize, b0)].xoff + self.nodes[@as(usize, b0)].xlen];
                    if (et_from_name(tname) == ET_NONE) { shaped2 = false; }
                }
                if (shaped2) {
                    self.check_expr(b1);
                    return;
                }
            }
            self.hit("builtin", off);
            return;
        }
        if (k == ND_FLOAT) { self.hit("float", off); return; }
        if (k == ND_SLIT) {
            // A struct literal `Name{ .f = e, … }` is in the subset
            // (v0.169): the initializer values walk in source order (the
            // name is sema's business — an unknown one is E0163).
            var fcur: i32 = self.nodes[u].a;
            while (fcur >= 0) {
                if (self.found) { return; }
                self.check_expr(self.nodes[@as(usize, fcur)].a);
                fcur = self.nodes[@as(usize, fcur)].next;
            }
            return;
        }
        if (k == ND_STRUCTTYPE) { self.hit("struct-type", off); return; }
        if (k == ND_MCALL) {
            // A method / associated call is in the subset (v0.170): the
            // receiver walks, then the arguments in order (name resolution
            // is sema's business — E0170/E0171/E0173 territory).
            self.check_expr(self.nodes[u].a);
            self.check_expr_list(self.nodes[u].b);
            return;
        }
        if (k == ND_NULL) {
            // `null` is in the subset (v0.173): its `?T` comes from the
            // expected-type context (no context is sema's E0180).
            return;
        }
        if (k == ND_ORELSE) {
            // `x orelse y` is in the subset (v0.173).
            self.check_expr(self.nodes[u].a);
            self.check_expr(self.nodes[u].b);
            return;
        }
        if (k == ND_UNWRAP) {
            // `x.?` is in the subset (v0.173).
            self.check_expr(self.nodes[u].a);
            return;
        }
        if (k == ND_ERRLIT) {
            // `error.X` is in the subset (v0.174): its `!T` comes from the
            // expected-type context (no context is sema's E0193).
            return;
        }
        if (k == ND_ENUMLIT) {
            // An unqualified `.V` is in the subset (v0.172): its enum
            // comes from the expected-type context (a no-context use is
            // sema's E0215); it carries nothing to walk.
            return;
        }
        if (k == ND_ALIT) {
            // An array literal `[N]T{ … }` is in the subset (v0.168): its
            // `[N]T` reference, then the elements, in order.
            self.check_type(self.nodes[u].a);
            self.check_expr_list(self.nodes[u].b);
            return;
        }
        if (k == ND_SLICEX) {
            // The slicing view `base[lo..hi]` is in the subset (v0.165);
            // base, lo and hi are ordinary subset expressions.
            self.check_expr(self.nodes[u].a);
            self.check_expr(self.nodes[u].b);
            self.check_expr(self.nodes[u].c);
            return;
        }
        if (k == ND_ADDROF) {
            // `&place` is in the subset (v0.175): the place walks (an
            // index place lowers through `_at`; non-places are sema's
            // E0231, const roots E0233).
            self.check_expr(self.nodes[u].a);
            return;
        }
        if (k == ND_DEREF) {
            // `p.*` is in the subset (v0.175).
            self.check_expr(self.nodes[u].a);
            return;
        }
        if (k == ND_TRY) {
            // `try e` is in the subset (v0.174); its statement-position
            // rule is sema's E0191.
            self.check_expr(self.nodes[u].a);
            return;
        }
        if (k == ND_CATCH) {
            // `e catch d` / `e catch |x| d` are in the subset (v0.174):
            // operand and default walk (the capture binds an i32).
            self.check_expr(self.nodes[u].a);
            self.check_expr(self.nodes[u].b);
            return;
        }
        if (k == ND_UNREACHABLE) { self.hit("unreachable", off); return; }
        // Any other kind in expression position is a walker bug; surface it
        // as a mismatch rather than silently accepting.
        self.hit("bad-expr", off);
    }

    fn check_block(self: *Self, n: i32) void {
        if (self.found or n < 0) { return; }
        var cur: i32 = self.nodes[@as(usize, n)].a;
        while (cur >= 0) {
            if (self.found) { return; }
            self.check_stmt(cur);
            cur = self.nodes[@as(usize, cur)].next;
        }
    }

    fn check_stmt(self: *Self, n: i32) void {
        if (self.found or n < 0) { return; }
        var u: usize = @as(usize, n);
        var k: u8 = self.nodes[u].kind;
        var fl: i64 = self.nodes[u].flags;
        var off: usize = self.nodes[u].off;
        if (k == ND_LET) {
            self.check_type(self.nodes[u].a);
            self.check_expr(self.nodes[u].b);
            return;
        }
        if (k == ND_ASSIGN) {
            self.check_expr(self.nodes[u].a);
            return;
        }
        if (k == ND_PASSIGN) {
            // A place-assignment over any FIELD/INDEX chain rooted at a
            // NAME is in the subset (v0.169; the v0.163 form was the
            // direct index write). The walk descends the chain — each
            // index expression where it sits — then the value; a place
            // rooted in anything else (a call, a deref, …) stays out.
            if (!self.place_rooted_in_name(self.nodes[u].a)) {
                self.hit("place-assign", off);
                return;
            }
            self.check_place(self.nodes[u].a);
            self.check_expr(self.nodes[u].b);
            return;
        }
        if (k == ND_RETURN) {
            self.check_expr(self.nodes[u].a);
            return;
        }
        if (k == ND_IF) {
            // The `if (opt) |v|` capture is in the subset (v0.173); the
            // walk sees cond, then, else like a plain if.
            self.check_expr(self.nodes[u].a);
            self.check_block(self.nodes[u].b);
            self.check_stmt(self.nodes[u].c);
            return;
        }
        if (k == ND_WHILE) {
            // Labeled loops are in the subset since v0.176.
            self.check_expr(self.nodes[u].a);
            self.check_stmt(self.nodes[u].b);
            self.check_block(self.nodes[u].c);
            return;
        }
        if (k == ND_FOR) {
            // `for (iter) |elem| { … }` / `for (iter, 0..) |elem, i| { … }`
            // is in the subset (v0.168; labeled since v0.176). The
            // iterable, then the body.
            self.check_expr(self.nodes[u].a);
            self.check_block(self.nodes[u].b);
            return;
        }
        if (k == ND_BREAK or k == ND_CONTINUE) {
            // Labeled targets are in the subset (v0.176; an unknown label
            // is sema's E0301).
            return;
        }
        if (k == ND_DEFER) {
            self.check_stmt(self.nodes[u].a);
            return;
        }
        if (k == ND_ERRDEFER) {
            // `errdefer <stmt>` is in the subset (v0.174).
            self.check_stmt(self.nodes[u].a);
            return;
        }
        if (k == ND_BLOCK) {
            self.check_block(n);
            return;
        }
        if (k == ND_SWITCH) {
            // `switch` is in the subset (v0.172): the scrutinee walks,
            // then per arm — a payload capture (tagged unions) is out —
            // each label (an unqualified `.V` is ADMITTED as a label; it
            // takes its enum from the scrutinee) and the body; the `else`
            // block last. Range labels carry literal bounds only.
            self.check_expr(self.nodes[u].a);
            var swa: i32 = self.nodes[u].b;
            while (swa >= 0) {
                if (self.found) { return; }
                var swu: usize = @as(usize, swa);
                if ((self.nodes[swu].flags & F_CAP) != 0) {
                    self.hit("capture", self.nodes[swu].off);
                    return;
                }
                var swl: i32 = self.nodes[swu].a;
                while (swl >= 0) {
                    if (self.found) { return; }
                    self.check_expr(swl);
                    swl = self.nodes[@as(usize, swl)].next;
                }
                self.check_block(self.nodes[swu].c);
                swa = self.nodes[swu].next;
            }
            self.check_block(self.nodes[u].c);
            return;
        }
        // An expression statement.
        self.check_expr(n);
    }

    fn check_fn(self: *Self, n: i32) void {
        if (self.found) { return; }
        var u: usize = @as(usize, n);
        var p: i32 = self.nodes[u].a;
        while (p >= 0) {
            if (self.found) { return; }
            var pu: usize = @as(usize, p);
            if ((self.nodes[pu].flags & F_COMPTIME) != 0) {
                self.hit("generic-param", self.nodes[pu].off);
                return;
            }
            self.check_type(self.nodes[pu].a);
            p = self.nodes[pu].next;
        }
        self.check_type(self.nodes[u].b);
        self.check_block(self.nodes[u].c);
    }
};

/// Subset verdict for a parsed module (`root` = the item-chain head), for
/// `EmitMode::Program`: the FIRST check is for a top-level `fn main` — a
/// module without one cannot be a Program-mode subset program (the Rust
/// pipeline rejects it as E0150 before emission), reported as `nomain` at
/// position 0. Test blocks are subset items in both modes (v0.166).
pub fn es_detect(src: []u8, nodes: []Node, root: i32) Det {
    return es_detect_mode(src, nodes, root, true);
}

/// The mode-aware verdict: `program_mode = false` (`EmitMode::Test`) drops
/// the `nomain` gate — Test-mode emission needs no `main` (a module without
/// test blocks lowers to the trivial harness with EVERY function live).
pub fn es_detect_mode(src: []u8, nodes: []Node, root: i32, program_mode: bool) Det {
    var d: Det = Det.init(src, nodes, root);
    if (program_mode) {
        var has_main: bool = false;
        var cur0: i32 = root;
        while (cur0 >= 0) {
            var u0: usize = @as(usize, cur0);
            if (nodes[u0].kind == ND_FN) {
                var name: []u8 = src[nodes[u0].xoff .. nodes[u0].xoff + nodes[u0].xlen];
                if (str_eq(name, "main")) { has_main = true; }
            }
            cur0 = nodes[u0].next;
        }
        if (!has_main) {
            d.hit("nomain", 0);
            return d;
        }
    }
    var cur: i32 = root;
    while (cur >= 0) {
        if (d.found) { return d; }
        var u: usize = @as(usize, cur);
        var k: u8 = nodes[u].kind;
        if (k == ND_FN) {
            d.check_fn(cur);
        } else if (k == ND_CONST) {
            d.check_type(nodes[u].a);
            d.check_expr(nodes[u].b);
        } else if (k == ND_TEST) {
            // A `test "name" { … }` block is a subset item (v0.166); its
            // body is an ordinary statement block.
            d.check_block(nodes[u].a);
        } else if (k == ND_STRUCT) {
            // A struct declaration is a subset item (v0.169 fields;
            // v0.170 admits its METHODS too): every field type must be
            // admissible, then every struct function walks exactly like a
            // top-level one — value receivers are ordinary parameters; a
            // pointer receiver (`self: *T`) is a `type-form` skip.
            var fcur: i32 = nodes[u].a;
            while (fcur >= 0 and !d.found) {
                d.check_type(nodes[@as(usize, fcur)].a);
                fcur = nodes[@as(usize, fcur)].next;
            }
            var mcur: i32 = nodes[u].b;
            while (mcur >= 0 and !d.found) {
                d.check_fn(mcur);
                mcur = nodes[@as(usize, mcur)].next;
            }
        } else if (k == ND_ENUM) {
            // An enum declaration is a subset item (v0.171): variant names
            // and literal integer values carry nothing to walk (a
            // duplicate variant is sema's E0211).
        } else if (k == ND_UNION) {
            d.hit("union", nodes[u].off);
        } else if (k == ND_IMPORT) {
            d.hit("import", nodes[u].off);
        } else if (k == ND_ERRSET) {
            // A named error set `const E = error{ A, B };` is a subset
            // item (v0.174): members carry nothing to walk (a duplicate
            // member is sema's E0331).
        } else {
            d.hit("bad-item", nodes[u].off);
        }
        cur = nodes[u].next;
    }
    return d;
}

// --- constant evaluation ----------------------------------------------------------
//
// The `const_eval::eval` mirror over the subset value kinds. A result is
// `(ok, isb, val)`: `ok = false` is any `E013x` outcome (the caller only
// needs the fact of failure — a failing top-level const is skipped, a
// failing `comptime` falls back to expression lowering, both exactly as in
// Rust). Integer arithmetic wraps as `i64` (with explicit guards where C
// would trap, see the header).

pub const EvRes = struct {
    ok: bool,
    isb: bool,
    val: i64,
};

fn ev_err() EvRes {
    return EvRes{ .ok = false, .isb = false, .val = 0 };
}

fn ev_int(v: i64) EvRes {
    return EvRes{ .ok = true, .isb = false, .val = v };
}

fn ev_bool(v: i64) EvRes {
    return EvRes{ .ok = true, .isb = true, .val = v };
}

/// The most negative `i64`, spelled without a negative literal.
fn ev_i64_min() i64 {
    return (0 - 9223372036854775807) - 1;
}

// --- string literals ---------------------------------------------------------------

/// Decode a string-literal token span (quotes included) to its bytes: the
/// four legal escapes `\n \t \\ \"` become their bytes, everything else is
/// verbatim (the lexer already rejected any other escape). Mirrors the Rust
/// lexer's decode that fills `Expr::StrLit.value`.
pub fn es_decode_str(a: Allocator, src: []u8, off: usize, len: usize) []u8 {
    var sb: StrBuilder = StrBuilder.init(a);
    var i: usize = off + 1;
    var end: usize = off + len - 1;
    while (i < end) {
        var b: u8 = src[i];
        if (b == 92 and i + 1 < end) {
            var e: u8 = src[i + 1];
            if (e == 110) { sb.append_byte(a, 10); }
            if (e == 116) { sb.append_byte(a, 9); }
            if (e == 92) { sb.append_byte(a, 92); }
            if (e == 34) { sb.append_byte(a, 34); }
            i += 2;
        } else {
            sb.append_byte(a, b);
            i += 1;
        }
    }
    var s: []u8 = sb.build(a);
    sb.deinit(a);
    return s;
}

/// Whether `b` is an ASCII hex digit (`0-9a-fA-F`).
fn es_is_hex_digit(b: u8) bool {
    if (b >= 48 and b <= 57) { return true; }
    if (b >= 97 and b <= 102) { return true; }
    return b >= 65 and b <= 70;
}

/// Append one lowercase hex digit for the value `v` (0..15).
fn es_hex_digit(a: Allocator, sb: *StrBuilder, v: u8) void {
    if (v < 10) {
        sb.append_byte(a, 48 + v);
    } else {
        sb.append_byte(a, 97 + (v - 10));
    }
}

/// `c_escape`: escape decoded bytes for a C double-quoted literal WITHOUT
/// the surrounding quotes — only `\` `"` `\n` `\t` `\r` are escaped, every
/// other byte passes through VERBATIM (unlike `c_string_literal`: no hex
/// escapes, no literal splitting). Used for the harness test-name table
/// (v0.166, `emit_test_harness`).
pub fn es_c_escape(a: Allocator, bytes: []u8) []u8 {
    var sb: StrBuilder = StrBuilder.init(a);
    var i: usize = 0;
    while (i < bytes.len) : (i += 1) {
        var b: u8 = bytes[i];
        if (b == 92) {
            sb.append(a, "\\\\");
        } else if (b == 34) {
            sb.append(a, "\\\"");
        } else if (b == 10) {
            sb.append(a, "\\n");
        } else if (b == 9) {
            sb.append(a, "\\t");
        } else if (b == 13) {
            sb.append(a, "\\r");
        } else {
            sb.append_byte(a, b);
        }
    }
    var s: []u8 = sb.build(a);
    sb.deinit(a);
    return s;
}

/// `c_string_literal`: render decoded bytes as a complete double-quoted C
/// string literal. Byte-exact escaping: `\` `"` are escaped, `\n`/`\t`/`\r`
/// stay readable, every byte outside printable ASCII becomes a two-digit
/// `\xNN` escape — and when such an escape is immediately followed by a
/// literal hex digit, the literal is split with `" "` so C cannot absorb
/// that digit into the escape.
pub fn es_c_string_literal(a: Allocator, bytes: []u8) []u8 {
    var sb: StrBuilder = StrBuilder.init(a);
    sb.append_byte(a, 34);
    var prev_hex: bool = false;
    var i: usize = 0;
    while (i < bytes.len) : (i += 1) {
        var b: u8 = bytes[i];
        if (b == 92) {
            sb.append(a, "\\\\");
            prev_hex = false;
        } else if (b == 34) {
            sb.append(a, "\\\"");
            prev_hex = false;
        } else if (b == 10) {
            sb.append(a, "\\n");
            prev_hex = false;
        } else if (b == 9) {
            sb.append(a, "\\t");
            prev_hex = false;
        } else if (b == 13) {
            sb.append(a, "\\r");
            prev_hex = false;
        } else if (b >= 32 and b <= 126) {
            if (prev_hex and es_is_hex_digit(b)) {
                sb.append(a, "\" \"");
            }
            sb.append_byte(a, b);
            prev_hex = false;
        } else {
            sb.append(a, "\\x");
            es_hex_digit(a, &sb, b >> 4);
            es_hex_digit(a, &sb, b & 15);
            prev_hex = true;
        }
    }
    sb.append_byte(a, 34);
    var s: []u8 = sb.build(a);
    sb.deinit(a);
    return s;
}

// --- the emitter -------------------------------------------------------------------

/// One lexical scope active during emission (`emit_c.rs::Scope`). The defers
/// and locals of every scope live in the emitter's flat `defers`/`vts`
/// stacks; a scope records where its span begins (`dstart`/`vstart`), so a
/// scope's own entries are `[start, next scope's start)` — pushes only ever
/// target the innermost scope, so the spans stay contiguous.
pub const EmScope = struct {
    is_loop: bool,
    cont: i32,
    // The `for` counter whose `__kd_fi{N} += 1;` is this loop scope's raw
    // continue-clause (`Scope::cont_raw`, SPEC §29.2); -1 = none.
    raw_fi: i64,
    // The loop's label span (v0.176, `Scope::loop_label`); llen 0 = none.
    loff: usize,
    llen: usize,
    dstart: i64,
    vstart: i64,
};

/// One local/param type record: the source name (a span) and its type code.
pub const VtEnt = struct {
    off: usize,
    len: usize,
    ty: i64,
};

/// One top-level function signature: name span, resolved return type code,
/// its arena node, and the §43.1 liveness verdict.
pub const FnSig = struct {
    off: usize,
    len: usize,
    ret: i64,
    node: i32,
    live: bool,
    // The `fn_params` window (v0.172): `pcount` parameter ET codes
    // starting at `pstart` in the emitter's flat `fp_ty` table — argument
    // coercion (a contextual `.V` argument) reads them by position.
    pstart: i64,
    pcount: i64,
};

/// One folded top-level constant: name span, kind, value.
pub const CEnt = struct {
    off: usize,
    len: usize,
    isb: bool,
    val: i64,
};

/// A pending name in the liveness worklist (a span into the source).
pub const PendName = struct {
    off: usize,
    len: usize,
};

pub const Em = struct {
    src: []u8,
    nodes: []Node,
    root: i32,
    // Output buffer (grown by doubling).
    out: []u8,
    out_len: usize,
    indent: i64,
    // Scope stack + the flat defer/local stacks it indexes into.
    scopes: []EmScope,
    sc_len: usize,
    defers: []i32,
    derr: []bool,
    df_len: usize,
    vts: []VtEnt,
    vt_len: usize,
    // Collected signatures and folded consts.
    fns: []FnSig,
    fn_len: usize,
    consts: []CEnt,
    ct_len: usize,
    // Return type of the function being emitted.
    cur_ret: i64,
    // Monotonic counter for the `__kd_str{N}` print-hoist temporaries
    // (`Emitter::str_counter`), reset at the start of every function body.
    str_count: i64,
    // Monotonic counter for the `__kd_idx{N}` bounds-checked index-write
    // temporaries (`Emitter::idx_counter`), reset per function body.
    idx_count: i64,
    // The interned slice ELEMENT type codes, in sema's first-intern order
    // (v0.164; see `intern_scan`). Drives the typedef section's content
    // and ORDER, which mirror `StructTable::slices()` iteration.
    slices: []i64,
    sl_len: usize,
    // The interned ARRAY (elem, len) pairs, in sema's first-intern order
    // (v0.168) — the `StructTable::array_info` mirror. Array type codes
    // are `ET_ARR_BASE + <index>` into these parallel tables. In emit's
    // dependency-ordered typedef walk, arrays precede slices.
    ar_elem: []i64,
    ar_len_: []i64,
    ar_count: usize,
    // Monotonic counter for the `__kd_for{N}`/`__kd_fi{N}` loop temporaries
    // (`Emitter::for_counter`), reset per function AND per test body.
    for_count: i64,
    // The struct table (v0.169) — the sema pass-0a/0b mirror. Ids are
    // declaration order; fields live flat in the `sf_*` arrays, one
    // `(start, count)` window per struct. Field types are ET codes.
    st_name_off: []i64,
    st_name_len: []i64,
    st_f_start: []i64,
    st_f_count: []i64,
    st_count: usize,
    sf_name_off: []i64,
    sf_name_len: []i64,
    sf_ty: []i64,
    sf_count: usize,
    // The struct-method table (v0.170) — one row per struct function, in
    // struct item order then declaration order: owning struct code, name
    // span, resolved return ET, the ND_FN node, and the name-level
    // liveness flag (SPEC §43.1 — receiver-agnostic).
    mt_sid: []i64,
    mt_noff: []i64,
    mt_nlen: []i64,
    mt_ret: []i64,
    mt_node: []i32,
    mt_live: []bool,
    mt_count: usize,
    // The enum table (v0.171) — sema pass 0's mirror. Ids are declaration
    // order; variants live flat in the `ev_*` arrays (name span + resolved
    // integer value), one `(start, count)` window per enum.
    // The flat parameter-type table backing `FnSig.pstart/pcount` AND the
    // method rows' `mt_p_*` windows (both are `resolve_ty`-resolved, in
    // declaration order — the `fn_params` / `method_params` mirror).
    fp_ty: []i64,
    fp_count: usize,
    mt_p_start: []i64,
    mt_p_count: []i64,
    en_name_off: []i64,
    en_name_len: []i64,
    en_v_start: []i64,
    en_v_count: []i64,
    en_count: usize,
    // The interned OPTIONAL inner-type codes, in sema's first-intern
    // order (v0.173) — the `optional_inners` mirror. Optional codes are
    // `ET_OPT_BASE + index`.
    opt_inners: []i64,
    opt_count: usize,
    // The interned ERROR-UNION payload codes (v0.174) — the
    // `error_union_payloads` mirror; codes are `ET_ERRU_BASE + index`.
    eu_payloads: []i64,
    eu_count: usize,
    // The pointer-pointee registry (v0.175) — `local_ptr_pointees` plus
    // struct-field pointees, dedup by pointee. `pt_local` marks entries
    // the Rust PRE-PASS registers (written `*T` in signatures, local/
    // const annotations, method signatures/bodies): `type_of(&place)`
    // consults ONLY those (the miss → untypeable mirror).
    pt_pointees: []i64,
    pt_local: []bool,
    pt_count: usize,
    // The pending loop label for the next pushed scope (v0.176).
    pend_loff: usize,
    pend_llen: usize,
    // The GLOBAL error-name table (`error_names` mirror): 1-based codes
    // in first-intern order — error-set members (pass 0), then `error.X`
    // literals in body-check order.
    er_off: []i64,
    er_len: []i64,
    er_count: usize,
    // `__kd_try{N}` / `__kd_eu{N}`+`__kd_catch{N}` counters, reset per
    // function AND per test body.
    try_count: i64,
    catch_count: i64,
    // Monotonic counter for the `__kd_if{N}` if-capture temporaries
    // (`Emitter::if_counter`), reset per function AND per test body.
    if_count: i64,
    ev_name_off: []i64,
    ev_name_len: []i64,
    ev_val: []i64,
    ev_count: usize,
    // `EmitMode::Test` (v0.166): swaps the entry-point wiring for the test
    // harness, roots liveness at the test bodies (every function live when
    // there are none), and enables the statement-level `expect` lowering.
    is_test: bool,

    fn init(a: Allocator, src: []u8, nodes: []Node, root: i32) Self {
        return Em{
            .src = src,
            .nodes = nodes,
            .root = root,
            .out = alloc(a, u8, 4096),
            .out_len = 0,
            .indent = 0,
            .scopes = alloc(a, EmScope, 16),
            .sc_len = 0,
            .defers = alloc(a, i32, 16),
            .derr = alloc(a, bool, 16),
            .df_len = 0,
            .vts = alloc(a, VtEnt, 32),
            .vt_len = 0,
            .fns = alloc(a, FnSig, 16),
            .fn_len = 0,
            .consts = alloc(a, CEnt, 16),
            .ct_len = 0,
            .cur_ret = ET_VOID,
            .str_count = 0,
            .idx_count = 0,
            .slices = alloc(a, i64, 8),
            .sl_len = 0,
            .ar_elem = alloc(a, i64, 8),
            .ar_len_ = alloc(a, i64, 8),
            .ar_count = 0,
            .for_count = 0,
            .st_name_off = alloc(a, i64, 8),
            .st_name_len = alloc(a, i64, 8),
            .st_f_start = alloc(a, i64, 8),
            .st_f_count = alloc(a, i64, 8),
            .st_count = 0,
            .sf_name_off = alloc(a, i64, 16),
            .sf_name_len = alloc(a, i64, 16),
            .sf_ty = alloc(a, i64, 16),
            .sf_count = 0,
            .mt_sid = alloc(a, i64, 8),
            .mt_noff = alloc(a, i64, 8),
            .mt_nlen = alloc(a, i64, 8),
            .mt_ret = alloc(a, i64, 8),
            .mt_node = alloc(a, i32, 8),
            .mt_live = alloc(a, bool, 8),
            .mt_count = 0,
            .fp_ty = alloc(a, i64, 16),
            .fp_count = 0,
            .mt_p_start = alloc(a, i64, 8),
            .mt_p_count = alloc(a, i64, 8),
            .opt_inners = alloc(a, i64, 4),
            .opt_count = 0,
            .eu_payloads = alloc(a, i64, 4),
            .eu_count = 0,
            .pt_pointees = alloc(a, i64, 4),
            .pt_local = alloc(a, bool, 4),
            .pt_count = 0,
            .pend_loff = 0,
            .pend_llen = 0,
            .er_off = alloc(a, i64, 8),
            .er_len = alloc(a, i64, 8),
            .er_count = 0,
            .try_count = 0,
            .catch_count = 0,
            .if_count = 0,
            .en_name_off = alloc(a, i64, 4),
            .en_name_len = alloc(a, i64, 4),
            .en_v_start = alloc(a, i64, 4),
            .en_v_count = alloc(a, i64, 4),
            .en_count = 0,
            .ev_name_off = alloc(a, i64, 8),
            .ev_name_len = alloc(a, i64, 8),
            .ev_val = alloc(a, i64, 8),
            .ev_count = 0,
            .is_test = false,
        };
    }

    // -- the interned-array table (the `array_info` mirror, v0.168) -------------

    /// `intern_array`: dedup-append of an `(elem, len)` pair; returns the
    /// array TYPE CODE (`ET_ARR_BASE + index`).
    fn arr_intern(self: *Self, a: Allocator, elem: i64, alen: i64) i64 {
        var i: usize = 0;
        while (i < self.ar_count) : (i += 1) {
            if (self.ar_elem[i] == elem and self.ar_len_[i] == alen) {
                return ET_ARR_BASE + @as(i64, i);
            }
        }
        if (self.ar_count == self.ar_elem.len) {
            var ge: []i64 = alloc(a, i64, self.ar_elem.len * 2);
            var gl: []i64 = alloc(a, i64, self.ar_len_.len * 2);
            var j: usize = 0;
            while (j < self.ar_count) : (j += 1) {
                ge[j] = self.ar_elem[j];
                gl[j] = self.ar_len_[j];
            }
            free(a, self.ar_elem);
            free(a, self.ar_len_);
            self.ar_elem = ge;
            self.ar_len_ = gl;
        }
        self.ar_elem[self.ar_count] = elem;
        self.ar_len_[self.ar_count] = alen;
        self.ar_count += 1;
        return ET_ARR_BASE + @as(i64, self.ar_count) - 1;
    }

    /// The element type / length of an interned array code.
    fn arr_elem_of(self: *Self, t: i64) i64 {
        return self.ar_elem[@as(usize, t - ET_ARR_BASE)];
    }

    fn arr_len_of(self: *Self, t: i64) i64 {
        return self.ar_len_[@as(usize, t - ET_ARR_BASE)];
    }

    /// `array_c_name`: `kd_arr_<type_mangle(elem)>_<N>` (built fresh — the
    /// length makes a static table impossible).
    fn arr_c_name(self: *Self, a: Allocator, t: i64) []u8 {
        var sb: StrBuilder = StrBuilder.init(a);
        sb.append(a, "kd_arr_");
        sb.append(a, self.mangle_of(a, self.arr_elem_of(t)));
        sb.append(a, "_");
        sb.append_i64(a, self.arr_len_of(t));
        var s: []u8 = sb.build(a);
        sb.deinit(a);
        return s;
    }

    /// `Emitter::cty_of` with the array, struct and slice families
    /// included: arrays/structs/struct-elem slices spell through their
    /// tables, everything else through `et_c_name`.
    fn cty_of(self: *Self, a: Allocator, t: i64) []u8 {
        if (et_is_arr(t)) { return self.arr_c_name(a, t); }
        if (et_is_struct(t)) { return self.st_c_name(a, t); }
        if (et_is_enum(t)) { return self.en_c_name(a, t); }
        if (et_is_opt(t)) { return self.opt_c_name(a, t); }
        if (et_is_erru(t)) { return self.eu_c_name(a, t); }
        if (et_is_ptr(t)) {
            // `*T` has no typedef: its C spelling is `<pointee cty>*`.
            var sbp: StrBuilder = StrBuilder.init(a);
            sbp.append(a, self.cty_of(a, self.pt_pointee_of(t)));
            sbp.append(a, "*");
            var sp: []u8 = sbp.build(a);
            sbp.deinit(a);
            return sp;
        }
        if (et_is_slice(t)) { return self.sl_c_name(a, t); }
        return et_c_name(t);
    }

    // -- the struct table (sema pass 0a/0b mirror, v0.169) ---------------------

    /// The struct id for a source name, as an ET code (`ET_STRUCT_BASE +
    /// id`), or `ET_NONE` when no struct of that name is declared.
    fn st_code_of(self: *Self, name: []u8) i64 {
        var i: usize = 0;
        while (i < self.st_count) : (i += 1) {
            var off: usize = @as(usize, self.st_name_off[i]);
            var len: usize = @as(usize, self.st_name_len[i]);
            if (str_eq(self.src[off .. off + len], name)) {
                return ET_STRUCT_BASE + @as(i64, i);
            }
        }
        return ET_NONE;
    }

    /// `StructTable::c_name`: `kd_struct_<Name>`.
    fn st_c_name(self: *Self, a: Allocator, t: i64) []u8 {
        var i: usize = @as(usize, t - ET_STRUCT_BASE);
        var off: usize = @as(usize, self.st_name_off[i]);
        var len: usize = @as(usize, self.st_name_len[i]);
        var sb: StrBuilder = StrBuilder.init(a);
        sb.append(a, "kd_struct_");
        sb.append(a, self.src[off .. off + len]);
        var out: []u8 = sb.build(a);
        sb.deinit(a);
        return out;
    }

    /// `StructInfo::field_type`: the ET code of field `fname` of the struct
    /// coded `t`, or `ET_NONE` when it has no such field.
    fn st_field_ty(self: *Self, t: i64, fname: []u8) i64 {
        var i: usize = @as(usize, t - ET_STRUCT_BASE);
        var start: usize = @as(usize, self.st_f_start[i]);
        var n: usize = @as(usize, self.st_f_count[i]);
        var j: usize = 0;
        while (j < n) : (j += 1) {
            var off: usize = @as(usize, self.sf_name_off[start + j]);
            var len: usize = @as(usize, self.sf_name_len[start + j]);
            if (str_eq(self.src[off .. off + len], fname)) {
                return self.sf_ty[start + j];
            }
        }
        return ET_NONE;
    }

    /// `StructTable::type_mangle` over the subset: scalars spell their C
    /// name, a struct spells `struct_<Name>` (no `kd_` prefix).
    fn mangle_of(self: *Self, a: Allocator, t: i64) []u8 {
        if (et_is_enum(t)) {
            var sbz: StrBuilder = StrBuilder.init(a);
            sbz.append(a, "enum_");
            sbz.append(a, self.en_name_of(t));
            var oz: []u8 = sbz.build(a);
            sbz.deinit(a);
            return oz;
        }
        if (et_is_struct(t)) {
            var i: usize = @as(usize, t - ET_STRUCT_BASE);
            var off: usize = @as(usize, self.st_name_off[i]);
            var len: usize = @as(usize, self.st_name_len[i]);
            var sb: StrBuilder = StrBuilder.init(a);
            sb.append(a, "struct_");
            sb.append(a, self.src[off .. off + len]);
            var out: []u8 = sb.build(a);
            sb.deinit(a);
            return out;
        }
        return et_c_name(t);
    }

    /// `slice_c_name`: `kd_slice_<type_mangle(elem)>` — the static-string
    /// fast path for scalar elements, built fresh for struct and enum
    /// elements (v0.171 fixed the enum arm falling to `kd_slice_void`).
    fn sl_c_name(self: *Self, a: Allocator, t: i64) []u8 {
        var e: i64 = et_slice_elem(t);
        if (!et_is_struct(e) and !et_is_enum(e)) { return et_slice_c_name(t); }
        var sb: StrBuilder = StrBuilder.init(a);
        sb.append(a, "kd_slice_");
        sb.append(a, self.mangle_of(a, e));
        var out: []u8 = sb.build(a);
        sb.deinit(a);
        return out;
    }

    /// Sema pass 0a/0b (v0.169): register every struct name in item order,
    /// then resolve every field type in item order (fields in declaration
    /// order) — interning any `[]T`/`[N]T` field exactly where sema's
    /// `wrap_type` does. Runs BEFORE the signature interning pass.
    fn st_collect(self: *Self, a: Allocator) void {
        // Pass 0a: names.
        var cur: i32 = self.root;
        while (cur >= 0) {
            var u: usize = @as(usize, cur);
            if (self.nodes[u].kind == ND_STRUCT) {
                if (self.st_count == self.st_name_off.len) {
                    var g0: []i64 = alloc(a, i64, self.st_name_off.len * 2);
                    var g1: []i64 = alloc(a, i64, self.st_name_len.len * 2);
                    var g2: []i64 = alloc(a, i64, self.st_f_start.len * 2);
                    var g3: []i64 = alloc(a, i64, self.st_f_count.len * 2);
                    var i0: usize = 0;
                    while (i0 < self.st_count) : (i0 += 1) {
                        g0[i0] = self.st_name_off[i0];
                        g1[i0] = self.st_name_len[i0];
                        g2[i0] = self.st_f_start[i0];
                        g3[i0] = self.st_f_count[i0];
                    }
                    free(a, self.st_name_off);
                    free(a, self.st_name_len);
                    free(a, self.st_f_start);
                    free(a, self.st_f_count);
                    self.st_name_off = g0;
                    self.st_name_len = g1;
                    self.st_f_start = g2;
                    self.st_f_count = g3;
                }
                self.st_name_off[self.st_count] = @as(i64, self.nodes[u].xoff);
                self.st_name_len[self.st_count] = @as(i64, self.nodes[u].xlen);
                self.st_f_start[self.st_count] = 0;
                self.st_f_count[self.st_count] = 0;
                self.st_count += 1;
            }
            cur = self.nodes[u].next;
        }
        // Pass 0b: field types (interning slice/array fields in order).
        var sid: usize = 0;
        cur = self.root;
        while (cur >= 0) {
            var u2: usize = @as(usize, cur);
            if (self.nodes[u2].kind == ND_STRUCT) {
                self.st_f_start[sid] = @as(i64, self.sf_count);
                var nf: i64 = 0;
                var fcur: i32 = self.nodes[u2].a;
                while (fcur >= 0) {
                    var fu: usize = @as(usize, fcur);
                    var fty: i64 = self.st_resolve_field(a, self.nodes[fu].a);
                    if (self.sf_count == self.sf_name_off.len) {
                        var h0: []i64 = alloc(a, i64, self.sf_name_off.len * 2);
                        var h1: []i64 = alloc(a, i64, self.sf_name_len.len * 2);
                        var h2: []i64 = alloc(a, i64, self.sf_ty.len * 2);
                        var j0: usize = 0;
                        while (j0 < self.sf_count) : (j0 += 1) {
                            h0[j0] = self.sf_name_off[j0];
                            h1[j0] = self.sf_name_len[j0];
                            h2[j0] = self.sf_ty[j0];
                        }
                        free(a, self.sf_name_off);
                        free(a, self.sf_name_len);
                        free(a, self.sf_ty);
                        self.sf_name_off = h0;
                        self.sf_name_len = h1;
                        self.sf_ty = h2;
                    }
                    self.sf_name_off[self.sf_count] = @as(i64, self.nodes[fu].xoff);
                    self.sf_name_len[self.sf_count] = @as(i64, self.nodes[fu].xlen);
                    self.sf_ty[self.sf_count] = fty;
                    self.sf_count += 1;
                    nf += 1;
                    fcur = self.nodes[fu].next;
                }
                self.st_f_count[sid] = nf;
                sid += 1;
            }
            cur = self.nodes[u2].next;
        }
    }

    /// `resolve_field_type` + `wrap_type` over the subset: the base name
    /// resolves builtin-first then struct (any declaration position — the
    /// forward/cyclic E0160 is sema's rejection, not emission's), and a
    /// slice/array wrapper INTERNS its composite exactly like a body
    /// annotation. An unresolvable field falls back to `i64` (sema records
    /// the same fallback so field lookups still succeed).
    fn st_resolve_field(self: *Self, a: Allocator, tn: i32) i64 {
        var u: usize = @as(usize, tn);
        var base: i64 = et_from_name(self.xname(tn));
        if (base == ET_NONE) { base = self.st_code_of(self.xname(tn)); }
        if ((self.nodes[u].flags & F_PTR) != 0) {
            // A `*T` field: register the pointee (NOT a pre-pass entry —
            // the Rust local registry excludes field types).
            if (base == ET_NONE) { return ET_I64; }
            return self.pt_intern(a, base, false);
        }
        if ((self.nodes[u].flags & F_SLICE) != 0) {
            if (base == ET_NONE) { return ET_I64; }
            self.intern_elem(a, base);
            return et_slice_of(base);
        }
        if ((self.nodes[u].flags & F_ARRLIT) != 0) {
            if (base == ET_NONE) { return ET_I64; }
            return self.arr_intern(a, base, self.nodes[u].val);
        }
        if ((self.nodes[u].flags & F_OPT) != 0) {
            if (base == ET_NONE) { return ET_I64; }
            return self.opt_intern(a, base);
        }
        if ((self.nodes[u].flags & F_ERR) != 0) {
            if (base == ET_NONE) { return ET_I64; }
            return self.eu_intern(a, base);
        }
        if (base == ET_NONE) { return ET_I64; }
        return base;
    }

    /// A bare type-name's ET code: builtins first, then declared structs,
    /// then declared enums (the `base_type` resolution order), else
    /// `ET_NONE`.
    fn base_code(self: *Self, name: []u8) i64 {
        var t: i64 = et_from_name(name);
        if (t != ET_NONE) { return t; }
        var st: i64 = self.st_code_of(name);
        if (st != ET_NONE) { return st; }
        return self.en_code_of(name);
    }

    // -- the enum table (sema pass 0 mirror, v0.171) -----------------------------

    /// The enum id for a source name, as an ET code (`ET_ENUM_BASE + id`),
    /// or `ET_NONE` when no enum of that name is declared.
    fn en_code_of(self: *Self, name: []u8) i64 {
        var i: usize = 0;
        while (i < self.en_count) : (i += 1) {
            var off: usize = @as(usize, self.en_name_off[i]);
            var len: usize = @as(usize, self.en_name_len[i]);
            if (str_eq(self.src[off .. off + len], name)) {
                return ET_ENUM_BASE + @as(i64, i);
            }
        }
        return ET_NONE;
    }

    /// The bare source name of an enum code.
    fn en_name_of(self: *Self, ecode: i64) []u8 {
        var i: usize = @as(usize, ecode - ET_ENUM_BASE);
        var off: usize = @as(usize, self.en_name_off[i]);
        var len: usize = @as(usize, self.en_name_len[i]);
        return self.src[off .. off + len];
    }

    /// `enum_c_name`: `kd_enum_<Name>`.
    fn en_c_name(self: *Self, a: Allocator, ecode: i64) []u8 {
        var sb: StrBuilder = StrBuilder.init(a);
        sb.append(a, "kd_enum_");
        sb.append(a, self.en_name_of(ecode));
        var out: []u8 = sb.build(a);
        sb.deinit(a);
        return out;
    }

    /// `intern_optional`: dedup-append of an inner code; returns the
    /// optional TYPE CODE (`ET_OPT_BASE + index`).
    fn opt_intern(self: *Self, a: Allocator, inner: i64) i64 {
        var i: usize = 0;
        while (i < self.opt_count) : (i += 1) {
            if (self.opt_inners[i] == inner) { return ET_OPT_BASE + @as(i64, i); }
        }
        if (self.opt_count == self.opt_inners.len) {
            var g: []i64 = alloc(a, i64, self.opt_inners.len * 2);
            var j: usize = 0;
            while (j < self.opt_count) : (j += 1) { g[j] = self.opt_inners[j]; }
            free(a, self.opt_inners);
            self.opt_inners = g;
        }
        self.opt_inners[self.opt_count] = inner;
        self.opt_count += 1;
        return ET_OPT_BASE + @as(i64, self.opt_count) - 1;
    }

    /// The inner code of an interned optional.
    fn opt_inner_of(self: *Self, t: i64) i64 {
        return self.opt_inners[@as(usize, t - ET_OPT_BASE)];
    }

    /// `optional_c_name`: `kd_opt_<type_mangle(inner)>`.
    fn opt_c_name(self: *Self, a: Allocator, t: i64) []u8 {
        var sb: StrBuilder = StrBuilder.init(a);
        sb.append(a, "kd_opt_");
        sb.append(a, self.mangle_of(a, self.opt_inner_of(t)));
        var out: []u8 = sb.build(a);
        sb.deinit(a);
        return out;
    }

    /// `intern_error_union`: dedup-append of a payload code; returns the
    /// error-union TYPE CODE (`ET_ERRU_BASE + index`).
    fn eu_intern(self: *Self, a: Allocator, payload: i64) i64 {
        var i: usize = 0;
        while (i < self.eu_count) : (i += 1) {
            if (self.eu_payloads[i] == payload) { return ET_ERRU_BASE + @as(i64, i); }
        }
        if (self.eu_count == self.eu_payloads.len) {
            var g: []i64 = alloc(a, i64, self.eu_payloads.len * 2);
            var j: usize = 0;
            while (j < self.eu_count) : (j += 1) { g[j] = self.eu_payloads[j]; }
            free(a, self.eu_payloads);
            self.eu_payloads = g;
        }
        self.eu_payloads[self.eu_count] = payload;
        self.eu_count += 1;
        return ET_ERRU_BASE + @as(i64, self.eu_count) - 1;
    }

    fn eu_payload_of(self: *Self, t: i64) i64 {
        return self.eu_payloads[@as(usize, t - ET_ERRU_BASE)];
    }

    /// `error_union_c_name`: `kd_err_<type_mangle(payload)>`.
    fn eu_c_name(self: *Self, a: Allocator, t: i64) []u8 {
        var sb: StrBuilder = StrBuilder.init(a);
        sb.append(a, "kd_err_");
        sb.append(a, self.mangle_of(a, self.eu_payload_of(t)));
        var out: []u8 = sb.build(a);
        sb.deinit(a);
        return out;
    }

    /// Register a pointer pointee (dedup); `local` marks pre-pass
    /// entries. Returns the pointer TYPE CODE (`ET_PTR_BASE + index`).
    fn pt_intern(self: *Self, a: Allocator, pointee: i64, local: bool) i64 {
        var i: usize = 0;
        while (i < self.pt_count) : (i += 1) {
            if (self.pt_pointees[i] == pointee) {
                if (local and !self.pt_local[i]) { self.pt_local[i] = true; }
                return ET_PTR_BASE + @as(i64, i);
            }
        }
        if (self.pt_count == self.pt_pointees.len) {
            var g: []i64 = alloc(a, i64, self.pt_pointees.len * 2);
            var g2: []bool = alloc(a, bool, self.pt_local.len * 2);
            var j: usize = 0;
            while (j < self.pt_count) : (j += 1) {
                g[j] = self.pt_pointees[j];
                g2[j] = self.pt_local[j];
            }
            free(a, self.pt_pointees);
            free(a, self.pt_local);
            self.pt_pointees = g;
            self.pt_local = g2;
        }
        self.pt_pointees[self.pt_count] = pointee;
        self.pt_local[self.pt_count] = local;
        self.pt_count += 1;
        return ET_PTR_BASE + @as(i64, self.pt_count) - 1;
    }

    fn pt_pointee_of(self: *Self, t: i64) i64 {
        return self.pt_pointees[@as(usize, t - ET_PTR_BASE)];
    }

    /// The LOCAL-registry code for a pointee (`type_of(&place)`): only
    /// pre-pass entries count; a miss is `ET_NONE` (untypeable, exactly
    /// the Rust `position(..)` → `None`).
    fn pt_local_code(self: *Self, pointee: i64) i64 {
        var i: usize = 0;
        while (i < self.pt_count) : (i += 1) {
            if (self.pt_pointees[i] == pointee and self.pt_local[i]) {
                return ET_PTR_BASE + @as(i64, i);
            }
        }
        return ET_NONE;
    }

    /// `collect_ptr_types` (v0.175): register every `*T` WRITTEN in a
    /// fn/method signature, a local/const annotation or a test body —
    /// before any `resolve_ty` (the items loop of `collect_signatures`).
    fn pt_collect(self: *Self, a: Allocator) void {
        var cur: i32 = self.root;
        while (cur >= 0) {
            var u: usize = @as(usize, cur);
            var k: u8 = self.nodes[u].kind;
            if (k == ND_FN) {
                self.pt_note_fn(a, cur);
            } else if (k == ND_STRUCT) {
                var m: i32 = self.nodes[u].b;
                while (m >= 0) {
                    self.pt_note_fn(a, m);
                    m = self.nodes[@as(usize, m)].next;
                }
            } else if (k == ND_CONST) {
                self.pt_note_ty(a, self.nodes[u].a);
            } else if (k == ND_TEST) {
                self.pt_note_block(a, self.nodes[u].a);
            }
            cur = self.nodes[u].next;
        }
    }

    fn pt_note_fn(self: *Self, a: Allocator, fnode: i32) void {
        var u: usize = @as(usize, fnode);
        self.pt_note_ty(a, self.nodes[u].b);
        var p: i32 = self.nodes[u].a;
        while (p >= 0) {
            self.pt_note_ty(a, self.nodes[@as(usize, p)].a);
            p = self.nodes[@as(usize, p)].next;
        }
        self.pt_note_block(a, self.nodes[u].c);
    }

    fn pt_note_block(self: *Self, a: Allocator, block: i32) void {
        if (block < 0) { return; }
        var cur: i32 = self.nodes[@as(usize, block)].a;
        while (cur >= 0) {
            self.pt_note_stmt(a, cur);
            cur = self.nodes[@as(usize, cur)].next;
        }
    }

    fn pt_note_stmt(self: *Self, a: Allocator, n: i32) void {
        if (n < 0) { return; }
        var u: usize = @as(usize, n);
        var k: u8 = self.nodes[u].kind;
        if (k == ND_LET) {
            self.pt_note_ty(a, self.nodes[u].a);
            return;
        }
        if (k == ND_IF) {
            self.pt_note_block(a, self.nodes[u].b);
            self.pt_note_stmt(a, self.nodes[u].c);
            return;
        }
        if (k == ND_WHILE) {
            self.pt_note_block(a, self.nodes[u].c);
            return;
        }
        if (k == ND_FOR) {
            self.pt_note_block(a, self.nodes[u].b);
            return;
        }
        if (k == ND_BLOCK) {
            self.pt_note_block(a, n);
            return;
        }
        if (k == ND_DEFER or k == ND_ERRDEFER) {
            self.pt_note_stmt(a, self.nodes[u].a);
            return;
        }
        if (k == ND_SWITCH) {
            var arm: i32 = self.nodes[u].b;
            while (arm >= 0) {
                self.pt_note_block(a, self.nodes[@as(usize, arm)].c);
                arm = self.nodes[@as(usize, arm)].next;
            }
            self.pt_note_block(a, self.nodes[u].c);
            return;
        }
    }

    fn pt_note_ty(self: *Self, a: Allocator, tn: i32) void {
        if (tn < 0) { return; }
        var u: usize = @as(usize, tn);
        if ((self.nodes[u].flags & F_PTR) == 0) { return; }
        var base: i64 = self.base_code(self.xname(tn));
        if (base == ET_NONE) { base = ET_VOID; }
        var unused: i64 = self.pt_intern(a, base, true);
        if (unused == 0) { }
    }

    /// `intern_error`: the global error-name table — dedup by NAME, codes
    /// are the 1-based first-intern positions (0 = "no error").
    fn er_intern(self: *Self, a: Allocator, off: usize, len: usize) i64 {
        var i: usize = 0;
        while (i < self.er_count) : (i += 1) {
            var eo: usize = @as(usize, self.er_off[i]);
            var el: usize = @as(usize, self.er_len[i]);
            if (str_eq(self.src[eo .. eo + el], self.src[off .. off + len])) {
                return @as(i64, i) + 1;
            }
        }
        if (self.er_count == self.er_off.len) {
            var g0: []i64 = alloc(a, i64, self.er_off.len * 2);
            var g1: []i64 = alloc(a, i64, self.er_len.len * 2);
            var j: usize = 0;
            while (j < self.er_count) : (j += 1) {
                g0[j] = self.er_off[j];
                g1[j] = self.er_len[j];
            }
            free(a, self.er_off);
            free(a, self.er_len);
            self.er_off = g0;
            self.er_len = g1;
        }
        self.er_off[self.er_count] = @as(i64, off);
        self.er_len[self.er_count] = @as(i64, len);
        self.er_count += 1;
        return @as(i64, self.er_count);
    }

    /// `error_code`: the 1-based code of a declared error name, or 0 (the
    /// Rust `unwrap_or(0)` fallback for an unknown name).
    fn er_code_of(self: *Self, name: []u8) i64 {
        var i: usize = 0;
        while (i < self.er_count) : (i += 1) {
            var eo: usize = @as(usize, self.er_off[i]);
            var el: usize = @as(usize, self.er_len[i]);
            if (str_eq(self.src[eo .. eo + el], name)) { return @as(i64, i) + 1; }
        }
        return 0;
    }

    /// Sema pass 0 (error sets, v0.174): register every named set's
    /// members as GLOBAL error names, in declaration order — after enums,
    /// before struct names (the sema pass sequence).
    fn er_collect(self: *Self, a: Allocator) void {
        var cur: i32 = self.root;
        while (cur >= 0) {
            var u: usize = @as(usize, cur);
            if (self.nodes[u].kind == ND_ERRSET) {
                var m: i32 = self.nodes[u].a;
                while (m >= 0) {
                    var mu: usize = @as(usize, m);
                    var unused: i64 = self.er_intern(a, self.nodes[mu].xoff, self.nodes[mu].xlen);
                    if (unused == 0) { }
                    m = self.nodes[mu].next;
                }
            }
            cur = self.nodes[u].next;
        }
    }

    /// Sema pass 0 (v0.171): register every enum in item order — variants
    /// resolve their integer values with the C auto-increment rule (an
    /// explicit `= N` sets the running counter to `N` and is used; a bare
    /// variant takes the counter; after each, the counter is the used
    /// value plus one, wrapping as i64). A duplicate variant name (E0211,
    /// sema-invalid) still advances the counter but records nothing —
    /// replayed exactly for totality.
    fn en_collect(self: *Self, a: Allocator) void {
        var cur: i32 = self.root;
        while (cur >= 0) {
            var u: usize = @as(usize, cur);
            if (self.nodes[u].kind == ND_ENUM) {
                if (self.en_count == self.en_name_off.len) {
                    var g0: []i64 = alloc(a, i64, self.en_name_off.len * 2);
                    var g1: []i64 = alloc(a, i64, self.en_name_len.len * 2);
                    var g2: []i64 = alloc(a, i64, self.en_v_start.len * 2);
                    var g3: []i64 = alloc(a, i64, self.en_v_count.len * 2);
                    var i0: usize = 0;
                    while (i0 < self.en_count) : (i0 += 1) {
                        g0[i0] = self.en_name_off[i0];
                        g1[i0] = self.en_name_len[i0];
                        g2[i0] = self.en_v_start[i0];
                        g3[i0] = self.en_v_count[i0];
                    }
                    free(a, self.en_name_off);
                    free(a, self.en_name_len);
                    free(a, self.en_v_start);
                    free(a, self.en_v_count);
                    self.en_name_off = g0;
                    self.en_name_len = g1;
                    self.en_v_start = g2;
                    self.en_v_count = g3;
                }
                self.en_name_off[self.en_count] = @as(i64, self.nodes[u].xoff);
                self.en_name_len[self.en_count] = @as(i64, self.nodes[u].xlen);
                self.en_v_start[self.en_count] = @as(i64, self.ev_count);
                var nvar: i64 = 0;
                var counter: i64 = 0;
                var vcur: i32 = self.nodes[u].a;
                while (vcur >= 0) {
                    var vu: usize = @as(usize, vcur);
                    var used: i64 = counter;
                    if ((self.nodes[vu].flags & F_VAL) != 0) {
                        used = self.nodes[vu].val;
                    }
                    counter = used + 1;
                    // Duplicate check against the variants already recorded
                    // for THIS enum (sema's per-enum seen-set).
                    var vstart: usize = @as(usize, self.en_v_start[self.en_count]);
                    var dup: bool = false;
                    var dj: usize = vstart;
                    while (dj < self.ev_count) : (dj += 1) {
                        var doff: usize = @as(usize, self.ev_name_off[dj]);
                        var dlen: usize = @as(usize, self.ev_name_len[dj]);
                        if (str_eq(self.src[doff .. doff + dlen], self.src[self.nodes[vu].xoff .. self.nodes[vu].xoff + self.nodes[vu].xlen])) {
                            dup = true;
                        }
                    }
                    if (!dup) {
                        if (self.ev_count == self.ev_name_off.len) {
                            var h0: []i64 = alloc(a, i64, self.ev_name_off.len * 2);
                            var h1: []i64 = alloc(a, i64, self.ev_name_len.len * 2);
                            var h2: []i64 = alloc(a, i64, self.ev_val.len * 2);
                            var j0: usize = 0;
                            while (j0 < self.ev_count) : (j0 += 1) {
                                h0[j0] = self.ev_name_off[j0];
                                h1[j0] = self.ev_name_len[j0];
                                h2[j0] = self.ev_val[j0];
                            }
                            free(a, self.ev_name_off);
                            free(a, self.ev_name_len);
                            free(a, self.ev_val);
                            self.ev_name_off = h0;
                            self.ev_name_len = h1;
                            self.ev_val = h2;
                        }
                        self.ev_name_off[self.ev_count] = @as(i64, self.nodes[vu].xoff);
                        self.ev_name_len[self.ev_count] = @as(i64, self.nodes[vu].xlen);
                        self.ev_val[self.ev_count] = used;
                        self.ev_count += 1;
                        nvar += 1;
                    }
                    vcur = self.nodes[vu].next;
                }
                self.en_v_count[self.en_count] = nvar;
                self.en_count += 1;
            }
            cur = self.nodes[u].next;
        }
    }

    // -- raw output -----------------------------------------------------------

    fn putc(self: *Self, a: Allocator, b: u8) void {
        if (self.out_len == self.out.len) {
            var grown: []u8 = alloc(a, u8, self.out.len * 2);
            var i: usize = 0;
            while (i < self.out_len) : (i += 1) { grown[i] = self.out[i]; }
            free(a, self.out);
            self.out = grown;
        }
        self.out[self.out_len] = b;
        self.out_len += 1;
    }

    fn put(self: *Self, a: Allocator, s: []u8) void {
        var i: usize = 0;
        while (i < s.len) : (i += 1) { self.putc(a, s[i]); }
    }

    /// `Emitter::line`: indentation, the text, a newline.
    fn line(self: *Self, a: Allocator, s: []u8) void {
        var i: i64 = 0;
        while (i < self.indent) : (i += 1) { self.put(a, "    "); }
        self.put(a, s);
        self.putc(a, 10);
    }

    /// `Emitter::blank`: one bare newline.
    fn blank(self: *Self, a: Allocator) void {
        self.putc(a, 10);
    }

    // -- name/text helpers ------------------------------------------------------

    /// The primary name text of node `n` (its `x` span).
    fn xname(self: *Self, n: i32) []u8 {
        var u: usize = @as(usize, n);
        return self.src[self.nodes[u].xoff .. self.nodes[u].xoff + self.nodes[u].xlen];
    }

    // -- stack growth -----------------------------------------------------------

    /// The label to attach to the NEXT pushed loop scope (set by the
    /// labeled-`while` arm just before `emit_block`, consumed by
    /// `push_scope`; `emit_for` sets its scope's label directly).
    fn set_pending_label(self: *Self, off: usize, len: usize) void {
        self.pend_loff = off;
        self.pend_llen = len;
    }

    fn push_scope(self: *Self, a: Allocator, is_loop: bool, cont: i32, raw_fi: i64) void {
        if (self.sc_len == self.scopes.len) {
            var grown: []EmScope = alloc(a, EmScope, self.scopes.len * 2);
            var i: usize = 0;
            while (i < self.sc_len) : (i += 1) { grown[i] = self.scopes[i]; }
            free(a, self.scopes);
            self.scopes = grown;
        }
        self.scopes[self.sc_len] = EmScope{
            .is_loop = is_loop,
            .cont = cont,
            .raw_fi = raw_fi,
            .loff = self.pend_loff,
            .llen = self.pend_llen,
            .dstart = @as(i64, self.df_len),
            .vstart = @as(i64, self.vt_len),
        };
        self.pend_loff = 0;
        self.pend_llen = 0;
        self.sc_len += 1;
    }

    /// Pop the innermost scope, dropping its defers and locals.
    fn pop_scope(self: *Self) void {
        var top: usize = self.sc_len - 1;
        self.df_len = @as(usize, self.scopes[top].dstart);
        self.vt_len = @as(usize, self.scopes[top].vstart);
        self.sc_len -= 1;
    }

    fn push_defer(self: *Self, a: Allocator, n: i32, is_err: bool) void {
        if (self.df_len == self.defers.len) {
            var grown: []i32 = alloc(a, i32, self.defers.len * 2);
            var ge: []bool = alloc(a, bool, self.derr.len * 2);
            var i: usize = 0;
            while (i < self.df_len) : (i += 1) {
                grown[i] = self.defers[i];
                ge[i] = self.derr[i];
            }
            free(a, self.defers);
            free(a, self.derr);
            self.defers = grown;
            self.derr = ge;
        }
        self.defers[self.df_len] = n;
        self.derr[self.df_len] = is_err;
        self.df_len += 1;
    }

    fn push_vt(self: *Self, a: Allocator, off: usize, len: usize, ty: i64) void {
        if (self.vt_len == self.vts.len) {
            var grown: []VtEnt = alloc(a, VtEnt, self.vts.len * 2);
            var i: usize = 0;
            while (i < self.vt_len) : (i += 1) { grown[i] = self.vts[i]; }
            free(a, self.vts);
            self.vts = grown;
        }
        self.vts[self.vt_len] = VtEnt{ .off = off, .len = len, .ty = ty };
        self.vt_len += 1;
    }

    fn push_fn(self: *Self, a: Allocator, off: usize, len: usize, ret: i64, node: i32, pstart: i64, pcount: i64) void {
        if (self.fn_len == self.fns.len) {
            var grown: []FnSig = alloc(a, FnSig, self.fns.len * 2);
            var i: usize = 0;
            while (i < self.fn_len) : (i += 1) { grown[i] = self.fns[i]; }
            free(a, self.fns);
            self.fns = grown;
        }
        self.fns[self.fn_len] = FnSig{ .off = off, .len = len, .ret = ret, .node = node, .live = false, .pstart = pstart, .pcount = pcount };
        self.fn_len += 1;
    }

    /// Append one parameter ET code to the flat `fp_ty` table.
    fn push_fp(self: *Self, a: Allocator, t: i64) void {
        if (self.fp_count == self.fp_ty.len) {
            var grown: []i64 = alloc(a, i64, self.fp_ty.len * 2);
            var i: usize = 0;
            while (i < self.fp_count) : (i += 1) { grown[i] = self.fp_ty[i]; }
            free(a, self.fp_ty);
            self.fp_ty = grown;
        }
        self.fp_ty[self.fp_count] = t;
        self.fp_count += 1;
    }

    fn push_const(self: *Self, a: Allocator, off: usize, len: usize, isb: bool, val: i64) void {
        if (self.ct_len == self.consts.len) {
            var grown: []CEnt = alloc(a, CEnt, self.consts.len * 2);
            var i: usize = 0;
            while (i < self.ct_len) : (i += 1) { grown[i] = self.consts[i]; }
            free(a, self.consts);
            self.consts = grown;
        }
        self.consts[self.ct_len] = CEnt{ .off = off, .len = len, .isb = isb, .val = val };
        self.ct_len += 1;
    }

    // -- lookups ------------------------------------------------------------------

    /// `Emitter::lookup_var_type`: innermost binding of `name` wins.
    fn vt_lookup(self: *Self, name: []u8) i64 {
        var i: i64 = @as(i64, self.vt_len) - 1;
        while (i >= 0) : (i -= 1) {
            var u: usize = @as(usize, i);
            var ent: []u8 = self.src[self.vts[u].off .. self.vts[u].off + self.vts[u].len];
            if (str_eq(ent, name)) { return self.vts[u].ty; }
        }
        return ET_NONE;
    }

    /// The row index of the top-level `fn` named `name`, or -1 (backs the
    /// positional `fn_params` lookups).
    fn fn_row_of(self: *Self, name: []u8) i64 {
        var i: usize = 0;
        while (i < self.fn_len) : (i += 1) {
            var ent: []u8 = self.src[self.fns[i].off .. self.fns[i].off + self.fns[i].len];
            if (str_eq(ent, name)) { return @as(i64, i); }
        }
        return 0 - 1;
    }

    /// The collected return type of the top-level `fn` named `name`, or
    /// `ET_NONE` (mirrors an `fn_ret` map miss).
    fn fn_ret_of(self: *Self, name: []u8) i64 {
        var i: usize = 0;
        while (i < self.fn_len) : (i += 1) {
            var ent: []u8 = self.src[self.fns[i].off .. self.fns[i].off + self.fns[i].len];
            if (str_eq(ent, name)) { return self.fns[i].ret; }
        }
        return ET_NONE;
    }

    /// The folded constant named `name`: `ok = false` mirrors an unknown /
    /// not-yet-folded const (`E0131`).
    fn const_lookup(self: *Self, name: []u8) EvRes {
        var i: usize = 0;
        while (i < self.ct_len) : (i += 1) {
            var ent: []u8 = self.src[self.consts[i].off .. self.consts[i].off + self.consts[i].len];
            if (str_eq(ent, name)) {
                return EvRes{ .ok = true, .isb = self.consts[i].isb, .val = self.consts[i].val };
            }
        }
        return ev_err();
    }

    // -- type resolution -----------------------------------------------------------

    /// `Emitter::resolve_ty`: a slice form maps to the interned `[]T` for
    /// its element (sema interns every written slice); an unresolvable
    /// element mirrors the Rust `unwrap_or(base)` fallback (base = `Void`).
    /// A bare name goes through `from_name`, else the `Void` fallback
    /// (struct/enum/... paths are empty in the subset).
    fn resolve_ty(self: *Self, n: i32) i64 {
        var u: usize = @as(usize, n);
        if ((self.nodes[u].flags & F_ARRLIT) != 0) {
            // `[N]T` maps back to the interned array (the scan interned
            // every written one); the miss fallback mirrors the Rust
            // `unwrap_or(base)` (base = the element, `Void` if unknown).
            var ae: i64 = self.base_code(self.xname(n));
            if (ae == ET_NONE) { return ET_VOID; }
            var alen: i64 = self.nodes[u].val;
            var i: usize = 0;
            while (i < self.ar_count) : (i += 1) {
                if (self.ar_elem[i] == ae and self.ar_len_[i] == alen) {
                    return ET_ARR_BASE + @as(i64, i);
                }
            }
            return ae;
        }
        if ((self.nodes[u].flags & F_SLICE) != 0) {
            var e: i64 = self.base_code(self.xname(n));
            if (e == ET_NONE) { return ET_VOID; }
            return et_slice_of(e);
        }
        if ((self.nodes[u].flags & F_OPT) != 0) {
            // `?T` maps back to the interned optional (sema interned every
            // written one); the miss mirrors the Rust `unwrap_or(base)`.
            var oe: i64 = self.base_code(self.xname(n));
            if (oe == ET_NONE) { return ET_VOID; }
            var oi: usize = 0;
            while (oi < self.opt_count) : (oi += 1) {
                if (self.opt_inners[oi] == oe) { return ET_OPT_BASE + @as(i64, oi); }
            }
            return oe;
        }
        if ((self.nodes[u].flags & F_ERR) != 0) {
            // `!T` / `Set!T` maps back to the interned error union (the set
            // name is sema's membership concern; the runtime type is the
            // payload's union either way).
            var ee: i64 = self.base_code(self.xname(n));
            if (ee == ET_NONE) { return ET_VOID; }
            var ei: usize = 0;
            while (ei < self.eu_count) : (ei += 1) {
                if (self.eu_payloads[ei] == ee) { return ET_ERRU_BASE + @as(i64, ei); }
            }
            return ee;
        }
        if ((self.nodes[u].flags & F_PTR) != 0) {
            // `*T` maps through the pre-pass registry; the miss falls to
            // the FIRST registry slot (the Rust `unwrap_or(PTR_LOCAL_BASE)`
            // arm — index 0 regardless of pointee).
            var pe: i64 = self.base_code(self.xname(n));
            if (pe == ET_NONE) { pe = ET_VOID; }
            var pi: usize = 0;
            while (pi < self.pt_count) : (pi += 1) {
                if (self.pt_pointees[pi] == pe and self.pt_local[pi]) {
                    return ET_PTR_BASE + @as(i64, pi);
                }
            }
            return ET_PTR_BASE;
        }
        var t: i64 = self.base_code(self.xname(n));
        if (t == ET_NONE) { return ET_VOID; }
        return t;
    }

    /// `Emitter::cty`: a slice form spells `kd_slice_<type_mangle(elem)>`
    /// directly (an unresolvable element falls back through the base's
    /// `int64_t`, mirroring the Rust cty base fallback); a bare name goes
    /// through `from_name`, else the `int64_t` fallback.
    fn cty(self: *Self, a: Allocator, n: i32) []u8 {
        var u: usize = @as(usize, n);
        if ((self.nodes[u].flags & F_ARRLIT) != 0) {
            // `kd_arr_<type_mangle(base)>_<N>` spelled directly; an
            // unresolvable element goes through the cty base fallback
            // (`int64_t`), mirroring the Rust arm.
            var ae: i64 = self.base_code(self.xname(n));
            var tag: []u8 = "int64_t";
            if (ae != ET_NONE) { tag = self.mangle_of(a, ae); }
            var sb: StrBuilder = StrBuilder.init(a);
            sb.append(a, "kd_arr_");
            sb.append(a, tag);
            sb.append(a, "_");
            sb.append_i64(a, self.nodes[u].val);
            var s: []u8 = sb.build(a);
            sb.deinit(a);
            return s;
        }
        if ((self.nodes[u].flags & F_SLICE) != 0) {
            var e: i64 = self.base_code(self.xname(n));
            if (e == ET_NONE) { return "kd_slice_int64_t"; }
            return self.sl_c_name(a, et_slice_of(e));
        }
        if ((self.nodes[u].flags & F_OPT) != 0) {
            // `kd_opt_<type_mangle(base)>` spelled directly (the Rust cty
            // optional arm); an unresolvable base falls to `int64_t`.
            var oe: i64 = self.base_code(self.xname(n));
            if (oe == ET_NONE) { return "kd_opt_int64_t"; }
            var sbo: StrBuilder = StrBuilder.init(a);
            sbo.append(a, "kd_opt_");
            sbo.append(a, self.mangle_of(a, oe));
            var so: []u8 = sbo.build(a);
            sbo.deinit(a);
            return so;
        }
        if ((self.nodes[u].flags & F_ERR) != 0) {
            // `kd_err_<type_mangle(base)>` spelled directly.
            var ee: i64 = self.base_code(self.xname(n));
            if (ee == ET_NONE) { return "kd_err_int64_t"; }
            var sbe: StrBuilder = StrBuilder.init(a);
            sbe.append(a, "kd_err_");
            sbe.append(a, self.mangle_of(a, ee));
            var se: []u8 = sbe.build(a);
            sbe.deinit(a);
            return se;
        }
        if ((self.nodes[u].flags & F_PTR) != 0) {
            // `<pointee cty>*` spelled structurally (no typedef, no id).
            var pe2: i64 = self.base_code(self.xname(n));
            var ptag: []u8 = "int64_t";
            if (pe2 != ET_NONE) { ptag = self.cty_of(a, pe2); }
            var sbp2: StrBuilder = StrBuilder.init(a);
            sbp2.append(a, ptag);
            sbp2.append(a, "*");
            var sp2: []u8 = sbp2.build(a);
            sbp2.deinit(a);
            return sp2;
        }
        var t: i64 = self.base_code(self.xname(n));
        if (t == ET_NONE) { return "int64_t"; }
        return self.cty_of(a, t);
    }

    // -- const evaluation -------------------------------------------------------------

    /// `const_eval::eval` over the arena (see the module header for the
    /// wrapping-arithmetic contract).
    fn eval(self: *Self, n: i32) EvRes {
        if (n < 0) { return ev_err(); }
        var u: usize = @as(usize, n);
        var k: u8 = self.nodes[u].kind;
        if (k == ND_INT) { return ev_int(self.nodes[u].val); }
        if (k == ND_BOOL) { return ev_bool(self.nodes[u].val); }
        if (k == ND_IDENT) { return self.const_lookup(self.xname(n)); }
        if (k == ND_COMPTIME) { return self.eval(self.nodes[u].a); }
        if (k == ND_UNARY) {
            var v: EvRes = self.eval(self.nodes[u].a);
            if (!v.ok) { return v; }
            var op: i64 = self.nodes[u].val;
            if (op == UOP_NEG) {
                if (v.isb) { return ev_err(); }
                if (v.val == ev_i64_min()) { return ev_int(v.val); }
                return ev_int(0 - v.val);
            }
            if (op == UOP_NOT) {
                if (!v.isb) { return ev_err(); }
                if (v.val == 0) { return ev_bool(1); }
                return ev_bool(0);
            }
            // UOP_BNOT
            if (v.isb) { return ev_err(); }
            return ev_int(~v.val);
        }
        if (k == ND_BIN) {
            var l: EvRes = self.eval(self.nodes[u].a);
            if (!l.ok) { return l; }
            var r: EvRes = self.eval(self.nodes[u].b);
            if (!r.ok) { return r; }
            return self.eval_binary(self.nodes[u].val, l, r);
        }
        // Calls and every other shape are not compile-time constants.
        return ev_err();
    }

    fn eval_binary(self: *Self, op: i64, l: EvRes, r: EvRes) EvRes {
        if (op == OPC_ADD or op == OPC_SUB or op == OPC_MUL or op == OPC_DIV or op == OPC_REM) {
            if (l.isb or r.isb) { return ev_err(); }
            if (op == OPC_ADD) { return ev_int(l.val + r.val); }
            if (op == OPC_SUB) { return ev_int(l.val - r.val); }
            if (op == OPC_MUL) { return ev_int(l.val * r.val); }
            if (r.val == 0) { return ev_err(); }
            // The lone case where Rust's wrapping division diverges from C.
            if (l.val == ev_i64_min() and r.val == 0 - 1) {
                if (op == OPC_DIV) { return ev_int(l.val); }
                return ev_int(0);
            }
            if (op == OPC_DIV) { return ev_int(l.val / r.val); }
            return ev_int(l.val % r.val);
        }
        if (op == OPC_EQ or op == OPC_NE) {
            if (l.isb != r.isb) { return ev_err(); }
            var eq: bool = l.val == r.val;
            if (op == OPC_NE) { eq = !eq; }
            if (eq) { return ev_bool(1); }
            return ev_bool(0);
        }
        if (op == OPC_LT or op == OPC_LE or op == OPC_GT or op == OPC_GE) {
            // Bools compare as 0/1 integers, mirroring `ConstVal::Bool as i64`.
            if (l.isb != r.isb) { return ev_err(); }
            var v: bool = false;
            if (op == OPC_LT) { v = l.val < r.val; }
            if (op == OPC_LE) { v = l.val <= r.val; }
            if (op == OPC_GT) { v = l.val > r.val; }
            if (op == OPC_GE) { v = l.val >= r.val; }
            if (v) { return ev_bool(1); }
            return ev_bool(0);
        }
        if (op == OPC_AND or op == OPC_OR) {
            if (!l.isb or !r.isb) { return ev_err(); }
            var b: bool = false;
            if (op == OPC_AND) { b = l.val != 0 and r.val != 0; }
            if (op == OPC_OR) { b = l.val != 0 or r.val != 0; }
            if (b) { return ev_bool(1); }
            return ev_bool(0);
        }
        // Bitwise / shifts.
        if (l.isb or r.isb) { return ev_err(); }
        if (op == OPC_BAND) { return ev_int(l.val & r.val); }
        if (op == OPC_BOR) { return ev_int(l.val | r.val); }
        if (op == OPC_BXOR) { return ev_int(l.val ^ r.val); }
        // Shift amounts mask to 0..63, mirroring `wrapping_shl`/`wrapping_shr`.
        if (op == OPC_SHL) { return ev_int(l.val << (r.val & 63)); }
        return ev_int(l.val >> (r.val & 63));
    }

    /// `promotes_in_c` truncate-back: `(({cty}){s})` (§28.2).
    fn trunc_back(self: *Self, a: Allocator, t: i64, s: []u8) []u8 {
        var sb: StrBuilder = StrBuilder.init(a);
        sb.append(a, "((");
        sb.append(a, et_c_name(t));
        sb.append(a, ")");
        sb.append(a, s);
        sb.append(a, ")");
        var r: []u8 = sb.build(a);
        sb.deinit(a);
        return r;
    }

    /// `const_literal`: a folded value as C source.
    fn const_literal(self: *Self, a: Allocator, v: EvRes) []u8 {
        if (v.isb) {
            if (v.val != 0) { return "true"; }
            return "false";
        }
        var sb: StrBuilder = StrBuilder.init(a);
        sb.append_i64(a, v.val);
        var s: []u8 = sb.build(a);
        sb.deinit(a);
        return s;
    }

    // -- type_of_expr ------------------------------------------------------------------

    /// `Emitter::type_of_expr` over the subset: the best-effort static type,
    /// `ET_NONE` for "cannot be determined" — including the mirrored quirk
    /// that a top-level const name is NOT resolvable here (only locals and
    /// params are), so an initializer referencing one infers `i64`.
    /// Whether `e` types as a `*Struct` (the field/method auto-deref
    /// gate, SPEC §30.1).
    fn is_ptr_to_struct(self: *Self, n: i32) bool {
        var t: i64 = self.type_of_expr(n);
        if (!et_is_ptr(t)) { return false; }
        return et_is_struct(self.pt_pointee_of(t));
    }

    fn type_of_expr(self: *Self, n: i32) i64 {
        if (n < 0) { return ET_NONE; }
        var u: usize = @as(usize, n);
        var k: u8 = self.nodes[u].kind;
        if (k == ND_INT) { return ET_I64; }
        if (k == ND_BOOL) { return ET_BOOL; }
        if (k == ND_STR) { return ET_SLICE_U8; }
        if (k == ND_IDENT) { return self.vt_lookup(self.xname(n)); }
        if (k == ND_UNARY) {
            if (self.nodes[u].val == UOP_NOT) { return ET_BOOL; }
            return self.type_of_expr(self.nodes[u].a);
        }
        if (k == ND_BIN) {
            if (es_is_bool_result(self.nodes[u].val)) { return ET_BOOL; }
            return self.type_of_expr(self.nodes[u].a);
        }
        if (k == ND_CALL) {
            // The allocator builtins have synthetic result types (SPEC §16),
            // checked BEFORE the collected signatures exactly as in Rust.
            var callee: []u8 = self.xname(n);
            if (str_eq(callee, "c_allocator")) { return ET_ALLOC; }
            if (str_eq(callee, "alloc")) {
                // `alloc(a, T, n)` is `[]T`, resolved from the type-name
                // identifier (arg 1) — the alloc call itself makes sema
                // intern that slice, so a subset element always resolves;
                // other shapes mirror the Rust `None` outcomes (a non-ident
                // arg, or an element with no interned slice).
                var a0: i32 = self.nodes[u].a;
                var a1: i32 = 0 - 1;
                if (a0 >= 0) { a1 = self.nodes[@as(usize, a0)].next; }
                if (a1 >= 0 and self.nodes[@as(usize, a1)].kind == ND_IDENT) {
                    var e: i64 = et_from_name(self.xname(a1));
                    if (et_is_slice_elem(e)) { return et_slice_of(e); }
                }
                return ET_NONE;
            }
            return self.fn_ret_of(callee);
        }
        if (k == ND_COMPTIME) { return self.type_of_expr(self.nodes[u].a); }
        if (k == ND_BUILTIN) {
            // `@as(T, e)` has type `T` (the cast target, SPEC §33): the
            // first argument names it; an unresolvable name mirrors the
            // Rust `base_type` fallback (`Void`), a non-identifier the
            // `None` arm. Every other builtin is out of the subset (their
            // sema-invalid remnants report no type).
            if (str_eq(self.xname(n), "as")) {
                var b0: i32 = self.nodes[u].a;
                if (b0 >= 0 and self.nodes[@as(usize, b0)].kind == ND_IDENT) {
                    var t2: i64 = et_from_name(self.xname(b0));
                    if (t2 == ET_NONE) { return ET_VOID; }
                    return t2;
                }
            }
            if (str_eq(self.xname(n), "intFromEnum")) { return ET_I64; }
            if (str_eq(self.xname(n), "enumFromInt")) {
                // The enum type named by the first argument (`base_type`
                // fallback = the Rust `None` arm).
                var eb0: i32 = self.nodes[u].a;
                if (eb0 >= 0 and self.nodes[@as(usize, eb0)].kind == ND_IDENT) {
                    return self.base_code(self.xname(eb0));
                }
                return ET_NONE;
            }
            return ET_NONE;
        }
        if (k == ND_FIELD) {
            // A qualified enum literal `Enum.V` (the base Ident names an
            // enum) has that enum's type — checked FIRST, like Rust. A
            // struct base yields the named field's type (v0.169); `.len`
            // is a `usize` on arrays (a compile-time constant) and slices
            // alike; anything else is untypeable here. The struct arm
            // precedes the `.len` arms exactly as in Rust — a struct field
            // named `len` is an ordinary member.
            var fb0: i32 = self.nodes[u].a;
            if (fb0 >= 0 and self.nodes[@as(usize, fb0)].kind == ND_IDENT) {
                var fec0: i64 = self.en_code_of(self.xname(fb0));
                if (fec0 != ET_NONE) { return fec0; }
            }
            var bt: i64 = self.type_of_expr(self.nodes[u].a);
            if (et_is_struct(bt)) {
                var ft: i64 = self.st_field_ty(bt, self.xname(n));
                if (ft == ET_NONE) { return ET_NONE; }
                return ft;
            }
            if (et_is_ptr(bt) and et_is_struct(self.pt_pointee_of(bt))) {
                // `p.field` on a `*Struct` auto-derefs (SPEC §30.1).
                var ft2: i64 = self.st_field_ty(self.pt_pointee_of(bt), self.xname(n));
                if (ft2 == ET_NONE) { return ET_NONE; }
                return ft2;
            }
            if (str_eq(self.xname(n), "len")) {
                if (et_is_arr(bt) or et_is_slice(bt)) { return ET_USIZE; }
            }
            return ET_NONE;
        }
        if (k == ND_SLIT) {
            // `Name{ … }` has the named struct's type (an unknown name is
            // sema's E0163 — untypeable here).
            return self.st_code_of(self.xname(n));
        }
        if (k == ND_NULL) { return ET_NONE; }
        if (k == ND_ERRLIT) { return ET_NONE; }
        if (k == ND_ADDROF) {
            // `&place` is `*T` — but ONLY when the pointee was registered
            // by the written-`*T` pre-pass (the Rust local-registry
            // position lookup; a miss is untypeable).
            var apt: i64 = self.type_of_expr(self.nodes[u].a);
            if (apt == ET_NONE) { return ET_NONE; }
            return self.pt_local_code(apt);
        }
        if (k == ND_DEREF) {
            var dpt: i64 = self.type_of_expr(self.nodes[u].a);
            if (et_is_ptr(dpt)) { return self.pt_pointee_of(dpt); }
            return dpt;
        }
        if (k == ND_TRY) {
            var tt: i64 = self.type_of_expr(self.nodes[u].a);
            if (et_is_erru(tt)) { return self.eu_payload_of(tt); }
            return tt;
        }
        if (k == ND_CATCH) {
            var ct: i64 = self.type_of_expr(self.nodes[u].a);
            if (et_is_erru(ct)) { return self.eu_payload_of(ct); }
            return ct;
        }
        if (k == ND_ORELSE) {
            // `x orelse y` yields the inner `T` of the `?T` lhs.
            var olt: i64 = self.type_of_expr(self.nodes[u].a);
            if (et_is_opt(olt)) { return self.opt_inner_of(olt); }
            return ET_NONE;
        }
        if (k == ND_UNWRAP) {
            var ut: i64 = self.type_of_expr(self.nodes[u].a);
            if (et_is_opt(ut)) { return self.opt_inner_of(ut); }
            return ET_NONE;
        }
        if (k == ND_MCALL) {
            // A method call's type is the invoked struct function's
            // recorded return: a type-name receiver resolves through the
            // struct table FIRST, else the receiver expression's struct.
            var rn: i32 = self.nodes[u].a;
            var rsid: i64 = ET_NONE;
            if (rn >= 0 and self.nodes[@as(usize, rn)].kind == ND_IDENT) {
                rsid = self.st_code_of(self.xname(rn));
            }
            if (rsid == ET_NONE) {
                var rt: i64 = self.type_of_expr(rn);
                if (et_is_struct(rt)) { rsid = rt; }
                if (et_is_ptr(rt) and et_is_struct(self.pt_pointee_of(rt))) {
                    rsid = self.pt_pointee_of(rt);
                }
            }
            if (rsid == ET_NONE) { return ET_NONE; }
            return self.mt_ret_of(rsid, self.xname(n));
        }
        if (k == ND_INDEX) {
            // `a[i]` / `s[i]` yields the element type.
            var bt2: i64 = self.type_of_expr(self.nodes[u].a);
            if (et_is_arr(bt2)) { return self.arr_elem_of(bt2); }
            if (et_is_slice(bt2)) { return et_slice_elem(bt2); }
            return ET_NONE;
        }
        if (k == ND_SLICEX) {
            // `base[lo..hi]` yields `[]T` over the base's element — the
            // base's own slice type for a slice base, the interned
            // `[]elem` for an array base (v0.168).
            var bt3: i64 = self.type_of_expr(self.nodes[u].a);
            if (et_is_arr(bt3)) { return et_slice_of(self.arr_elem_of(bt3)); }
            if (et_is_slice(bt3)) { return bt3; }
            return ET_NONE;
        }
        if (k == ND_ALIT) {
            // `[N]T{ … }` has the array type of its full `elem` reference.
            var at: i64 = self.resolve_ty(self.nodes[u].a);
            if (et_is_arr(at)) { return at; }
            return ET_NONE;
        }
        return ET_NONE;
    }

    // -- expressions --------------------------------------------------------------------

    /// `Emitter::emit_coerced` over the subset (v0.172): the ONLY
    /// non-identity coercion is an unqualified enum literal `.V` against
    /// an expected enum — it lowers to that enum's C enumerator. (The
    /// optional/error-union widenings stay out of the subset.)
    fn emit_coerced(self: *Self, a: Allocator, n: i32, expected: i64) []u8 {
        if (n >= 0 and et_is_enum(expected) and self.nodes[@as(usize, n)].kind == ND_ENUMLIT) {
            var sb: StrBuilder = StrBuilder.init(a);
            sb.append(a, self.en_c_name(a, expected));
            sb.append(a, "_");
            sb.append(a, self.xname(n));
            var out: []u8 = sb.build(a);
            sb.deinit(a);
            return out;
        }
        if (n >= 0 and et_is_opt(expected)) {
            // The `?T` widenings (v0.173, SPEC §11.2): `null` → the empty
            // optional; an already-optional value passes through; a `T`
            // value wraps `{ .has = true, .val = <e> }`.
            var oname: []u8 = self.opt_c_name(a, expected);
            if (self.nodes[@as(usize, n)].kind == ND_NULL) {
                var sb1: StrBuilder = StrBuilder.init(a);
                sb1.append(a, "((");
                sb1.append(a, oname);
                sb1.append(a, "){ .has = false })");
                var o1: []u8 = sb1.build(a);
                sb1.deinit(a);
                return o1;
            }
            if (et_is_opt(self.type_of_expr(n))) {
                return self.emit_expr(a, n);
            }
            var inner: []u8 = self.emit_expr(a, n);
            var sb2: StrBuilder = StrBuilder.init(a);
            sb2.append(a, "((");
            sb2.append(a, oname);
            sb2.append(a, "){ .has = true, .val = ");
            sb2.append(a, inner);
            sb2.append(a, " })");
            var o2: []u8 = sb2.build(a);
            sb2.deinit(a);
            return o2;
        }
        if (n >= 0 and et_is_erru(expected)) {
            // The `!T` widenings (v0.174, SPEC §12.2): `error.X` → the
            // failure value carrying its 1-based code; an already-union
            // value passes through; a `!void` target evaluates the void
            // source then constructs the payload-less success via a comma
            // expression; else the success wrap.
            var ename: []u8 = self.eu_c_name(a, expected);
            if (self.nodes[@as(usize, n)].kind == ND_ERRLIT) {
                var sb3: StrBuilder = StrBuilder.init(a);
                sb3.append(a, "((");
                sb3.append(a, ename);
                sb3.append(a, "){ .err = ");
                sb3.append_i64(a, self.er_code_of(self.xname(n)));
                sb3.append(a, " })");
                var o3: []u8 = sb3.build(a);
                sb3.deinit(a);
                return o3;
            }
            if (et_is_erru(self.type_of_expr(n))) {
                return self.emit_expr(a, n);
            }
            if (self.eu_payload_of(expected) == ET_VOID) {
                var vsrc: []u8 = self.emit_expr(a, n);
                var sb4: StrBuilder = StrBuilder.init(a);
                sb4.append(a, "((");
                sb4.append(a, vsrc);
                sb4.append(a, "), ((");
                sb4.append(a, ename);
                sb4.append(a, "){ .err = 0 }))");
                var o4: []u8 = sb4.build(a);
                sb4.deinit(a);
                return o4;
            }
            var succ: []u8 = self.emit_expr(a, n);
            var sb5: StrBuilder = StrBuilder.init(a);
            sb5.append(a, "((");
            sb5.append(a, ename);
            sb5.append(a, "){ .err = 0, .val = ");
            sb5.append(a, succ);
            sb5.append(a, " })");
            var o5: []u8 = sb5.build(a);
            sb5.deinit(a);
            return o5;
        }
        return self.emit_expr(a, n);
    }

    /// `Emitter::coerce_str`: the string-level widening for an
    /// ALREADY-EMITTED payload (the `try` positions). Optional and
    /// error-union targets wrap; everything else passes through.
    fn coerce_str(self: *Self, a: Allocator, raw: []u8, src: i64, expected: i64) []u8 {
        if (et_is_opt(expected)) {
            if (et_is_opt(src)) { return raw; }
            var sb: StrBuilder = StrBuilder.init(a);
            sb.append(a, "((");
            sb.append(a, self.opt_c_name(a, expected));
            sb.append(a, "){ .has = true, .val = ");
            sb.append(a, raw);
            sb.append(a, " })");
            var o: []u8 = sb.build(a);
            sb.deinit(a);
            return o;
        }
        if (et_is_erru(expected)) {
            if (et_is_erru(src)) { return raw; }
            var ename: []u8 = self.eu_c_name(a, expected);
            if (self.eu_payload_of(expected) == ET_VOID) {
                var sb2: StrBuilder = StrBuilder.init(a);
                sb2.append(a, "((");
                sb2.append(a, raw);
                sb2.append(a, "), ((");
                sb2.append(a, ename);
                sb2.append(a, "){ .err = 0 }))");
                var o2: []u8 = sb2.build(a);
                sb2.deinit(a);
                return o2;
            }
            var sb3: StrBuilder = StrBuilder.init(a);
            sb3.append(a, "((");
            sb3.append(a, ename);
            sb3.append(a, "){ .err = 0, .val = ");
            sb3.append(a, raw);
            sb3.append(a, " })");
            var o3: []u8 = sb3.build(a);
            sb3.deinit(a);
            return o3;
        }
        return raw;
    }

    /// `Emitter::emit_binop_operand`: an `.V` operand takes its enum from
    /// the SIBLING operand's type (`c == .Red`); anything else is plain.
    fn emit_binop_operand(self: *Self, a: Allocator, n: i32, sibling: i32) []u8 {
        if (n >= 0 and self.nodes[@as(usize, n)].kind == ND_ENUMLIT) {
            var st: i64 = self.type_of_expr(sibling);
            if (et_is_enum(st)) { return self.emit_coerced(a, n, st); }
        }
        return self.emit_expr(a, n);
    }

    /// `Emitter::emit_expr`: lower an expression to a C expression string.
    /// Scalar coercion (beyond the `.V` arm above) is the identity.
    fn emit_expr(self: *Self, a: Allocator, n: i32) []u8 {
        var u: usize = @as(usize, n);
        var k: u8 = self.nodes[u].kind;
        if (k == ND_INT) {
            var sb: StrBuilder = StrBuilder.init(a);
            sb.append_i64(a, self.nodes[u].val);
            var s: []u8 = sb.build(a);
            sb.deinit(a);
            return s;
        }
        if (k == ND_BOOL) {
            if (self.nodes[u].val != 0) { return "true"; }
            return "false";
        }
        if (k == ND_STR) {
            // A string literal is a `[]u8` over static bytes (SPEC §23.2):
            // a compound literal whose `.ptr` is the escaped C string and
            // whose `.len` is the DECODED byte count.
            var bytes: []u8 = es_decode_str(a, self.src, self.nodes[u].off, self.nodes[u].len);
            var lit: []u8 = es_c_string_literal(a, bytes);
            var sb: StrBuilder = StrBuilder.init(a);
            sb.append(a, "((kd_slice_uint8_t){ .ptr = (uint8_t *)");
            sb.append(a, lit);
            sb.append(a, ", .len = ");
            sb.append_i64(a, @as(i64, bytes.len));
            sb.append(a, " })");
            var s: []u8 = sb.build(a);
            sb.deinit(a);
            return s;
        }
        if (k == ND_ENUMLIT) {
            // A bare `.V` with no expected type is sema-invalid (E0215);
            // the Rust arm emits a harmless `0` placeholder.
            return "0";
        }
        if (k == ND_NULL) {
            // A bare `null` with no expected `?T` is sema-invalid (E0180);
            // the Rust arm emits a harmless `0` placeholder.
            return "0";
        }
        if (k == ND_ERRLIT) {
            // A bare `error.X` with no expected `!T` is sema-invalid
            // (E0193); the Rust arm emits the bare 1-based code.
            var sbel: StrBuilder = StrBuilder.init(a);
            sbel.append_i64(a, self.er_code_of(self.xname(n)));
            var sel: []u8 = sbel.build(a);
            sbel.deinit(a);
            return sel;
        }
        if (k == ND_TRY) {
            // A non-statement-position `try` is sema-invalid (E0191); the
            // hoisting statement lowering still runs for totality.
            return self.emit_try(a, self.nodes[u].a);
        }
        if (k == ND_CATCH) {
            // `!void` operands (either form) run the handler lazily as a
            // statement; the capturing form hoists; the eager form lowers
            // through the `_catch` helper with a coerced default.
            var cxt: i64 = self.type_of_expr(self.nodes[u].a);
            if (et_is_erru(cxt) and self.eu_payload_of(cxt) == ET_VOID) {
                return self.emit_catch_void(a, n);
            }
            if ((self.nodes[u].flags & F_CAP) != 0) {
                return self.emit_catch_capture(a, n);
            }
            var cl: []u8 = self.emit_expr(a, self.nodes[u].a);
            if (et_is_erru(cxt)) {
                var cr: []u8 = self.emit_coerced(a, self.nodes[u].b, self.eu_payload_of(cxt));
                var sbc: StrBuilder = StrBuilder.init(a);
                sbc.append(a, self.eu_c_name(a, cxt));
                sbc.append(a, "_catch(");
                sbc.append(a, cl);
                sbc.append(a, ", ");
                sbc.append(a, cr);
                sbc.append(a, ")");
                var sc: []u8 = sbc.build(a);
                sbc.deinit(a);
                return sc;
            }
            var sbc2: StrBuilder = StrBuilder.init(a);
            sbc2.append(a, "(");
            sbc2.append(a, cl);
            sbc2.append(a, ")");
            var sc2: []u8 = sbc2.build(a);
            sbc2.deinit(a);
            return sc2;
        }
        if (k == ND_ORELSE) {
            // `x orelse y` → `kd_opt_<tag>_orelse(<x>, <y>)` (`y` eager);
            // the non-optional lhs fallback mirrors the Rust `({l})` arm.
            var ol: []u8 = self.emit_expr(a, self.nodes[u].a);
            var orr: []u8 = self.emit_expr(a, self.nodes[u].b);
            var olt: i64 = self.type_of_expr(self.nodes[u].a);
            var sbo: StrBuilder = StrBuilder.init(a);
            if (et_is_opt(olt)) {
                sbo.append(a, self.opt_c_name(a, olt));
                sbo.append(a, "_orelse(");
                sbo.append(a, ol);
                sbo.append(a, ", ");
                sbo.append(a, orr);
                sbo.append(a, ")");
            } else {
                sbo.append(a, "(");
                sbo.append(a, ol);
                sbo.append(a, ")");
            }
            var so: []u8 = sbo.build(a);
            sbo.deinit(a);
            return so;
        }
        if (k == ND_UNWRAP) {
            // `x.?` → `kd_opt_<tag>_unwrap(<x>)` (panics + exit 101 on
            // null); the non-optional fallback mirrors `({x})`.
            var ui2: []u8 = self.emit_expr(a, self.nodes[u].a);
            var ut: i64 = self.type_of_expr(self.nodes[u].a);
            var sbu: StrBuilder = StrBuilder.init(a);
            if (et_is_opt(ut)) {
                sbu.append(a, self.opt_c_name(a, ut));
                sbu.append(a, "_unwrap(");
                sbu.append(a, ui2);
                sbu.append(a, ")");
            } else {
                sbu.append(a, "(");
                sbu.append(a, ui2);
                sbu.append(a, ")");
            }
            var su: []u8 = sbu.build(a);
            sbu.deinit(a);
            return su;
        }
        if (k == ND_IDENT) {
            var sb: StrBuilder = StrBuilder.init(a);
            sb.append(a, "kd_");
            sb.append(a, self.xname(n));
            var s: []u8 = sb.build(a);
            sb.deinit(a);
            return s;
        }
        if (k == ND_UNARY) {
            var inner: []u8 = self.emit_expr(a, self.nodes[u].a);
            var sb: StrBuilder = StrBuilder.init(a);
            sb.append(a, "(");
            var op: i64 = self.nodes[u].val;
            if (op == UOP_NEG) { sb.append(a, "-"); }
            if (op == UOP_NOT) { sb.append(a, "!"); }
            if (op == UOP_BNOT) { sb.append(a, "~"); }
            sb.append(a, inner);
            sb.append(a, ")");
            var s: []u8 = sb.build(a);
            sb.deinit(a);
            // §28.2: `~x` yields the operand's type; a narrow (`u8`) operand
            // would leak C's `int` promotion, so truncate back.
            if (op == UOP_BNOT) {
                var t: i64 = self.type_of_expr(self.nodes[u].a);
                if (et_promotes_in_c(t)) {
                    return self.trunc_back(a, t, s);
                }
            }
            return s;
        }
        if (k == ND_BIN) {
            var l: []u8 = self.emit_binop_operand(a, self.nodes[u].a, self.nodes[u].b);
            var r: []u8 = self.emit_binop_operand(a, self.nodes[u].b, self.nodes[u].a);
            var sb: StrBuilder = StrBuilder.init(a);
            sb.append(a, "(");
            sb.append(a, l);
            sb.append(a, " ");
            sb.append(a, es_c_op(self.nodes[u].val));
            sb.append(a, " ");
            sb.append(a, r);
            sb.append(a, ")");
            var s: []u8 = sb.build(a);
            sb.deinit(a);
            // §28.2: `x << n` yields `x`'s type; only `<<` can outgrow a
            // narrow operand, so only it truncates back.
            if (self.nodes[u].val == OPC_SHL) {
                var t: i64 = self.type_of_expr(self.nodes[u].a);
                if (et_promotes_in_c(t)) {
                    return self.trunc_back(a, t, s);
                }
            }
            return s;
        }
        if (k == ND_FIELD) {
            var fname: []u8 = self.xname(n);
            // A qualified enum literal `Enum.Variant` reuses the field
            // shape (its base is an Ident naming an enum) and lowers to
            // the C enumerator — checked BEFORE everything else, exactly
            // like the Rust arm.
            var fb: i32 = self.nodes[u].a;
            if (fb >= 0 and self.nodes[@as(usize, fb)].kind == ND_IDENT) {
                var fec: i64 = self.en_code_of(self.xname(fb));
                if (fec != ET_NONE) {
                    var sbe: StrBuilder = StrBuilder.init(a);
                    sbe.append(a, self.en_c_name(a, fec));
                    sbe.append(a, "_");
                    sbe.append(a, fname);
                    var se: []u8 = sbe.build(a);
                    sbe.deinit(a);
                    return se;
                }
            }
            if (str_eq(fname, "len")) {
                // `a.len` on an array → the compile-time length as a
                // `usize` constant (SPEC §14.3) — checked BEFORE the slice
                // arm, exactly as in Rust.
                var bta: i64 = self.type_of_expr(self.nodes[u].a);
                if (et_is_arr(bta)) {
                    var sba: StrBuilder = StrBuilder.init(a);
                    sba.append(a, "((uintptr_t)");
                    sba.append_i64(a, self.arr_len_of(bta));
                    sba.append(a, ")");
                    var saa: []u8 = sba.build(a);
                    sba.deinit(a);
                    return saa;
                }
                // `s.len` on a slice → the runtime `.len` field (SPEC §15.2).
                var bt: i64 = bta;
                if (et_is_slice(bt)) {
                    var b: []u8 = self.emit_expr(a, self.nodes[u].a);
                    var sb: StrBuilder = StrBuilder.init(a);
                    sb.append(a, "(");
                    sb.append(a, b);
                    sb.append(a, ").len");
                    var s: []u8 = sb.build(a);
                    sb.deinit(a);
                    return s;
                }
            }
            // Ordinary field access `(<base>).kd_<field>`; a `*Struct`
            // base auto-derefs through the pointer (`(*(<base>)).kd_f`,
            // SPEC §30.1) — v0.175.
            var b2: []u8 = self.emit_expr(a, self.nodes[u].a);
            var sb2: StrBuilder = StrBuilder.init(a);
            if (self.is_ptr_to_struct(self.nodes[u].a)) {
                sb2.append(a, "(*(");
                sb2.append(a, b2);
                sb2.append(a, ")).kd_");
            } else {
                sb2.append(a, "(");
                sb2.append(a, b2);
                sb2.append(a, ").kd_");
            }
            sb2.append(a, fname);
            var s2: []u8 = sb2.build(a);
            sb2.deinit(a);
            return s2;
        }
        if (k == ND_INDEX) {
            // `s[i]` (read) → the bounds-checked `_get` helper (SPEC §15.2).
            var b3: []u8 = self.emit_expr(a, self.nodes[u].a);
            var i3: []u8 = self.emit_expr(a, self.nodes[u].b);
            var bt3: i64 = self.type_of_expr(self.nodes[u].a);
            var sb3: StrBuilder = StrBuilder.init(a);
            if (et_is_arr(bt3)) {
                sb3.append(a, self.arr_c_name(a, bt3));
                sb3.append(a, "_get(");
                sb3.append(a, b3);
                sb3.append(a, ", ");
                sb3.append(a, i3);
                sb3.append(a, ")");
            } else if (et_is_slice(bt3)) {
                sb3.append(a, self.sl_c_name(a, bt3));
                sb3.append(a, "_get(");
                sb3.append(a, b3);
                sb3.append(a, ", ");
                sb3.append(a, i3);
                sb3.append(a, ")");
            } else {
                // Unreachable for validated input (`base` is a slice).
                sb3.append(a, "(");
                sb3.append(a, b3);
                sb3.append(a, ")[");
                sb3.append(a, i3);
                sb3.append(a, "]");
            }
            var s3: []u8 = sb3.build(a);
            sb3.deinit(a);
            return s3;
        }
        if (k == ND_MCALL) {
            // `Emitter::emit_method_call` (v0.170). Associated form — the
            // receiver is an identifier naming a struct — lowers to
            // `kd_<Struct>_<method>(<args>)`, the receiver NOT passed
            // (an explicit-self `Counter.get(c)` call already carries it).
            // The value form prepends the receiver as the leading `self`
            // argument. The unresolvable-receiver fallback keeps the empty
            // struct name, mirroring the Rust `unwrap_or_default` arm.
            var mrecv: i32 = self.nodes[u].a;
            var msid: i64 = ET_NONE;
            var is_assoc: bool = false;
            if (mrecv >= 0 and self.nodes[@as(usize, mrecv)].kind == ND_IDENT) {
                msid = self.st_code_of(self.xname(mrecv));
                if (msid != ET_NONE) { is_assoc = true; }
            }
            if (!is_assoc) {
                var mrt: i64 = self.type_of_expr(mrecv);
                if (et_is_struct(mrt)) { msid = mrt; }
                if (et_is_ptr(mrt) and et_is_struct(self.pt_pointee_of(mrt))) {
                    // A `*Struct` receiver auto-derefs (SPEC §30.1).
                    msid = self.pt_pointee_of(mrt);
                }
            }
            var mrow: i64 = self.mt_row_of(msid, self.xname(n));
            var moffset: i64 = 0;
            if (!is_assoc) { moffset = 1; }
            var msb: StrBuilder = StrBuilder.init(a);
            msb.append(a, "kd_");
            if (msid != ET_NONE) { msb.append(a, self.st_name_of(msid)); }
            msb.append(a, "_");
            msb.append(a, self.xname(n));
            msb.append(a, "(");
            var first: bool = true;
            if (!is_assoc) {
                // The auto-ref/deref matrix (SPEC §30.2): a POINTER-
                // receiver method over a value receiver takes `&` (an
                // element receiver refs its `_at` pointer); a VALUE-
                // receiver method over a `*Struct` receiver derefs.
                var ptr_recv: bool = false;
                if (mrow >= 0 and self.mt_p_count[@as(usize, mrow)] > 0) {
                    ptr_recv = et_is_ptr(self.fp_ty[@as(usize, self.mt_p_start[@as(usize, mrow)])]);
                }
                var recv_is_ptr: bool = self.is_ptr_to_struct(mrecv);
                if (ptr_recv and !recv_is_ptr) {
                    var ru: usize = @as(usize, mrecv);
                    if (self.nodes[ru].kind == ND_INDEX) {
                        var sbr: StrBuilder = StrBuilder.init(a);
                        sbr.append(a, "(");
                        sbr.append(a, self.emit_index_addr(a, self.nodes[ru].a, self.nodes[ru].b));
                        sbr.append(a, ")");
                        msb.append(a, sbr.build(a));
                        sbr.deinit(a);
                    } else if (es_chain_has_index(self.nodes, mrecv)) {
                        var sbr2: StrBuilder = StrBuilder.init(a);
                        sbr2.append(a, "(&(");
                        sbr2.append(a, self.emit_place(a, mrecv));
                        sbr2.append(a, "))");
                        msb.append(a, sbr2.build(a));
                        sbr2.deinit(a);
                    } else {
                        var sbr3: StrBuilder = StrBuilder.init(a);
                        sbr3.append(a, "(&(");
                        sbr3.append(a, self.emit_expr(a, mrecv));
                        sbr3.append(a, "))");
                        msb.append(a, sbr3.build(a));
                        sbr3.deinit(a);
                    }
                } else if (!ptr_recv and recv_is_ptr) {
                    var sbr4: StrBuilder = StrBuilder.init(a);
                    sbr4.append(a, "(*(");
                    sbr4.append(a, self.emit_expr(a, mrecv));
                    sbr4.append(a, "))");
                    msb.append(a, sbr4.build(a));
                    sbr4.deinit(a);
                } else {
                    msb.append(a, self.emit_expr(a, mrecv));
                }
                first = false;
            }
            var marg: i32 = self.nodes[u].b;
            var margi: i64 = 0;
            while (marg >= 0) {
                if (!first) { msb.append(a, ", "); }
                first = false;
                var mex: i64 = ET_NONE;
                if (mrow >= 0 and margi + moffset < self.mt_p_count[@as(usize, mrow)]) {
                    mex = self.fp_ty[@as(usize, self.mt_p_start[@as(usize, mrow)] + margi + moffset)];
                }
                msb.append(a, self.emit_coerced(a, marg, mex));
                margi += 1;
                marg = self.nodes[@as(usize, marg)].next;
            }
            msb.append(a, ")");
            var ms: []u8 = msb.build(a);
            msb.deinit(a);
            return ms;
        }
        if (k == ND_SLIT) {
            // `Name{ .f = e, … }` → the C99 compound literal
            // `((kd_struct_<Name>){ .kd_<f> = <v>, … })` (initializers in
            // SOURCE order — C designated initializers reorder); the empty
            // literal zero-initialises. An unresolvable name falls back to
            // the canonical spelling (`kd_struct_<name>`), mirroring the
            // Rust defensive arm.
            var scn: []u8 = "";
            var slc: i64 = self.st_code_of(self.xname(n));
            if (slc != ET_NONE) {
                scn = self.st_c_name(a, slc);
            } else {
                var sbn: StrBuilder = StrBuilder.init(a);
                sbn.append(a, "kd_struct_");
                sbn.append(a, self.xname(n));
                scn = sbn.build(a);
                sbn.deinit(a);
            }
            var fin: i32 = self.nodes[u].a;
            if (fin < 0) {
                var sbe: StrBuilder = StrBuilder.init(a);
                sbe.append(a, "((");
                sbe.append(a, scn);
                sbe.append(a, "){0})");
                var se: []u8 = sbe.build(a);
                sbe.deinit(a);
                return se;
            }
            var sbl: StrBuilder = StrBuilder.init(a);
            sbl.append(a, "((");
            sbl.append(a, scn);
            sbl.append(a, "){ ");
            var lfirst: bool = true;
            while (fin >= 0) {
                var fu: usize = @as(usize, fin);
                if (!lfirst) { sbl.append(a, ", "); }
                lfirst = false;
                sbl.append(a, ".kd_");
                sbl.append(a, self.src[self.nodes[fu].xoff .. self.nodes[fu].xoff + self.nodes[fu].xlen]);
                sbl.append(a, " = ");
                var fexp: i64 = ET_NONE;
                if (slc != ET_NONE) {
                    fexp = self.st_field_ty(slc, self.src[self.nodes[fu].xoff .. self.nodes[fu].xoff + self.nodes[fu].xlen]);
                }
                sbl.append(a, self.emit_coerced(a, self.nodes[fu].a, fexp));
                fin = self.nodes[fu].next;
            }
            sbl.append(a, " })");
            var sl: []u8 = sbl.build(a);
            sbl.deinit(a);
            return sl;
        }
        if (k == ND_ALIT) {
            // `[N]T{ e0, e1, … }` → `((kd_arr_<tag>_<N>){ .data = { … } })`
            // (SPEC §14.3); a zero-element literal zero-initialises; an
            // unresolvable literal type takes the Rust brace-init fallback.
            var alt: i64 = self.resolve_ty(self.nodes[u].a);
            if (et_is_arr(alt)) {
                var acn: []u8 = self.arr_c_name(a, alt);
                var e0: i32 = self.nodes[u].b;
                if (e0 < 0) {
                    var sbz: StrBuilder = StrBuilder.init(a);
                    sbz.append(a, "((");
                    sbz.append(a, acn);
                    sbz.append(a, "){0})");
                    var sz: []u8 = sbz.build(a);
                    sbz.deinit(a);
                    return sz;
                }
                var aelem: i64 = self.arr_elem_of(alt);
                var sbal: StrBuilder = StrBuilder.init(a);
                sbal.append(a, "((");
                sbal.append(a, acn);
                sbal.append(a, "){ .data = { ");
                var acur: i32 = e0;
                var afirst: bool = true;
                while (acur >= 0) {
                    if (!afirst) { sbal.append(a, ", "); }
                    afirst = false;
                    sbal.append(a, self.emit_coerced(a, acur, aelem));
                    acur = self.nodes[@as(usize, acur)].next;
                }
                sbal.append(a, " } })");
                var sal: []u8 = sbal.build(a);
                sbal.deinit(a);
                return sal;
            }
            // Unreachable for validated input: the brace-init fallback.
            var sbf: StrBuilder = StrBuilder.init(a);
            sbf.append(a, "{ ");
            var fcur: i32 = self.nodes[u].b;
            var ffirst: bool = true;
            while (fcur >= 0) {
                if (!ffirst) { sbf.append(a, ", "); }
                ffirst = false;
                sbf.append(a, self.emit_expr(a, fcur));
                fcur = self.nodes[@as(usize, fcur)].next;
            }
            sbf.append(a, " }");
            var sf: []u8 = sbf.build(a);
            sbf.deinit(a);
            return sf;
        }
        if (k == ND_ADDROF) {
            // `&place` (SPEC §15.1): an index place IS the bounds-checked
            // `_at` element pointer; a chain through an index takes `&` of
            // its `_at` lvalue; anything else is already a C lvalue.
            var apl: i32 = self.nodes[u].a;
            if (apl >= 0 and self.nodes[@as(usize, apl)].kind == ND_INDEX) {
                var sba: StrBuilder = StrBuilder.init(a);
                sba.append(a, "(");
                sba.append(a, self.emit_index_addr(a, self.nodes[@as(usize, apl)].a, self.nodes[@as(usize, apl)].b));
                sba.append(a, ")");
                var sa: []u8 = sba.build(a);
                sba.deinit(a);
                return sa;
            }
            var alv: []u8 = "";
            if (apl >= 0 and es_chain_has_index(self.nodes, apl)) {
                alv = self.emit_place(a, apl);
            } else {
                alv = self.emit_expr(a, apl);
            }
            var sba2: StrBuilder = StrBuilder.init(a);
            sba2.append(a, "(&(");
            sba2.append(a, alv);
            sba2.append(a, "))");
            var sa2: []u8 = sba2.build(a);
            sba2.deinit(a);
            return sa2;
        }
        if (k == ND_DEREF) {
            // `p.*` (read) → `(*(<p>))` (SPEC §15.1).
            var din: []u8 = self.emit_expr(a, self.nodes[u].a);
            var sbd: StrBuilder = StrBuilder.init(a);
            sbd.append(a, "(*(");
            sbd.append(a, din);
            sbd.append(a, "))");
            var sd: []u8 = sbd.build(a);
            sbd.deinit(a);
            return sd;
        }
        if (k == ND_SLICEX) {
            // `base[lo..hi]` (SPEC §15.2): a `{ptr, len}` view over the
            // base's storage with the bounds check folded into a portable
            // conditional whose failing branch never returns. The base, lo
            // and hi strings are spliced in MULTIPLE times, exactly like
            // the Rust format string. A slice base reads `.ptr`/`.len`;
            // the non-slice fallback (unreachable behind the detector)
            // mirrors the Rust `(<base>)` / `0` / `kd_slice_void` arms.
            // An ARRAY base reached through an index (`xs[i].buf[lo..hi]`,
            // v0.169) spells as an LVALUE via `_at` — the by-value `_get`
            // would view a dangling temporary copy.
            var bn4: i32 = self.nodes[u].a;
            var bs4: []u8 = "";
            if (et_is_arr(self.type_of_expr(bn4)) and es_chain_has_index(self.nodes, bn4)) {
                bs4 = self.emit_place(a, bn4);
            } else {
                bs4 = self.emit_expr(a, bn4);
            }
            var lo4: []u8 = self.emit_expr(a, self.nodes[u].b);
            var hi4: []u8 = self.emit_expr(a, self.nodes[u].c);
            var bt4: i64 = self.type_of_expr(self.nodes[u].a);
            var sn4: []u8 = "kd_slice_void";
            if (et_is_arr(bt4)) {
                sn4 = self.sl_c_name(a, et_slice_of(self.arr_elem_of(bt4)));
            } else if (et_is_slice(bt4)) {
                sn4 = self.sl_c_name(a, bt4);
            }
            var sb4: StrBuilder = StrBuilder.init(a);
            sb4.append(a, "(( (");
            sb4.append(a, lo4);
            sb4.append(a, ") < 0 || (");
            sb4.append(a, hi4);
            sb4.append(a, ") < (");
            sb4.append(a, lo4);
            sb4.append(a, ") || (");
            sb4.append(a, hi4);
            sb4.append(a, ") > (");
            if (et_is_arr(bt4)) {
                sb4.append_i64(a, self.arr_len_of(bt4));
            } else if (et_is_slice(bt4)) {
                sb4.append(a, "(");
                sb4.append(a, bs4);
                sb4.append(a, ").len");
            } else {
                sb4.append(a, "0");
            }
            sb4.append(a, ") ) ? (fputs(\"panic: slice bounds out of range\\n\", stderr), exit(101), (");
            sb4.append(a, sn4);
            sb4.append(a, "){0}) : (");
            sb4.append(a, sn4);
            sb4.append(a, "){ .ptr = (");
            sb4.append(a, bs4);
            if (et_is_arr(bt4)) {
                sb4.append(a, ").data + (");
            } else if (et_is_slice(bt4)) {
                sb4.append(a, ").ptr + (");
            } else {
                sb4.append(a, ") + (");
            }
            sb4.append(a, lo4);
            sb4.append(a, "), .len = (");
            sb4.append(a, hi4);
            sb4.append(a, ") - (");
            sb4.append(a, lo4);
            sb4.append(a, ") })");
            var s4: []u8 = sb4.build(a);
            sb4.deinit(a);
            return s4;
        }
        if (k == ND_CALL) {
            var callee: []u8 = self.xname(n);
            if (str_eq(callee, "print")) {
                var arg: i32 = self.nodes[u].a;
                // `print(s)` of a `[]u8` string (SPEC §23.2): hoist the
                // slice into a fresh `__kd_str{N}` temporary so it is
                // evaluated once, then `fwrite` + newline. (The f64 route
                // cannot appear in the subset.)
                if (arg >= 0 and self.type_of_expr(arg) == ET_SLICE_U8) {
                    var sstr: []u8 = self.emit_expr(a, arg);
                    var nn: i64 = self.str_count;
                    self.str_count += 1;
                    var sbs: StrBuilder = StrBuilder.init(a);
                    sbs.append(a, "{ kd_slice_uint8_t __kd_str");
                    sbs.append_i64(a, nn);
                    sbs.append(a, " = (");
                    sbs.append(a, sstr);
                    sbs.append(a, "); fwrite(__kd_str");
                    sbs.append_i64(a, nn);
                    sbs.append(a, ".ptr, 1, __kd_str");
                    sbs.append_i64(a, nn);
                    sbs.append(a, ".len, stdout); fputc('\\n', stdout); }");
                    var ss: []u8 = sbs.build(a);
                    sbs.deinit(a);
                    return ss;
                }
                // `print(<int>)` → `kd_print((long long)(<e>))`.
                var astr: []u8 = "0";
                if (arg >= 0) { astr = self.emit_expr(a, arg); }
                var sb: StrBuilder = StrBuilder.init(a);
                sb.append(a, "kd_print((long long)(");
                sb.append(a, astr);
                sb.append(a, "))");
                var s: []u8 = sb.build(a);
                sb.deinit(a);
                return s;
            }
            if (str_eq(callee, "expect")) {
                // Value-position `expect` is a no-op placeholder (Program
                // mode; sema rejects it, output must stay well-formed).
                return "((void)0)";
            }
            if (str_eq(callee, "c_allocator")) {
                // The malloc/free-backed allocator value (SPEC §16.2): a
                // zero-initialised compound literal IS the whole allocator.
                return "((kd_allocator){0})";
            }
            if (str_eq(callee, "alloc")) {
                // `alloc(a, T, n)` → the slice's inline `_alloc` helper
                // (SPEC §16.2). The allocator argument is accepted but
                // UNUSED (never emitted); arg 1 names the element type
                // (`u8` behind the detector); arg 2 is the element count.
                var a0: i32 = self.nodes[u].a;
                var a1: i32 = 0 - 1;
                var a2: i32 = 0 - 1;
                if (a0 >= 0) { a1 = self.nodes[@as(usize, a0)].next; }
                if (a1 >= 0) { a2 = self.nodes[@as(usize, a1)].next; }
                var tag: []u8 = "void";
                if (a1 >= 0 and self.nodes[@as(usize, a1)].kind == ND_IDENT) {
                    var et: i64 = et_from_name(self.xname(a1));
                    if (et != ET_NONE) { tag = et_c_name(et); }
                }
                var nstr: []u8 = "0";
                if (a2 >= 0) { nstr = self.emit_expr(a, a2); }
                var sba: StrBuilder = StrBuilder.init(a);
                sba.append(a, "kd_slice_");
                sba.append(a, tag);
                sba.append(a, "_alloc((uintptr_t)(");
                sba.append(a, nstr);
                sba.append(a, "))");
                var sa: []u8 = sba.build(a);
                sba.deinit(a);
                return sa;
            }
            if (str_eq(callee, "free")) {
                // `free(a, s)` → release the slice's backing pointer (SPEC
                // §16.2); the allocator argument is unused and not emitted.
                var f0: i32 = self.nodes[u].a;
                var f1: i32 = 0 - 1;
                if (f0 >= 0) { f1 = self.nodes[@as(usize, f0)].next; }
                var fstr: []u8 = "0";
                if (f1 >= 0) { fstr = self.emit_expr(a, f1); }
                var sbf: StrBuilder = StrBuilder.init(a);
                sbf.append(a, "free((");
                sbf.append(a, fstr);
                sbf.append(a, ").ptr)");
                var sf: []u8 = sbf.build(a);
                sbf.deinit(a);
                return sf;
            }
            // Coerce each argument to its parameter type (a contextual
            // `.V` argument takes the enum from the signature, v0.172).
            var frow: i64 = self.fn_row_of(callee);
            var sb: StrBuilder = StrBuilder.init(a);
            sb.append(a, "kd_");
            sb.append(a, callee);
            sb.append(a, "(");
            var cur: i32 = self.nodes[u].a;
            var first: bool = true;
            var argi: i64 = 0;
            while (cur >= 0) {
                if (!first) { sb.append(a, ", "); }
                first = false;
                var expct: i64 = ET_NONE;
                if (frow >= 0 and argi < self.fns[@as(usize, frow)].pcount) {
                    expct = self.fp_ty[@as(usize, self.fns[@as(usize, frow)].pstart + argi)];
                }
                var e: []u8 = self.emit_coerced(a, cur, expct);
                sb.append(a, e);
                argi += 1;
                cur = self.nodes[@as(usize, cur)].next;
            }
            sb.append(a, ")");
            var s: []u8 = sb.build(a);
            sb.deinit(a);
            return s;
        }
        if (k == ND_COMPTIME) {
            // Fold to a literal when possible; else fall back to the inner
            // expression (the C compiler folds it itself).
            var v: EvRes = self.eval(self.nodes[u].a);
            if (v.ok) { return self.const_literal(a, v); }
            return self.emit_expr(a, self.nodes[u].a);
        }
        if (k == ND_BUILTIN) {
            // `@as(T, e)` → a C cast `((T)(e))` (v0.137, SPEC §33). The
            // first argument names the target type (an unresolvable name
            // falls back through `base_type` to `void`, a missing value to
            // `0`, mirroring the Rust arms). v0.171 adds the enum
            // conversions `@intFromEnum(e)` → `((int64_t)(<e>))` and
            // `@enumFromInt(E, n)` → `((kd_enum_E)(<n>))`. Every other
            // builtin is out of the subset; its sema-invalid remnants take
            // the Rust unknown-builtin placeholder `0`.
            var bname0: []u8 = self.xname(n);
            if (str_eq(bname0, "intFromEnum")) {
                var ia: i32 = self.nodes[u].a;
                var iv: []u8 = "0";
                if (ia >= 0) { iv = self.emit_expr(a, ia); }
                var sbi: StrBuilder = StrBuilder.init(a);
                sbi.append(a, "((int64_t)(");
                sbi.append(a, iv);
                sbi.append(a, "))");
                var si2: []u8 = sbi.build(a);
                sbi.deinit(a);
                return si2;
            }
            if (str_eq(bname0, "enumFromInt")) {
                var ea: i32 = self.nodes[u].a;
                var ecty: []u8 = "void";
                var ev: []u8 = "0";
                if (ea >= 0) {
                    if (self.nodes[@as(usize, ea)].kind == ND_IDENT) {
                        var ec2: i64 = self.base_code(self.xname(ea));
                        if (ec2 != ET_NONE) { ecty = self.cty_of(a, ec2); }
                    }
                    var eb: i32 = self.nodes[@as(usize, ea)].next;
                    if (eb >= 0) { ev = self.emit_expr(a, eb); }
                }
                var sbe2: StrBuilder = StrBuilder.init(a);
                sbe2.append(a, "((");
                sbe2.append(a, ecty);
                sbe2.append(a, ")(");
                sbe2.append(a, ev);
                sbe2.append(a, "))");
                var se2: []u8 = sbe2.build(a);
                sbe2.deinit(a);
                return se2;
            }
            if (str_eq(bname0, "as")) {
                var b0: i32 = self.nodes[u].a;
                var b1: i32 = 0 - 1;
                if (b0 >= 0) { b1 = self.nodes[@as(usize, b0)].next; }
                var ty: i64 = ET_VOID;
                if (b0 >= 0 and self.nodes[@as(usize, b0)].kind == ND_IDENT) {
                    var t3: i64 = et_from_name(self.xname(b0));
                    if (t3 != ET_NONE) { ty = t3; }
                }
                var val: []u8 = "0";
                if (b1 >= 0) { val = self.emit_expr(a, b1); }
                var sbb: StrBuilder = StrBuilder.init(a);
                sbb.append(a, "((");
                sbb.append(a, et_c_name(ty));
                sbb.append(a, ")(");
                sbb.append(a, val);
                sbb.append(a, "))");
                var sv: []u8 = sbb.build(a);
                sbb.deinit(a);
                return sv;
            }
            return "0";
        }
        // Unreachable behind the detector: keep the output well-formed.
        return "0";
    }

    // -- defer flushing --------------------------------------------------------------

    /// Whether any scope holds a deferred statement (`any_defer_active`;
    /// the subset has no `errdefer`, so there is no error-edge variant).
    /// Whether any scope holds a pending defer; on an error-return edge
    /// (`inc_err`) errdefers count too.
    fn any_defer_active(self: *Self, inc_err: bool) bool {
        var i: usize = 0;
        while (i < self.df_len) : (i += 1) {
            if (inc_err or !self.derr[i]) { return true; }
        }
        return false;
    }

    /// The end of scope `idx`'s defer span: the next scope's start, or the
    /// stack top for the innermost scope.
    fn defer_end(self: *Self, idx: usize) i64 {
        if (idx + 1 < self.sc_len) { return self.scopes[idx + 1].dstart; }
        return @as(i64, self.df_len);
    }

    /// `flush_scope`: one scope's defers in reverse registration order. The
    /// span is snapshotted first (Rust clones the list), so a defer body
    /// that itself registers defers cannot extend the flush.
    fn flush_scope(self: *Self, a: Allocator, idx: usize, inc_err: bool) void {
        var lo: i64 = self.scopes[idx].dstart;
        var hi: i64 = self.defer_end(idx);
        var i: i64 = hi - 1;
        while (i >= lo) : (i -= 1) {
            // `errdefer`s run only on error-return edges (SPEC §34.3).
            if (!inc_err and self.derr[@as(usize, i)]) { continue; }
            var st: i32 = self.defers[@as(usize, i)];
            var d: bool = self.emit_stmt(a, st);
            // The divergence verdict of a flushed defer body is discarded,
            // exactly as in Rust (`emit_stmt(s);` in `flush_scope`).
            if (d) { }
        }
    }

    /// Flush scopes innermost-first down to and including the loop-body
    /// scope labeled `name` (normal exits — no errdefers); -1 (nothing
    /// flushed) when no enclosing loop carries the label, mirroring the
    /// Rust early-`None`.
    fn flush_to_labeled_loop(self: *Self, a: Allocator, name: []u8) i64 {
        var loop_idx: i64 = 0 - 1;
        var i: i64 = @as(i64, self.sc_len) - 1;
        while (i >= 0) : (i -= 1) {
            var sc: usize = @as(usize, i);
            if (self.scopes[sc].is_loop and self.scopes[sc].llen > 0) {
                if (str_eq(self.src[self.scopes[sc].loff .. self.scopes[sc].loff + self.scopes[sc].llen], name)) {
                    loop_idx = i;
                    break;
                }
            }
        }
        if (loop_idx < 0) { return loop_idx; }
        i = @as(i64, self.sc_len) - 1;
        while (i >= loop_idx) : (i -= 1) {
            self.flush_scope(a, @as(usize, i), false);
        }
        return loop_idx;
    }

    fn flush_current(self: *Self, a: Allocator) void {
        if (self.sc_len > 0) { self.flush_scope(a, self.sc_len - 1, false); }
    }

    fn flush_all(self: *Self, a: Allocator, inc_err: bool) void {
        var i: i64 = @as(i64, self.sc_len) - 1;
        while (i >= 0) : (i -= 1) {
            self.flush_scope(a, @as(usize, i), inc_err);
        }
    }

    /// Flush innermost-first down to and including the nearest loop-body
    /// scope; returns its index, or -1 when there is no enclosing loop (a
    /// sema-invalid `break`/`continue` — nothing is flushed, mirroring the
    /// early `None` return).
    fn flush_to_loop(self: *Self, a: Allocator) i64 {
        var loop_idx: i64 = 0 - 1;
        var i: i64 = @as(i64, self.sc_len) - 1;
        while (i >= 0) : (i -= 1) {
            if (self.scopes[@as(usize, i)].is_loop) {
                loop_idx = i;
                break;
            }
        }
        if (loop_idx < 0) { return loop_idx; }
        i = @as(i64, self.sc_len) - 1;
        while (i >= loop_idx) : (i -= 1) {
            self.flush_scope(a, @as(usize, i), false);
        }
        return loop_idx;
    }

    // -- statements ---------------------------------------------------------------------

    /// `emit_cont`: a `while` continue-clause (an assignment or expression).
    fn emit_cont(self: *Self, a: Allocator, n: i32) void {
        var u: usize = @as(usize, n);
        var k: u8 = self.nodes[u].kind;
        if (k == ND_ASSIGN) {
            self.emit_assign(a, n);
            return;
        }
        // The parser only produces an assignment or an expression here.
        var es: []u8 = self.emit_expr(a, n);
        var sb: StrBuilder = StrBuilder.init(a);
        sb.append(a, es);
        sb.append(a, ";");
        var s: []u8 = sb.build(a);
        sb.deinit(a);
        self.line(a, s);
    }

    /// `emit_loop_cont`: the continue-clause of the loop-body scope at
    /// `idx`, if any (the subset has no `for`, so no raw clause).
    fn emit_loop_cont(self: *Self, a: Allocator, idx: usize) void {
        var c: i32 = self.scopes[idx].cont;
        if (c >= 0) { self.emit_cont(a, c); }
        var rf: i64 = self.scopes[idx].raw_fi;
        if (rf >= 0) {
            var sb: StrBuilder = StrBuilder.init(a);
            sb.append(a, "__kd_fi");
            sb.append_i64(a, rf);
            sb.append(a, " += 1;");
            var s: []u8 = sb.build(a);
            sb.deinit(a);
            self.line(a, s);
        }
    }

    /// The (compound) name-assignment lowering, shared by `Stmt::Assign` and
    /// the continue-clause: `kd_x = (<e>);` / `kd_x = kd_x <op> (<e>);`.
    fn emit_assign(self: *Self, a: Allocator, n: i32) void {
        var u: usize = @as(usize, n);
        var name: []u8 = self.xname(n);
        var es: []u8 = self.emit_coerced(a, self.nodes[u].a, self.vt_lookup(name));
        var op: i64 = self.nodes[u].val;
        var sb: StrBuilder = StrBuilder.init(a);
        sb.append(a, "kd_");
        sb.append(a, name);
        sb.append(a, " = ");
        if (op >= 0) {
            sb.append(a, "kd_");
            sb.append(a, name);
            sb.append(a, " ");
            sb.append(a, es_c_op(op));
            sb.append(a, " (");
            sb.append(a, es);
            sb.append(a, ");");
        } else {
            sb.append(a, es);
            sb.append(a, ";");
        }
        var s: []u8 = sb.build(a);
        sb.deinit(a);
        self.line(a, s);
    }

    /// `Emitter::store_str` into `sb`: the C store for a side-effect-free
    /// (already-hoisted) lvalue `target` (SPEC §27.3). A plain `=` is
    /// `target = (val);`; a compound `op=` re-spells the place on both
    /// sides — `target = target <c-op> (val);` — correct precisely because
    /// the target re-read re-evaluates nothing.
    fn put_store(self: *Self, a: Allocator, sb: *StrBuilder, target: []u8, op: i64, val: []u8) void {
        sb.append(a, target);
        sb.append(a, " = ");
        if (op >= 0) {
            sb.append(a, target);
            sb.append(a, " ");
            sb.append(a, es_c_op(op));
            sb.append(a, " ");
        }
        sb.append(a, "(");
        sb.append(a, val);
        sb.append(a, ");");
    }

    /// `Stmt::FieldAssign`, restricted to the subset's DIRECT index write
    /// `s[i] (op)= e` (SPEC §15.2/§27.3): one bounds-checked block hoisting
    /// the index into a fresh `__kd_idx{k}` — the SINGLE evaluation of the
    /// index, so the compound form re-spells the element slot without
    /// re-evaluating `i`. A slice base writes through `.ptr` and the
    /// runtime `.len`; the non-slice fallback mirrors the Rust
    /// unreachable-for-validated-input array arm (length 0, `.data`, the
    /// "array" panic message). Any non-index place takes the field-chain
    /// default (`(<place>) = (<value>);`) — equally unreachable behind the
    /// detector, mirrored for totality.
    /// `Emitter::emit_index_addr`: lower `base[index]` to an element
    /// POINTER via the `_at` helper — `kd_arr_<tag>_<N>_at(&(<base>), <i>)`
    /// for an array (the base spelled as an lvalue, recursively), or
    /// `kd_slice_<tag>_at(<base>, <i>)` for a slice (by value). The index
    /// is emitted FIRST, exactly like the Rust method.
    fn emit_index_addr(self: *Self, a: Allocator, basen: i32, idxn: i32) []u8 {
        var i: []u8 = self.emit_expr(a, idxn);
        var bt: i64 = self.type_of_expr(basen);
        var sb: StrBuilder = StrBuilder.init(a);
        if (et_is_arr(bt)) {
            sb.append(a, self.arr_c_name(a, bt));
            sb.append(a, "_at(&(");
            sb.append(a, self.emit_place(a, basen));
            sb.append(a, "), ");
            sb.append(a, i);
            sb.append(a, ")");
        } else if (et_is_slice(bt)) {
            sb.append(a, self.sl_c_name(a, bt));
            sb.append(a, "_at(");
            sb.append(a, self.emit_expr(a, basen));
            sb.append(a, ", ");
            sb.append(a, i);
            sb.append(a, ")");
        } else {
            // Unreachable for validated input (`base` is an array/slice).
            sb.append(a, "(&((");
            sb.append(a, self.emit_expr(a, basen));
            sb.append(a, ")[");
            sb.append(a, i);
            sb.append(a, "]))");
        }
        var out: []u8 = sb.build(a);
        sb.deinit(a);
        return out;
    }

    /// `Emitter::emit_place`: a place as a C LVALUE string. Index-free
    /// chains lower like ordinary expressions; an `Index` step goes
    /// through the element-pointer `_at` helper (`(*at)` / `at->kd_f`).
    fn emit_place(self: *Self, a: Allocator, n: i32) []u8 {
        if (!es_chain_has_index(self.nodes, n)) {
            return self.emit_expr(a, n);
        }
        var u: usize = @as(usize, n);
        var k: u8 = self.nodes[u].kind;
        if (k == ND_INDEX) {
            var sb: StrBuilder = StrBuilder.init(a);
            sb.append(a, "(*");
            sb.append(a, self.emit_index_addr(a, self.nodes[u].a, self.nodes[u].b));
            sb.append(a, ")");
            var out: []u8 = sb.build(a);
            sb.deinit(a);
            return out;
        }
        if (k == ND_FIELD) {
            var basen: i32 = self.nodes[u].a;
            var bu: usize = @as(usize, basen);
            var sb2: StrBuilder = StrBuilder.init(a);
            if (self.nodes[bu].kind == ND_INDEX) {
                // A field directly on an element: `at(...)->kd_f`.
                sb2.append(a, self.emit_index_addr(a, self.nodes[bu].a, self.nodes[bu].b));
                sb2.append(a, "->kd_");
                sb2.append(a, self.xname(n));
            } else {
                var pb: []u8 = self.emit_place(a, basen);
                if (self.is_ptr_to_struct(basen)) {
                    sb2.append(a, "(*(");
                    sb2.append(a, pb);
                    sb2.append(a, ")).kd_");
                } else {
                    sb2.append(a, "(");
                    sb2.append(a, pb);
                    sb2.append(a, ").kd_");
                }
                sb2.append(a, self.xname(n));
            }
            var out2: []u8 = sb2.build(a);
            sb2.deinit(a);
            return out2;
        }
        // Unreachable: the two arms above cover every has-index chain.
        return self.emit_expr(a, n);
    }

    fn emit_place_assign(self: *Self, a: Allocator, n: i32) void {
        var u: usize = @as(usize, n);
        var place: i32 = self.nodes[u].a;
        var value: i32 = self.nodes[u].b;
        var op: i64 = self.nodes[u].val;
        // A place whose chain passes THROUGH an index below the top step
        // — `a[i].f = e`, `s[i].f.g = e`, `xs[i].buf[j] = e` — takes the
        // `_at` element-pointer lowering (v0.169). A compound form hoists
        // the place's address into `__kd_pl{k}` (one index evaluation,
        // one bounds check), sharing the `__kd_idx` counter.
        var needs_at: bool = false;
        if (place >= 0) {
            var pk: u8 = self.nodes[@as(usize, place)].kind;
            if (pk == ND_INDEX or pk == ND_FIELD) {
                needs_at = es_chain_has_index(self.nodes, self.nodes[@as(usize, place)].a);
            }
        }
        if (needs_at) {
            var lv: []u8 = self.emit_place(a, place);
            var pt: i64 = self.type_of_expr(place);
            var es0: []u8 = self.emit_coerced(a, value, pt);
            if (op < 0) {
                var sbp: StrBuilder = StrBuilder.init(a);
                sbp.append(a, "(");
                sbp.append(a, lv);
                sbp.append(a, ") = (");
                sbp.append(a, es0);
                sbp.append(a, ");");
                var sp: []u8 = sbp.build(a);
                sbp.deinit(a);
                self.line(a, sp);
                return;
            }
            var kctr0: i64 = self.idx_count;
            self.idx_count += 1;
            var plcty: []u8 = "int64_t";
            if (pt != ET_NONE) { plcty = self.cty_of(a, pt); }
            var sbc: StrBuilder = StrBuilder.init(a);
            sbc.append(a, "{ ");
            sbc.append(a, plcty);
            sbc.append(a, " *__kd_pl");
            sbc.append_i64(a, kctr0);
            sbc.append(a, " = (&(");
            sbc.append(a, lv);
            sbc.append(a, ")); *__kd_pl");
            sbc.append_i64(a, kctr0);
            sbc.append(a, " = *__kd_pl");
            sbc.append_i64(a, kctr0);
            sbc.append(a, " ");
            sbc.append(a, es_c_op(op));
            sbc.append(a, " (");
            sbc.append(a, es0);
            sbc.append(a, "); }");
            var sc: []u8 = sbc.build(a);
            sbc.deinit(a);
            self.line(a, sc);
            return;
        }
        if (place >= 0 and self.nodes[@as(usize, place)].kind == ND_INDEX) {
            var pu: usize = @as(usize, place);
            var kctr: i64 = self.idx_count;
            self.idx_count += 1;
            var idx: []u8 = self.emit_expr(a, self.nodes[pu].b);
            var base_str: []u8 = self.emit_expr(a, self.nodes[pu].a);
            var bt: i64 = self.type_of_expr(self.nodes[pu].a);
            var pelem: i64 = ET_NONE;
            if (et_is_slice(bt)) { pelem = et_slice_elem(bt); }
            if (et_is_arr(bt)) { pelem = self.arr_elem_of(bt); }
            var val: []u8 = self.emit_coerced(a, value, pelem);
            // The hoisted-slot target: `(<base>).ptr[__kd_idx{k}]` for a
            // slice, `(<base>).data[__kd_idx{k}]` for the fallback arm.
            var tsb: StrBuilder = StrBuilder.init(a);
            tsb.append(a, "(");
            tsb.append(a, base_str);
            if (et_is_slice(bt)) {
                tsb.append(a, ").ptr[__kd_idx");
            } else {
                tsb.append(a, ").data[__kd_idx");
            }
            tsb.append_i64(a, kctr);
            tsb.append(a, "]");
            var target: []u8 = tsb.build(a);
            tsb.deinit(a);
            var sb: StrBuilder = StrBuilder.init(a);
            sb.append(a, "{ int64_t __kd_idx");
            sb.append_i64(a, kctr);
            sb.append(a, " = (");
            sb.append(a, idx);
            sb.append(a, "); if (__kd_idx");
            sb.append_i64(a, kctr);
            sb.append(a, " < 0 || (uint64_t)__kd_idx");
            sb.append_i64(a, kctr);
            if (et_is_slice(bt)) {
                sb.append(a, " >= (");
                sb.append(a, base_str);
                sb.append(a, ").len) { fputs(\"panic: slice index out of bounds\\n\", stderr); exit(101); } ");
            } else {
                // The array arm bounds against the compile-time length
                // (0 for the unreachable non-array fallback).
                var pal: i64 = 0;
                if (et_is_arr(bt)) { pal = self.arr_len_of(bt); }
                sb.append(a, " >= ");
                sb.append_i64(a, pal);
                sb.append(a, ") { fputs(\"panic: array index out of bounds\\n\", stderr); exit(101); } ");
            }
            self.put_store(a, &sb, target, op, val);
            sb.append(a, " }");
            var s: []u8 = sb.build(a);
            sb.deinit(a);
            self.line(a, s);
            return;
        }
        if (place >= 0 and self.nodes[@as(usize, place)].kind == ND_DEREF) {
            // Deref-assignment `p.* = e;` → `*(<p>) = (<e>);` (SPEC §15.1);
            // the pointer expression is side-effect-free, so a compound
            // form re-spells the dereference on both sides.
            var dpin: []u8 = self.emit_expr(a, self.nodes[@as(usize, place)].a);
            var des: []u8 = self.emit_coerced(a, value, self.type_of_expr(place));
            var dtsb: StrBuilder = StrBuilder.init(a);
            dtsb.append(a, "*(");
            dtsb.append(a, dpin);
            dtsb.append(a, ")");
            var dtarget: []u8 = dtsb.build(a);
            dtsb.deinit(a);
            var dsb: StrBuilder = StrBuilder.init(a);
            self.put_store(a, &dsb, dtarget, op, des);
            var dls: []u8 = dsb.build(a);
            dsb.deinit(a);
            self.line(a, dls);
            return;
        }
        // The field-chain place: `(<place>) (op)= (<value>);` — the value
        // coerced to the place's type.
        var ps: []u8 = "0";
        if (place >= 0) { ps = self.emit_expr(a, place); }
        var es: []u8 = self.emit_coerced(a, value, self.type_of_expr(place));
        var tsb2: StrBuilder = StrBuilder.init(a);
        tsb2.append(a, "(");
        tsb2.append(a, ps);
        tsb2.append(a, ")");
        var target2: []u8 = tsb2.build(a);
        tsb2.deinit(a);
        var sb2: StrBuilder = StrBuilder.init(a);
        self.put_store(a, &sb2, target2, op, es);
        var s2: []u8 = sb2.build(a);
        sb2.deinit(a);
        self.line(a, s2);
    }

    /// `finish_return`: the deferred-temp dance. `has_val` distinguishes
    /// `return;` from `return <e>;` (`es` is meaningful only when set).
    fn finish_return(self: *Self, a: Allocator, has_val: bool, es: []u8, inc_err: bool) void {
        var non_void: bool = self.cur_ret != ET_VOID;
        var active: bool = self.any_defer_active(inc_err);
        if (active and non_void) {
            // Evaluate into a temporary before the defers run; a missing
            // value falls back to `0` (the `unwrap_or` arm — sema-invalid
            // input only).
            var v: []u8 = "0";
            if (has_val) { v = es; }
            var sb: StrBuilder = StrBuilder.init(a);
            sb.append(a, self.cty_of(a, self.cur_ret));
            sb.append(a, " __kd_ret = (");
            sb.append(a, v);
            sb.append(a, ");");
            var s: []u8 = sb.build(a);
            sb.deinit(a);
            self.line(a, s);
            self.flush_all(a, inc_err);
            self.line(a, "return __kd_ret;");
            return;
        }
        if (active) { self.flush_all(a, inc_err); }
        if (has_val) {
            var sb2: StrBuilder = StrBuilder.init(a);
            sb2.append(a, "return (");
            sb2.append(a, es);
            sb2.append(a, ");");
            var s2: []u8 = sb2.build(a);
            sb2.deinit(a);
            self.line(a, s2);
            return;
        }
        self.line(a, "return;");
    }

    /// `Emitter::emit_try` (v0.174): hoist the `!T` operand into
    /// `__kd_try{N}`, early-return the error (flushing defers AND
    /// errdefers — an error edge) re-wrapped in the enclosing return
    /// type, and yield the success payload (`.val`, or `((void)0)` for a
    /// `!void` operand).
    fn emit_try(self: *Self, a: Allocator, inner: i32) []u8 {
        var nctr: i64 = self.try_count;
        self.try_count += 1;
        var it: i64 = self.type_of_expr(inner);
        var err_cty: []u8 = "";
        if (et_is_erru(it)) {
            err_cty = self.cty_of(a, it);
        } else {
            err_cty = self.cty_of(a, self.cur_ret);
        }
        var es: []u8 = self.emit_expr(a, inner);
        var sb: StrBuilder = StrBuilder.init(a);
        sb.append(a, err_cty);
        sb.append(a, " __kd_try");
        sb.append_i64(a, nctr);
        sb.append(a, " = ");
        sb.append(a, es);
        sb.append(a, ";");
        var s1: []u8 = sb.build(a);
        sb.deinit(a);
        self.line(a, s1);
        var sb2: StrBuilder = StrBuilder.init(a);
        sb2.append(a, "if (__kd_try");
        sb2.append_i64(a, nctr);
        sb2.append(a, ".err != 0) {");
        var s2: []u8 = sb2.build(a);
        sb2.deinit(a);
        self.line(a, s2);
        self.indent += 1;
        self.flush_all(a, true);
        var sb3: StrBuilder = StrBuilder.init(a);
        sb3.append(a, "return (");
        sb3.append(a, self.cty_of(a, self.cur_ret));
        sb3.append(a, "){ .err = __kd_try");
        sb3.append_i64(a, nctr);
        sb3.append(a, ".err };");
        var s3: []u8 = sb3.build(a);
        sb3.deinit(a);
        self.line(a, s3);
        self.indent -= 1;
        self.line(a, "}");
        if (et_is_erru(it) and self.eu_payload_of(it) == ET_VOID) {
            return "((void)0)";
        }
        var sb4: StrBuilder = StrBuilder.init(a);
        sb4.append(a, "__kd_try");
        sb4.append_i64(a, nctr);
        sb4.append(a, ".val");
        var s4: []u8 = sb4.build(a);
        sb4.deinit(a);
        return s4;
    }

    /// The payload `T` of a `try inner` (the enclosing function's payload
    /// as the validated-input fallback).
    fn try_payload_type(self: *Self, inner: i32) i64 {
        var it: i64 = self.type_of_expr(inner);
        if (et_is_erru(it)) { return self.eu_payload_of(it); }
        if (et_is_erru(self.cur_ret)) { return self.eu_payload_of(self.cur_ret); }
        return self.cur_ret;
    }

    /// `emit_catch_capture`: `e catch |name| d` — hoist the operand into
    /// `__kd_eu{N}`, declare `__kd_catch{N}` of the payload type, run `d`
    /// ONLY on the error path with `int32_t kd_<name>` bound to the code.
    fn emit_catch_capture(self: *Self, a: Allocator, n: i32) []u8 {
        var u: usize = @as(usize, n);
        var nctr: i64 = self.catch_count;
        self.catch_count += 1;
        var xt: i64 = self.type_of_expr(self.nodes[u].a);
        var err_cty: []u8 = "";
        var payload: i64 = ET_NONE;
        if (et_is_erru(xt)) {
            err_cty = self.eu_c_name(a, xt);
            payload = self.eu_payload_of(xt);
        } else if (et_is_erru(self.cur_ret)) {
            err_cty = self.eu_c_name(a, self.cur_ret);
            payload = self.eu_payload_of(self.cur_ret);
        } else {
            err_cty = self.cty_of(a, self.cur_ret);
            payload = self.cur_ret;
        }
        var es: []u8 = self.emit_expr(a, self.nodes[u].a);
        var sb: StrBuilder = StrBuilder.init(a);
        sb.append(a, err_cty);
        sb.append(a, " __kd_eu");
        sb.append_i64(a, nctr);
        sb.append(a, " = ");
        sb.append(a, es);
        sb.append(a, ";");
        var s1: []u8 = sb.build(a);
        sb.deinit(a);
        self.line(a, s1);
        var sb2: StrBuilder = StrBuilder.init(a);
        sb2.append(a, self.cty_of(a, payload));
        sb2.append(a, " __kd_catch");
        sb2.append_i64(a, nctr);
        sb2.append(a, ";");
        var s2: []u8 = sb2.build(a);
        sb2.deinit(a);
        self.line(a, s2);
        var sb3: StrBuilder = StrBuilder.init(a);
        sb3.append(a, "if (__kd_eu");
        sb3.append_i64(a, nctr);
        sb3.append(a, ".err != 0) {");
        var s3: []u8 = sb3.build(a);
        sb3.deinit(a);
        self.line(a, s3);
        self.indent += 1;
        var sb4: StrBuilder = StrBuilder.init(a);
        sb4.append(a, "int32_t kd_");
        sb4.append(a, self.xname(n));
        sb4.append(a, " = __kd_eu");
        sb4.append_i64(a, nctr);
        sb4.append(a, ".err;");
        var s4: []u8 = sb4.build(a);
        sb4.deinit(a);
        self.line(a, s4);
        self.push_scope(a, false, 0 - 1, 0 - 1);
        self.push_vt(a, self.nodes[u].xoff, self.nodes[u].xlen, ET_I32);
        var d: []u8 = self.emit_coerced(a, self.nodes[u].b, payload);
        var sb5: StrBuilder = StrBuilder.init(a);
        sb5.append(a, "__kd_catch");
        sb5.append_i64(a, nctr);
        sb5.append(a, " = ");
        sb5.append(a, d);
        sb5.append(a, ";");
        var s5: []u8 = sb5.build(a);
        sb5.deinit(a);
        self.line(a, s5);
        self.pop_scope();
        self.indent -= 1;
        self.line(a, "} else {");
        self.indent += 1;
        var sb6: StrBuilder = StrBuilder.init(a);
        sb6.append(a, "__kd_catch");
        sb6.append_i64(a, nctr);
        sb6.append(a, " = __kd_eu");
        sb6.append_i64(a, nctr);
        sb6.append(a, ".val;");
        var s6: []u8 = sb6.build(a);
        sb6.deinit(a);
        self.line(a, s6);
        self.indent -= 1;
        self.line(a, "}");
        var sb7: StrBuilder = StrBuilder.init(a);
        sb7.append(a, "__kd_catch");
        sb7.append_i64(a, nctr);
        var s7: []u8 = sb7.build(a);
        sb7.deinit(a);
        return s7;
    }

    /// `emit_catch_void`: a `catch` over `!void` — capturing or not —
    /// hoists the operand and runs the (void) handler as a statement on
    /// the error path only; yields `((void)0)`.
    fn emit_catch_void(self: *Self, a: Allocator, n: i32) []u8 {
        var u: usize = @as(usize, n);
        var nctr: i64 = self.catch_count;
        self.catch_count += 1;
        var xt: i64 = self.type_of_expr(self.nodes[u].a);
        var err_cty: []u8 = "";
        if (et_is_erru(xt)) {
            err_cty = self.cty_of(a, xt);
        } else {
            err_cty = self.cty_of(a, self.cur_ret);
        }
        var es: []u8 = self.emit_expr(a, self.nodes[u].a);
        var sb: StrBuilder = StrBuilder.init(a);
        sb.append(a, err_cty);
        sb.append(a, " __kd_eu");
        sb.append_i64(a, nctr);
        sb.append(a, " = ");
        sb.append(a, es);
        sb.append(a, ";");
        var s1: []u8 = sb.build(a);
        sb.deinit(a);
        self.line(a, s1);
        var sb2: StrBuilder = StrBuilder.init(a);
        sb2.append(a, "if (__kd_eu");
        sb2.append_i64(a, nctr);
        sb2.append(a, ".err != 0) {");
        var s2: []u8 = sb2.build(a);
        sb2.deinit(a);
        self.line(a, s2);
        self.indent += 1;
        if ((self.nodes[u].flags & F_CAP) != 0) {
            var sb3: StrBuilder = StrBuilder.init(a);
            sb3.append(a, "int32_t kd_");
            sb3.append(a, self.xname(n));
            sb3.append(a, " = __kd_eu");
            sb3.append_i64(a, nctr);
            sb3.append(a, ".err;");
            var s3: []u8 = sb3.build(a);
            sb3.deinit(a);
            self.line(a, s3);
            self.push_scope(a, false, 0 - 1, 0 - 1);
            self.push_vt(a, self.nodes[u].xoff, self.nodes[u].xlen, ET_I32);
            var d: []u8 = self.emit_expr(a, self.nodes[u].b);
            var sb4: StrBuilder = StrBuilder.init(a);
            sb4.append(a, d);
            sb4.append(a, ";");
            var s4: []u8 = sb4.build(a);
            sb4.deinit(a);
            self.line(a, s4);
            self.pop_scope();
        } else {
            var d2: []u8 = self.emit_expr(a, self.nodes[u].b);
            var sb5: StrBuilder = StrBuilder.init(a);
            sb5.append(a, d2);
            sb5.append(a, ";");
            var s5: []u8 = sb5.build(a);
            sb5.deinit(a);
            self.line(a, s5);
        }
        self.indent -= 1;
        self.line(a, "}");
        return "((void)0)";
    }

    /// `emit_if`: flatten the `else if` chain into one C ladder. Returns
    /// whether every arm AND a final `else` diverge.
    /// `Emitter::emit_if_capture` (v0.173): `if (opt) |v| { … } else …` —
    /// the optional hoists into `__kd_if{N}` (evaluated once), the then
    /// branch runs under `.has` with the payload bound
    /// `<inner> kd_<v> = __kd_if{N}.val;` inside its own scope; a
    /// non-optional condition (unreachable for validated input) falls
    /// back to the plain `if`. Never diverges.
    fn emit_if_capture(self: *Self, a: Allocator, n: i32) bool {
        var u: usize = @as(usize, n);
        var ct: i64 = self.type_of_expr(self.nodes[u].a);
        if (!et_is_opt(ct)) {
            return self.emit_if(a, n);
        }
        var inner_ty: i64 = self.opt_inner_of(ct);
        var nctr: i64 = self.if_count;
        self.if_count += 1;
        var cs: []u8 = self.emit_expr(a, self.nodes[u].a);
        self.line(a, "{");
        self.indent += 1;
        var sb: StrBuilder = StrBuilder.init(a);
        sb.append(a, self.opt_c_name(a, ct));
        sb.append(a, " __kd_if");
        sb.append_i64(a, nctr);
        sb.append(a, " = ");
        sb.append(a, cs);
        sb.append(a, ";");
        var s1: []u8 = sb.build(a);
        sb.deinit(a);
        self.line(a, s1);
        var sb2: StrBuilder = StrBuilder.init(a);
        sb2.append(a, "if (__kd_if");
        sb2.append_i64(a, nctr);
        sb2.append(a, ".has) {");
        var s2: []u8 = sb2.build(a);
        sb2.deinit(a);
        self.line(a, s2);
        self.indent += 1;
        // The then branch: a plain scope binding the payload.
        self.push_scope(a, false, 0 - 1, 0 - 1);
        self.push_vt(a, self.nodes[u].xoff, self.nodes[u].xlen, inner_ty);
        var sb3: StrBuilder = StrBuilder.init(a);
        sb3.append(a, self.cty_of(a, inner_ty));
        sb3.append(a, " kd_");
        sb3.append(a, self.xname(n));
        sb3.append(a, " = __kd_if");
        sb3.append_i64(a, nctr);
        sb3.append(a, ".val;");
        var s3: []u8 = sb3.build(a);
        sb3.deinit(a);
        self.line(a, s3);
        var diverged: bool = false;
        var cur: i32 = self.nodes[@as(usize, self.nodes[u].b)].a;
        while (cur >= 0) {
            diverged = self.emit_stmt(a, cur);
            if (diverged) { break; }
            cur = self.nodes[@as(usize, cur)].next;
        }
        if (!diverged) {
            self.flush_current(a);
        }
        self.pop_scope();
        self.indent -= 1;
        var els: i32 = self.nodes[u].c;
        if (els >= 0) {
            self.line(a, "} else {");
            self.indent += 1;
            var du: bool = self.emit_stmt(a, els);
            if (du) { }
            self.indent -= 1;
            self.line(a, "}");
        } else {
            self.line(a, "}");
        }
        self.indent -= 1;
        self.line(a, "}");
        return false;
    }

    fn emit_if(self: *Self, a: Allocator, n: i32) bool {
        var u: usize = @as(usize, n);
        var cs: []u8 = self.emit_expr(a, self.nodes[u].a);
        var sb: StrBuilder = StrBuilder.init(a);
        sb.append(a, "if (");
        sb.append(a, cs);
        sb.append(a, ") {");
        var s: []u8 = sb.build(a);
        sb.deinit(a);
        self.line(a, s);
        var all: bool = true;
        var d: bool = self.emit_block(a, self.nodes[u].b, false, 0 - 1);
        if (!d) { all = false; }
        var els: i32 = self.nodes[u].c;
        while (els >= 0) {
            var eu: usize = @as(usize, els);
            var ek: u8 = self.nodes[eu].kind;
            if (ek == ND_IF and (self.nodes[eu].flags & F_CAP) == 0) {
                var cs2: []u8 = self.emit_expr(a, self.nodes[eu].a);
                var sb2: StrBuilder = StrBuilder.init(a);
                sb2.append(a, "} else if (");
                sb2.append(a, cs2);
                sb2.append(a, ") {");
                var s2: []u8 = sb2.build(a);
                sb2.deinit(a);
                self.line(a, s2);
                var d2: bool = self.emit_block(a, self.nodes[eu].b, false, 0 - 1);
                if (!d2) { all = false; }
                els = self.nodes[eu].c;
            } else if (ek == ND_BLOCK) {
                self.line(a, "} else {");
                var d3: bool = self.emit_block(a, els, false, 0 - 1);
                self.line(a, "}");
                return all and d3;
            } else {
                // A single-statement `else` (unreachable in the subset
                // grammar; mirrored for totality).
                self.line(a, "} else {");
                self.indent += 1;
                var d4: bool = self.emit_stmt(a, els);
                self.indent -= 1;
                self.line(a, "}");
                return all and d4;
            }
        }
        self.line(a, "}");
        // No `else`: control can skip every arm.
        return false;
    }

    /// The secondary name text of node `n` (its `y` span — the `for`
    /// index capture).
    fn yname(self: *Self, n: i32) []u8 {
        var u: usize = @as(usize, n);
        return self.src[self.nodes[u].yoff .. self.nodes[u].yoff + self.nodes[u].ylen];
    }

    /// `Emitter::emit_for` (SPEC §29.2): evaluate the iterable ONCE into
    /// `__kd_for{N}`, walk a `uintptr_t __kd_fi{N}` from 0 to the length
    /// (the compile-time length of an array, the runtime `.len` of a
    /// slice), bind the element by value each iteration (and, for the
    /// index form, the index), with the loop-body scope carrying the raw
    /// continue-clause `__kd_fi{N} += 1;` so `continue` still advances.
    /// An untypeable iterable emits NOTHING (the Rust `return false` arm).
    fn emit_for(self: *Self, a: Allocator, n: i32) bool {
        var u: usize = @as(usize, n);
        var itn: i32 = self.nodes[u].a;
        var it_t: i64 = self.type_of_expr(itn);
        if (!et_is_arr(it_t) and !et_is_slice(it_t)) { return false; }
        var iter_cty: []u8 = "";
        var elem_ty: i64 = ET_NONE;
        if (et_is_arr(it_t)) {
            iter_cty = self.arr_c_name(a, it_t);
            elem_ty = self.arr_elem_of(it_t);
        } else {
            iter_cty = self.sl_c_name(a, it_t);
            elem_ty = et_slice_elem(it_t);
        }
        var nctr: i64 = self.for_count;
        self.for_count += 1;
        var iter_str: []u8 = self.emit_expr(a, itn);
        self.line(a, "{");
        self.indent += 1;
        var sb1: StrBuilder = StrBuilder.init(a);
        sb1.append(a, iter_cty);
        sb1.append(a, " __kd_for");
        sb1.append_i64(a, nctr);
        sb1.append(a, " = ");
        sb1.append(a, iter_str);
        sb1.append(a, ";");
        var s1: []u8 = sb1.build(a);
        sb1.deinit(a);
        self.line(a, s1);
        var sb2: StrBuilder = StrBuilder.init(a);
        sb2.append(a, "uintptr_t __kd_fi");
        sb2.append_i64(a, nctr);
        sb2.append(a, " = 0;");
        var s2: []u8 = sb2.build(a);
        sb2.deinit(a);
        self.line(a, s2);
        var sb3: StrBuilder = StrBuilder.init(a);
        sb3.append(a, "while (__kd_fi");
        sb3.append_i64(a, nctr);
        sb3.append(a, " < ");
        if (et_is_arr(it_t)) {
            sb3.append_i64(a, self.arr_len_of(it_t));
        } else {
            sb3.append(a, "__kd_for");
            sb3.append_i64(a, nctr);
            sb3.append(a, ".len");
        }
        sb3.append(a, ") {");
        var s3: []u8 = sb3.build(a);
        sb3.deinit(a);
        self.line(a, s3);
        // The loop-body scope: no AST continue-clause, the raw index
        // increment instead; the element/index binding types recorded.
        if ((self.nodes[u].flags & F_LABEL) != 0) {
            self.set_pending_label(self.nodes[u].zoff, self.nodes[u].zlen);
        }
        self.push_scope(a, true, 0 - 1, nctr);
        self.push_vt(a, self.nodes[u].xoff, self.nodes[u].xlen, elem_ty);
        if ((self.nodes[u].flags & F_IDX) != 0) {
            self.push_vt(a, self.nodes[u].yoff, self.nodes[u].ylen, ET_USIZE);
        }
        self.indent += 1;
        var sb4: StrBuilder = StrBuilder.init(a);
        sb4.append(a, self.cty_of(a, elem_ty));
        sb4.append(a, " kd_");
        sb4.append(a, self.xname(n));
        sb4.append(a, " = __kd_for");
        sb4.append_i64(a, nctr);
        if (et_is_arr(it_t)) {
            sb4.append(a, ".data[__kd_fi");
        } else {
            sb4.append(a, ".ptr[__kd_fi");
        }
        sb4.append_i64(a, nctr);
        sb4.append(a, "];");
        var s4: []u8 = sb4.build(a);
        sb4.deinit(a);
        self.line(a, s4);
        if ((self.nodes[u].flags & F_IDX) != 0) {
            var sb5: StrBuilder = StrBuilder.init(a);
            sb5.append(a, "uintptr_t kd_");
            sb5.append(a, self.yname(n));
            sb5.append(a, " = __kd_fi");
            sb5.append_i64(a, nctr);
            sb5.append(a, ";");
            var s5: []u8 = sb5.build(a);
            sb5.deinit(a);
            self.line(a, s5);
        }
        var diverged: bool = false;
        var cur: i32 = self.nodes[@as(usize, self.nodes[u].b)].a;
        while (cur >= 0) {
            diverged = self.emit_stmt(a, cur);
            if (diverged) { break; }
            cur = self.nodes[@as(usize, cur)].next;
        }
        if (!diverged) {
            self.flush_current(a);
        }
        var top: usize = self.sc_len - 1;
        // A labeled `for` (v0.176): the continue-label precedes the index
        // increment (a `continue :L` `goto`s here past the flushed
        // defers); the increment runs even when the body diverged, since
        // a deeper `goto` still targets it.
        var fhas_lbl: bool = (self.nodes[u].flags & F_LABEL) != 0;
        if (fhas_lbl) {
            var sbfl: StrBuilder = StrBuilder.init(a);
            sbfl.append(a, "__kd_cont_");
            sbfl.append(a, self.src[self.nodes[u].zoff .. self.nodes[u].zoff + self.nodes[u].zlen]);
            sbfl.append(a, ":;");
            var sfl: []u8 = sbfl.build(a);
            sbfl.deinit(a);
            self.line(a, sfl);
        }
        if (!diverged or fhas_lbl) {
            self.emit_loop_cont(a, top);
        }
        self.pop_scope();
        self.indent -= 1;
        self.line(a, "}");
        self.indent -= 1;
        self.line(a, "}");
        // A labeled `for` places its break-label past the outer block
        // close, so `break :L` lands beyond the whole loop.
        if (fhas_lbl) {
            var sbfb: StrBuilder = StrBuilder.init(a);
            sbfb.append(a, "__kd_brk_");
            sbfb.append(a, self.src[self.nodes[u].zoff .. self.nodes[u].zoff + self.nodes[u].zlen]);
            sbfb.append(a, ":;");
            var sfb: []u8 = sbfb.build(a);
            sbfb.deinit(a);
            self.line(a, sfb);
        }
        // A `for` may iterate zero times, so it never diverges.
        return false;
    }

    /// `Emitter::emit_stmt`. Returns true if the statement unconditionally
    /// transfers control.
    fn emit_stmt(self: *Self, a: Allocator, n: i32) bool {
        var u: usize = @as(usize, n);
        var k: u8 = self.nodes[u].kind;
        if (k == ND_LET) {
            // The binding's type: annotation, else inferred (i64 fallback).
            var ann: i32 = self.nodes[u].a;
            var lty: i64 = ET_NONE;
            var ct: []u8 = "";
            if (ann >= 0) {
                lty = self.resolve_ty(ann);
                ct = self.cty(a, ann);
            } else {
                lty = self.type_of_expr(self.nodes[u].b);
                if (lty == ET_NONE) { lty = ET_I64; }
                ct = self.cty_of(a, lty);
            }
            // `var x = try e;` hoists the error propagation (which may
            // early-return) and binds the unwrapped payload, coerced back
            // to the binding's type.
            var es: []u8 = "";
            var ini: i32 = self.nodes[u].b;
            if (ini >= 0 and self.nodes[@as(usize, ini)].kind == ND_TRY) {
                var pay: []u8 = self.emit_try(a, self.nodes[@as(usize, ini)].a);
                es = self.coerce_str(a, pay, self.try_payload_type(self.nodes[@as(usize, ini)].a), lty);
            } else {
                es = self.emit_coerced(a, ini, lty);
            }
            var sb: StrBuilder = StrBuilder.init(a);
            if ((self.nodes[u].flags & F_CONST) != 0) { sb.append(a, "const "); }
            sb.append(a, ct);
            sb.append(a, " kd_");
            sb.append(a, self.xname(n));
            sb.append(a, " = ");
            sb.append(a, es);
            sb.append(a, ";");
            var s: []u8 = sb.build(a);
            sb.deinit(a);
            self.line(a, s);
            self.push_vt(a, self.nodes[u].xoff, self.nodes[u].xlen, lty);
            return false;
        }
        if (k == ND_ASSIGN) {
            self.emit_assign(a, n);
            return false;
        }
        if (k == ND_PASSIGN) {
            self.emit_place_assign(a, n);
            return false;
        }
        if (k == ND_TRY) {
            // `try e;` as a bare statement: hoist the propagation, discard
            // the unwrapped payload.
            var tval: []u8 = self.emit_try(a, self.nodes[u].a);
            var sbt: StrBuilder = StrBuilder.init(a);
            sbt.append(a, "(void)(");
            sbt.append(a, tval);
            sbt.append(a, ");");
            var st: []u8 = sbt.build(a);
            sbt.deinit(a);
            self.line(a, st);
            return false;
        }
        if (k == ND_RETURN) {
            var v: i32 = self.nodes[u].a;
            var has: bool = false;
            var es: []u8 = "";
            var inc_err: bool = false;
            if (v >= 0) {
                if (self.nodes[@as(usize, v)].kind == ND_TRY) {
                    // `return try e;` — the propagation early-returns
                    // inside emit_try; the value returned HERE is the
                    // success payload (not an error edge).
                    var pay: []u8 = self.emit_try(a, self.nodes[@as(usize, v)].a);
                    es = self.coerce_str(a, pay, self.try_payload_type(self.nodes[@as(usize, v)].a), self.cur_ret);
                    has = true;
                } else {
                    es = self.emit_coerced(a, v, self.cur_ret);
                    has = true;
                    // `return error.X;` is an error-return edge: errdefers
                    // run too.
                    if (self.nodes[@as(usize, v)].kind == ND_ERRLIT) { inc_err = true; }
                }
            } else {
                // `return;` in a `!void` function is the success return:
                // construct the payload-less value (SPEC §12.3).
                if (et_is_erru(self.cur_ret) and self.eu_payload_of(self.cur_ret) == ET_VOID) {
                    var sbv: StrBuilder = StrBuilder.init(a);
                    sbv.append(a, "((");
                    sbv.append(a, self.eu_c_name(a, self.cur_ret));
                    sbv.append(a, "){ .err = 0 })");
                    es = sbv.build(a);
                    sbv.deinit(a);
                    has = true;
                }
            }
            self.finish_return(a, has, es, inc_err);
            return true;
        }
        if (k == ND_IF) {
            if ((self.nodes[u].flags & F_CAP) != 0) {
                return self.emit_if_capture(a, n);
            }
            return self.emit_if(a, n);
        }
        if (k == ND_WHILE) {
            var cs: []u8 = self.emit_expr(a, self.nodes[u].a);
            var sb: StrBuilder = StrBuilder.init(a);
            sb.append(a, "while (");
            sb.append(a, cs);
            sb.append(a, ") {");
            var s: []u8 = sb.build(a);
            sb.deinit(a);
            self.line(a, s);
            if ((self.nodes[u].flags & F_LABEL) != 0) {
                self.set_pending_label(self.nodes[u].xoff, self.nodes[u].xlen);
            }
            var d: bool = self.emit_block(a, self.nodes[u].c, true, self.nodes[u].b);
            if (d) { }
            self.line(a, "}");
            // A labeled loop's break-label sits past the closing brace, so
            // a `break :L` `goto` lands beyond nested loops too (v0.176).
            if ((self.nodes[u].flags & F_LABEL) != 0) {
                var sbb: StrBuilder = StrBuilder.init(a);
                sbb.append(a, "__kd_brk_");
                sbb.append(a, self.xname(n));
                sbb.append(a, ":;");
                var sb2: []u8 = sbb.build(a);
                sbb.deinit(a);
                self.line(a, sb2);
            }
            // A `while` may iterate zero times, so it never diverges.
            return false;
        }
        if (k == ND_FOR) {
            return self.emit_for(a, n);
        }
        if (k == ND_BREAK) {
            if ((self.nodes[u].flags & F_LABEL) != 0) {
                // `break :L` (v0.176): flush out to and including L's
                // scope, then `goto` its break-label past the loop close.
                var bl: i64 = self.flush_to_labeled_loop(a, self.xname(n));
                if (bl >= 0) { }
                var sbg: StrBuilder = StrBuilder.init(a);
                sbg.append(a, "goto __kd_brk_");
                sbg.append(a, self.xname(n));
                sbg.append(a, ";");
                var sg: []u8 = sbg.build(a);
                sbg.deinit(a);
                self.line(a, sg);
                return true;
            }
            var i: i64 = self.flush_to_loop(a);
            if (i >= 0) { }
            self.line(a, "break;");
            return true;
        }
        if (k == ND_CONTINUE) {
            if ((self.nodes[u].flags & F_LABEL) != 0) {
                // `continue :L` (v0.176): flush out to L's scope, then
                // `goto` its continue-label — the TARGET runs L's clause,
                // so it is not emitted here.
                var cl: i64 = self.flush_to_labeled_loop(a, self.xname(n));
                if (cl >= 0) { }
                var sbg2: StrBuilder = StrBuilder.init(a);
                sbg2.append(a, "goto __kd_cont_");
                sbg2.append(a, self.xname(n));
                sbg2.append(a, ";");
                var sg2: []u8 = sbg2.build(a);
                sbg2.deinit(a);
                self.line(a, sg2);
                return true;
            }
            var j: i64 = self.flush_to_loop(a);
            if (j >= 0) { self.emit_loop_cont(a, @as(usize, j)); }
            self.line(a, "continue;");
            return true;
        }
        if (k == ND_DEFER) {
            // Register only; the body re-lowers at every exit edge.
            self.push_defer(a, self.nodes[u].a, false);
            return false;
        }
        if (k == ND_ERRDEFER) {
            // Register tagged as errdefer (v0.174): runs only on
            // error-return edges (`return error.X` / `try` propagation).
            self.push_defer(a, self.nodes[u].a, true);
            return false;
        }
        if (k == ND_BLOCK) {
            // A bare block is its own C scope.
            self.line(a, "{");
            var d: bool = self.emit_block(a, n, false, 0 - 1);
            self.line(a, "}");
            return d;
        }
        if (k == ND_SWITCH) {
            return self.emit_switch(a, n);
        }
        // In Test mode, `expect(c)` is a statement-level construct returning
        // a failure code through the deferred-return path (SPEC §4.5):
        // `if (!(<c>)) { <flush all defers> return 1; }`.
        if (self.is_test and k == ND_CALL and str_eq(self.xname(n), "expect")) {
            var earg: i32 = self.nodes[u].a;
            var ecs: []u8 = "0";
            if (earg >= 0) { ecs = self.emit_expr(a, earg); }
            var esb: StrBuilder = StrBuilder.init(a);
            esb.append(a, "if (!(");
            esb.append(a, ecs);
            esb.append(a, ")) {");
            var esl: []u8 = esb.build(a);
            esb.deinit(a);
            self.line(a, esl);
            self.indent += 1;
            self.flush_all(a, false);
            self.line(a, "return 1;");
            self.indent -= 1;
            self.line(a, "}");
            return false;
        }
        // An expression statement: `<e>;`.
        var es2: []u8 = self.emit_expr(a, n);
        var sb2: StrBuilder = StrBuilder.init(a);
        sb2.append(a, es2);
        sb2.append(a, ";");
        var s2: []u8 = sb2.build(a);
        sb2.deinit(a);
        self.line(a, s2);
        return false;
    }

    /// `Emitter::emit_block`: statements inside a fresh scope; fall-through
    /// flushes that scope's defers, a loop body then runs its
    /// continue-clause. The braces belong to the caller.
    fn emit_block(self: *Self, a: Allocator, block: i32, is_loop: bool, cont: i32) bool {
        self.indent += 1;
        self.push_scope(a, is_loop, cont, 0 - 1);
        var diverged: bool = false;
        var cur: i32 = self.nodes[@as(usize, block)].a;
        while (cur >= 0) {
            diverged = self.emit_stmt(a, cur);
            if (diverged) { break; }
            cur = self.nodes[@as(usize, cur)].next;
        }
        if (!diverged) {
            self.flush_current(a);
        }
        var top: usize = self.sc_len - 1;
        if (self.scopes[top].is_loop) {
            // A labeled loop's C continue-label precedes the clause (a
            // `continue :L` `goto`s here, past the already-flushed
            // defers); the clause runs even when the body diverged, since
            // a deeper `goto` still targets it (v0.176).
            var has_lbl: bool = self.scopes[top].llen > 0;
            if (has_lbl) {
                var sbl: StrBuilder = StrBuilder.init(a);
                sbl.append(a, "__kd_cont_");
                sbl.append(a, self.src[self.scopes[top].loff .. self.scopes[top].loff + self.scopes[top].llen]);
                sbl.append(a, ":;");
                var sl: []u8 = sbl.build(a);
                sbl.deinit(a);
                self.line(a, sl);
            }
            if (!diverged or has_lbl) {
                self.emit_loop_cont(a, top);
            }
        }
        self.pop_scope();
        self.indent -= 1;
        return diverged;
    }

    /// `Emitter::emit_switch` (v0.172): a C `switch` — one `case` line per
    /// value label (the LAST label overall opens the arm's brace), then a
    /// GNU `case <lo> ... <hi>:` per inclusive range; each arm body is a
    /// plain scope closed by `} break;` (SPEC's no-fallthrough); `else` is
    /// `default:`. The statement diverges iff it is TOTAL — an `else`
    /// present, or an enum scrutinee (sema proved coverage) — and every
    /// arm (and the else) diverges.
    fn emit_switch(self: *Self, a: Allocator, n: i32) bool {
        var u: usize = @as(usize, n);
        var scrut_ty: i64 = self.type_of_expr(self.nodes[u].a);
        var scrut: []u8 = self.emit_expr(a, self.nodes[u].a);
        var sb: StrBuilder = StrBuilder.init(a);
        sb.append(a, "switch (");
        sb.append(a, scrut);
        sb.append(a, ") {");
        var hl: []u8 = sb.build(a);
        sb.deinit(a);
        self.line(a, hl);
        self.indent += 1;
        var all_diverge: bool = true;
        var arm: i32 = self.nodes[u].b;
        while (arm >= 0) {
            var au: usize = @as(usize, arm);
            // Count the labels + ranges so the LAST case line opens `{`.
            var total: i64 = 0;
            var lc0: i32 = self.nodes[au].a;
            while (lc0 >= 0) : (lc0 = self.nodes[@as(usize, lc0)].next) { total += 1; }
            var rc0: i32 = self.nodes[au].b;
            while (rc0 >= 0) : (rc0 = self.nodes[@as(usize, rc0)].next) { total += 1; }
            var i: i64 = 0;
            var lcur: i32 = self.nodes[au].a;
            while (lcur >= 0) {
                var lc: []u8 = self.emit_switch_label(a, lcur, scrut_ty);
                var lsb: StrBuilder = StrBuilder.init(a);
                lsb.append(a, "case ");
                lsb.append(a, lc);
                lsb.append(a, ":");
                if (i + 1 >= total) { lsb.append(a, " {"); }
                var ll: []u8 = lsb.build(a);
                lsb.deinit(a);
                self.line(a, ll);
                i += 1;
                lcur = self.nodes[@as(usize, lcur)].next;
            }
            var rcur: i32 = self.nodes[au].b;
            while (rcur >= 0) {
                var ru: usize = @as(usize, rcur);
                var rsb: StrBuilder = StrBuilder.init(a);
                rsb.append(a, "case ");
                rsb.append_i64(a, self.nodes[ru].val);
                rsb.append(a, " ... ");
                rsb.append_i64(a, self.nodes[ru].val2);
                rsb.append(a, ":");
                if (i + 1 >= total) { rsb.append(a, " {"); }
                var rl: []u8 = rsb.build(a);
                rsb.deinit(a);
                self.line(a, rl);
                i += 1;
                rcur = self.nodes[ru].next;
            }
            if (total == 0) { self.line(a, "{"); }
            var d: bool = self.emit_block(a, self.nodes[au].c, false, 0 - 1);
            self.line(a, "} break;");
            all_diverge = all_diverge and d;
            arm = self.nodes[au].next;
        }
        if (self.nodes[u].c >= 0) {
            self.line(a, "default: {");
            var d2: bool = self.emit_block(a, self.nodes[u].c, false, 0 - 1);
            self.line(a, "} break;");
            all_diverge = all_diverge and d2;
        }
        self.indent -= 1;
        self.line(a, "}");
        var total_sw: bool = self.nodes[u].c >= 0 or et_is_enum(scrut_ty);
        return total_sw and all_diverge;
    }

    /// `emit_switch_label`: a bare `.V` takes its enum from the scrutinee;
    /// everything else (a qualified `Enum.V` Field, an integer literal, a
    /// named const) lowers through the ordinary expression path.
    fn emit_switch_label(self: *Self, a: Allocator, label: i32, scrut_ty: i64) []u8 {
        var lu: usize = @as(usize, label);
        if (self.nodes[lu].kind == ND_ENUMLIT and et_is_enum(scrut_ty)) {
            var sb: StrBuilder = StrBuilder.init(a);
            sb.append(a, self.en_c_name(a, scrut_ty));
            sb.append(a, "_");
            sb.append(a, self.xname(label));
            var out: []u8 = sb.build(a);
            sb.deinit(a);
            return out;
        }
        return self.emit_expr(a, label);
    }

    // -- functions ----------------------------------------------------------------------

    /// `format_params` into `sb`: `void` for an empty list, else
    /// `<cty> kd_<name>` joined by `, `.
    fn put_params(self: *Self, a: Allocator, sb: *StrBuilder, fnode: i32) void {
        var p: i32 = self.nodes[@as(usize, fnode)].a;
        if (p < 0) {
            sb.append(a, "void");
            return;
        }
        var first: bool = true;
        while (p >= 0) {
            var pu: usize = @as(usize, p);
            if (!first) { sb.append(a, ", "); }
            first = false;
            sb.append(a, self.cty(a, self.nodes[pu].a));
            sb.append(a, " kd_");
            sb.append(a, self.src[self.nodes[pu].xoff .. self.nodes[pu].xoff + self.nodes[pu].xlen]);
            p = self.nodes[pu].next;
        }
    }

    /// `emit_func` (+ `emit_func_named`): reset per-function state, open the
    /// signature line, seed the function scope with the parameter types,
    /// emit the body, close.
    fn emit_func(self: *Self, a: Allocator, fnode: i32) void {
        self.emit_func_named(a, fnode, "", self.xname(fnode));
    }

    /// `Emitter::emit_func_named` (v0.170): emit a function definition
    /// under `kd_<prefix><name>` — ordinary functions pass an empty
    /// prefix; struct functions pass `<Struct>_` so a `self` parameter is
    /// just an ordinary by-value struct parameter and the body reuses
    /// every lowering unchanged.
    fn emit_func_named(self: *Self, a: Allocator, fnode: i32, prefix: []u8, fname: []u8) void {
        var u: usize = @as(usize, fnode);
        // Reset the scope machinery and the per-function temp counters.
        self.sc_len = 0;
        self.df_len = 0;
        self.vt_len = 0;
        self.str_count = 0;
        self.idx_count = 0;
        self.for_count = 0;
        self.if_count = 0;
        self.try_count = 0;
        self.catch_count = 0;
        self.cur_ret = self.resolve_ty(self.nodes[u].b);
        var sb: StrBuilder = StrBuilder.init(a);
        sb.append(a, self.cty(a, self.nodes[u].b));
        sb.append(a, " kd_");
        sb.append(a, prefix);
        sb.append(a, fname);
        sb.append(a, "(");
        self.put_params(a, &sb, fnode);
        sb.append(a, ") {");
        var s: []u8 = sb.build(a);
        sb.deinit(a);
        self.line(a, s);
        // The function scope, seeded with the parameters.
        self.push_scope(a, false, 0 - 1, 0 - 1);
        var p: i32 = self.nodes[u].a;
        while (p >= 0) {
            var pu: usize = @as(usize, p);
            self.push_vt(a, self.nodes[pu].xoff, self.nodes[pu].xlen, self.resolve_ty(self.nodes[pu].a));
            p = self.nodes[pu].next;
        }
        // The body statements run inside the function scope itself — mirror
        // `emit_block(&f.body, scope)` by inlining its fall-through flush.
        self.indent += 1;
        var diverged: bool = false;
        var cur: i32 = self.nodes[@as(usize, self.nodes[u].c)].a;
        while (cur >= 0) {
            diverged = self.emit_stmt(a, cur);
            if (diverged) { break; }
            cur = self.nodes[@as(usize, cur)].next;
        }
        if (!diverged) {
            self.flush_current(a);
        }
        self.pop_scope();
        self.indent -= 1;
        // A `fn … !void` body that falls off its end returns success
        // (SPEC §12.3): the implicit exit constructs `{ .err = 0 }`.
        // QUIRK: the Rust arm emits AFTER emit_block restored the
        // function-level indent, so the line lands at column 0.
        if (!diverged and et_is_erru(self.cur_ret) and self.eu_payload_of(self.cur_ret) == ET_VOID) {
            var sbv2: StrBuilder = StrBuilder.init(a);
            sbv2.append(a, "return ((");
            sbv2.append(a, self.eu_c_name(a, self.cur_ret));
            sbv2.append(a, "){ .err = 0 });");
            var sv2: []u8 = sbv2.build(a);
            sbv2.deinit(a);
            self.line(a, sv2);
        }
        self.line(a, "}");
    }

    // -- liveness (SPEC §43.1) -------------------------------------------------------

    /// Collect every free-call name in a statement subtree into the pending
    /// worklist (the `collect_called_names` visitor: `Call{callee}` only —
    /// the subset has no method calls).
    fn collect_calls_expr(self: *Self, a: Allocator, pend: *PendList, pendm: *PendList, n: i32) void {
        if (n < 0) { return; }
        var u: usize = @as(usize, n);
        var k: u8 = self.nodes[u].kind;
        if (k == ND_CALL) {
            pend.push(a, self.nodes[u].xoff, self.nodes[u].xlen);
            var cur: i32 = self.nodes[u].a;
            while (cur >= 0) {
                self.collect_calls_expr(a, pend, pendm, cur);
                cur = self.nodes[@as(usize, cur)].next;
            }
            return;
        }
        if (k == ND_UNARY or k == ND_COMPTIME or k == ND_FIELD or k == ND_UNWRAP or k == ND_TRY or k == ND_ADDROF or k == ND_DEREF) {
            self.collect_calls_expr(a, pend, pendm, self.nodes[u].a);
            return;
        }
        if (k == ND_BIN or k == ND_INDEX or k == ND_ORELSE or k == ND_CATCH) {
            self.collect_calls_expr(a, pend, pendm, self.nodes[u].a);
            self.collect_calls_expr(a, pend, pendm, self.nodes[u].b);
            return;
        }
        if (k == ND_SLICEX) {
            self.collect_calls_expr(a, pend, pendm, self.nodes[u].a);
            self.collect_calls_expr(a, pend, pendm, self.nodes[u].b);
            self.collect_calls_expr(a, pend, pendm, self.nodes[u].c);
            return;
        }
        if (k == ND_ALIT) {
            var alc: i32 = self.nodes[u].b;
            while (alc >= 0) {
                self.collect_calls_expr(a, pend, pendm, alc);
                alc = self.nodes[@as(usize, alc)].next;
            }
            return;
        }
        if (k == ND_SLIT) {
            // `visit_expr` walks a struct literal's initializer values.
            var fcur: i32 = self.nodes[u].a;
            while (fcur >= 0) {
                self.collect_calls_expr(a, pend, pendm, self.nodes[@as(usize, fcur)].a);
                fcur = self.nodes[@as(usize, fcur)].next;
            }
            return;
        }
        if (k == ND_MCALL) {
            // `MethodCall{method}` contributes a METHOD name (name-level,
            // receiver-agnostic, SPEC §43.1); receiver and args walk.
            pendm.push(a, self.nodes[u].xoff, self.nodes[u].xlen);
            self.collect_calls_expr(a, pend, pendm, self.nodes[u].a);
            var mcur: i32 = self.nodes[u].b;
            while (mcur >= 0) {
                self.collect_calls_expr(a, pend, pendm, mcur);
                mcur = self.nodes[@as(usize, mcur)].next;
            }
            return;
        }
        if (k == ND_BUILTIN) {
            // `visit_expr` recurses into a builtin's arguments (the `@as`
            // value may contain calls; the type-name Ident is harmless).
            var bcur: i32 = self.nodes[u].a;
            while (bcur >= 0) {
                self.collect_calls_expr(a, pend, pendm, bcur);
                bcur = self.nodes[@as(usize, bcur)].next;
            }
            return;
        }
    }

    fn collect_calls_block(self: *Self, a: Allocator, pend: *PendList, pendm: *PendList, block: i32) void {
        if (block < 0) { return; }
        var cur: i32 = self.nodes[@as(usize, block)].a;
        while (cur >= 0) {
            self.collect_calls_stmt(a, pend, pendm, cur);
            cur = self.nodes[@as(usize, cur)].next;
        }
    }

    fn collect_calls_stmt(self: *Self, a: Allocator, pend: *PendList, pendm: *PendList, n: i32) void {
        if (n < 0) { return; }
        var u: usize = @as(usize, n);
        var k: u8 = self.nodes[u].kind;
        if (k == ND_LET or k == ND_ASSIGN) {
            var v: i32 = self.nodes[u].b;
            if (k == ND_ASSIGN) { v = self.nodes[u].a; }
            self.collect_calls_expr(a, pend, pendm, v);
            return;
        }
        if (k == ND_PASSIGN) {
            // `visit_stmt_exprs` visits the place, then the value.
            self.collect_calls_expr(a, pend, pendm, self.nodes[u].a);
            self.collect_calls_expr(a, pend, pendm, self.nodes[u].b);
            return;
        }
        if (k == ND_RETURN) {
            self.collect_calls_expr(a, pend, pendm, self.nodes[u].a);
            return;
        }
        if (k == ND_IF) {
            self.collect_calls_expr(a, pend, pendm, self.nodes[u].a);
            self.collect_calls_block(a, pend, pendm, self.nodes[u].b);
            self.collect_calls_stmt(a, pend, pendm, self.nodes[u].c);
            return;
        }
        if (k == ND_WHILE) {
            self.collect_calls_expr(a, pend, pendm, self.nodes[u].a);
            self.collect_calls_stmt(a, pend, pendm, self.nodes[u].b);
            self.collect_calls_block(a, pend, pendm, self.nodes[u].c);
            return;
        }
        if (k == ND_FOR) {
            self.collect_calls_expr(a, pend, pendm, self.nodes[u].a);
            self.collect_calls_block(a, pend, pendm, self.nodes[u].b);
            return;
        }
        if (k == ND_DEFER or k == ND_ERRDEFER) {
            self.collect_calls_stmt(a, pend, pendm, self.nodes[u].a);
            return;
        }
        if (k == ND_BLOCK) {
            self.collect_calls_block(a, pend, pendm, n);
            return;
        }
        if (k == ND_SWITCH) {
            // `visit_stmt_exprs`: scrutinee, then per arm its labels and
            // body, then the default block.
            self.collect_calls_expr(a, pend, pendm, self.nodes[u].a);
            var sw: i32 = self.nodes[u].b;
            while (sw >= 0) {
                var swu: usize = @as(usize, sw);
                var swl: i32 = self.nodes[swu].a;
                while (swl >= 0) {
                    self.collect_calls_expr(a, pend, pendm, swl);
                    swl = self.nodes[@as(usize, swl)].next;
                }
                self.collect_calls_block(a, pend, pendm, self.nodes[swu].c);
                sw = self.nodes[swu].next;
            }
            self.collect_calls_block(a, pend, pendm, self.nodes[u].c);
            return;
        }
        if (k == ND_BREAK or k == ND_CONTINUE) { return; }
        // An expression statement.
        self.collect_calls_expr(a, pend, pendm, n);
    }

    /// `live_functions` for the subset: the worklist closure of called
    /// names. Roots (SPEC §43.1): `main` in Program mode (the synthetic
    /// (0, 0) span, which `pend_text` decodes back to the text `main`);
    /// every `test` block body in Test mode — and a Test-mode module with
    /// NO test blocks has no root, so EVERY function is live
    /// (`LiveFns::all_of`). A name goes live once; going live marks EVERY
    /// top-level `fn` of that name and walks each of their bodies.
    fn compute_live(self: *Self, a: Allocator) void {
        var pend: PendList = PendList.init(a);
        var pendm: PendList = PendList.init(a);
        var done: PendList = PendList.init(a);
        var donem: PendList = PendList.init(a);
        if (self.is_test) {
            var any_test: bool = false;
            var tcur: i32 = self.root;
            while (tcur >= 0) {
                var tu: usize = @as(usize, tcur);
                if (self.nodes[tu].kind == ND_TEST) {
                    any_test = true;
                    self.collect_calls_block(a, &pend, &pendm, self.nodes[tu].a);
                }
                tcur = self.nodes[tu].next;
            }
            if (!any_test) {
                // The no-root fallback: mark everything live — free
                // functions AND struct functions (`LiveFns::all_of`).
                var fi: usize = 0;
                while (fi < self.fn_len) : (fi += 1) {
                    self.fns[fi].live = true;
                }
                var mj: usize = 0;
                while (mj < self.mt_count) : (mj += 1) {
                    self.mt_live[mj] = true;
                }
                pend.deinit(a);
                pendm.deinit(a);
                done.deinit(a);
                donem.deinit(a);
                return;
            }
        } else {
            pend.push(a, 0, 0);
        }
        // Worklist closure over BOTH name spaces (SPEC §43.1): free names
        // drain first, then method names (drain order does not affect the
        // final sets — both queues feed each other until empty).
        while (pend.len > 0 or pendm.len > 0) {
            if (pend.len > 0) {
                pend.len -= 1;
                var noff: usize = pend.offs[pend.len];
                var nlen: usize = pend.lens[pend.len];
                var name: []u8 = self.pend_text(noff, nlen);
                if (done.contains(self.src, name)) { continue; }
                done.push(a, noff, nlen);
                // Mark and walk every function of this name.
                var i: usize = 0;
                while (i < self.fn_len) : (i += 1) {
                    var fname: []u8 = self.src[self.fns[i].off .. self.fns[i].off + self.fns[i].len];
                    if (str_eq(fname, name)) {
                        self.fns[i].live = true;
                        var fu: usize = @as(usize, self.fns[i].node);
                        self.collect_calls_block(a, &pend, &pendm, self.nodes[fu].c);
                    }
                }
            } else {
                pendm.len -= 1;
                var moff: usize = pendm.offs[pendm.len];
                var mlen: usize = pendm.lens[pendm.len];
                var mname: []u8 = self.src[moff .. moff + mlen];
                if (donem.contains(self.src, mname)) { continue; }
                donem.push(a, moff, mlen);
                // Name-level: the method of this name on EVERY struct goes
                // live; each of their bodies is walked.
                var mi: usize = 0;
                while (mi < self.mt_count) : (mi += 1) {
                    var off2: usize = @as(usize, self.mt_noff[mi]);
                    var len2: usize = @as(usize, self.mt_nlen[mi]);
                    if (str_eq(self.src[off2 .. off2 + len2], mname)) {
                        self.mt_live[mi] = true;
                        var mnu: usize = @as(usize, self.mt_node[mi]);
                        self.collect_calls_block(a, &pend, &pendm, self.nodes[mnu].c);
                    }
                }
            }
        }
        pend.deinit(a);
        pendm.deinit(a);
        done.deinit(a);
        donem.deinit(a);
    }

    /// The text of a pending name: a span into `src` — except the synthetic
    /// root `main`, marked by the (0, 0) span (no source bytes spell it: the
    /// module may call `main` nowhere).
    fn pend_text(self: *Self, off: usize, len: usize) []u8 {
        if (len == 0) { return "main"; }
        return self.src[off .. off + len];
    }

    // -- top-level passes -----------------------------------------------------------------

    /// `collect_signatures` for the subset: name span + resolved return type
    /// of every top-level `fn`.
    fn collect_signatures(self: *Self, a: Allocator) void {
        // The `*T` pre-pass MUST precede any resolve_ty below (v0.175).
        self.pt_collect(a);
        var cur: i32 = self.root;
        while (cur >= 0) {
            var u: usize = @as(usize, cur);
            if (self.nodes[u].kind == ND_FN) {
                var ps: i64 = @as(i64, self.fp_count);
                var pc: i64 = 0;
                var pp: i32 = self.nodes[u].a;
                while (pp >= 0) {
                    self.push_fp(a, self.resolve_ty(self.nodes[@as(usize, pp)].a));
                    pc += 1;
                    pp = self.nodes[@as(usize, pp)].next;
                }
                self.push_fn(a, self.nodes[u].xoff, self.nodes[u].xlen, self.resolve_ty(self.nodes[u].b), cur, ps, pc);
            }
            if (self.nodes[u].kind == ND_STRUCT) {
                // Register every struct function's return AND parameter
                // types under (struct, name) — the `method_ret` /
                // `method_params` mirror (v0.170/v0.172; every param
                // resolves via `resolve_ty`, `self`'s annotation included
                // — the EMITTER rule). The rows double as the emission
                // worklist (nodes + liveness).
                var scode: i64 = ET_STRUCT_BASE + @as(i64, self.st_index_of(self.nodes[u].xoff, self.nodes[u].xlen));
                var m: i32 = self.nodes[u].b;
                while (m >= 0) {
                    var mu: usize = @as(usize, m);
                    var mps: i64 = @as(i64, self.fp_count);
                    var mpc: i64 = 0;
                    var mp: i32 = self.nodes[mu].a;
                    while (mp >= 0) {
                        self.push_fp(a, self.resolve_ty(self.nodes[@as(usize, mp)].a));
                        mpc += 1;
                        mp = self.nodes[@as(usize, mp)].next;
                    }
                    self.push_mt(a, scode, self.nodes[mu].xoff, self.nodes[mu].xlen, self.resolve_ty(self.nodes[mu].b), m, mps, mpc);
                    m = self.nodes[mu].next;
                }
            }
            cur = self.nodes[u].next;
        }
    }

    /// The struct-table index for a name SPAN (collect-time helper: the
    /// table is already populated by `st_collect`, so the span always
    /// resolves; `0` is the defensive miss).
    fn st_index_of(self: *Self, off: usize, len: usize) i64 {
        var i: usize = 0;
        while (i < self.st_count) : (i += 1) {
            if (self.st_name_off[i] == @as(i64, off) and self.st_name_len[i] == @as(i64, len)) {
                return @as(i64, i);
            }
        }
        return 0;
    }

    fn push_mt(self: *Self, a: Allocator, sid: i64, noff: usize, nlen: usize, ret: i64, node: i32, pstart: i64, pcount: i64) void {
        if (self.mt_count == self.mt_sid.len) {
            var g0: []i64 = alloc(a, i64, self.mt_sid.len * 2);
            var g1: []i64 = alloc(a, i64, self.mt_noff.len * 2);
            var g2: []i64 = alloc(a, i64, self.mt_nlen.len * 2);
            var g3: []i64 = alloc(a, i64, self.mt_ret.len * 2);
            var g4: []i32 = alloc(a, i32, self.mt_node.len * 2);
            var g5: []bool = alloc(a, bool, self.mt_live.len * 2);
            var g6: []i64 = alloc(a, i64, self.mt_p_start.len * 2);
            var g7: []i64 = alloc(a, i64, self.mt_p_count.len * 2);
            var i: usize = 0;
            while (i < self.mt_count) : (i += 1) {
                g0[i] = self.mt_sid[i];
                g1[i] = self.mt_noff[i];
                g2[i] = self.mt_nlen[i];
                g3[i] = self.mt_ret[i];
                g4[i] = self.mt_node[i];
                g5[i] = self.mt_live[i];
                g6[i] = self.mt_p_start[i];
                g7[i] = self.mt_p_count[i];
            }
            free(a, self.mt_sid);
            free(a, self.mt_noff);
            free(a, self.mt_nlen);
            free(a, self.mt_ret);
            free(a, self.mt_node);
            free(a, self.mt_live);
            free(a, self.mt_p_start);
            free(a, self.mt_p_count);
            self.mt_sid = g0;
            self.mt_noff = g1;
            self.mt_nlen = g2;
            self.mt_ret = g3;
            self.mt_node = g4;
            self.mt_live = g5;
            self.mt_p_start = g6;
            self.mt_p_count = g7;
        }
        self.mt_sid[self.mt_count] = sid;
        self.mt_noff[self.mt_count] = @as(i64, noff);
        self.mt_nlen[self.mt_count] = @as(i64, nlen);
        self.mt_ret[self.mt_count] = ret;
        self.mt_node[self.mt_count] = node;
        self.mt_live[self.mt_count] = false;
        self.mt_p_start[self.mt_count] = pstart;
        self.mt_p_count[self.mt_count] = pcount;
        self.mt_count += 1;
    }

    /// The method-table row for `(struct, name)`, or -1.
    fn mt_row_of(self: *Self, scode: i64, name: []u8) i64 {
        var i: usize = 0;
        while (i < self.mt_count) : (i += 1) {
            if (self.mt_sid[i] == scode) {
                var off: usize = @as(usize, self.mt_noff[i]);
                var len: usize = @as(usize, self.mt_nlen[i]);
                if (str_eq(self.src[off .. off + len], name)) {
                    return @as(i64, i);
                }
            }
        }
        return 0 - 1;
    }

    /// `method_ret[(sid, name)]`: the recorded return ET of the named
    /// struct function, `ET_NONE` when the struct has none of that name.
    fn mt_ret_of(self: *Self, scode: i64, name: []u8) i64 {
        var i: usize = 0;
        while (i < self.mt_count) : (i += 1) {
            if (self.mt_sid[i] == scode) {
                var off: usize = @as(usize, self.mt_noff[i]);
                var len: usize = @as(usize, self.mt_nlen[i]);
                if (str_eq(self.src[off .. off + len], name)) {
                    return self.mt_ret[i];
                }
            }
        }
        return ET_NONE;
    }

    // -- the slice interning scan (the sema-order mirror, v0.164) ---------------------
    //
    // The typedef section's content AND ORDER mirror `StructTable::slices()`
    // = sema's FIRST-INTERN order. `sema::check`'s interning walk (verified
    // against sema.rs and empirically via typedef-order probes):
    //
    //   pass 1: every fn signature, items in source order — params left to
    //           right, then the return type (sema.rs:530-535);
    //   pass 2: every top-level const's ANNOTATION, in source order
    //           (sema.rs:633; initializers go through const_eval, which can
    //           never intern — a string there is E0130);
    //   pass 3: every fn body, in source order. Per statement: Let resolves
    //           the annotation BEFORE the initializer (sema.rs:1618-1619);
    //           an index write checks the INDEX, then the base place, then
    //           the value (resolve_place, sema.rs:2520-2522); While checks
    //           cond, then the CONTINUE-CLAUSE, then the body
    //           (sema.rs:1855-1864); If cond/then/else; Defer in place.
    //           Per expression: Binary lhs→rhs, Index base→index, a string
    //           literal interns `[]u8` where it sits (sema.rs:2782),
    //           `alloc(a, T, n)` checks the allocator arg, then the COUNT
    //           arg, and interns `[]T` LAST (sema.rs:4635-4650; the type
    //           arg is never walked as an expression), `@as(T, e)` walks
    //           only `e`, and a `comptime` subtree NEVER interns (it folds
    //           through const_eval, sema.rs:2795-2813).

    /// Append `elem` to the interned-slice list if unseen (the
    /// `intern_slice` dedup-append).
    fn intern_elem(self: *Self, a: Allocator, e: i64) void {
        var i: usize = 0;
        while (i < self.sl_len) : (i += 1) {
            if (self.slices[i] == e) { return; }
        }
        if (self.sl_len == self.slices.len) {
            var grown: []i64 = alloc(a, i64, self.slices.len * 2);
            var j: usize = 0;
            while (j < self.sl_len) : (j += 1) { grown[j] = self.slices[j]; }
            free(a, self.slices);
            self.slices = grown;
        }
        self.slices[self.sl_len] = e;
        self.sl_len += 1;
    }

    /// A written `[]T` type interns its element; a written `[N]T` interns
    /// the `(elem, len)` array pair (v0.168). Unknown elements resolve to
    /// `None` before `wrap_type` and intern nothing.
    fn intern_ty(self: *Self, a: Allocator, n: i32) void {
        if (n < 0) { return; }
        var u: usize = @as(usize, n);
        if ((self.nodes[u].flags & F_ARRLIT) != 0) {
            var ae: i64 = self.base_code(self.xname(n));
            if (ae != ET_NONE) {
                var unused: i64 = self.arr_intern(a, ae, self.nodes[u].val);
                if (unused == 0) { }
            }
            return;
        }
        if ((self.nodes[u].flags & F_OPT) != 0) {
            var oe: i64 = self.base_code(self.xname(n));
            if (oe != ET_NONE) {
                var unused2: i64 = self.opt_intern(a, oe);
                if (unused2 == 0) { }
            }
            return;
        }
        if ((self.nodes[u].flags & F_ERR) != 0) {
            var ee: i64 = self.base_code(self.xname(n));
            if (ee != ET_NONE) {
                var unused3: i64 = self.eu_intern(a, ee);
                if (unused3 == 0) { }
            }
            return;
        }
        if ((self.nodes[u].flags & F_PTR) != 0) {
            // `*T` has no typedef table — sema's intern_ptr never affects
            // the emitted bytes (ids are structural `T*` spellings).
            return;
        }
        if ((self.nodes[u].flags & F_SLICE) == 0) { return; }
        var e: i64 = self.base_code(self.xname(n));
        if (e != ET_NONE) { self.intern_elem(a, e); }
    }

    /// The `resolve_place`/`resolve_index_base` interning order over a
    /// place chain: at an INDEX step the index expression is checked (and
    /// so interns) BEFORE the base is descended; a FIELD step is a type
    /// lookup only and just descends; the root name interns nothing.
    fn intern_place(self: *Self, a: Allocator, n: i32) void {
        if (n < 0) { return; }
        var u: usize = @as(usize, n);
        var k: u8 = self.nodes[u].kind;
        if (k == ND_INDEX) {
            self.intern_expr(a, self.nodes[u].b);
            self.intern_place(a, self.nodes[u].a);
            return;
        }
        if (k == ND_FIELD) {
            self.intern_place(a, self.nodes[u].a);
            return;
        }
        if (k == ND_DEREF) {
            // `p.* = e` / `&p.*`: the pointer expression checks fully.
            self.intern_expr(a, self.nodes[u].a);
            return;
        }
        // A non-chain place (unreachable behind the detector) checks as an
        // ordinary expression, exactly like sema's fallback arm.
        self.intern_expr(a, n);
    }

    /// Whether any active binding carries `name` (the scan's mirror of
    /// sema's `lookup(name)` presence check — the assoc-call gate).
    fn vt_has(self: *Self, name: []u8) bool {
        var i: usize = self.vt_len;
        while (i > 0) {
            i -= 1;
            var off: usize = self.vts[i].off;
            var len: usize = self.vts[i].len;
            if (str_eq(self.src[off .. off + len], name)) { return true; }
        }
        return false;
    }

    fn intern_expr(self: *Self, a: Allocator, n: i32) void {
        if (n < 0) { return; }
        var u: usize = @as(usize, n);
        var k: u8 = self.nodes[u].kind;
        if (k == ND_STR) {
            self.intern_elem(a, ET_U8);
            return;
        }
        if (k == ND_MCALL) {
            // `check_method_call`: an Ident receiver naming a struct and
            // NOT shadowed by a value binding is the associated/static
            // form — only the args check (in order). Otherwise the
            // receiver checks first, then the args (L→R), each against
            // its parameter type.
            var mrecv: i32 = self.nodes[u].a;
            var massoc: bool = false;
            if (mrecv >= 0 and self.nodes[@as(usize, mrecv)].kind == ND_IDENT) {
                if (!self.vt_has(self.xname(mrecv)) and self.st_code_of(self.xname(mrecv)) != ET_NONE) {
                    massoc = true;
                }
            }
            if (!massoc) {
                self.intern_expr(a, mrecv);
            }
            var marg: i32 = self.nodes[u].b;
            while (marg >= 0) {
                self.intern_expr(a, marg);
                marg = self.nodes[@as(usize, marg)].next;
            }
            return;
        }
        if (k == ND_SLIT) {
            // `check_struct_lit` checks each initializer value in SOURCE
            // order (against its field type — a `check_coerce` walk, so
            // interning inside the values fires here).
            var fcur: i32 = self.nodes[u].a;
            while (fcur >= 0) {
                self.intern_expr(a, self.nodes[@as(usize, fcur)].a);
                fcur = self.nodes[@as(usize, fcur)].next;
            }
            return;
        }
        if (k == ND_ERRLIT) {
            // `check_expr`'s ErrorLit arm interns the GLOBAL error name
            // wherever the literal is checked — body-check order joins
            // the pass-0 set members in one 1-based code space.
            var unused: i64 = self.er_intern(a, self.nodes[u].xoff, self.nodes[u].xlen);
            if (unused == 0) { }
            return;
        }
        if (k == ND_TRY) {
            self.intern_expr(a, self.nodes[u].a);
            return;
        }
        if (k == ND_CATCH) {
            // Operand first; a capturing form binds `|e|` (an i32) in a
            // scope around the default; the default checks second.
            self.intern_expr(a, self.nodes[u].a);
            if ((self.nodes[u].flags & F_CAP) != 0) {
                self.push_scope(a, false, 0 - 1, 0 - 1);
                self.push_vt(a, self.nodes[u].xoff, self.nodes[u].xlen, ET_I32);
                self.intern_expr(a, self.nodes[u].b);
                self.pop_scope();
            } else {
                self.intern_expr(a, self.nodes[u].b);
            }
            return;
        }
        if (k == ND_UNARY or k == ND_FIELD or k == ND_UNWRAP or k == ND_DEREF) {
            self.intern_expr(a, self.nodes[u].a);
            return;
        }
        if (k == ND_ADDROF) {
            // `resolve_lvalue_type`: each INDEX step checks its index
            // first, then the base; FIELD steps descend; a DEREF place
            // checks its pointer expression.
            self.intern_place(a, self.nodes[u].a);
            return;
        }
        if (k == ND_BIN or k == ND_INDEX or k == ND_ORELSE) {
            self.intern_expr(a, self.nodes[u].a);
            self.intern_expr(a, self.nodes[u].b);
            return;
        }
        if (k == ND_SLICEX) {
            // Sema checks base, lo, hi, THEN interns the result slice's
            // element (sema.rs:3110-3128). For a slice base that final
            // intern is a no-op; for an ARRAY base (v0.168) it can
            // FIRST-intern `[]elem` — which is why this scan carries the
            // full type environment.
            self.intern_expr(a, self.nodes[u].a);
            self.intern_expr(a, self.nodes[u].b);
            self.intern_expr(a, self.nodes[u].c);
            var sxt: i64 = self.type_of_expr(self.nodes[u].a);
            if (et_is_arr(sxt)) {
                self.intern_elem(a, self.arr_elem_of(sxt));
            }
            return;
        }
        if (k == ND_ALIT) {
            // Sema resolves the literal's `[N]T` FIRST (interning the
            // array), then checks the elements left to right.
            self.intern_ty(a, self.nodes[u].a);
            var alc: i32 = self.nodes[u].b;
            while (alc >= 0) {
                self.intern_expr(a, alc);
                alc = self.nodes[@as(usize, alc)].next;
            }
            return;
        }
        if (k == ND_CALL) {
            var callee: []u8 = self.xname(n);
            if (str_eq(callee, "alloc")) {
                // arg0 (allocator), then arg2 (count), then the `[]T`
                // intern — the type arg is never walked as an expression.
                var a0: i32 = self.nodes[u].a;
                var a1: i32 = 0 - 1;
                var a2: i32 = 0 - 1;
                var a3: i32 = 0 - 1;
                if (a0 >= 0) { a1 = self.nodes[@as(usize, a0)].next; }
                if (a1 >= 0) { a2 = self.nodes[@as(usize, a1)].next; }
                if (a2 >= 0) { a3 = self.nodes[@as(usize, a2)].next; }
                if (a2 >= 0 and a3 < 0) {
                    self.intern_expr(a, a0);
                    self.intern_expr(a, a2);
                    if (self.nodes[@as(usize, a1)].kind == ND_IDENT) {
                        var e: i64 = et_from_name(self.xname(a1));
                        if (e != ET_NONE) { self.intern_elem(a, e); }
                    }
                    return;
                }
                // Mis-shaped arity (sema's recovery: arg0, then args[2..],
                // no intern) — unreachable behind the detector.
                self.intern_expr(a, a0);
                if (a2 >= 0) { self.intern_expr(a, a2); }
                return;
            }
            var cur: i32 = self.nodes[u].a;
            while (cur >= 0) {
                self.intern_expr(a, cur);
                cur = self.nodes[@as(usize, cur)].next;
            }
            return;
        }
        if (k == ND_BUILTIN) {
            // `@as(T, e)`: only the value expression is walked (the type
            // arg resolves through `resolve_type_arg`, identifier-only).
            // v0.171: `@intFromEnum(e)` checks its one value argument;
            // `@enumFromInt(E, n)` resolves `E` without walking it and
            // checks only `n` — the same shape as `@as`.
            if (str_eq(self.xname(n), "as") or str_eq(self.xname(n), "enumFromInt")) {
                var b0: i32 = self.nodes[u].a;
                var b1: i32 = 0 - 1;
                if (b0 >= 0) { b1 = self.nodes[@as(usize, b0)].next; }
                self.intern_expr(a, b1);
            }
            if (str_eq(self.xname(n), "intFromEnum")) {
                self.intern_expr(a, self.nodes[u].a);
            }
            return;
        }
        // INT/BOOL/IDENT intern nothing; COMPTIME subtrees fold through
        // const_eval and can never intern.
    }

    fn intern_block(self: *Self, a: Allocator, block: i32) void {
        if (block < 0) { return; }
        self.push_scope(a, false, 0 - 1, 0 - 1);
        var cur: i32 = self.nodes[@as(usize, block)].a;
        while (cur >= 0) {
            self.intern_stmt(a, cur);
            cur = self.nodes[@as(usize, cur)].next;
        }
        self.pop_scope();
    }

    fn intern_stmt(self: *Self, a: Allocator, n: i32) void {
        if (n < 0) { return; }
        var u: usize = @as(usize, n);
        var k: u8 = self.nodes[u].kind;
        if (k == ND_LET) {
            self.intern_ty(a, self.nodes[u].a);
            self.intern_expr(a, self.nodes[u].b);
            // Record the binding so later SLICEX/for typing resolves
            // (annotation first, else the emit-identical inference).
            var slty: i64 = ET_NONE;
            if (self.nodes[u].a >= 0) {
                slty = self.resolve_ty(self.nodes[u].a);
            } else {
                slty = self.type_of_expr(self.nodes[u].b);
                if (slty == ET_NONE) { slty = ET_I64; }
            }
            self.push_vt(a, self.nodes[u].xoff, self.nodes[u].xlen, slty);
            return;
        }
        if (k == ND_ASSIGN) {
            self.intern_expr(a, self.nodes[u].a);
            return;
        }
        if (k == ND_FOR) {
            // iter first (sema 1888), then the body with the element and
            // index captures bound (1915).
            self.intern_expr(a, self.nodes[u].a);
            var fit: i64 = self.type_of_expr(self.nodes[u].a);
            var felem: i64 = ET_NONE;
            if (et_is_arr(fit)) { felem = self.arr_elem_of(fit); }
            if (et_is_slice(fit)) { felem = et_slice_elem(fit); }
            self.push_scope(a, false, 0 - 1, 0 - 1);
            self.push_vt(a, self.nodes[u].xoff, self.nodes[u].xlen, felem);
            if ((self.nodes[u].flags & F_IDX) != 0) {
                self.push_vt(a, self.nodes[u].yoff, self.nodes[u].ylen, ET_USIZE);
            }
            var fcur: i32 = self.nodes[@as(usize, self.nodes[u].b)].a;
            while (fcur >= 0) {
                self.intern_stmt(a, fcur);
                fcur = self.nodes[@as(usize, fcur)].next;
            }
            self.pop_scope();
            return;
        }
        if (k == ND_PASSIGN) {
            // `resolve_place`: every INDEX step checks its index expr
            // FIRST, then descends into its base (`resolve_index_base`
            // recurses the same way — outer indexes before inner ones);
            // FIELD steps are lookups only. The value is checked last.
            self.intern_place(a, self.nodes[u].a);
            self.intern_expr(a, self.nodes[u].b);
            return;
        }
        if (k == ND_RETURN) {
            self.intern_expr(a, self.nodes[u].a);
            return;
        }
        if (k == ND_IF) {
            // Cond first; a capture `|v|` (v0.173) binds the optional's
            // inner type in a scope wrapping the then-block (check_block
            // nests its own scope inside), else no binding.
            self.intern_expr(a, self.nodes[u].a);
            if ((self.nodes[u].flags & F_CAP) != 0) {
                var ict: i64 = self.type_of_expr(self.nodes[u].a);
                var iinner: i64 = ET_I64;
                if (et_is_opt(ict)) { iinner = self.opt_inner_of(ict); }
                self.push_scope(a, false, 0 - 1, 0 - 1);
                self.push_vt(a, self.nodes[u].xoff, self.nodes[u].xlen, iinner);
                self.intern_block(a, self.nodes[u].b);
                self.pop_scope();
            } else {
                self.intern_block(a, self.nodes[u].b);
            }
            self.intern_stmt(a, self.nodes[u].c);
            return;
        }
        if (k == ND_WHILE) {
            // cond, then the CONTINUE-CLAUSE, then the body (sema checks
            // the clause before the body although it runs after it).
            self.intern_expr(a, self.nodes[u].a);
            self.intern_stmt(a, self.nodes[u].b);
            self.intern_block(a, self.nodes[u].c);
            return;
        }
        if (k == ND_DEFER or k == ND_ERRDEFER) {
            self.intern_stmt(a, self.nodes[u].a);
            return;
        }
        if (k == ND_BLOCK) {
            self.intern_block(a, n);
            return;
        }
        if (k == ND_SWITCH) {
            // `check_switch` (v0.172): the scrutinee first (full check).
            // Enum scrutinee: an `.V` label and a MATCHING qualified
            // `Enum.V` are index lookups (never checked as expressions);
            // any other label checks fully. Integer scrutinee: EVERY value
            // label checks fully (then const-folds — folding never
            // interns). Any other scrutinee kind: labels are skipped
            // entirely (`check_switch_blocks`). Arm bodies check per arm
            // AFTER its labels; the `else` block last. Ranges carry only
            // literal bounds — nothing to walk.
            self.intern_expr(a, self.nodes[u].a);
            var swt: i64 = self.type_of_expr(self.nodes[u].a);
            var sarm: i32 = self.nodes[u].b;
            while (sarm >= 0) {
                var sau: usize = @as(usize, sarm);
                if (et_is_enum(swt) or et_is_int(swt)) {
                    var slab: i32 = self.nodes[sau].a;
                    while (slab >= 0) {
                        var slu: usize = @as(usize, slab);
                        var skip_label: bool = false;
                        if (et_is_enum(swt)) {
                            if (self.nodes[slu].kind == ND_ENUMLIT) { skip_label = true; }
                            if (self.nodes[slu].kind == ND_FIELD) {
                                var sfb: i32 = self.nodes[slu].a;
                                if (sfb >= 0 and self.nodes[@as(usize, sfb)].kind == ND_IDENT) {
                                    if (self.en_code_of(self.xname(sfb)) == swt) { skip_label = true; }
                                }
                            }
                        }
                        if (!skip_label) { self.intern_expr(a, slab); }
                        slab = self.nodes[slu].next;
                    }
                }
                self.intern_block(a, self.nodes[sau].c);
                sarm = self.nodes[sau].next;
            }
            self.intern_block(a, self.nodes[u].c);
            return;
        }
        if (k == ND_BREAK or k == ND_CONTINUE) { return; }
        self.intern_expr(a, n);
    }

    /// Interning passes 1+2 (type-blind): every fn signature, then every
    /// const annotation — run BEFORE `collect_signatures` so a `[N]T`
    /// param/return resolves against a populated array table.
    fn intern_scan_sigs(self: *Self, a: Allocator) void {
        // Pass 1: every fn signature — params left-to-right, then return.
        var cur: i32 = self.root;
        while (cur >= 0) {
            var u: usize = @as(usize, cur);
            if (self.nodes[u].kind == ND_FN) {
                var p: i32 = self.nodes[u].a;
                while (p >= 0) {
                    self.intern_ty(a, self.nodes[@as(usize, p)].a);
                    p = self.nodes[@as(usize, p)].next;
                }
                self.intern_ty(a, self.nodes[u].b);
            }
            cur = self.nodes[u].next;
        }
        // Pass 1b (v0.170): every struct function's signature — structs
        // in item order, methods in declaration order, params left to
        // right then the return. A leading `self` receiver's annotation
        // is NEVER resolved (sema substitutes the enclosing struct type
        // without touching it), so it interns nothing.
        cur = self.root;
        while (cur >= 0) {
            var u1: usize = @as(usize, cur);
            if (self.nodes[u1].kind == ND_STRUCT) {
                var m1: i32 = self.nodes[u1].b;
                while (m1 >= 0) {
                    var mu1: usize = @as(usize, m1);
                    var p1: i32 = self.nodes[mu1].a;
                    var pi: i64 = 0;
                    while (p1 >= 0) {
                        var pu1: usize = @as(usize, p1);
                        var skip_self: bool = pi == 0 and str_eq(self.src[self.nodes[pu1].xoff .. self.nodes[pu1].xoff + self.nodes[pu1].xlen], "self");
                        if (!skip_self) {
                            self.intern_ty(a, self.nodes[pu1].a);
                        }
                        pi += 1;
                        p1 = self.nodes[pu1].next;
                    }
                    self.intern_ty(a, self.nodes[mu1].b);
                    m1 = self.nodes[mu1].next;
                }
            }
            cur = self.nodes[u1].next;
        }
        // Pass 2: const annotations (initializers fold via const_eval and
        // never intern).
        cur = self.root;
        while (cur >= 0) {
            var u2: usize = @as(usize, cur);
            if (self.nodes[u2].kind == ND_CONST) {
                self.intern_ty(a, self.nodes[u2].a);
            }
            cur = self.nodes[u2].next;
        }
    }

    /// Interning pass 3 (type-aware): fn and test bodies — run AFTER
    /// `collect_signatures` (the SLICEX/for typing consults `fn_ret`).
    fn intern_scan_bodies(self: *Self, a: Allocator) void {
        var cur: i32 = 0 - 1;
        // Pass 3: fn AND test bodies, interleaved in source order (sema
        // checks both in one item loop; a test body is an ordinary block).
        // The scan carries the emit-identical type environment (scopes +
        // bindings) so the SLICEX-over-array intern point resolves.
        if (cur < 0) { cur = self.root; }
        while (cur >= 0) {
            var u3: usize = @as(usize, cur);
            if (self.nodes[u3].kind == ND_FN) {
                self.push_scope(a, false, 0 - 1, 0 - 1);
                var p3: i32 = self.nodes[u3].a;
                while (p3 >= 0) {
                    var pu3: usize = @as(usize, p3);
                    self.push_vt(a, self.nodes[pu3].xoff, self.nodes[pu3].xlen, self.resolve_ty(self.nodes[pu3].a));
                    p3 = self.nodes[pu3].next;
                }
                var bc3: i32 = self.nodes[@as(usize, self.nodes[u3].c)].a;
                while (bc3 >= 0) {
                    self.intern_stmt(a, bc3);
                    bc3 = self.nodes[@as(usize, bc3)].next;
                }
                self.pop_scope();
            } else if (self.nodes[u3].kind == ND_TEST) {
                self.intern_block(a, self.nodes[u3].a);
            } else if (self.nodes[u3].kind == ND_STRUCT) {
                // `check_struct_methods` (v0.170): each method body checks
                // in declaration order. A leading `self` receiver binds
                // the ENCLOSING STRUCT type regardless of its written
                // annotation (sema never resolves it); other params
                // resolve normally.
                var scode3: i64 = ET_STRUCT_BASE + @as(i64, self.st_index_of(self.nodes[u3].xoff, self.nodes[u3].xlen));
                var m3: i32 = self.nodes[u3].b;
                while (m3 >= 0) {
                    var mu3: usize = @as(usize, m3);
                    self.push_scope(a, false, 0 - 1, 0 - 1);
                    var mp3: i32 = self.nodes[mu3].a;
                    var mpi: i64 = 0;
                    while (mp3 >= 0) {
                        var mpu3: usize = @as(usize, mp3);
                        var is_self: bool = mpi == 0 and str_eq(self.src[self.nodes[mpu3].xoff .. self.nodes[mpu3].xoff + self.nodes[mpu3].xlen], "self");
                        if (is_self) {
                            self.push_vt(a, self.nodes[mpu3].xoff, self.nodes[mpu3].xlen, scode3);
                        } else {
                            self.push_vt(a, self.nodes[mpu3].xoff, self.nodes[mpu3].xlen, self.resolve_ty(self.nodes[mpu3].a));
                        }
                        mpi += 1;
                        mp3 = self.nodes[mpu3].next;
                    }
                    var mb3: i32 = self.nodes[@as(usize, self.nodes[mu3].c)].a;
                    while (mb3 >= 0) {
                        self.intern_stmt(a, mb3);
                        mb3 = self.nodes[@as(usize, mb3)].next;
                    }
                    self.pop_scope();
                    m3 = self.nodes[mu3].next;
                }
            }
            cur = self.nodes[u3].next;
        }
        // Leave the stacks clean for emission.
        self.sc_len = 0;
        self.vt_len = 0;
        self.df_len = 0;
    }

    /// `emit_one_slice`: the typedef and its three `static inline` helpers
    /// for one interned element type.
    fn emit_one_slice(self: *Self, a: Allocator, e: i64) void {
        var ec: []u8 = self.cty_of(a, e);
        var sn: []u8 = self.sl_c_name(a, et_slice_of(e));
        var sb: StrBuilder = StrBuilder.init(a);
        sb.append(a, "typedef struct { ");
        sb.append(a, ec);
        sb.append(a, " *ptr; uintptr_t len; } ");
        sb.append(a, sn);
        sb.append(a, ";");
        var s: []u8 = sb.build(a);
        sb.deinit(a);
        self.line(a, s);
        var g: StrBuilder = StrBuilder.init(a);
        g.append(a, "static inline ");
        g.append(a, ec);
        g.append(a, " ");
        g.append(a, sn);
        g.append(a, "_get(");
        g.append(a, sn);
        g.append(a, " s, int64_t i) { if (i < 0 || (uint64_t)i >= s.len) { fputs(\"panic: slice index out of bounds\\n\", stderr); exit(101); } return s.ptr[i]; }");
        var gs: []u8 = g.build(a);
        g.deinit(a);
        self.line(a, gs);
        var t: StrBuilder = StrBuilder.init(a);
        t.append(a, "static inline ");
        t.append(a, ec);
        t.append(a, " *");
        t.append(a, sn);
        t.append(a, "_at(");
        t.append(a, sn);
        t.append(a, " s, int64_t i) { if (i < 0 || (uint64_t)i >= s.len) { fputs(\"panic: slice index out of bounds\\n\", stderr); exit(101); } return s.ptr + i; }");
        var ts: []u8 = t.build(a);
        t.deinit(a);
        self.line(a, ts);
        var w: StrBuilder = StrBuilder.init(a);
        w.append(a, "static inline ");
        w.append(a, sn);
        w.append(a, " ");
        w.append(a, sn);
        w.append(a, "_alloc(uintptr_t n) { ");
        w.append(a, sn);
        w.append(a, " s; s.ptr = malloc(n * sizeof(");
        w.append(a, ec);
        w.append(a, ")); if (!s.ptr && n != 0) { fputs(\"panic: out of memory\\n\", stderr); exit(101); } s.len = n; return s; }");
        var ws: []u8 = w.build(a);
        w.deinit(a);
        self.line(a, ws);
    }

    /// `emit_one_array` (SPEC §14.3): the value-struct typedef (a
    /// zero-length array still reserves ONE storage element so the C stays
    /// portable) plus the bounds-checked `_get` / `_at` helpers, both
    /// checking against the TRUE length.
    fn emit_one_array(self: *Self, a: Allocator, t: i64) void {
        var ec: []u8 = self.cty_of(a, self.arr_elem_of(t));
        var alen: i64 = self.arr_len_of(t);
        var storage: i64 = alen;
        if (storage < 1) { storage = 1; }
        var cn: []u8 = self.arr_c_name(a, t);
        var sb: StrBuilder = StrBuilder.init(a);
        sb.append(a, "typedef struct { ");
        sb.append(a, ec);
        sb.append(a, " data[");
        sb.append_i64(a, storage);
        sb.append(a, "]; } ");
        sb.append(a, cn);
        sb.append(a, ";");
        var s1: []u8 = sb.build(a);
        sb.deinit(a);
        self.line(a, s1);
        var g: StrBuilder = StrBuilder.init(a);
        g.append(a, "static inline ");
        g.append(a, ec);
        g.append(a, " ");
        g.append(a, cn);
        g.append(a, "_get(");
        g.append(a, cn);
        g.append(a, " a, int64_t i) { if (i < 0 || (uint64_t)i >= ");
        g.append_i64(a, alen);
        g.append(a, ") { fputs(\"panic: array index out of bounds\\n\", stderr); exit(101); } return a.data[i]; }");
        var s2: []u8 = g.build(a);
        g.deinit(a);
        self.line(a, s2);
        var t2: StrBuilder = StrBuilder.init(a);
        t2.append(a, "static inline ");
        t2.append(a, ec);
        t2.append(a, " *");
        t2.append(a, cn);
        t2.append(a, "_at(const ");
        t2.append(a, cn);
        t2.append(a, " *a, int64_t i) { if (i < 0 || (uint64_t)i >= ");
        t2.append_i64(a, alen);
        t2.append(a, ") { fputs(\"panic: array index out of bounds\\n\", stderr); exit(101); } return (");
        t2.append(a, ec);
        t2.append(a, " *)a->data + i; }");
        var s3: []u8 = t2.build(a);
        t2.deinit(a);
        self.line(a, s3);
    }

    /// `emit_type_defs` for the subset: the dependency-ordered walk visits
    /// ARRAYS (in first-intern order) before SLICES (likewise) — scalar
    /// elements carry no dependencies, so each family keeps its intern
    /// order — then the section blank. Nothing at all when the module
    /// interns nothing (the Rust early-return keeps even the blank out).
    /// The typedef DEPENDENCY WALK (emit_c.rs `emit_type_defs`): seeds in
    /// the fixed order structs (id order) → arrays (intern order) →
    /// slices (intern order); each node's dependencies — a struct's field
    /// types, an array's/slice's element — are visited (and so emitted)
    /// first, deduplicated by a seen-set. v0.168's arrays-then-slices was
    /// this walk's struct-free special case.
    fn emit_type_defs(self: *Self, a: Allocator) void {
        if (self.sl_len == 0 and self.ar_count == 0 and self.st_count == 0 and self.en_count == 0 and self.opt_count == 0 and self.eu_count == 0) { return; }
        var total: usize = self.en_count + self.st_count + self.opt_count + self.eu_count + self.ar_count + self.sl_len;
        var seen: []bool = alloc(a, bool, total);
        var si: usize = 0;
        while (si < total) : (si += 1) { seen[si] = false; }
        // Enums seed FIRST (the Rust walk visits them before structs);
        // they have no dependencies of their own.
        var en_i: usize = 0;
        while (en_i < self.en_count) : (en_i += 1) {
            self.visit_type_def(a, seen, ET_ENUM_BASE + @as(i64, en_i));
        }
        var st_i: usize = 0;
        while (st_i < self.st_count) : (st_i += 1) {
            self.visit_type_def(a, seen, ET_STRUCT_BASE + @as(i64, st_i));
        }
        // Optionals seed after structs, before error unions/arrays/slices.
        var op_i: usize = 0;
        while (op_i < self.opt_count) : (op_i += 1) {
            self.visit_type_def(a, seen, ET_OPT_BASE + @as(i64, op_i));
        }
        // Error unions seed after optionals, before arrays/slices.
        var eu_i: usize = 0;
        while (eu_i < self.eu_count) : (eu_i += 1) {
            self.visit_type_def(a, seen, ET_ERRU_BASE + @as(i64, eu_i));
        }
        var ai: usize = 0;
        while (ai < self.ar_count) : (ai += 1) {
            self.visit_type_def(a, seen, ET_ARR_BASE + @as(i64, ai));
        }
        var i: usize = 0;
        while (i < self.sl_len) : (i += 1) {
            self.visit_type_def(a, seen, et_slice_of(self.slices[i]));
        }
        free(a, seen);
        self.blank(a);
    }

    /// The seen-set slot for a type-def node: structs first, then arrays,
    /// then slices (slice codes map through the intern table's index).
    fn type_def_slot(self: *Self, t: i64) i64 {
        if (et_is_enum(t)) { return t - ET_ENUM_BASE; }
        if (et_is_struct(t)) { return @as(i64, self.en_count) + (t - ET_STRUCT_BASE); }
        if (et_is_opt(t)) { return @as(i64, self.en_count + self.st_count) + (t - ET_OPT_BASE); }
        if (et_is_erru(t)) { return @as(i64, self.en_count + self.st_count + self.opt_count) + (t - ET_ERRU_BASE); }
        if (et_is_arr(t)) { return @as(i64, self.en_count + self.st_count + self.opt_count + self.eu_count) + (t - ET_ARR_BASE); }
        // A slice: find its element's intern index.
        var e: i64 = et_slice_elem(t);
        var i: usize = 0;
        while (i < self.sl_len) : (i += 1) {
            if (self.slices[i] == e) {
                return @as(i64, self.en_count + self.st_count + self.opt_count + self.eu_count + self.ar_count + i);
            }
        }
        return 0 - 1;
    }

    fn visit_type_def(self: *Self, a: Allocator, seen: []bool, t: i64) void {
        var slot: i64 = self.type_def_slot(t);
        if (slot < 0) { return; }
        if (seen[@as(usize, slot)]) { return; }
        seen[@as(usize, slot)] = true;
        if (et_is_enum(t)) {
            self.emit_one_enum(a, t);
            return;
        }
        if (et_is_opt(t)) {
            var oin: i64 = self.opt_inner_of(t);
            if (et_is_struct(oin) or et_is_arr(oin) or et_is_slice(oin) or et_is_enum(oin) or et_is_opt(oin) or et_is_erru(oin)) {
                self.visit_type_def(a, seen, oin);
            }
            self.emit_one_optional(a, t);
            return;
        }
        if (et_is_erru(t)) {
            var epl: i64 = self.eu_payload_of(t);
            if (et_is_struct(epl) or et_is_arr(epl) or et_is_slice(epl) or et_is_enum(epl) or et_is_opt(epl) or et_is_erru(epl)) {
                self.visit_type_def(a, seen, epl);
            }
            self.emit_one_error_union(a, t);
            return;
        }
        if (et_is_struct(t)) {
            var i: usize = @as(usize, t - ET_STRUCT_BASE);
            var start: usize = @as(usize, self.st_f_start[i]);
            var n: usize = @as(usize, self.st_f_count[i]);
            var j: usize = 0;
            while (j < n) : (j += 1) {
                var ft: i64 = self.sf_ty[start + j];
                if (et_is_struct(ft) or et_is_arr(ft) or et_is_slice(ft) or et_is_enum(ft) or et_is_opt(ft) or et_is_erru(ft)) {
                    self.visit_type_def(a, seen, ft);
                }
            }
            self.emit_one_struct(a, t);
            return;
        }
        if (et_is_arr(t)) {
            var ae: i64 = self.arr_elem_of(t);
            if (et_is_struct(ae) or et_is_arr(ae) or et_is_slice(ae) or et_is_enum(ae)) {
                self.visit_type_def(a, seen, ae);
            }
            self.emit_one_array(a, t);
            return;
        }
        var se: i64 = et_slice_elem(t);
        if (et_is_struct(se) or et_is_arr(se) or et_is_slice(se) or et_is_enum(se)) {
            self.visit_type_def(a, seen, se);
        }
        self.emit_one_slice(a, se);
    }

    /// `emit_one_enum`: `typedef enum { kd_enum_<N>_<V> = <val>, … } kd_enum_<N>;`
    /// — every enumerator carries its RESOLVED value explicitly; the
    /// degenerate variant-less enum (sema-invalid) keeps a placeholder so
    /// the output stays compilable.
    fn emit_one_enum(self: *Self, a: Allocator, t: i64) void {
        var i: usize = @as(usize, t - ET_ENUM_BASE);
        var start: usize = @as(usize, self.en_v_start[i]);
        var n: usize = @as(usize, self.en_v_count[i]);
        var cn: []u8 = self.en_c_name(a, t);
        var sb: StrBuilder = StrBuilder.init(a);
        sb.append(a, "typedef enum { ");
        if (n == 0) {
            sb.append(a, cn);
            sb.append(a, "__empty = 0");
        } else {
            var j: usize = 0;
            while (j < n) : (j += 1) {
                if (j > 0) { sb.append(a, ", "); }
                sb.append(a, cn);
                sb.append(a, "_");
                var voff: usize = @as(usize, self.ev_name_off[start + j]);
                var vlen: usize = @as(usize, self.ev_name_len[start + j]);
                sb.append(a, self.src[voff .. voff + vlen]);
                sb.append(a, " = ");
                sb.append_i64(a, self.ev_val[start + j]);
            }
        }
        sb.append(a, " } ");
        sb.append(a, cn);
        sb.append(a, ";");
        var out: []u8 = sb.build(a);
        sb.deinit(a);
        self.line(a, out);
    }

    /// `emit_one_optional`: `typedef struct { bool has; <inner> val; }`
    /// plus the inline `_orelse` / `_unwrap` helpers (`_unwrap` panics
    /// with exit 101 on null, SPEC §11.3).
    fn emit_one_optional(self: *Self, a: Allocator, t: i64) void {
        var oname: []u8 = self.opt_c_name(a, t);
        var icty: []u8 = self.cty_of(a, self.opt_inner_of(t));
        var sb: StrBuilder = StrBuilder.init(a);
        sb.append(a, "typedef struct { bool has; ");
        sb.append(a, icty);
        sb.append(a, " val; } ");
        sb.append(a, oname);
        sb.append(a, ";");
        var s1: []u8 = sb.build(a);
        sb.deinit(a);
        self.line(a, s1);
        var g: StrBuilder = StrBuilder.init(a);
        g.append(a, "static inline ");
        g.append(a, icty);
        g.append(a, " ");
        g.append(a, oname);
        g.append(a, "_orelse(");
        g.append(a, oname);
        g.append(a, " o, ");
        g.append(a, icty);
        g.append(a, " d) { return o.has ? o.val : d; }");
        var s2: []u8 = g.build(a);
        g.deinit(a);
        self.line(a, s2);
        var h: StrBuilder = StrBuilder.init(a);
        h.append(a, "static inline ");
        h.append(a, icty);
        h.append(a, " ");
        h.append(a, oname);
        h.append(a, "_unwrap(");
        h.append(a, oname);
        h.append(a, " o) { if (!o.has) { fputs(\"panic: unwrapped a null optional\\n\", stderr); exit(101); } return o.val; }");
        var s3: []u8 = h.build(a);
        h.deinit(a);
        self.line(a, s3);
    }

    /// `emit_one_error_union`: `typedef struct { int32_t err; <T> val; }`
    /// plus the inline `_catch` helper; a `!void` union carries only the
    /// `err` field and SKIPS the helper (its `catch` lowers lazily).
    fn emit_one_error_union(self: *Self, a: Allocator, t: i64) void {
        var ename: []u8 = self.eu_c_name(a, t);
        var pl: i64 = self.eu_payload_of(t);
        if (pl == ET_VOID) {
            var sbv: StrBuilder = StrBuilder.init(a);
            sbv.append(a, "typedef struct { int32_t err; } ");
            sbv.append(a, ename);
            sbv.append(a, ";");
            var sv: []u8 = sbv.build(a);
            sbv.deinit(a);
            self.line(a, sv);
            return;
        }
        var pcty: []u8 = self.cty_of(a, pl);
        var sb: StrBuilder = StrBuilder.init(a);
        sb.append(a, "typedef struct { int32_t err; ");
        sb.append(a, pcty);
        sb.append(a, " val; } ");
        sb.append(a, ename);
        sb.append(a, ";");
        var s1: []u8 = sb.build(a);
        sb.deinit(a);
        self.line(a, s1);
        var g: StrBuilder = StrBuilder.init(a);
        g.append(a, "static inline ");
        g.append(a, pcty);
        g.append(a, " ");
        g.append(a, ename);
        g.append(a, "_catch(");
        g.append(a, ename);
        g.append(a, " e, ");
        g.append(a, pcty);
        g.append(a, " d) { return e.err == 0 ? e.val : d; }");
        var s2: []u8 = g.build(a);
        g.deinit(a);
        self.line(a, s2);
    }

    /// `emit_one_struct`: `typedef struct { <cty> kd_<f>; … } kd_struct_<Name>;`
    /// — fields joined by single spaces, the empty struct spelling
    /// `char _unused;` (C forbids empty structs; `int` is the allocator's).
    fn emit_one_struct(self: *Self, a: Allocator, t: i64) void {
        var i: usize = @as(usize, t - ET_STRUCT_BASE);
        var start: usize = @as(usize, self.st_f_start[i]);
        var n: usize = @as(usize, self.st_f_count[i]);
        var sb: StrBuilder = StrBuilder.init(a);
        sb.append(a, "typedef struct { ");
        if (n == 0) {
            sb.append(a, "char _unused;");
        } else {
            var j: usize = 0;
            while (j < n) : (j += 1) {
                if (j > 0) { sb.append(a, " "); }
                sb.append(a, self.cty_of(a, self.sf_ty[start + j]));
                sb.append(a, " kd_");
                var noff: usize = @as(usize, self.sf_name_off[start + j]);
                var nlen: usize = @as(usize, self.sf_name_len[start + j]);
                sb.append(a, self.src[noff .. noff + nlen]);
                sb.append(a, ";");
            }
        }
        sb.append(a, " } ");
        sb.append(a, self.st_c_name(a, t));
        sb.append(a, ";");
        var out: []u8 = sb.build(a);
        sb.deinit(a);
        self.line(a, out);
    }

    fn emit_prelude(self: *Self, a: Allocator) void {
        self.put(a, "#include <stdint.h>\n");
        self.put(a, "#include <stdbool.h>\n");
        self.put(a, "#include <stdio.h>\n");
        self.put(a, "#include <stdlib.h>\n");
        self.put(a, "#include <string.h>\n");
        self.put(a, "#include <time.h>\n");
        self.put(a, "typedef struct { int _unused; } kd_allocator;\n");
        self.put(a, "static void kd_print(long long v) { printf(\"%lld\\n\", v); }\n");
        self.put(a, "static void kd_print_f64(double x) { printf(\"%g\\n\", x); }\n");
        self.put(a, "_Noreturn void kd_unreachable(void) { fputs(\"reached unreachable code\\n\", stderr); exit(101); }\n");
        self.blank(a);
    }

    /// `emit_consts`: fold each top-level const in source order; a failing
    /// fold skips the const (never a crash); a trailing blank if any.
    fn emit_consts(self: *Self, a: Allocator) void {
        var any: bool = false;
        var cur: i32 = self.root;
        while (cur >= 0) {
            var u: usize = @as(usize, cur);
            if (self.nodes[u].kind == ND_CONST) {
                var v: EvRes = self.eval(self.nodes[u].b);
                if (v.ok) {
                    var ct: []u8 = "";
                    if (self.nodes[u].a >= 0) {
                        ct = self.cty(a, self.nodes[u].a);
                    } else if (v.isb) {
                        ct = "bool";
                    } else {
                        ct = "int64_t";
                    }
                    var sb: StrBuilder = StrBuilder.init(a);
                    sb.append(a, "static const ");
                    sb.append(a, ct);
                    sb.append(a, " kd_");
                    sb.append(a, self.xname(cur));
                    sb.append(a, " = ");
                    sb.append(a, self.const_literal(a, v));
                    sb.append(a, ";");
                    var s: []u8 = sb.build(a);
                    sb.deinit(a);
                    self.line(a, s);
                    self.push_const(a, self.nodes[u].xoff, self.nodes[u].xlen, v.isb, v.val);
                    any = true;
                }
            }
            cur = self.nodes[u].next;
        }
        if (any) { self.blank(a); }
    }

    /// `emit_forward_decls`: one line per live function, then a blank.
    fn emit_forward_decls(self: *Self, a: Allocator) void {
        var any: bool = false;
        var i: usize = 0;
        while (i < self.fn_len) : (i += 1) {
            if (!self.fns[i].live) { continue; }
            var fnode: i32 = self.fns[i].node;
            var u: usize = @as(usize, fnode);
            var sb: StrBuilder = StrBuilder.init(a);
            sb.append(a, self.cty(a, self.nodes[u].b));
            sb.append(a, " kd_");
            sb.append(a, self.xname(fnode));
            sb.append(a, "(");
            self.put_params(a, &sb, fnode);
            sb.append(a, ");");
            var s: []u8 = sb.build(a);
            sb.deinit(a);
            self.line(a, s);
            any = true;
        }
        // Then every struct function (v0.170), declared alongside ordinary
        // ones as `kd_<Struct>_<method>` — name-level liveness gated.
        var mi: usize = 0;
        while (mi < self.mt_count) : (mi += 1) {
            if (!self.mt_live[mi]) { continue; }
            var mnode: i32 = self.mt_node[mi];
            var mu: usize = @as(usize, mnode);
            var sb2: StrBuilder = StrBuilder.init(a);
            sb2.append(a, self.cty(a, self.nodes[mu].b));
            sb2.append(a, " kd_");
            sb2.append(a, self.st_name_of(self.mt_sid[mi]));
            sb2.append(a, "_");
            sb2.append(a, self.xname(mnode));
            sb2.append(a, "(");
            self.put_params(a, &sb2, mnode);
            sb2.append(a, ");");
            var s2: []u8 = sb2.build(a);
            sb2.deinit(a);
            self.line(a, s2);
            any = true;
        }
        if (any) { self.blank(a); }
    }

    /// The bare source name of a struct code (no `kd_struct_` prefix).
    fn st_name_of(self: *Self, scode: i64) []u8 {
        var i: usize = @as(usize, scode - ET_STRUCT_BASE);
        var off: usize = @as(usize, self.st_name_off[i]);
        var len: usize = @as(usize, self.st_name_len[i]);
        return self.src[off .. off + len];
    }

    /// `emit_func_defs`: every live function, each followed by a blank —
    /// free functions first, then struct functions (v0.170), matching the
    /// forward-declaration order.
    fn emit_func_defs(self: *Self, a: Allocator) void {
        var i: usize = 0;
        while (i < self.fn_len) : (i += 1) {
            if (!self.fns[i].live) { continue; }
            self.emit_func(a, self.fns[i].node);
            self.blank(a);
        }
        var mi: usize = 0;
        while (mi < self.mt_count) : (mi += 1) {
            if (!self.mt_live[mi]) { continue; }
            var pfx: StrBuilder = StrBuilder.init(a);
            pfx.append(a, self.st_name_of(self.mt_sid[mi]));
            pfx.append(a, "_");
            var pl: []u8 = pfx.build(a);
            pfx.deinit(a);
            self.emit_func_named(a, self.mt_node[mi], pl, self.xname(self.mt_node[mi]));
            self.blank(a);
            free(a, pl);
        }
    }

    /// `emit_program_main`: the C entry point wiring `kd_main`.
    fn emit_program_main(self: *Self, a: Allocator) void {
        var is_int: bool = false;
        var cur: i32 = self.root;
        while (cur >= 0) {
            var u: usize = @as(usize, cur);
            if (self.nodes[u].kind == ND_FN and str_eq(self.xname(cur), "main")) {
                var rt: i64 = et_from_name(self.xname(self.nodes[u].b));
                is_int = et_is_int(rt);
                break;
            }
            cur = self.nodes[u].next;
        }
        if (is_int) {
            self.put(a, "int main(int argc, char **argv){ (void)argc;(void)argv; return (int) kd_main(); }\n");
        } else {
            self.put(a, "int main(int argc, char **argv){ (void)argc;(void)argv; kd_main(); return 0; }\n");
        }
    }

    // -- the test harness (EmitMode::Test, v0.166) ------------------------------------

    /// `emit_test_fn`: one `static int kd_test_<idx>(void)` per test block.
    /// Resets the scope machinery and the per-function temp counters —
    /// EXCEPT `str_count`, which the Rust `emit_test_fn` does not reset (the
    /// `__kd_str{N}` numbering continues across test functions; mirrored
    /// quirk). The trailing `return 0;` is unconditional.
    fn emit_test_fn(self: *Self, a: Allocator, idx: i64, tnode: i32) void {
        var u: usize = @as(usize, tnode);
        self.sc_len = 0;
        self.df_len = 0;
        self.vt_len = 0;
        self.idx_count = 0;
        self.for_count = 0;
        self.if_count = 0;
        self.try_count = 0;
        self.catch_count = 0;
        self.cur_ret = ET_I32;
        var sb: StrBuilder = StrBuilder.init(a);
        sb.append(a, "static int kd_test_");
        sb.append_i64(a, idx);
        sb.append(a, "(void) {");
        var s: []u8 = sb.build(a);
        sb.deinit(a);
        self.line(a, s);
        self.indent += 1;
        self.push_scope(a, false, 0 - 1, 0 - 1);
        var diverged: bool = false;
        var cur: i32 = self.nodes[@as(usize, self.nodes[u].a)].a;
        while (cur >= 0) {
            diverged = self.emit_stmt(a, cur);
            if (diverged) { break; }
            cur = self.nodes[@as(usize, cur)].next;
        }
        if (!diverged) {
            self.flush_current(a);
        }
        self.pop_scope();
        self.line(a, "return 0;");
        self.indent -= 1;
        self.line(a, "}");
    }

    /// `emit_test_harness`: every test function (each + a blank), the name
    /// and function-pointer tables (only when any test exists), a blank,
    /// then the driver `main` with the v0.150 `--filter`/`--bench` loop.
    fn emit_test_harness(self: *Self, a: Allocator) void {
        var total: i64 = 0;
        var cur: i32 = self.root;
        while (cur >= 0) {
            var u: usize = @as(usize, cur);
            if (self.nodes[u].kind == ND_TEST) {
                self.emit_test_fn(a, total, cur);
                self.blank(a);
                total += 1;
            }
            cur = self.nodes[u].next;
        }
        if (total > 0) {
            var nsb: StrBuilder = StrBuilder.init(a);
            nsb.append(a, "static const char *kd_test_names[] = { ");
            var first: bool = true;
            cur = self.root;
            while (cur >= 0) {
                var u2: usize = @as(usize, cur);
                if (self.nodes[u2].kind == ND_TEST) {
                    if (!first) { nsb.append(a, ", "); }
                    first = false;
                    var raw: []u8 = es_decode_str(a, self.src, self.nodes[u2].xoff, self.nodes[u2].xlen);
                    nsb.append_byte(a, 34);
                    nsb.append(a, es_c_escape(a, raw));
                    nsb.append_byte(a, 34);
                }
                cur = self.nodes[u2].next;
            }
            nsb.append(a, " };");
            var ns: []u8 = nsb.build(a);
            nsb.deinit(a);
            self.line(a, ns);
            var fsb: StrBuilder = StrBuilder.init(a);
            fsb.append(a, "static int (*kd_test_fns[])(void) = { ");
            var i: i64 = 0;
            while (i < total) : (i += 1) {
                if (i > 0) { fsb.append(a, ", "); }
                fsb.append(a, "kd_test_");
                fsb.append_i64(a, i);
            }
            fsb.append(a, " };");
            var fs: []u8 = fsb.build(a);
            fsb.deinit(a);
            self.line(a, fs);
        }
        self.blank(a);
        self.line(a, "int main(int argc, char **argv) {");
        self.indent += 1;
        self.line(a, "const char *filter = 0; int bench = 0;");
        self.line(a, "for (int ai = 1; ai < argc; ai++) {");
        self.indent += 1;
        self.line(a, "if (strcmp(argv[ai], \"--bench\") == 0) { bench = 1; }");
        self.line(a, "else if (strcmp(argv[ai], \"--filter\") == 0) { if (ai + 1 < argc) { filter = argv[++ai]; } }");
        self.line(a, "else { filter = argv[ai]; }");
        self.indent -= 1;
        self.line(a, "}");
        var tsb: StrBuilder = StrBuilder.init(a);
        tsb.append(a, "int total = ");
        tsb.append_i64(a, total);
        tsb.append(a, ";");
        var ts: []u8 = tsb.build(a);
        tsb.deinit(a);
        self.line(a, ts);
        self.line(a, "int failures = 0; int ran = 0;");
        if (total > 0) {
            self.line(a, "for (int ti = 0; ti < total; ti++) {");
            self.indent += 1;
            self.line(a, "if (filter && !strstr(kd_test_names[ti], filter)) { continue; }");
            self.line(a, "ran++;");
            self.line(a, "int rc; clock_t t0 = clock();");
            self.line(a, "rc = kd_test_fns[ti]();");
            self.line(a, "if (bench) {");
            self.indent += 1;
            self.line(a, "double ms = (double)(clock() - t0) * 1000.0 / (double)CLOCKS_PER_SEC;");
            self.line(a, "fprintf(stderr, \"%s: %.3f ms%s\\n\", kd_test_names[ti], ms, rc == 0 ? \"\" : \" (FAIL)\");");
            self.indent -= 1;
            self.line(a, "} else {");
            self.indent += 1;
            self.line(a, "fprintf(stderr, \"%s: %s\\n\", rc == 0 ? \"ok\" : \"FAIL\", kd_test_names[ti]);");
            self.indent -= 1;
            self.line(a, "}");
            self.line(a, "if (rc != 0) { failures++; }");
            self.indent -= 1;
            self.line(a, "}");
        }
        self.line(a, "fprintf(stderr, \"%d/%d tests passed%s\\n\", ran - failures, ran, filter ? \" (filtered)\" : \"\");");
        self.line(a, "return failures;");
        self.indent -= 1;
        self.line(a, "}");
    }

    /// The whole `emit_c::emit` pass sequence: the shared sections, then
    /// the mode-specific entry point (`EmitMode::Program` wires `kd_main`;
    /// `EmitMode::Test` emits the harness). The result is
    /// `self.out[0 .. self.out_len]`.
    fn run(self: *Self, a: Allocator) void {
        // Sema's pass order: enums FIRST (pass 0 — no dependencies), then
        // struct names + field types (0a/0b — field slices/arrays intern
        // here), then signatures, then bodies. Interning passes 1+2 fill
        // the array table BEFORE the signature collection resolves `[N]T`
        // params/returns; the type-aware body pass then consults those
        // signatures.
        self.en_collect(a);
        self.er_collect(a);
        self.st_collect(a);
        self.intern_scan_sigs(a);
        self.collect_signatures(a);
        self.intern_scan_bodies(a);
        self.compute_live(a);
        self.emit_prelude(a);
        self.emit_type_defs(a);
        self.emit_consts(a);
        self.emit_forward_decls(a);
        self.emit_func_defs(a);
        if (self.is_test) {
            self.emit_test_harness(a);
        } else {
            self.emit_program_main(a);
        }
    }
};

/// The liveness worklist / done-set: parallel span arrays. The synthetic
/// root name `main` is encoded as the (0, 0) span (see `Em.pend_text`).
pub const PendList = struct {
    offs: []usize,
    lens: []usize,
    len: usize,

    fn init(a: Allocator) Self {
        return PendList{ .offs = alloc(a, usize, 16), .lens = alloc(a, usize, 16), .len = 0 };
    }

    fn push(self: *Self, a: Allocator, off: usize, len: usize) void {
        if (self.len == self.offs.len) {
            var goffs: []usize = alloc(a, usize, self.offs.len * 2);
            var glens: []usize = alloc(a, usize, self.lens.len * 2);
            var i: usize = 0;
            while (i < self.len) : (i += 1) {
                goffs[i] = self.offs[i];
                glens[i] = self.lens[i];
            }
            free(a, self.offs);
            free(a, self.lens);
            self.offs = goffs;
            self.lens = glens;
        }
        self.offs[self.len] = off;
        self.lens[self.len] = len;
        self.len += 1;
    }

    /// Whether `name` is already recorded (`src` decodes the stored spans;
    /// the (0,0) span decodes to `main`).
    fn contains(self: *Self, src: []u8, name: []u8) bool {
        var i: usize = 0;
        while (i < self.len) : (i += 1) {
            var ent: []u8 = "main";
            if (self.lens[i] != 0) {
                ent = src[self.offs[i] .. self.offs[i] + self.lens[i]];
            }
            if (str_eq(ent, name)) { return true; }
        }
        return false;
    }

    fn deinit(self: *Self, a: Allocator) void {
        free(a, self.offs);
        free(a, self.lens);
    }
};

/// Convenience entry point: emit `EmitMode::Program` C for a parsed subset
/// module. The caller must have run `es_detect` first (a non-subset module
/// yields unspecified — but total — output).
pub fn es_emit_program(a: Allocator, src: []u8, nodes: []Node, root: i32) []u8 {
    var em: Em = Em.init(a, src, nodes, root);
    em.run(a);
    return em.out[0 .. em.out_len];
}

/// Convenience entry point: emit `EmitMode::Test` C — the test harness —
/// for a parsed subset module (v0.166). The caller must have run
/// `es_detect_mode(.., false)` first.
pub fn es_emit_test(a: Allocator, src: []u8, nodes: []Node, root: i32) []u8 {
    var em: Em = Em.init(a, src, nodes, root);
    em.is_test = true;
    em.run(a);
    return em.out[0 .. em.out_len];
}
