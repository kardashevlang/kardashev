#!/usr/bin/env bash
# v72 — f64 transcendental math library. Unary intrinsics (sin/cos/exp/ln/log2/
# log10/trunc/round), unary libm externs (tan/asin/acos/atan/cbrt), binary
# intrinsics (pow/copysign/min/max), binary libm externs (atan2/hypot/fmod).
# Results checked via `as i64` truncation (stable across platforms; avoids
# last-digit libm differences). Differential JIT==AOT.
set -uo pipefail
KARDC=""
for c in "${TEST_SRCDIR:-}/_main/compiler/kardc" "${RUNFILES_DIR:-}/_main/compiler/kardc" "./compiler/kardc" "./build.local/kardc"; do
  [[ -n "$c" && -x "$c" ]] && { KARDC="$c"; break; }; done
[[ -z "$KARDC" ]] && { echo "FAIL: kardc not found"; exit 1; }
echo "Using kardc at: $KARDC"
TMP=$(mktemp -d); trap 'rm -rf "$TMP"' EXIT

diff_run() { local name="$1" expect="$2" src="$3"
  local n; n=$(printf '%s\n' "$expect" | wc -l)
  printf '%s' "$src" > "$TMP/$name.kd"
  local jit; jit=$("$KARDC" "$TMP/$name.kd" 2>/dev/null | head -n "$n") || true
  [[ "$jit" == "$expect" ]] || { echo "FAIL [$name/jit]: expected '$expect' got '$jit'"; "$KARDC" "$TMP/$name.kd" 2>&1|head -4; exit 1; }
  "$KARDC" --no-cache -o "$TMP/$name" "$TMP/$name.kd" >/dev/null 2>&1
  local aot; aot=$("$TMP/$name" 2>/dev/null | head -n "$n") || true
  [[ "$aot" == "$expect" ]] || { echo "FAIL [$name/aot]: expected '$expect' got '$aot'"; exit 1; }
  echo "PASS: $name"; }
reject() { local name="$1" needle="$2" src="$3"; printf '%s' "$src" > "$TMP/$name.kd"
  local e; e=$("$KARDC" "$TMP/$name.kd" 2>&1 >/dev/null || true)
  echo "$e" | grep -qi "$needle" || { echo "FAIL[reject $name]: want '$needle' got: $e"; exit 1; }
  echo "PASS(reject): $name"; }

# trig + exp/log at exact points.
diff_run trig $'0\n1\n0\n1\n0\n3\n3' \
'fn main() -> i64 ! { io } {
  print(f64_sin(0.0) as i64); print(f64_cos(0.0) as i64); print(f64_tan(0.0) as i64);
  print(f64_exp(0.0) as i64); print(f64_ln(1.0) as i64);
  print(f64_log2(8.0) as i64); print(f64_log10(1000.0) as i64); 0 }'

# binary ops with integer-exact results.
diff_run binops $'1024\n0\n5\n3\n1\n2\n5\n-3' \
'fn main() -> i64 ! { io } {
  print(f64_pow(2.0, 10.0) as i64); print(f64_atan2(0.0, 1.0) as i64);
  print(f64_hypot(3.0, 4.0) as i64); print(f64_cbrt(27.0) as i64);
  print(f64_fmod(10.0, 3.0) as i64); print(f64_min(2.0, 5.0) as i64);
  print(f64_max(2.0, 5.0) as i64); print(f64_copysign(3.0, 0.0 - 1.0) as i64); 0 }'

# rounding.
diff_run rounding $'3\n2\n4\n3' \
'fn main() -> i64 ! { io } {
  print(f64_round(2.6) as i64); print(f64_trunc(2.9) as i64);
  print(f64_ceil(3.1) as i64); print(f64_floor(3.9) as i64); 0 }'

# transcendental precision: pi via asin, e via exp, scaled to capture digits.
diff_run precision $'3141\n2718\n1414' \
'fn main() -> i64 ! { io } {
  print((f64_asin(1.0) * 2000.0) as i64);   // pi*1000 -> 3141
  print((f64_exp(1.0) * 1000.0) as i64);    // e*1000  -> 2718
  print((f64_sqrt(2.0) * 1000.0) as i64);   // sqrt2   -> 1414
  0 }'

# acos/atan round out the inverse-trig set (acos(1)=0, atan(1)*4=pi).
diff_run invtrig $'0\n3141' \
'fn main() -> i64 ! { io } {
  print(f64_acos(1.0) as i64);
  print((f64_atan(1.0) * 4000.0) as i64);   // pi*1000 -> 3141
  0 }'

# --- type errors ---
reject argcount 'argument\|expects\|arity\|expected'  'fn main() -> i64 { f64_pow(2.0) as i64 }'
reject badtype  'mismatch\|expected\|f64'             'fn main() -> i64 { f64_sin(5) as i64 }'

echo "ALL F64-MATH SMOKE TESTS PASSED"
