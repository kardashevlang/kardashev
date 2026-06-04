#!/usr/bin/env bash
# Roadmap v98 — self-hosted STATIC (monomorphized) TRAIT DISPATCH: the self-hosted
# LLVM-IR compiler (examples/selfhost/structgen.kd) now parses `trait Name { fn m(&self,
# p: TY) -> RET ; ... }` (method SIGNATURES only) and `impl Name for Widget { fn m(&self,
# p: TY) -> RET { body } }`. Each impl method is registered as an ordinary `Fn` under a
# MANGLED name `Widget_method` (struct name + "_" + method) with param 0 synthesized as
# `self : &Widget` (passed by reference). The new `recv.method(args)` syntax STATICALLY
# resolves to `@StructName_method` via the receiver's compile-time struct type and emits
# a DIRECT call `call <ret> @StructName_method(ptr %recvslot, <args>)` — no vtable. The
# receiver is passed by reference (its alloca slot / an already-`ptr` &Struct receiver).
#
# USE-GATED (Risk R0): a trait-free program emits BYTE-IDENTICAL IR — the existing
# self-host gates (phase117/118, selfhost_refs/calls/loops/vec/generics) are the guard,
# and case 0 below asserts the default demo has no `@Widget_`/`_method` symbols.
#
# Differential-gated vs the host: the self-hosted-emitted IR (clang -> native) must
# exit-match `kardc` on the equivalent program. Test programs keep
# `f(a: i64, b: i64) -> i64` so the host's `fn main() { f(a, b) }` wrapper works; the
# trait/impl decls are OTHER decls that `f` uses. Exit codes are compared mod 256.
# Skips if clang is unavailable.
#
# NOTE: trait names that PREFIX a prelude operator trait (e.g. `Adder` prefixes `Add`)
# trip a HOST prelude-resolution quirk, so the arg case uses `trait Bumper { fn add }`.
#
# DEFERRALS (v98): default trait method bodies; dynamic dispatch / `dyn Trait` (vtables);
# generic trait bounds (`fn g<T: Trait>`); associated types/consts; supertraits;
# multi-param method receivers other than `&self`.
set -uo pipefail
KARDC=""
for candidate in \
    "${TEST_SRCDIR:-}/_main/compiler/kardc" "${TEST_SRCDIR:-}/kardashev/compiler/kardc" \
    "${RUNFILES_DIR:-}/_main/compiler/kardc" "${RUNFILES_DIR:-}/kardashev/compiler/kardc" \
    "./compiler/kardc" "./build.local/kardc"; do
    if [[ -n "$candidate" && -x "$candidate" ]]; then KARDC="$candidate"; break; fi
done
[[ -z "$KARDC" ]] && { echo "FAIL: kardc binary not found"; exit 1; }
SRC=""
for cand in \
    "${TEST_SRCDIR:-}/_main/examples/selfhost/structgen.kd" "${TEST_SRCDIR:-}/kardashev/examples/selfhost/structgen.kd" \
    "${RUNFILES_DIR:-}/_main/examples/selfhost/structgen.kd" "${RUNFILES_DIR:-}/kardashev/examples/selfhost/structgen.kd" \
    "examples/selfhost/structgen.kd"; do
    if [[ -f "$cand" ]]; then SRC="$cand"; break; fi
done
[[ -z "$SRC" ]] && { echo "FAIL: examples/selfhost/structgen.kd not found"; exit 1; }
CLANG="$(command -v clang || true)"
[[ -z "$CLANG" ]] && { echo "PASS [v98-traits]: SKIPPED (no clang to compile the emitted IR)"; exit 0; }
echo "Using kardc at: $KARDC ; clang at: $CLANG"
TMP=$(mktemp -d); trap 'rm -rf "$TMP"' EXIT

"$KARDC" --no-cache -o "$TMP/sg" "$SRC" >/dev/null 2>&1 || { echo "FAIL [v98-traits]: structgen did not build"; exit 1; }

