#!/usr/bin/env bash
# v70 — saturating integer arithmetic + bit-manipulation intrinsics on i64.
# saturating_<op> clamps to INT64_MIN/MAX on overflow (vs checked_* -> Option);
# the bit ops lower to LLVM intrinsics (ctpop/ctlz/cttz/bswap/fshl/fshr).
# Differential JIT==AOT.
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

# saturating arithmetic: clamp at the boundaries, exact in range.
diff_run sat_add $'9223372036854775807\n-9223372036854775808\n42' \
'fn main() -> i64 ! { io } {
  let max = 9223372036854775807; let min = 0 - 9223372036854775807 - 1;
  print(saturating_add(max, 1)); print(saturating_add(min, 0 - 1)); print(saturating_add(40, 2)); 0 }'

diff_run sat_sub $'-9223372036854775808\n9223372036854775807\n5' \
'fn main() -> i64 ! { io } {
  let max = 9223372036854775807; let min = 0 - 9223372036854775807 - 1;
  print(saturating_sub(min, 1)); print(saturating_sub(max, 0 - 1)); print(saturating_sub(8, 3)); 0 }'

# mul clamps in the correct direction (sign of the true product).
diff_run sat_mul $'9223372036854775807\n-9223372036854775808\n12' \
'fn main() -> i64 ! { io } {
  let max = 9223372036854775807;
  print(saturating_mul(max, 2)); print(saturating_mul(max, 0 - 2)); print(saturating_mul(3, 4)); 0 }'

# unary bit ops.
diff_run bits_unary $'3\n64\n63\n3\n64\n72057594037927936' \
'fn main() -> i64 ! { io } {
  print(count_ones(7)); print(count_zeros(0)); print(leading_zeros(1));
  print(trailing_zeros(8)); print(leading_zeros(0)); print(reverse_bytes(1)); 0 }'

# rotates (amount modulo 64).
diff_run rotates $'2\n-9223372036854775808\n1\n1' \
'fn main() -> i64 ! { io } {
  print(rotate_left(1, 1)); print(rotate_right(1, 1));
  print(rotate_left(1, 64)); print(rotate_right(1, 64)); 0 }'

# --- type errors ---
reject argcount 'expects\|argument\|arity\|expected'   'fn main() -> i64 { saturating_add(1) }'
reject badtype  'mismatch\|expected\|i64'              'fn main() -> i64 ! { io } { print(count_ones(true)); 0 }'

echo "ALL SATURATING/BIT-INTRINSIC SMOKE TESTS PASSED"
