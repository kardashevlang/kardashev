#!/usr/bin/env bash
# Roadmap v91 — self-hosting control flow: the self-hosted LLVM-IR compiler
# (examples/selfhost/structgen.kd) now emits REAL CONTROL FLOW. It gained
# `let mut x = e`, `x = e` assignment, `while cond { ... }`, `for i in lo .. hi`
# (parser-desugared to `let mut i = lo ; while i < hi { body ; i = i + 1 ; }`),
# and `break` / `continue`. Mutable locals lower to `alloca`/`store`/`load`;
# `while`/`if`(stmt) lower to real basic blocks (loop.header/body/exit,
# if.then/else/end) with a fresh-label counter and one-terminator-per-block
# discipline; `break`/`continue` `br` to a stack of (header,exit) labels.
# IMMUTABLE `let` keeps the existing SSA-value path UNCHANGED, so the
# v84-v86 gates (phase117/118, selfhost_refs, selfhost_calls) stay BYTE-IDENTICAL.
#
# Differential-gated vs the host: the self-hosted-emitted IR (clang -> native)
# must exit-match `kardc` on the equivalent program. Test programs keep
# `f(a: i64, b: i64)` so the host's `fn main() { f(a, b) }` wrapper still works.
# Skips if clang is unavailable.
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
[[ -z "$CLANG" ]] && { echo "PASS [v91-loops]: SKIPPED (no clang to compile the emitted IR)"; exit 0; }
echo "Using kardc at: $KARDC ; clang at: $CLANG"
TMP=$(mktemp -d); trap 'rm -rf "$TMP"' EXIT

"$KARDC" --no-cache -o "$TMP/sg" "$SRC" >/dev/null 2>&1 || { echo "FAIL [v91-loops]: structgen did not build"; exit 1; }

# 1. BYTE-IDENTITY guard: the all-i64 immutable demo still emits the SSA path
#    ({ i64, i64 } insertvalue/extractvalue, no alloca) and exits 7. The mutable
#    path (alloca/load/store) is strictly additive.
"$TMP/sg" > "$TMP/d.ll" 2>/dev/null || { echo "FAIL [v91-loops]: demo did not run"; exit 1; }
grep -q 'insertvalue { i64, i64 }' "$TMP/d.ll"  || { echo "FAIL [v91-loops]: { i64, i64 } insertvalue regressed"; cat "$TMP/d.ll"; exit 1; }
grep -q 'extractvalue { i64, i64 }' "$TMP/d.ll" || { echo "FAIL [v91-loops]: { i64, i64 } extractvalue regressed"; cat "$TMP/d.ll"; exit 1; }
grep -q 'alloca' "$TMP/d.ll" && { echo "FAIL [v91-loops]: immutable demo unexpectedly used an alloca (SSA path regressed)"; cat "$TMP/d.ll"; exit 1; }
"$CLANG" "$TMP/d.ll" -o "$TMP/d" 2>/dev/null || { echo "FAIL [v91-loops]: clang rejected demo IR"; cat "$TMP/d.ll"; exit 1; }
"$TMP/d" >/dev/null 2>&1; [[ $? -eq 7 ]] || { echo "FAIL [v91-loops]: demo exit != 7"; exit 1; }
echo "PASS [byte-identity]: immutable SSA path unchanged (no alloca); demo exit 7"

# 2. CFG IR shape: a while program must emit alloca'd mutable locals + loop blocks.
WSUM='fn f(a: i64, b: i64) -> i64 { let mut r = 0 ; let mut i = 1 ; while i <= a { r = r + i ; i = i + 1 ; } ; r }'
"$TMP/sg" "$WSUM" 10 0 > "$TMP/w.ll" 2>/dev/null || { echo "FAIL [v91-loops]: while program errored"; exit 1; }
grep -q 'alloca i64'        "$TMP/w.ll" || { echo "FAIL [v91-loops]: no alloca for let mut"; cat "$TMP/w.ll"; exit 1; }
grep -q 'store i64'         "$TMP/w.ll" || { echo "FAIL [v91-loops]: no store for assignment"; cat "$TMP/w.ll"; exit 1; }
grep -q 'load i64, ptr '    "$TMP/w.ll" || { echo "FAIL [v91-loops]: no load of a mutable local"; cat "$TMP/w.ll"; exit 1; }
grep -q 'loop.header'       "$TMP/w.ll" || { echo "FAIL [v91-loops]: no loop.header block"; cat "$TMP/w.ll"; exit 1; }
grep -q 'br i1 '            "$TMP/w.ll" || { echo "FAIL [v91-loops]: no conditional branch"; cat "$TMP/w.ll"; exit 1; }
"$CLANG" "$TMP/w.ll" -o "$TMP/w" 2>/dev/null || { echo "FAIL [v91-loops]: clang rejected while IR"; cat "$TMP/w.ll"; exit 1; }
"$TMP/w" >/dev/null 2>&1; [[ $? -eq 55 ]] || { echo "FAIL [v91-loops]: while-sum exit != 55"; exit 1; }
echo "PASS [cfg-ir]: let mut -> alloca; while -> loop.header/body/exit + br i1; exit 55"

