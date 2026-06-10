// ast.ks — self-host stage 2 (v0.160): the kardashev AST as an arena of
// generic nodes, written in kardashev.
//
// The Rust reference AST (`crates/kardc/src/ast.rs`) is a recursive enum
// forest; kardashev has no recursive types, so this module uses the proven
// json-module design instead: ONE flat `Node` table where nodes reference
// each other by arena index (`i32`, -1 = none) and every list (items,
// params, fields, statements, arguments, switch labels, …) is a
// first-child / next-sibling chain through the `next` link.
//
// One generic node shape covers every `ast.rs` variant:
//   - `kind`      one of the `ND_*` constants below (the variant tag);
//   - `a/b/c`     child links, with per-kind meaning (documented per ND_*);
//   - `next`      the sibling link of whatever list this node is a member of;
//   - `off/len`   the node's source span (== the Rust node's `Span`:
//                 `off = span.start`, `len = span.end - span.start`);
//   - `xoff/xlen` the primary NAME span (identifier text by source span —
//   - `yoff/ylen`  never copied), `y`/`z` are the secondary/tertiary name
//   - `zoff/zlen`  spans for forms that carry several names (see per-kind);
//   - `val/val2`  decoded integer payloads (INT value, operator codes,
//                 explicit enum value, switch-range bounds);
//   - `flags`     the `F_*` bits below (bool fields of the Rust node).
//
// String payloads are SPANS into the original source: the text of a name is
// `src[xoff .. xoff + xlen]`. The single exception is `@This()` in type
// position, which the Rust parser desugars to the NAME `"Self"` — no source
// bytes spell it, so the node carries `F_THIS` and consumers print `Self`.
//
// Per-kind layout (only the links/fields listed are meaningful):
//
//   ND_FN         x=name  a=params b=ret-TYPE c=body-BLOCK   F_PUB
//   ND_PARAM      x=name  a=TYPE                             F_COMPTIME
//   ND_CONST      x=name  a=TYPE|-1 b=value                  F_PUB
//   ND_TEST       x=name-string-token  a=body-BLOCK
//   ND_STRUCT     x=name  a=SFIELDs b=method-FNs             F_PUB
//   ND_SFIELD     x=name  a=TYPE
//   ND_ENUM       x=name  a=VARIANTs                         F_PUB
//   ND_VARIANT    x=name  val=explicit value (F_VAL set iff written)
//   ND_UNION      x=name  a=UVARs                            F_PUB
//   ND_UVAR       x=name  a=payload-TYPE
//   ND_IMPORT     x=path-string-token
//   ND_ERRSET     x=name  a=MEMBERs                          F_PUB
//   ND_MEMBER     x=name                  (error-set member; never a line)
//   ND_TYPE       x=base name (F_THIS → "Self")  a=ctor-arg TYPEs (F_APP)
//                 y=error-set name (F_ERRSET; F_ESETTHIS → "Self")
//                   OR array-size param name (F_ARRPARAM)
//                 val=array length (F_ARRLIT)
//                 F_OPT / F_ERR / F_PTR / F_SLICE
//   ND_BLOCK      a=stmts
//   ND_LET        x=name  a=TYPE|-1 b=value                  F_CONST
//   ND_ASSIGN     x=name  a=value  val=op (OPC_* or -1 = plain `=`)
//   ND_PASSIGN    a=place b=value  val=op (OPC_* or -1)
//   ND_RETURN     a=value|-1
//   ND_IF         a=cond b=then-BLOCK c=else-stmt|-1  x=capture (F_CAP)
//   ND_WHILE      a=cond b=cont-stmt|-1 c=body  x=label (F_LABEL)
//   ND_FOR        a=iter b=body  x=elem  y=index (F_IDX)  z=label (F_LABEL)
//   ND_BREAK      x=target label (F_LABEL)
//   ND_CONTINUE   x=target label (F_LABEL)
//   ND_DEFER      a=stmt
//   ND_ERRDEFER   a=stmt
//   ND_SWITCH     a=scrutinee b=ARMs c=default-BLOCK|-1
//   ND_ARM        a=label-exprs b=RANGEs c=body-BLOCK  x=capture (F_CAP)
//   ND_RANGE      val=lo val2=hi          (range arm label; never a line)
//   ND_INT        val=decoded value
//   ND_FLOAT      (span only — the value is the source text)
//   ND_BOOL       val=0|1
//   ND_IDENT      x=name
//   ND_UNARY      a=operand  val=UOP_*
//   ND_BIN        a=lhs b=rhs  val=OPC_*
//   ND_CALL       x=callee  a=args
//   ND_COMPTIME   a=operand
//   ND_SLIT       x=struct name  a=FINITs
//   ND_FINIT      x=field name  a=value
//   ND_FIELD      x=field name  a=base
//   ND_STR        (span only)
//   ND_UNREACHABLE
//   ND_BUILTIN    x=builtin name  a=args
//   ND_STRUCTTYPE a=SFIELDs b=method-FNs
//   ND_MCALL      x=method name  a=receiver b=args
//   ND_NULL
//   ND_ORELSE     a=lhs b=rhs
//   ND_UNWRAP     a=operand
//   ND_ERRLIT     x=error name
//   ND_ENUMLIT    x=variant name
//   ND_ALIT       a=elem-TYPE b=elements
//   ND_INDEX      a=base b=index
//   ND_ADDROF     a=place
//   ND_DEREF      a=operand
//   ND_SLICEX     a=base b=lo c=hi
//   ND_TRY        a=operand
//   ND_CATCH      a=operand b=default  x=capture (F_CAP)