# 0. BYTE-IDENTITY guard (Risk R0): the default (no-trait) demo must emit NO trait
#    dispatch symbols, still build a `{ i64, i64 }` struct, and exit 7.
"$TMP/sg" > "$TMP/d.ll" 2>/dev/null || { echo "FAIL [v98-traits]: demo did not run"; exit 1; }
grep -q 'insertvalue { i64, i64 }' "$TMP/d.ll" || { echo "FAIL [v98-traits]: { i64, i64 } struct regressed"; cat "$TMP/d.ll"; exit 1; }
grep -Eq '@Widget_|@Gadget_|_method' "$TMP/d.ll" && { echo "FAIL [v98-traits]: no-trait demo emitted a dispatch symbol (R0)"; cat "$TMP/d.ll"; exit 1; }
"$CLANG" "$TMP/d.ll" -o "$TMP/d" 2>/dev/null || { echo "FAIL [v98-traits]: clang rejected demo IR"; cat "$TMP/d.ll"; exit 1; }
"$TMP/d" >/dev/null 2>&1; rd=$?
[[ "$rd" -eq 7 ]] || { echo "FAIL [v98-traits]: demo exit $rd (want 7)"; exit 1; }
echo "PASS [byte-identity]: no-trait demo has no dispatch symbol; native exit 7"

# DIFFERENTIAL helper: emit IR from the self-hosted compiler, clang -> native, and
# compare its exit to the host kardc on `<prog>\nfn main() { f(a, b) }`.
diff_case() {  # $1 source, $2 a, $3 b, $4 label
    "$TMP/sg" "$1" "$2" "$3" > "$TMP/s.ll" 2>/dev/null || { echo "FAIL [v98-traits/$4]: selfcc errored"; exit 1; }
    "$CLANG" "$TMP/s.ll" -o "$TMP/s" 2>/dev/null || { echo "FAIL [v98-traits/$4]: clang rejected IR"; cat "$TMP/s.ll"; exit 1; }
    "$TMP/s" >/dev/null 2>&1; local r_self=$?
    printf '%s\nfn main() -> i64 { f(%s, %s) }\n' "$1" "$2" "$3" > "$TMP/h.kd"
    "$KARDC" --no-cache -o "$TMP/h" "$TMP/h.kd" >/dev/null 2>&1 || { echo "FAIL [v98-traits/$4]: host rejected program"; exit 1; }
    "$TMP/h" >/dev/null 2>&1; local r_host=$?
    [[ "$r_self" -eq "$r_host" ]] || { echo "FAIL [v98-traits/$4]: self=$r_self != host=$r_host"; exit 1; }
    echo "PASS [$4]: self == host == $r_self"
}
# Same as diff_case but also asserts the self-hosted IR contains a regex.
diff_case_ir() {  # $1 source, $2 a, $3 b, $4 label, $5 ir-grep-regex
    diff_case "$1" "$2" "$3" "$4"
    grep -Eq "$5" "$TMP/s.ll" || { echo "FAIL [v98-traits/$4]: IR missing /$5/"; cat "$TMP/s.ll"; exit 1; }
    echo "PASS [$4-ir]: IR contains /$5/"
}

# 1. One trait + one impl: `w.get()` returns a field. IR must contain @Widget_get.
diff_case_ir "struct Widget { v: i64 } trait Getter { fn get(&self) -> i64 ; } impl Getter for Widget { fn get(&self) -> i64 { self.v } } fn f(a: i64, b: i64) -> i64 { let w = Widget { v: a + b } ; w.get() }" \
    3 4 "one-impl-field" 'call i64 @Widget_get\(ptr'

