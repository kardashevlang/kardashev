#!/usr/bin/env bash
# v77 — stdlib container convenience ops, all pure-prelude over existing
# intrinsics: Vec (is_empty / first / last / clear / truncate / extend),
# HashMap (is_empty / get_or / clear), HashSet (is_empty / clear). The mutating
# ops avoid re-reading the `&mut` container in the `while` condition (which would
# trip the borrow checker). Differential JIT==AOT.
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

# Vec: is_empty / first / last / extend / truncate / clear.
diff_run vec_ops $'0\n10\n30\n3\n2\n0' \
'fn main() -> i64 ! { io, alloc } {
  let mut v = vec_new(); vec_push(&mut v, 10); vec_push(&mut v, 20); vec_push(&mut v, 30);
  print(if vec_is_empty(&v) { 1 } else { 0 });
  match vec_first(&v) { Some(a) => print(a), None => print(0 - 1) }
  match vec_last(&v) { Some(a) => print(a), None => print(0 - 1) }
  let mut w = vec_new(); vec_extend(&mut w, &v); print(vec_len(&w));
  vec_truncate(&mut w, 2); print(vec_len(&w));
  vec_clear(&mut w); print(vec_len(&w)); 0 }'

# Vec on an empty (typed) Vec: is_empty true, first is None.
diff_run vec_empty $'1\n-1' \
'fn main() -> i64 ! { io, alloc } {
  let e: Vec<i64> = vec_new();
  print(if vec_is_empty(&e) { 1 } else { 0 });
  match vec_first(&e) { Some(a) => print(a), None => print(0 - 1) } 0 }'

# HashMap: is_empty / get_or (hit + miss) / clear.
diff_run hashmap_ops $'0\n200\n-1\n0' \
'fn main() -> i64 ! { io, alloc } {
  let mut m = hashmap_new(); hashmap_insert(&mut m, 1, 100); hashmap_insert(&mut m, 2, 200);
  print(if hashmap_is_empty(&m) { 1 } else { 0 });
  print(hashmap_get_or(&m, 2, 0 - 1));
  print(hashmap_get_or(&m, 9, 0 - 1));
  hashmap_clear(&mut m); print(hashmap_len(&m)); 0 }'

# HashSet: is_empty / clear.
diff_run hashset_ops $'0\n0' \
'fn main() -> i64 ! { io, alloc } {
  let mut s = hashset_new(); hashset_insert(&mut s, 5); hashset_insert(&mut s, 7);
  print(if hashset_is_empty(&s) { 1 } else { 0 });
  hashset_clear(&mut s); print(hashset_len(&s)); 0 }'

# string-keyed HashMap get_or (generic over K).
diff_run hashmap_str_key $'42\n7' \
'fn main() -> i64 ! { io, alloc } {
  let mut m = hashmap_new();
  hashmap_insert(&mut m, "a", 42);
  print(hashmap_get_or(&m, "a", 0));
  print(hashmap_get_or(&m, "z", 7)); 0 }'

echo "ALL CONTAINER-OPS SMOKE TESTS PASSED"