@import("std");

// --- node kinds (one per ast.rs shape) ---------------------------------------

/// Function definition (top-level item, struct method, or struct-type method).
pub const ND_FN: u8 = 1;
/// One function parameter.
pub const ND_PARAM: u8 = 2;
/// A `const` value declaration (top-level item).
pub const ND_CONST: u8 = 3;
/// A `test "name" { … }` block.
pub const ND_TEST: u8 = 4;
/// A named struct declaration `const Name = struct { … };`.
pub const ND_STRUCT: u8 = 5;
/// One struct field declaration `name: T`.
pub const ND_SFIELD: u8 = 6;
/// An enum declaration `const Name = enum { … };`.
pub const ND_ENUM: u8 = 7;
/// One enum variant, optionally with an explicit value.
pub const ND_VARIANT: u8 = 8;
/// A tagged-union declaration `const Name = union(enum) { … };`.
pub const ND_UNION: u8 = 9;
/// One union variant `name: T`.
pub const ND_UVAR: u8 = 10;
/// A top-level `@import("path");`.
pub const ND_IMPORT: u8 = 11;
/// A named error-set declaration `const Name = error{ … };`.
pub const ND_ERRSET: u8 = 12;
/// One error-set member name (internal; rendered inline on the ERRSET line).
pub const ND_MEMBER: u8 = 13;
/// A type reference (all of ast.rs `TypeExpr` in one node).
pub const ND_TYPE: u8 = 14;
/// A brace-delimited block.
pub const ND_BLOCK: u8 = 15;
/// A `var`/`const` binding statement.
pub const ND_LET: u8 = 16;
/// A simple-name assignment statement (plain or compound).
pub const ND_ASSIGN: u8 = 17;
/// A place (field/index/deref chain) assignment statement.
pub const ND_PASSIGN: u8 = 18;
/// A `return` statement.
pub const ND_RETURN: u8 = 19;
/// An `if` statement (with optional payload capture and else).
pub const ND_IF: u8 = 20;
/// A `while` loop (with optional continue-clause and label).
pub const ND_WHILE: u8 = 21;
/// A `for` loop (with optional index capture and label).
pub const ND_FOR: u8 = 22;
/// A `break` statement (with optional target label).
pub const ND_BREAK: u8 = 23;
/// A `continue` statement (with optional target label).
pub const ND_CONTINUE: u8 = 24;
/// A `defer` statement.
pub const ND_DEFER: u8 = 25;
/// An `errdefer` statement.
pub const ND_ERRDEFER: u8 = 26;
/// A `switch` statement.
pub const ND_SWITCH: u8 = 27;
/// One `labels => body` switch arm.
pub const ND_ARM: u8 = 28;
/// One inclusive integer range label `lo..hi` of a switch arm (internal;
/// rendered inline on the ARM line).
pub const ND_RANGE: u8 = 29;
/// An integer literal.
pub const ND_INT: u8 = 30;
/// A float literal.
pub const ND_FLOAT: u8 = 31;
/// A `true`/`false` literal.
pub const ND_BOOL: u8 = 32;
/// An identifier expression.
pub const ND_IDENT: u8 = 33;
/// A unary operation `-x`/`!x`/`~x`.
pub const ND_UNARY: u8 = 34;
/// A binary operation.
pub const ND_BIN: u8 = 35;
/// A free-function call `name(args)`.
pub const ND_CALL: u8 = 36;
/// A `comptime expr` expression.
pub const ND_COMPTIME: u8 = 37;
/// A struct literal `Name{ .f = e, … }`.
pub const ND_SLIT: u8 = 38;
/// One `.name = value` struct-literal initializer.
pub const ND_FINIT: u8 = 39;
/// A field access `base.name`.
pub const ND_FIELD: u8 = 40;
/// A string literal.
pub const ND_STR: u8 = 41;
/// The `unreachable` expression.
pub const ND_UNREACHABLE: u8 = 42;
/// A builtin call `@name(args)` in expression position.
pub const ND_BUILTIN: u8 = 43;
/// An anonymous `struct { … }` type value.
pub const ND_STRUCTTYPE: u8 = 44;
/// A method / associated-function call `recv.name(args)`.
pub const ND_MCALL: u8 = 45;
/// The `null` literal.
pub const ND_NULL: u8 = 46;
/// `lhs orelse rhs`.
pub const ND_ORELSE: u8 = 47;
/// `expr.?` — optional force-unwrap.
pub const ND_UNWRAP: u8 = 48;
/// An error literal `error.Name`.
pub const ND_ERRLIT: u8 = 49;
/// An unqualified enum literal `.Variant`.
pub const ND_ENUMLIT: u8 = 50;
/// An array literal `[N]T{ … }`.
pub const ND_ALIT: u8 = 51;
/// An index expression `base[i]`.
pub const ND_INDEX: u8 = 52;
/// An address-of expression `&place`.
pub const ND_ADDROF: u8 = 53;
/// A pointer dereference `expr.*`.
pub const ND_DEREF: u8 = 54;
/// A slice expression `base[lo..hi]`.
pub const ND_SLICEX: u8 = 55;
/// `try expr` — error propagation.
pub const ND_TRY: u8 = 56;
/// `expr catch [|e|] default` — error handling.
pub const ND_CATCH: u8 = 57;

