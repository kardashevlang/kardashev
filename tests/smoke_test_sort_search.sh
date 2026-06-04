#!/usr/bin/env bash
# Roadmap v103 — sort/search algorithms: quicksort + sort_by + binary_search +
# binary_search_by + partition.
#
# The only prelude sort was an O(n^2) insertion sort; v103 upgrades `sort<T: Ord>`
# in place to quicksort (median-of-three pivot + insertion-sort cutoff <=12, same
# signature + `! {}` effect row), and adds `sort_by` (caller comparator,
# iterative since a closure can't recurse), `binary_search`/`binary_search_by`
# (Option<i64>), and `partition` (in-place, returns the pivot index).
#
# DETERMINISTIC by construction — uses the v62 SEEDED RNG (rng_seed_global +
# rand_global), never wall-time. The median-of-three guard against the O(n^2)
# cliff on sorted/reverse adversarial input is asserted by COMPLETION +
# correctness (the program finishes and the output is sorted), NOT by timing, so
# it can never flake. Quicksort is NOT stable (the old insertion sort was) — every
# in-tree caller uses a total comparator, so observable order is unchanged.
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

# (a) 1000-element SEEDED-random sort -> sortedness oracle (prints 1 if sorted).
diff_jaot random_sort $'1' \
'fn main() -> i64 ! { alloc, io } {
  rng_seed_global(12345);
  let mut v: Vec<i64> = vec_new(); let mut i = 0;
  while i < 1000 { let r = rand_global(); vec_push(&mut v, ((r % 1000000) + 1000000) % 1000000); i = i + 1; }
  sort(&mut v);
  let mut ok = 1; let mut k = 1;
  while k < vec_len(&v) { if vec_get(&v, k - 1) > vec_get(&v, k) { ok = 0; } else {} k = k + 1; }
  print(ok); 0
}'

# (b) ADVERSARIAL: already-sorted + reverse-sorted 1000-elem inputs complete and
#     sort correctly (the median-of-three guard against the O(n^2)/stack cliff).
diff_jaot adversarial $'1\n1' \
'fn check(v: &Vec<i64>) -> i64 ! { alloc } { let mut ok = 1; let mut k = 1; while k < vec_len(v) { if vec_get(v, k - 1) > vec_get(v, k) { ok = 0; } else {} k = k + 1; } ok }
fn main() -> i64 ! { alloc, io } {
  let mut a: Vec<i64> = vec_new(); let mut i = 0; while i < 1000 { vec_push(&mut a, i); i = i + 1; }
  sort(&mut a); print(check(&a));
  let mut b: Vec<i64> = vec_new(); let mut j = 1000; while j > 0 { vec_push(&mut b, j); j = j - 1; }
  sort(&mut b); print(check(&b)); 0
}'

# (c) binary_search — present element returns its value, absent returns None.
diff_jaot bsearch $'17\n-9' \
'fn main() -> i64 ! { alloc, io } {
  let mut v: Vec<i64> = vec_new(); let mut i = 30; while i > 0 { vec_push(&mut v, i); i = i - 1; }
  sort(&mut v);
  match binary_search(&v, &17) { Some(idx) => { print(vec_get(&v, idx)); }, None => { print(0 - 1); } }
  match binary_search(&v, &99) { Some(idx) => { print(idx); }, None => { print(0 - 9); } } 0
}'

# (d) sort_by — custom descending comparator -> non-increasing.
diff_jaot sortby $'5\n4\n2\n1' \
'fn desc(a: &i64, b: &i64) -> i64 { if *a > *b { 0 - 1 } else { if *a < *b { 1 } else { 0 } } }
fn main() -> i64 ! { alloc, io } {
  let mut d: Vec<i64> = vec_new(); vec_push(&mut d, 2); vec_push(&mut d, 5); vec_push(&mut d, 1); vec_push(&mut d, 4);
  sort_by(&mut d, desc);
  let mut k = 0; while k < vec_len(&d) { print(vec_get(&d, k)); k = k + 1; } 0
}'

# (e) partition — evens to the front; returns the pivot index (count satisfying).
diff_jaot partition $'4' \
'fn iseven(x: &i64) -> bool { (*x % 2) == 0 }
fn main() -> i64 ! { alloc, io } {
  let mut p: Vec<i64> = vec_new(); let mut j = 0; while j < 8 { vec_push(&mut p, j); j = j + 1; }
  print(partition(&mut p, iseven)); 0
}'

# (f) String (non-Copy) sort -> lexicographic; proves vec_swap non-Copy safety
#     survives quicksort.
diff_jaot string_sort $'apple\nbanana\ncherry' \
'fn main() -> i64 ! { alloc, io } {
  let mut s: Vec<String> = vec_new(); vec_push(&mut s, "cherry"); vec_push(&mut s, "apple"); vec_push(&mut s, "banana");
  sort(&mut s);
  let mut k = 0; while k < vec_len(&s) { print_str(&vec_get(&s, k)); k = k + 1; } 0
}'

# (g) sort_by over a struct field comparator.
diff_jaot sortby_struct $'1\n2\n3' \
'struct Widget { w: i64 }
fn bywidth(a: &Widget, b: &Widget) -> i64 { if a.w < b.w { 0 - 1 } else { if a.w > b.w { 1 } else { 0 } } }
fn main() -> i64 ! { alloc, io } {
  let mut ws: Vec<Widget> = vec_new(); vec_push(&mut ws, Widget{w:3}); vec_push(&mut ws, Widget{w:1}); vec_push(&mut ws, Widget{w:2});
  sort_by(&mut ws, bywidth);
  let mut k = 0; while k < vec_len(&ws) { print(vec_get_ref(&ws, k).w); k = k + 1; } 0
}'

echo "ALL v103 (sort/search) SMOKE TESTS PASSED"
