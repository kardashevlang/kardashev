#!/usr/bin/env bash
# v42 — Duration: operator-overloaded time arithmetic (deterministic).
# Add/Sub (v37 operator traits, by-value) + Ord cmp (&self) + conversions.
set -euo pipefail
KARDC=""
for c in "${TEST_SRCDIR:-}/_main/compiler/kardc" "${RUNFILES_DIR:-}/_main/compiler/kardc" "./compiler/kardc" "./build.local/kardc"; do
  [[ -n "$c" && -x "$c" ]] && { KARDC="$c"; break; }; done
[[ -z "$KARDC" ]] && { echo "FAIL: kardc not found"; exit 1; }
echo "Using kardc at: $KARDC"
TMP=$(mktemp -d); trap 'rm -rf "$TMP"' EXIT
diff_run() {
  local name="$1" expect="$2" src="$3"; local n; n=$(printf '%s\n' "$expect" | wc -l | tr -d ' ')
  printf '%s' "$src" > "$TMP/$name.kd"
  local jit; jit=$("$KARDC" "$TMP/$name.kd" 2>/dev/null | head -n "$n") || true
  [[ "$jit" == "$expect" ]] || { echo "FAIL [$name/jit]: exp '$expect' got '$jit'"; exit 1; }
  "$KARDC" --no-cache -o "$TMP/$name" "$TMP/$name.kd" >/dev/null 2>&1
  local aot; aot=$("$TMP/$name" 2>/dev/null | head -n "$n") || true
  [[ "$aot" == "$expect" ]] || { echo "FAIL [$name/aot]: exp '$expect' got '$aot'"; exit 1; }
  echo "PASS: $name"
}
diff_run duration $'1\n-1\n0\n2500\n2\n1500' '
fn main() -> i64 ! { io } {
    let a = duration_from_secs(2);
    let b = duration_from_millis(500);
    print(a.cmp(&b));                       // 1
    print(b.cmp(&a));                       // -1
    print(a.cmp(&duration_from_secs(2)));   // 0
    print(duration_as_millis(&(duration_from_secs(2) + duration_from_millis(500))));  // 2500
    print(duration_as_secs(&(duration_from_secs(2) + duration_from_millis(500))));    // 2
    print(duration_as_millis(&(duration_from_secs(2) - duration_from_millis(500))));  // 1500
    0
}
'
echo "ALL TIME/DURATION SMOKE TESTS PASSED"
