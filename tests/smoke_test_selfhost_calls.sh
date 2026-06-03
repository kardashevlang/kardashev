#!/usr/bin/env bash
# Roadmap v86 — self-hosting completeness: the self-hosted LLVM-IR compiler
# (examples/selfhost/structgen.kd) now handles USER FUNCTION CALLS and READ-ONLY
# STRINGS.
#   * Multi-fn registry: every top-level `fn` is parsed, type-checked, and
#     emitted; a `Call(name, args)` lowers to `call <rty> @name(...)` using the
#     CALLEE's param types. The entry fn stays named `f` so the host differential
#     wrapper (`fn main() { f(a,b) }`) still compiles.
#   * String literals: a new `"..."` token (kind 24) -> `StrLit`; each literal
#     emits a private `@.str.<off>` constant into a new module PREAMBLE buffer
#     (globals precede the defines); a literal lowers to the host's borrowed
#     String aggregate `{ ptr, i64, cap=0 }`; `str_len(&s)` -> getelementptr
#     field 1 + load. (These are the resequenced-from-v85 half — they needed the
#     call-parsing + module-global buffer introduced here.)
#   * Also fixes a latent `is_alpha` bug: `_` (95) fell into the dead A-Z branch,
#     so identifiers with underscores (e.g. `str_len`) never lexed correctly. No
#     prior test used underscores, so it never surfaced.
#
# DEFERRED (honest, no stubs): while/for-loop CFG + mutable locals + assignment,
# and scalar Vec<i64> + growable strings, stay in the XL real-bootstrap mega-arc
# (they require a block-terminator/CFG rework + alloca-backed mutable locals that
# is an architectural change in this branch-free emitter). See ROADMAP-v81-v90.md.
#
# BYTE-IDENTITY: a program with no calls/strings emits an EMPTY preamble, so the
# all-i64 struct path stays `{ i64, i64 }` and the demo still exits 7 -> phase117
# / phase118 / v85-refs hold. Differential-gated vs the host. Skips without clang.
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
[[ -z "$CLANG" ]] && { echo "PASS [v86-calls]: SKIPPED (no clang)"; exit 0; }
echo "Using kardc at: $KARDC ; clang at: $CLANG"
TMP=$(mktemp -d); trap 'rm -rf "$TMP"' EXIT

"$KARDC" --no-cache -o "$TMP/sg" "$SRC" >/dev/null 2>&1 || { echo "FAIL [v86-calls]: structgen did not build"; exit 1; }

# 1. BYTE-IDENTITY guard: the no-arg demo is callless/stringless -> { i64, i64 },
#    exit 7, and NO @.str leaks.
"$TMP/sg" > "$TMP/d.ll" 2>/dev/null || { echo "FAIL [v86-calls]: demo did not run"; exit 1; }
grep -q 'insertvalue { i64, i64 }'  "$TMP/d.ll" || { echo "FAIL: { i64, i64 } insertvalue regressed";  cat "$TMP/d.ll"; exit 1; }
grep -q 'extractvalue { i64, i64 }' "$TMP/d.ll" || { echo "FAIL: { i64, i64 } extractvalue regressed"; cat "$TMP/d.ll"; exit 1; }
grep -q '@.str' "$TMP/d.ll" && { echo "FAIL: stringless demo leaked a @.str global"; cat "$TMP/d.ll"; exit 1; }
"$CLANG" "$TMP/d.ll" -o "$TMP/d" 2>/dev/null || { echo "FAIL: clang rejected demo IR"; cat "$TMP/d.ll"; exit 1; }
"$TMP/d" >/dev/null 2>&1; [[ $? -eq 7 ]] || { echo "FAIL: demo exit != 7"; exit 1; }
echo "PASS [byte-identity]: all-i64 struct demo unchanged; exit 7; empty preamble"

