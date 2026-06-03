#!/usr/bin/env bash
# Roadmap v85 — self-hosting completeness: the self-hosted LLVM-IR compiler
# (examples/selfhost/structgen.kd) now handles BY-REFERENCE values. New lexer
# token '&' (kind 23); a type `&T` carries tag 200+base (ty_llvm -> opaque
# `ptr`); `&e` (Expr::Ref) materializes its operand into a stack slot
# (alloca/store) and yields the pointer; field access THROUGH a `&Struct` loads
# the aggregate then `extractvalue`s. The self-hosted type-checker REJECTS
# returning a reference (a returned `&local` would dangle) — sound by
# construction in this subset (no ref fields, no ref-of-ref, no stored refs).
#
# The all-i64 struct path stays byte-identical ({ i64, i64 }), so phase117/118
# hold. Differential-gated vs the host (self-hosted emitted-IR exit == host exit);
# the differential program keeps `f(a: i64, b: i64)` and builds the reference as a
# LOCAL, so the host's `fn main() { f(a, b) }` wrapper still works.
#
# Read-only strings (the planned second half of v85) are sequenced into v86,
# where call-expression parsing + a module-preamble buffer make them ~half the
# code — see ROADMAP-v81-v90.md. Skips if clang is unavailable.
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
[[ -z "$CLANG" ]] && { echo "PASS [v85-refs]: SKIPPED (no clang to compile the emitted IR)"; exit 0; }
echo "Using kardc at: $KARDC ; clang at: $CLANG"
TMP=$(mktemp -d); trap 'rm -rf "$TMP"' EXIT

"$KARDC" --no-cache -o "$TMP/sg" "$SRC" >/dev/null 2>&1 || { echo "FAIL [v85-refs]: structgen did not build"; exit 1; }

# 1. BYTE-IDENTITY guard: the all-i64 demo still emits { i64, i64 } and exits 7.
"$TMP/sg" > "$TMP/d.ll" 2>/dev/null || { echo "FAIL [v85-refs]: demo did not run"; exit 1; }
grep -q 'insertvalue { i64, i64 }' "$TMP/d.ll"  || { echo "FAIL [v85-refs]: { i64, i64 } insertvalue regressed"; cat "$TMP/d.ll"; exit 1; }
grep -q 'extractvalue { i64, i64 }' "$TMP/d.ll" || { echo "FAIL [v85-refs]: { i64, i64 } extractvalue regressed"; cat "$TMP/d.ll"; exit 1; }
"$CLANG" "$TMP/d.ll" -o "$TMP/d" 2>/dev/null || { echo "FAIL [v85-refs]: clang rejected demo IR"; cat "$TMP/d.ll"; exit 1; }
"$TMP/d" >/dev/null 2>&1; [[ $? -eq 7 ]] || { echo "FAIL [v85-refs]: demo exit != 7"; exit 1; }
echo "PASS [byte-identity]: all-i64 struct path unchanged; demo exit 7"

# 2. REF IR shape: a program using &p must emit alloca/store + load-through-ptr.
REFP='struct Point { x: i64, y: i64 } fn f(a: i64, b: i64) -> i64 { let p = Point { x: a, y: b } ; let r = &p ; r.x + r.y }'
"$TMP/sg" "$REFP" 3 4 > "$TMP/r.ll" 2>/dev/null || { echo "FAIL [v85-refs]: ref program errored"; exit 1; }
grep -q 'alloca'           "$TMP/r.ll" || { echo "FAIL [v85-refs]: no alloca for &local"; cat "$TMP/r.ll"; exit 1; }
grep -q 'load .*, ptr '    "$TMP/r.ll" || { echo "FAIL [v85-refs]: no load-through-ref"; cat "$TMP/r.ll"; exit 1; }
"$CLANG" "$TMP/r.ll" -o "$TMP/r" 2>/dev/null || { echo "FAIL [v85-refs]: clang rejected ref IR"; cat "$TMP/r.ll"; exit 1; }
"$TMP/r" >/dev/null 2>&1; [[ $? -eq 7 ]] || { echo "FAIL [v85-refs]: ref demo exit != 7"; exit 1; }
echo "PASS [ref-ir]: &local -> alloca/store; r.x -> load + extractvalue; exit 7"

# 3. DIFFERENTIAL: ref programs, self vs host (f keeps (i64,i64) so the wrapper works).
diff_case() {  # $1 src  $2 a  $3 b  $4 label
    "$TMP/sg" "$1" "$2" "$3" > "$TMP/s.ll" 2>/dev/null || { echo "FAIL [v85-refs/$4]: self errored"; exit 1; }
    "$CLANG" "$TMP/s.ll" -o "$TMP/s" 2>/dev/null || { echo "FAIL [v85-refs/$4]: clang rejected IR"; cat "$TMP/s.ll"; exit 1; }
    "$TMP/s" >/dev/null 2>&1; local r_self=$?
    printf '%s\nfn main() -> i64 { f(%s, %s) }\n' "$1" "$2" "$3" > "$TMP/h.kd"
    "$KARDC" --no-cache -o "$TMP/h" "$TMP/h.kd" >/dev/null 2>&1 || { echo "FAIL [v85-refs/$4]: host rejected program"; exit 1; }
    "$TMP/h" >/dev/null 2>&1; local r_host=$?
    [[ "$r_self" -eq "$r_host" ]] || { echo "FAIL [v85-refs/$4]: self=$r_self != host=$r_host"; exit 1; }
    echo "PASS [$4]: self == host == $r_self"
}
diff_case "$REFP" 3 4 "ref-field-sum"
diff_case 'struct Point { x: i64, y: i64 } fn f(a: i64, b: i64) -> i64 { let p = Point { x: a, y: b } ; let r = &p ; if r.x < r.y { r.y } else { r.x } }' 9 2 "ref-field-in-if"
diff_case 'struct Box3 { p: i64, q: i64, s: i64 } fn f(a: i64, b: i64) -> i64 { let t = Box3 { p: a, q: b, s: a * b } ; let r = &t ; (r.p + r.q) + r.s }' 3 4 "ref-three-field"
diff_case 'struct Inner { a: i64, b: i64 } struct Outer { v: i64, inner: Inner } fn f(a: i64, b: i64) -> i64 { let o = Outer { v: a, inner: Inner { a: a + b, b: b } } ; let r = &o ; r.inner.a + r.v }' 6 4 "ref-nested-struct"

# 4. NEGATIVE: returning a reference is rejected by the self-hosted checker
#    (emits "; TYPE ERROR", not valid IR).
BADREF='struct Point { x: i64, y: i64 } fn f(a: i64, b: i64) -> &Point { let p = Point { x: a, y: b } ; &p }'
"$TMP/sg" "$BADREF" 3 4 > "$TMP/bad.ll" 2>/dev/null
grep -q 'TYPE ERROR' "$TMP/bad.ll" || { echo "FAIL [neg-return-ref]: returning &local not rejected"; cat "$TMP/bad.ll"; exit 1; }
echo "PASS [neg-return-ref]: returning a reference is a type error"

echo "ALL v85 (refs) SMOKE TESTS PASSED"
