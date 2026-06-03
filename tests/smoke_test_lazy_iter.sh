#!/usr/bin/env bash
# v78 — lazy iterator adaptors over the v61 Iterator tower: Map / Filter (lazy,
# pull-on-demand, holding a fn/closure field), iter_fold (eager terminal,
# effect-polymorphic accumulator), and Peekable (one element of lookahead).
# i64-specialized like the rest of the tower. Differential JIT==AOT.
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

# lazy map over a range, collected.
diff_run map $'0\n2\n4\n6' \
'fn dbl(x: i64) -> i64 { x * 2 }
fn main() -> i64 ! { io, alloc } {
  let mut it = iter_map(0..4, dbl); let v = iter_collect(&mut it);
  let mut i = 0; while i < vec_len(&v) { print(vec_get(&v, i)); i = i + 1; } 0 }'

# lazy filter.
diff_run filter $'0\n2\n4\n6' \
'fn is_even(x: i64) -> bool { x % 2 == 0 }
fn main() -> i64 ! { io, alloc } {
  let mut it = iter_filter(0..8, is_even); let v = iter_collect(&mut it);
  let mut i = 0; while i < vec_len(&v) { print(vec_get(&v, i)); i = i + 1; } 0 }'

# fusion: map -> filter -> take, single pass.
diff_run fusion $'0\n6\n12' \
'fn tpl(x: i64) -> i64 { x * 3 }
fn is_even(x: i64) -> bool { x % 2 == 0 }
fn main() -> i64 ! { io, alloc } {
  let mut it = iter_take(iter_filter(iter_map(0..100, tpl), is_even), 3);
  let v = iter_collect(&mut it);
  let mut i = 0; while i < vec_len(&v) { print(vec_get(&v, i)); i = i + 1; } 0 }'

# closure (capturing) stored in the adaptor fn-field.
diff_run closure_map $'10\n11\n12' \
'fn main() -> i64 ! { io, alloc } {
  let k = 10; let mut it = iter_map(0..3, |x| x + k);
  let v = iter_collect(&mut it);
  let mut i = 0; while i < vec_len(&v) { print(vec_get(&v, i)); i = i + 1; } 0 }'

# iter_fold terminal reduction (sum 1..5 = 10, product 1..5 = 24).
diff_run fold $'10\n24' \
'fn add(a: i64, b: i64) -> i64 { a + b }
fn mul(a: i64, b: i64) -> i64 { a * b }
fn main() -> i64 ! { io, alloc } {
  let mut a = 1..5; print(iter_fold(&mut a, 0, add));
  let mut m = 1..5; print(iter_fold(&mut m, 1, mul)); 0 }'

# peekable: peek twice (non-consuming), then consume.
diff_run peekable $'10\n10\n10\n11\n12\n-1' \
'fn main() -> i64 ! { io, alloc } {
  let mut it = iter_peekable(10..13);
  match it.peek() { Some(x) => print(x), None => print(0 - 1) }
  match it.peek() { Some(x) => print(x), None => print(0 - 1) }
  match it.next() { Some(x) => print(x), None => print(0 - 1) }
  match it.next() { Some(x) => print(x), None => print(0 - 1) }
  match it.next() { Some(x) => print(x), None => print(0 - 1) }
  match it.next() { Some(x) => print(x), None => print(0 - 1) } 0 }'

echo "ALL LAZY-ITERATOR SMOKE TESTS PASSED"