# 3. DIFFERENTIAL: loop programs, self-hosted-emitted IR exit == host exit.
diff_case() {  # $1 src  $2 a  $3 b  $4 label
    "$TMP/sg" "$1" "$2" "$3" > "$TMP/s.ll" 2>/dev/null || { echo "FAIL [v91-loops/$4]: self errored"; exit 1; }
    "$CLANG" "$TMP/s.ll" -o "$TMP/s" 2>/dev/null || { echo "FAIL [v91-loops/$4]: clang rejected IR"; cat "$TMP/s.ll"; exit 1; }
    "$TMP/s" >/dev/null 2>&1; local r_self=$?
    printf '%s\nfn main() -> i64 { f(%s, %s) }\n' "$1" "$2" "$3" > "$TMP/h.kd"
    "$KARDC" --no-cache -o "$TMP/h" "$TMP/h.kd" >/dev/null 2>&1 || { echo "FAIL [v91-loops/$4]: host rejected program"; exit 1; }
    "$TMP/h" >/dev/null 2>&1; local r_host=$?
    [[ "$r_self" -eq "$r_host" ]] || { echo "FAIL [v91-loops/$4]: self=$r_self != host=$r_host"; exit 1; }
    echo "PASS [$4]: self == host == $r_self"
}
# while accumulator: sum 1..=a  (a=10 -> 55)
diff_case "$WSUM" 10 0 "while-sum"
# factorial via while: prod 1..=a  (a=5 -> 120)
diff_case 'fn f(a: i64, b: i64) -> i64 { let mut r = 1 ; let mut i = 1 ; while i <= a { r = r * i ; i = i + 1 ; } ; r }' 5 0 "while-factorial"
# factorial via for: prod 1..a then * a  (a=5 -> 24 * 5 = 120)
diff_case 'fn f(a: i64, b: i64) -> i64 { let mut r = 1 ; for i in 1 .. a { r = r * i ; } ; r * a }' 5 0 "for-factorial"
# while with break: sum 1..=a but stop early when i reaches b  (a=10,b=4 -> 1+2+3 = 6)
diff_case 'fn f(a: i64, b: i64) -> i64 { let mut r = 0 ; let mut i = 1 ; while i <= a { if i == b { break ; } else { r = r + i ; } ; i = i + 1 ; } ; r }' 10 4 "while-break"
# while with continue: sum 1..=a but skip i == b  (a=10,b=5 -> 55-5 = 50)
diff_case 'fn f(a: i64, b: i64) -> i64 { let mut r = 0 ; let mut i = 0 ; while i < a { i = i + 1 ; if i == b { continue ; } else { r = r + i ; } } ; r }' 10 5 "while-continue"
# mutable accumulator read+write across iterations: iterative Fibonacci  (a=10 -> 55)
diff_case 'fn f(a: i64, b: i64) -> i64 { let mut x = 0 ; let mut y = 1 ; let mut i = 0 ; while i < a { let t = x + y ; x = y ; y = t ; i = i + 1 ; } ; x }' 10 0 "fib-accumulator"
# nested loops: a*b via repeated +1  (a=6,b=7 -> 42)
diff_case 'fn f(a: i64, b: i64) -> i64 { let mut r = 0 ; let mut i = 0 ; while i < a { let mut j = 0 ; while j < b { r = r + 1 ; j = j + 1 ; } ; i = i + 1 ; } ; r }' 6 7 "nested-loops"

# 4. NEGATIVE: break outside any loop is a type error (the checker emits
#    "; TYPE ERROR", not valid IR).
BADBRK='fn f(a: i64, b: i64) -> i64 { break ; a }'
"$TMP/sg" "$BADBRK" 1 2 > "$TMP/bad.ll" 2>/dev/null
grep -q 'TYPE ERROR' "$TMP/bad.ll" || { echo "FAIL [neg-break-outside-loop]: break outside a loop not rejected"; cat "$TMP/bad.ll"; exit 1; }
echo "PASS [neg-break-outside-loop]: break outside a loop is a type error"

echo "ALL v91 (loops) SMOKE TESTS PASSED"
