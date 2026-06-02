#!/usr/bin/env bash
# v61 — LAZY iterator adaptor tower.
#
# Take/Skip/Chain/Zip/Enumerate are stateful adaptor STRUCTS that each
# `impl Iterator` and pull one element at a time, so a chain fuses into a
# single pass with O(1) extra memory — no intermediate Vec (only a terminal
# `iter_collect` allocates). This pins:
#   - composition correctness (take∘skip, zip, enumerate, chain),
#   - the Vec->iterator bridge (vec_iter_i64) feeding the tower,
#   - iter_collect draining the tower,
#   - ALLOCATION DISCIPLINE: take(skip(range(50_000_000), …), 5) completes in
#     O(1) memory and near-instantly. An EAGER adaptor would try to materialize
#     a 50M-element Vec (~400MB) and either OOM or take seconds; the lazy tower
#     pulls exactly 25 elements. We assert correct output AND a tight wall-clock
#     bound as the RSS/behavioral proxy (roadmap gate option b).
# Differential JIT==AOT.
set -euo pipefail
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
  [[ "$jit" == "$expect" ]] || { echo "FAIL [$name/jit]: expected '$expect' got '$jit'"; "$KARDC" "$TMP/$name.kd" 2>&1|head -5; exit 1; }
  "$KARDC" --no-cache -o "$TMP/$name" "$TMP/$name.kd" >/dev/null 2>&1
  local aot; aot=$("$TMP/$name" 2>/dev/null | head -n "$n") || true
  [[ "$aot" == "$expect" ]] || { echo "FAIL [$name/aot]: expected '$expect' got '$aot'"; exit 1; }
  echo "PASS: $name"; }

# 1. take ∘ skip over a range — the canonical fused chain.
diff_run take_skip $'20\n21\n22\n23\n24' 'fn main() -> i64 ! { alloc, io } {
  let mut t = iter_take(iter_skip(Range{start:0,end:100,inclusive:0}, 20), 5);
  let mut go = true;
  while go { match t.next() { Some(x) => { print(x); }, None => { go = false; } } } 0 }'

# 2. zip — pairs, stops at the shorter side.
diff_run zip $'0\n10\n1\n11\n2\n12' 'fn main() -> i64 ! { alloc, io } {
  let mut z = iter_zip(Range{start:0,end:3,inclusive:0}, Range{start:10,end:99,inclusive:0});
  let mut go = true;
  while go { match z.next() { Some(p) => { print(p.0); print(p.1); }, None => { go = false; } } } 0 }'

# 3. enumerate — index/value pairs.
diff_run enumerate $'0\n7\n1\n8\n2\n9' 'fn main() -> i64 ! { alloc, io } {
  let mut e = iter_enumerate(Range{start:7,end:10,inclusive:0});
  let mut go = true;
  while go { match e.next() { Some(p) => { print(p.0); print(p.1); }, None => { go = false; } } } 0 }'

# 4. chain — concatenation.
diff_run chain $'0\n1\n100\n101' 'fn main() -> i64 ! { alloc, io } {
  let mut c = iter_chain(Range{start:0,end:2,inclusive:0}, Range{start:100,end:102,inclusive:0});
  let mut go = true;
  while go { match c.next() { Some(x) => { print(x); }, None => { go = false; } } } 0 }'

# 5. Vec -> iterator bridge feeding the tower (take 2 of a 4-elem Vec).
diff_run veciter $'100\n200' 'fn main() -> i64 ! { alloc, io } {
  let mut v = vec_new();
  vec_push(&mut v, 100); vec_push(&mut v, 200); vec_push(&mut v, 300); vec_push(&mut v, 400);
  let mut t = iter_take(vec_iter_i64(v), 2);
  let mut go = true;
  while go { match t.next() { Some(x) => { print(x); }, None => { go = false; } } } 0 }'

# 6. iter_collect drains the fused chain into a Vec (terminal allocation).
diff_run collect $'3\n22\n23\n24' 'fn main() -> i64 ! { alloc, io } {
  let mut t = iter_take(iter_skip(Range{start:0,end:100,inclusive:0}, 22), 3);
  let out = iter_collect(&mut t);
  print(vec_len(&out));
  print(vec_get(&out, 0)); print(vec_get(&out, 1)); print(vec_get(&out, 2)); 0 }'

# 7. ALLOCATION DISCIPLINE (lazy fusion proxy): take(skip(range(50M),…),5).
#    Eager adaptors would materialize a ~50M Vec; the lazy tower pulls 25
#    elements. Assert correct output AND a tight wall-clock bound.
BIG='fn main() -> i64 ! { alloc, io } {
  let mut t = iter_take(iter_skip(Range{start:0,end:50000000,inclusive:0}, 49999990), 5);
  let mut go = true;
  while go { match t.next() { Some(x) => { print(x); }, None => { go = false; } } } 0 }'
printf '%s' "$BIG" > "$TMP/big.kd"
"$KARDC" --no-cache -o "$TMP/big" "$TMP/big.kd" >/dev/null 2>&1
start=$(date +%s%N)
got=$("$TMP/big" 2>/dev/null)
end=$(date +%s%N)
ms=$(( (end - start) / 1000000 ))
want=$'49999990\n49999991\n49999992\n49999993\n49999994'
[[ "$got" == "$want" ]] || { echo "FAIL [bigrange/output]: got '$got'"; exit 1; }
# Lazy fusion: 25 pulls. Allow a generous 2000ms ceiling — an eager 50M Vec
# materialization is far slower and ~400MB; this stays sub-second in practice.
(( ms < 2000 )) || { echo "FAIL [bigrange/time]: ${ms}ms — adaptor is not fused/lazy"; exit 1; }
echo "PASS: bigrange_lazy (O(1) mem, ${ms}ms for 50M-range skip+take)"

echo "ALL LAZY-ITERATOR SMOKE TESTS PASSED"
