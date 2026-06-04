#!/usr/bin/env bash
# Roadmap v104 — slice utilities (prelude-only).
#
# Slices (&[T]) were first-class (v89/v90/v93) but had only scalar
# get/len/set/get_ref/get_mut builtins — no conversion, iteration, or windowing.
# v104 adds: slice_to_vec<T: Clone> (owned deep-copy), a borrowing
# SliceIter<T>{s: &[T]} + slice_iter (element-generic, rides the v101 g* tower),
# slice_chunks/slice_windows -> Vec<&[T]> zero-copy views (rooted in a &Vec<T>
# param so the views stay sound), and slice_contains/slice_index_of<T: Eq>.
#
# JIT==AOT only (no --emit-c leg): these are generic/&[T] over Vec<&[T]>, which
# the C backend cleanly refuses (scalar-i64-slice only) — deterministic regardless.
set -uo pipefail
KARDC=""
for c in "${TEST_SRCDIR:-}/_main/compiler/kardc" "${RUNFILES_DIR:-}/_main/compiler/kardc" "./compiler/kardc" "./build.local/kardc"; do
  [[ -n "$c" && -x "$c" ]] && { KARDC="$c"; break; }; done
[[ -z "$KARDC" ]] && { echo "FAIL: kardc not found"; exit 1; }
echo "Using kardc at: $KARDC"
TMP=$(mktemp -d); trap 'rm -rf "$TMP"' EXIT

diff_jaot() {  # $1 name  $2 expected(multiline)  $3 src
  local n; n=$(printf '%s\n' "$2" | wc -l)
  printf '%s' "$3" > "$TMP/$1.kd"
  local j; j=$("$KARDC" --no-cache "$TMP/$1.kd" 2>/dev/null | head -n "$n")
  [[ "$j" == "$2" ]] || { echo "FAIL [$1/jit]: got '$j' want '$2'"; "$KARDC" "$TMP/$1.kd" 2>&1 | head -4; exit 1; }
  "$KARDC" --no-cache -o "$TMP/$1" "$TMP/$1.kd" >/dev/null 2>&1 || { echo "FAIL [$1]: AOT build failed"; "$KARDC" "$TMP/$1.kd" 2>&1 | head -4; exit 1; }
  local a; a=$("$TMP/$1" 2>/dev/null | head -n "$n")
  [[ "$a" == "$2" ]] || { echo "FAIL [$1/aot]: got '$a' want '$2'"; exit 1; }
  echo "PASS [$1]"
}

# (a) slice_to_vec INDEPENDENCE — copy &v[0..3], mutate the source, copy unchanged.
diff_jaot to_vec_independent $'0\n1\n2\n99' \
'fn main() -> i64 ! { alloc, io } {
  let mut v: Vec<i64> = vec_new(); vec_push(&mut v, 0); vec_push(&mut v, 1); vec_push(&mut v, 2); vec_push(&mut v, 3);
  let cp = slice_to_vec(&v[0..3]);
  slice_set(&mut v[0..3], 0, 99);
  print(vec_get(&cp, 0)); print(vec_get(&cp, 1)); print(vec_get(&cp, 2));
  print(vec_get(&v, 0)); 0
}'

# (b) slice_iter chains the v101 g* tower: slice_iter(&v[1..4]).map(dbl).collect()
diff_jaot iter_chain $'2\n4\n6' \
'fn dbl(x: &i64) -> i64 { *x * 2 }
fn main() -> i64 ! { alloc, io } {
  let mut v: Vec<i64> = vec_new(); let mut i = 0; while i < 6 { vec_push(&mut v, i); i = i + 1; }
  let it: SliceIter<i64> = slice_iter(&v[1..4]);
  let out = iter_collect(&mut gmap(it, dbl));
  let mut k = 0; while k < vec_len(&out) { print(vec_get(&out, k)); k = k + 1; } 0
}'

# (c) slice_chunks of 10 by 3 -> 4 chunks of lengths [3,3,3,1]
diff_jaot chunks $'4\n3\n3\n3\n1' \
'fn main() -> i64 ! { alloc, io } {
  let mut v: Vec<i64> = vec_new(); let mut i = 0; while i < 10 { vec_push(&mut v, i); i = i + 1; }
  let cs = slice_chunks(&v, 3);
  print(vec_len(&cs));
  let mut k = 0; while k < vec_len(&cs) { print(slice_len(vec_get(&cs, k))); k = k + 1; } 0
}'

# (d) slice_windows of [10,20,30] by 2 -> [[10,20],[20,30]]
diff_jaot windows $'2\n10\n20\n20\n30' \
'fn main() -> i64 ! { alloc, io } {
  let mut v: Vec<i64> = vec_new(); vec_push(&mut v, 10); vec_push(&mut v, 20); vec_push(&mut v, 30);
  let ws = slice_windows(&v, 2);
  print(vec_len(&ws));
  let mut k = 0;
  while k < vec_len(&ws) { let w = vec_get(&ws, k); print(slice_get(w, 0)); print(slice_get(w, 1)); k = k + 1; } 0
}'

# (e) slice_contains / slice_index_of — present + absent
diff_jaot contains $'1\n3\n0\n-1' \
'fn main() -> i64 ! { alloc, io } {
  let mut v: Vec<i64> = vec_new(); let mut i = 0; while i < 5 { vec_push(&mut v, i); i = i + 1; }
  if slice_contains(&v[0..5], &3) { print(1); } else { print(0); }
  print(slice_index_of(&v[0..5], &3));
  if slice_contains(&v[0..5], &99) { print(1); } else { print(0); }
  print(slice_index_of(&v[0..5], &99)); 0
}'

# (f) slice_to_vec over a non-Copy String element (deep clone)
diff_jaot to_vec_string $'2\n3' \
'fn main() -> i64 ! { alloc, io } {
  let mut v: Vec<String> = vec_new(); vec_push(&mut v, "ab"); vec_push(&mut v, "cde"); vec_push(&mut v, "fg");
  let cp = slice_to_vec(&v[0..2]);
  print(str_len(&vec_get(&cp, 0))); print(str_len(&vec_get(&cp, 1))); 0
}'

echo "ALL v104 (slice utilities) SMOKE TESTS PASSED"
