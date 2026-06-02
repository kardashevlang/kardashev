#!/usr/bin/env bash
# v60 — type-inference depth regression suite.
#
# The v60 roadmap entry targeted "match-arm + nested-closure type inference".
# Investigation found the inference engine ALREADY handles these comprehensively
# (HM unification flows through match-arm payloads, closure params/returns,
# closure values stored in struct fields / Vecs / lets, nested enums, and
# generic methods on payloads). Rather than fabricate new inference code, this
# version LOCKS IN that behavior with an explicit regression suite so a future
# refactor can't silently weaken it. Each program relies on inference for a
# binding/param/return type that is never written down. Differential JIT==AOT.
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
  [[ "$jit" == "$expect" ]] || { echo "FAIL [$name/jit]: expected '$expect' got '$jit'"; "$KARDC" "$TMP/$name.kd" 2>&1|head -4; exit 1; }
  "$KARDC" --no-cache -o "$TMP/$name" "$TMP/$name.kd" >/dev/null 2>&1
  local aot; aot=$("$TMP/$name" 2>/dev/null | head -n "$n") || true
  [[ "$aot" == "$expect" ]] || { echo "FAIL [$name/aot]: expected '$expect' got '$aot'"; exit 1; }
  echo "PASS: $name"; }

# 1. match-arm payload type inferred (x : i64 from Some(i64)), no annotation.
diff_run m_payload $'7' 'fn main() -> i64 ! { io } {
  let o = Some(7);
  match o { Some(x) => print(x), None => print(0) } 0 }'

# 2. match arms unify to one result type, bound without annotation.
diff_run m_unify $'42' 'fn main() -> i64 ! { io } {
  let o = Some(40);
  let r = match o { Some(x) => x + 2, None => 0 };
  print(r); 0 }'

# 3. nested enum payload (Some(Some(x))) flows two levels.
diff_run m_nested $'5' 'fn main() -> i64 ! { io } {
  let o = Some(Some(5));
  match o { Some(inner) => match inner { Some(x) => print(x), None => print(1) }, None => print(2) } 0 }'

# 4. closure param type inferred from how it is applied.
diff_run cl_param $'8' 'fn main() -> i64 ! { io } {
  let add = |a, b| a + b;
  print(add(3, 5)); 0 }'

# 5. closure stored in a let, return type inferred, then called.
diff_run cl_let $'12' 'fn main() -> i64 ! { io } {
  let f = |n| n * 3;
  let r = f(4);
  print(r); 0 }'

# 6. closure value passed to a higher-order fn (param/return inferred).
diff_run cl_hof $'20' 'fn apply(x: i64, f: fn(i64) -> i64) -> i64 { f(x) }
fn main() -> i64 ! { io } {
  let r = apply(10, |n| n * 2);
  print(r); 0 }'

# 7. closure capturing an outer binding; capture type inferred.
diff_run cl_capture $'15' 'fn main() -> i64 ! { io } {
  let base = 10;
  let bump = |n| n + base;
  print(bump(5)); 0 }'

# 8. generic method/free-fn on a payload: option_map mapper inferred.
diff_run g_map $'9' 'fn main() -> i64 ! { io } {
  let r = option_map(Some(3), |x| x * 3);
  match r { Some(v) => print(v), None => print(0) } 0 }'

# 9. let with no annotation gets its element type from later vec_push calls.
diff_run let_generic $'2\n3' 'fn main() -> i64 ! { alloc, io } {
  let v = vec_new();
  vec_push(&mut v, 2);
  vec_push(&mut v, 3);
  print(vec_get(&v, 0));
  print(vec_get(&v, 1)); 0 }'

# 10. if-as-value branches unify to the bound type without annotation.
diff_run if_unify $'100' 'fn main() -> i64 ! { io } {
  let c = true;
  let r = if c { 100 } else { 200 };
  print(r); 0 }'

# 11. tuple destructuring infers each element type.
diff_run tup_destr $'1\n2' 'fn main() -> i64 ! { io } {
  let t = (1, 2);
  let (a, b) = t;
  print(a); print(b); 0 }'

# 12. match on a user enum with a payload, arm binding inferred.
diff_run user_enum $'77' 'enum E { N(i64), Z }
fn main() -> i64 ! { io } {
  let e = E::N(77);
  match e { N(x) => print(x), Z => print(0) } 0 }'

echo "ALL INFERENCE-DEPTH SMOKE TESTS PASSED"