// --- flag bits (the Rust nodes' bool / Option-presence fields) ---------------

/// `pub` on an item / method.
pub const F_PUB: i64 = 1;
/// `const` (vs `var`) on a let binding.
pub const F_CONST: i64 = 2;
/// `comptime` on a parameter.
pub const F_COMPTIME: i64 = 4;
/// `?T` — optional type.
pub const F_OPT: i64 = 8;
/// `!T` — error-union type.
pub const F_ERR: i64 = 16;
/// `*T` — pointer type.
pub const F_PTR: i64 = 32;
/// `[]T` — slice type.
pub const F_SLICE: i64 = 64;
/// `[N]T` with a literal length (in `val`).
pub const F_ARRLIT: i64 = 128;
/// `[n]T` with a comptime value-parameter length (name in `y`).
pub const F_ARRPARAM: i64 = 256;
/// `Set!T` — a NAMED error union; the set name is in `y`.
pub const F_ERRSET: i64 = 512;
/// `Name(…)` — a type-constructor application; args chain in `a` (possibly
/// empty: `Name()` sets the flag with `a = -1`).
pub const F_APP: i64 = 1024;
/// The type base was written `@This()` — its NAME is `Self` (no source span).
pub const F_THIS: i64 = 2048;
/// A `|name|` capture is present (name in `x`).
pub const F_CAP: i64 = 4096;
/// A loop label / break-continue target is present.
pub const F_LABEL: i64 = 8192;
/// The `for` index capture is present (name in `y`).
pub const F_IDX: i64 = 16384;
/// The enum variant has an explicit `= value` (in `val`).
pub const F_VAL: i64 = 32768;
/// The `Set` of `Set!T` was written `@This()` (prints as `Self`).
pub const F_ESETTHIS: i64 = 65536;