# 2. Method with an argument: `w.add(b)` -> self.v + k. (trait name `Bumper` avoids
#    the prelude `Add`-prefix host quirk; the METHOD is named `add`.)
diff_case_ir "struct Widget { v: i64 } trait Bumper { fn add(&self, k: i64) -> i64 ; } impl Bumper for Widget { fn add(&self, k: i64) -> i64 { self.v + k } } fn f(a: i64, b: i64) -> i64 { let w = Widget { v: a } ; w.add(b) }" \
    3 4 "method-arg" 'call i64 @Widget_add\(ptr [^,]+, i64'

# 3. SAME trait, TWO impls (Widget, Gadget): two DISTINCT symbols, no collision, both
#    called by reference.
PROG3='struct Widget { v: i64 } struct Gadget { v: i64 } trait Getter { fn get(&self) -> i64 ; } impl Getter for Widget { fn get(&self) -> i64 { self.v } } impl Getter for Gadget { fn get(&self) -> i64 { self.v * 2 } } fn f(a: i64, b: i64) -> i64 { let w = Widget { v: a } ; let g = Gadget { v: b } ; w.get() + g.get() }'
diff_case "$PROG3" 3 4 "two-impls"
"$TMP/sg" "$PROG3" 3 4 > "$TMP/c3.ll" 2>/dev/null
nw=$(grep -Ec 'define i64 @Widget_get\(' "$TMP/c3.ll")
ng=$(grep -Ec 'define i64 @Gadget_get\(' "$TMP/c3.ll")
[[ "$nw" -eq 1 && "$ng" -eq 1 ]] || { echo "FAIL [v98-traits/two-impls]: want 1 @Widget_get + 1 @Gadget_get def, got $nw/$ng"; cat "$TMP/c3.ll"; exit 1; }
grep -Eq 'call i64 @Widget_get\(ptr' "$TMP/c3.ll" && grep -Eq 'call i64 @Gadget_get\(ptr' "$TMP/c3.ll" || { echo "FAIL [v98-traits/two-impls]: both impls not called by ref"; cat "$TMP/c3.ll"; exit 1; }
echo "PASS [two-impls-distinct]: @Widget_get and @Gadget_get are distinct symbols, both called"

# 4. A method body calling ANOTHER method on self (`self.one() + self.one()`) ->
#    chained static dispatch: @Widget_dbl calls @Widget_one twice.
PROG4='struct Widget { v: i64 } trait Doubler { fn one(&self) -> i64 ; fn dbl(&self) -> i64 ; } impl Doubler for Widget { fn one(&self) -> i64 { self.v } fn dbl(&self) -> i64 { self.one() + self.one() } } fn f(a: i64, b: i64) -> i64 { let w = Widget { v: a + b } ; w.dbl() }'
diff_case "$PROG4" 3 4 "chained-self"
"$TMP/sg" "$PROG4" 3 4 > "$TMP/c4.ll" 2>/dev/null
nc=$(grep -Ec 'call i64 @Widget_one\(' "$TMP/c4.ll")
[[ "$nc" -eq 2 ]] || { echo "FAIL [v98-traits/chained-self]: expected 2 chained @Widget_one calls, got $nc"; cat "$TMP/c4.ll"; exit 1; }
echo "PASS [chained-self-dispatch]: @Widget_dbl calls @Widget_one twice"

# 5. NEGATIVE: calling a method NO impl provides -> the self-hosted type checker
#    rejects it (no @StructName_method symbol exists).
"$TMP/sg" "struct Widget { v: i64 } trait Getter { fn get(&self) -> i64 ; } impl Getter for Widget { fn get(&self) -> i64 { self.v } } fn f(a: i64, b: i64) -> i64 { let w = Widget { v: a } ; w.missing() }" 3 4 > "$TMP/neg.ll" 2>/dev/null
grep -q 'TYPE ERROR' "$TMP/neg.ll" || { echo "FAIL [v98-traits/neg-no-impl]: calling an unprovided method was not rejected"; cat "$TMP/neg.ll"; exit 1; }
echo "PASS [neg-no-impl]: a method with no matching impl is a type error"

echo "ALL v98 (self-hosted static trait dispatch) SMOKE TESTS PASSED"
