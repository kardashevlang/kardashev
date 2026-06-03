#!/usr/bin/env bash
# v69 — integer range patterns `lo..hi` / `lo..=hi` in match arms. Implemented
# as sugar over v68 guards: a range arm binds the scrutinee to a fresh name and
# guards `(v >= lo) && (v < hi)` (or `<= hi` inclusive), reusing the suffix-tree
# fall-through + guard-aware exhaustiveness (so a range arm doesn't cover — a
# range-only match still needs `_`). Differential JIT==AOT. (@-bindings deferred.)
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

# exclusive ranges + wildcard.
diff_run age $'0\n1\n2' 'fn cls(age: i64) -> i64 { match age { 0..13 => 0, 13..18 => 1, _ => 2 } }
fn main() -> i64 ! { io } { print(cls(5)); print(cls(15)); print(cls(30)); 0 }'

# inclusive range boundary.
diff_run incl $'1\n2' 'fn f(n: i64) -> i64 { match n { 0..=9 => 1, _ => 2 } }
fn main() -> i64 ! { io } { print(f(9)); print(f(10)); 0 }'

# range mixed with an explicit guard arm.
diff_run mixed $'1\n2\n9' 'fn f(n: i64) -> i64 { match n { x if x < 0 => 9, 0..100 => 1, _ => 2 } }
fn main() -> i64 ! { io } { print(f(50)); print(f(150)); print(f(0 - 5)); 0 }'

# multiple range arms chain (fall-through across ranges).
diff_run chain $'1\n2\n3\n0' 'fn f(n: i64) -> i64 { match n { 0..10 => 1, 10..20 => 2, 20..30 => 3, _ => 0 } }
fn main() -> i64 ! { io } { print(f(5)); print(f(15)); print(f(25)); print(f(99)); 0 }'

# --- rejects ---
reject nonexh   'non-exhaustive'        'fn f(n: i64) -> i64 { match n { 0..10 => 1, 10..20 => 2 } } fn main() -> i64 { f(1) }'
reject at_defer 'not yet supported'      'fn f(n: i64) -> i64 { match n { x @ 5 => x, _ => 0 } } fn main() -> i64 { f(5) }'

echo "ALL RANGE-PATTERN SMOKE TESTS PASSED"