// --- operator codes -----------------------------------------------------------

/// `+`
pub const OPC_ADD: i64 = 0;
/// `-`
pub const OPC_SUB: i64 = 1;
/// `*`
pub const OPC_MUL: i64 = 2;
/// `/`
pub const OPC_DIV: i64 = 3;
/// `%`
pub const OPC_REM: i64 = 4;
/// `==`
pub const OPC_EQ: i64 = 5;
/// `!=`
pub const OPC_NE: i64 = 6;
/// `<`
pub const OPC_LT: i64 = 7;
/// `<=`
pub const OPC_LE: i64 = 8;
/// `>`
pub const OPC_GT: i64 = 9;
/// `>=`
pub const OPC_GE: i64 = 10;
/// `and`
pub const OPC_AND: i64 = 11;
/// `or`
pub const OPC_OR: i64 = 12;
/// `&`
pub const OPC_BAND: i64 = 13;
/// `|`
pub const OPC_BOR: i64 = 14;
/// `^`
pub const OPC_BXOR: i64 = 15;
/// `<<`
pub const OPC_SHL: i64 = 16;
/// `>>`
pub const OPC_SHR: i64 = 17;

/// `-x`
pub const UOP_NEG: i64 = 0;
/// `!x`
pub const UOP_NOT: i64 = 1;
/// `~x`
pub const UOP_BNOT: i64 = 2;

/// The canonical dump spelling of a binary-operator code (the Rust
/// differential driver's `bin_name` table).
pub fn opc_name(op: i64) []u8 {
    if (op == OPC_ADD) { return "add"; }
    if (op == OPC_SUB) { return "sub"; }
    if (op == OPC_MUL) { return "mul"; }
    if (op == OPC_DIV) { return "div"; }
    if (op == OPC_REM) { return "rem"; }
    if (op == OPC_EQ) { return "eq"; }
    if (op == OPC_NE) { return "ne"; }
    if (op == OPC_LT) { return "lt"; }
    if (op == OPC_LE) { return "le"; }
    if (op == OPC_GT) { return "gt"; }
    if (op == OPC_GE) { return "ge"; }
    if (op == OPC_AND) { return "and"; }
    if (op == OPC_OR) { return "or"; }
    if (op == OPC_BAND) { return "band"; }
    if (op == OPC_BOR) { return "bor"; }
    if (op == OPC_BXOR) { return "bxor"; }
    if (op == OPC_SHL) { return "shl"; }
    if (op == OPC_SHR) { return "shr"; }
    return "unknown";
}

/// The canonical dump spelling of a unary-operator code.
pub fn uop_name(op: i64) []u8 {
    if (op == UOP_NEG) { return "neg"; }
    if (op == UOP_NOT) { return "not"; }
    if (op == UOP_BNOT) { return "bnot"; }
    return "unknown";
}

// --- the node -----------------------------------------------------------------

/// One arena node (layout table in the module header). 16 fields cover every
/// `ast.rs` variant; unused links are -1 and unused spans/values are 0.
pub const Node = struct {
    kind: u8,
    a: i32,
    b: i32,
    c: i32,
    next: i32,
    off: usize,
    len: usize,
    xoff: usize,
    xlen: usize,
    yoff: usize,
    ylen: usize,
    zoff: usize,
    zlen: usize,
    val: i64,
    val2: i64,
    flags: i64,
};

// --- list building -------------------------------------------------------------

/// A first-child/next-sibling list under construction: O(1) append via the
/// tail. `head` is -1 while empty — exactly the parent's stored link value.
pub const Chain = struct {
    head: i32,
    tail: i32,

    /// An empty chain.
    fn init() Self {
        return Self{ .head = 0 - 1, .tail = 0 - 1 };
    }

    /// Append node `idx` by linking the current tail's `next` to it. `nodes`
    /// is the CURRENT arena slice (pass it fresh on every call — the arena
    /// reallocates as it grows).
    fn add(self: *Self, nodes: []Node, idx: i32) void {
        if (self.tail >= 0) {
            nodes[@as(usize, self.tail)].next = idx;
        } else {
            self.head = idx;
        }
        self.tail = idx;
    }
};
