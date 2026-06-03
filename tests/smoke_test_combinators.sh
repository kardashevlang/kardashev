#!/usr/bin/env bash
# v79 — generic Option/Result combinators. The i64-only option_map / and_then /
# unwrap_or / is_some / ok_or and result_map / unwrap_or / is_ok are generalized
# to `<T, …>` (mirroring the already-generic result_is_err/ok/err/map_err), and
# the rest of the vocabulary is added (option_is_none/map_or/or/or_else/
# ok_or_else; result_and_then/unwrap_or_else/map_or/or/or_else). All pure-prelude
# `match` over the enum, closures effect-polymorphic. Also re-verifies the (v35)
# `?` operator + Error trait. Differential JIT==AOT.
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

PRE='fn dbl(x: i64) -> i64 { x * 2 }
fn chk(x: i64) -> Option<i64> { if x > 0 { Some(x) } else { None } }
fn rdbl(x: i64) -> Result<i64, i64> { Ok(x * 2) }
fn none_i() -> Option<i64> { None }
fn err_i() -> Result<i64, i64> { Err(5) }
'

diff_run option_combinators $'42\n99\n5\n6\n7\n8\n1\n1' \
"$PRE"'fn main() -> i64 ! { io, alloc } {
  match option_map(Some(21), dbl) { Some(x) => print(x), None => print(0 - 1) }
  print(option_unwrap_or(none_i(), 99));
  match option_and_then(Some(5), chk) { Some(x) => print(x), None => print(0 - 1) }
  print(option_map_or(Some(3), 0, dbl));
  print(option_map_or(none_i(), 7, dbl));
  match option_or(none_i(), Some(8)) { Some(x) => print(x), None => print(0 - 1) }
  if option_is_some(Some(1)) { print(1); } else { print(0); }
  if option_is_none(none_i()) { print(1); } else { print(0); } 0 }'

diff_run result_combinators $'40\n99\n8\n10\n12\n6' \
"$PRE"'fn main() -> i64 ! { io, alloc } {
  match result_map(rdbl(10), dbl) { Ok(x) => print(x), Err(e) => print(0 - 1) }
  print(result_unwrap_or(err_i(), 99));
  match result_and_then(rdbl(2), rdbl) { Ok(x) => print(x), Err(e) => print(0 - 1) }
  print(result_unwrap_or_else(err_i(), dbl));
  print(result_map_or(rdbl(3), 0, dbl));
  match result_or(err_i(), rdbl(3)) { Ok(x) => print(x), Err(e) => print(0 - 1) } 0 }'

# Option/Result type-changing combinators: ok_or (Option -> Result), result_ok
# (Result -> Option), map_err (change E type), generic over a String value.
diff_run type_changing $'1\n2\n9' \
"$PRE"'fn neg(e: i64) -> i64 { e + 1 }
fn main() -> i64 ! { io, alloc } {
  match option_ok_or(Some(1), 0) { Ok(x) => print(x), Err(e) => print(0 - 1) }
  match result_ok(rdbl(1)) { Some(x) => print(x), None => print(0 - 1) }
  match result_map_err(err_i(), neg) { Ok(x) => print(0 - 1), Err(e) => print(e + 3) } 0 }'

# (v35) `?` operator + Error trait still work end-to-end.
diff_run try_and_error $'15\n-1' \
'enum MyErr { Bad }
impl Error for MyErr { fn message(&self) -> String ! { alloc } { "bad" } }
fn step(ok: bool) -> Result<i64, MyErr> { if ok { Ok(10) } else { Err(MyErr::Bad) } }
fn run(ok: bool) -> Result<i64, MyErr> { let v = step(ok)?; Ok(v + 5) }
fn main() -> i64 ! { io, alloc } {
  match run(true)  { Ok(x) => print(x), Err(e) => print(0 - 1) }
  match run(false) { Ok(x) => print(x), Err(e) => print(0 - 1) } 0 }'

echo "ALL COMBINATOR SMOKE TESTS PASSED"
