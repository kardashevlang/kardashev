#!/usr/bin/env bash
# v51 perf — prelude scan functions gained early-exit `break` (O(n)->O(k) on a
# hit). This pins their CORRECTNESS, especially the boundary cases that the
# early-exit refactor must not regress: a prefix/suffix LONGER than the string
# (must stay false WITHOUT an out-of-bounds read — guarded by `while ok && ...`),
# an empty needle, first-match-wins for index_of, and the all/any/find/contains
# truth tables. Differentially JIT==AOT.
set -euo pipefail
KARDC=""
for c in "${TEST_SRCDIR:-}/_main/compiler/kardc" "${RUNFILES_DIR:-}/_main/compiler/kardc" "./compiler/kardc" "./build.local/kardc"; do
  [[ -n "$c" && -x "$c" ]] && { KARDC="$c"; break; }; done
[[ -z "$KARDC" ]] && { echo "FAIL: kardc not found"; exit 1; }
echo "Using kardc at: $KARDC"
TMP=$(mktemp -d); trap 'rm -rf "$TMP"' EXIT

run() { # name expected src  — JIT stdout (minus the trailing return line) == AOT == expected
  local name="$1" expect="$2" src="$3"
  local n; n=$(printf '%s\n' "$expect" | wc -l)
  printf '%s' "$src" > "$TMP/$name.kd"
  local jit; jit=$("$KARDC" "$TMP/$name.kd" 2>/dev/null | head -n "$n") || true
  [[ "$jit" == "$expect" ]] || { echo "FAIL [$name/jit]: expected '$expect' got '$jit'"; "$KARDC" "$TMP/$name.kd" 2>&1 | head -3; exit 1; }
  "$KARDC" --no-cache -o "$TMP/$name" "$TMP/$name.kd" >/dev/null 2>&1
  local aot; aot=$("$TMP/$name" 2>/dev/null | head -n "$n") || true
  [[ "$aot" == "$expect" ]] || { echo "FAIL [$name/aot]: expected '$expect' got '$aot'"; exit 1; }
  echo "PASS: $name"
}

# str_starts_with / str_ends_with — incl. prefix/suffix longer than the string
# (must be false, NOT an OOB read) and exact-length match.
run str_prefix $'0\n0\n1\n1\n1' '
fn main() -> i64 ! { io } {
  let s = "hi"; let long = "hello";
  if str_starts_with(&s, &long) { print(1); } else { print(0); }   // prefix longer -> 0
  if str_ends_with(&s, &long) { print(1); } else { print(0); }     // suffix longer -> 0
  if str_starts_with(&long, &"hel") { print(1); } else { print(0); }
  if str_ends_with(&long, &"llo") { print(1); } else { print(0); }
  if str_starts_with(&long, &long) { print(1); } else { print(0); }// exact -> 1
  0
}'

# str_index_of — found (first match), not found, empty needle (0), first-of-two.
run str_idx $'2\n-1\n0\n1' '
fn main() -> i64 ! { io } {
  let long = "hello";
  print(str_index_of(&long, &"ll"));   // 2
  print(str_index_of(&long, &"xyz"));  // -1
  print(str_index_of(&long, &""));     // 0 (empty needle)
  print(str_index_of(&"aXbXc", &"X")); // first match -> 1
  0
}'

# vec_contains / vec_index_of / vec_any / vec_all / vec_find truth tables.
run vec_scan $'1\n2\n1\n0\n7' '
fn main() -> i64 ! { io, alloc } {
  let mut v = vec_new();
  vec_push(&mut v, 5); vec_push(&mut v, 7); vec_push(&mut v, 9);
  if vec_contains(&v, &7) { print(1); } else { print(0); }   // present -> 1
  print(vec_index_of(&v, &9));                                // 2
  if vec_any(&v, |x: &i64| *x > 8) { print(1); } else { print(0); }  // 9>8 -> 1
  if vec_all(&v, |x: &i64| *x > 6) { print(1); } else { print(0); }  // 5 fails -> 0
  match vec_find(&v, |x: &i64| *x > 6) { Some(n) => print(n), None => print(0 - 1) } // 7
  0
}'

echo "ALL PRELUDE EARLY-EXIT SMOKE TESTS PASSED"