# 2. CAPSTONE: calls + strings + struct + ref; check IR shape, run -> 21 = (7+12)+2.
CAP='struct R { sum: i64, prod: i64 } fn add(x: i64, y: i64) -> i64 { x + y } fn scale(x: i64, y: i64) -> i64 { x * y } fn f(a: i64, b: i64) -> i64 { let r = R { sum: add(a, b), prod: scale(a, b) } ; let p = &r ; let tag = "ok" ; (p.sum + p.prod) + str_len(&tag) }'
"$TMP/sg" "$CAP" 3 4 > "$TMP/c.ll" 2>/dev/null || { echo "FAIL [capstone]: self errored"; exit 1; }
grep -q '@.str'          "$TMP/c.ll" || { echo "FAIL [capstone]: no @.str global"; cat "$TMP/c.ll"; exit 1; }
grep -q 'call i64 @add'   "$TMP/c.ll" || { echo "FAIL [capstone]: no call @add";   cat "$TMP/c.ll"; exit 1; }
grep -q 'call i64 @scale' "$TMP/c.ll" || { echo "FAIL [capstone]: no call @scale"; cat "$TMP/c.ll"; exit 1; }
grep -q 'getelementptr'   "$TMP/c.ll" || { echo "FAIL [capstone]: no GEP (str data/len)"; cat "$TMP/c.ll"; exit 1; }
# globals must precede defines: first @.str line before first `define`.
awk '/^@\.str/{g=NR} /^define/{d=NR} END{exit !(g && d && g<d)}' "$TMP/c.ll" || { echo "FAIL [capstone]: @.str not in module preamble (before define)"; cat "$TMP/c.ll"; exit 1; }
"$CLANG" "$TMP/c.ll" -o "$TMP/c" 2>/dev/null || { echo "FAIL [capstone]: clang rejected IR"; cat "$TMP/c.ll"; exit 1; }
"$TMP/c" >/dev/null 2>&1; rc=$?
[[ "$rc" -eq 21 ]] || { echo "FAIL [capstone]: exit $rc (want 21 = (7+12)+2)"; exit 1; }
echo "PASS [capstone]: calls + @.str + str_len + struct + ref; native exit 21"

# 3. DIFFERENTIAL: self vs host (host wrapper appends `fn main(){ f(a,b) }`).
diff_case() {  # $1 src  $2 a  $3 b  $4 label
    "$TMP/sg" "$1" "$2" "$3" > "$TMP/s.ll" 2>/dev/null || { echo "FAIL [$4]: self errored"; exit 1; }
    "$CLANG" "$TMP/s.ll" -o "$TMP/s" 2>/dev/null || { echo "FAIL [$4]: clang rejected"; cat "$TMP/s.ll"; exit 1; }
    "$TMP/s" >/dev/null 2>&1; local r_self=$?
    printf '%s\nfn main() -> i64 { f(%s, %s) }\n' "$1" "$2" "$3" > "$TMP/h.kd"
    "$KARDC" --no-cache -o "$TMP/h" "$TMP/h.kd" >/dev/null 2>&1 || { echo "FAIL [$4]: host rejected"; exit 1; }
    "$TMP/h" >/dev/null 2>&1; local r_host=$?
    [[ "$r_self" -eq "$r_host" ]] || { echo "FAIL [$4]: self=$r_self != host=$r_host"; exit 1; }
    echo "PASS [$4]: self == host == $r_self"
}
diff_case "$CAP" 3 4 "capstone-diff"
diff_case "$CAP" 6 5 "capstone-diff-2"
diff_case 'fn g(x: i64) -> i64 { x * x } fn f(a: i64, b: i64) -> i64 { g(a) + g(b) }' 3 4 "call-one-arg"
diff_case 'fn add3(a: i64, b: i64, c: i64) -> i64 { (a + b) + c } fn f(a: i64, b: i64) -> i64 { add3(a, b, a) }' 5 6 "call-three-arg"
diff_case 'fn dbl(x: i64) -> i64 { x + x } fn quad(x: i64) -> i64 { dbl(dbl(x)) } fn f(a: i64, b: i64) -> i64 { quad(a) + b }' 5 3 "nested-calls"
diff_case 'fn f(a: i64, b: i64) -> i64 { let s = "hello" ; str_len(&s) + (a * 0) + (b * 0) }' 1 2 "str-len-hello"
diff_case 'fn f(a: i64, b: i64) -> i64 { let s = "" ; str_len(&s) + a + b }' 4 5 "str-len-empty"

# 4. NEGATIVE: unknown callee is a type error (no IR, prints "; TYPE ERROR").
"$TMP/sg" 'fn f(a: i64, b: i64) -> i64 { nope(a, b) }' 1 2 > "$TMP/bad.ll" 2>/dev/null
grep -q 'TYPE ERROR' "$TMP/bad.ll" || { echo "FAIL [neg-unknown-call]: unknown fn not rejected"; cat "$TMP/bad.ll"; exit 1; }
echo "PASS [neg-unknown-call]: unknown callee is a type error"
# 5. NEGATIVE: arity mismatch is a type error.
"$TMP/sg" 'fn g(x: i64) -> i64 { x } fn f(a: i64, b: i64) -> i64 { g(a, b) }' 1 2 > "$TMP/bad2.ll" 2>/dev/null
grep -q 'TYPE ERROR' "$TMP/bad2.ll" || { echo "FAIL [neg-arity]: arity mismatch not rejected"; cat "$TMP/bad2.ll"; exit 1; }
echo "PASS [neg-arity]: arity mismatch is a type error"

echo "ALL v86 (calls + strings) SMOKE TESTS PASSED"
