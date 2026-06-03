#!/usr/bin/env bash
# v76 — parameter destructuring: a tuple-pattern param `(a, b): (T, U)` and a
# wildcard param `_: T` in fn / impl-method signatures. Desugared to a fresh
# synthetic param plus a `let (a, b) = <synthetic>;` prepended to the body
# (reusing the existing tuple-destructuring `let`). Differential JIT==AOT.
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
reject() { local name="$1" needle="$2" src="$3"; printf '%s' "$src" > "$TMP/r.kd"
  local e; e=$("$KARDC" --emit-c "$TMP/r.kd" 2>&1 >/dev/null || true)
  echo "$e" | grep -qi "$needle" || { echo "FAIL[reject $name]: want '$needle' got: $e"; exit 1; }
  echo "PASS(reject): $name"; }

diff_run tuple_param $'42' \
'fn dist((x, y): (i64, i64)) -> i64 { x + y }
fn main() -> i64 ! { io } { print(dist((30, 12))); 0 }'

diff_run tuple_with_wild $'7' \
'fn fst((a, _): (i64, i64)) -> i64 { a }
fn main() -> i64 ! { io } { print(fst((7, 99))); 0 }'

diff_run wildcard_param $'5' \
'fn ignore(_: i64, y: i64) -> i64 { y }
fn main() -> i64 ! { io } { print(ignore(99, 5)); 0 }'

diff_run two_tuple_params $'10' \
'fn f((a, b): (i64, i64), (c, d): (i64, i64)) -> i64 { a + b + c + d }
fn main() -> i64 ! { io } { print(f((1, 2), (3, 4))); 0 }'

diff_run impl_method_tuple $'42' \
'struct Calc {}
impl Calc { fn add(&self, (a, b): (i64, i64)) -> i64 { a + b } }
fn main() -> i64 ! { io } { let c = Calc{}; print(c.add((20, 22))); 0 }'

diff_run three_tuple $'6\n9' \
'fn s((a, b, c): (i64, i64, i64)) -> i64 { a + b + c }
fn main() -> i64 ! { io } { print(s((1, 2, 3))); print(s((2, 3, 4))); 0 }'

# the desugar reuses tuple-destructuring `let`, which the C backend refuses;
# confirm a tuple-pattern param is refused cleanly (no miscompile).
reject c_tuple_param 'tuple-destructuring' \
'fn dist((x, y): (i64, i64)) -> i64 { x + y } fn main() -> i64 { dist((3, 4)) }'

echo "ALL PARAM-DESTRUCTURE SMOKE TESTS PASSED"
