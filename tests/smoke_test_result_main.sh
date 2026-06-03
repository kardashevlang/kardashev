#!/usr/bin/env bash
# v82 — Result + ownership as the error story:
#  (1) `fn main() -> Result<T, E>` — codegen synthesizes an i64 exit-code wrapper
#      (Ok => 0, Err => 1); AOT uses it as the process exit code, JIT prints it.
#  (2) `#[allow(missing_effect)]` silences the strict-mode undeclared-effect error.
#  (3) result_flatten / option_flatten prelude combinators.
#  (4) `?` works in a no-row Result fn (v81 opt-in).
set -uo pipefail
KARDC=""
for c in "${TEST_SRCDIR:-}/_main/compiler/kardc" "${RUNFILES_DIR:-}/_main/compiler/kardc" "./compiler/kardc" "./build.local/kardc"; do
  [[ -n "$c" && -x "$c" ]] && { KARDC="$c"; break; }; done
[[ -z "$KARDC" ]] && { echo "FAIL: kardc not found"; exit 1; }
echo "Using kardc at: $KARDC"
TMP=$(mktemp -d); trap 'rm -rf "$TMP"' EXIT

# AOT exit-code check (main -> Result).
aot_exit() { local name="$1" want="$2" src="$3"
  printf '%s' "$src" > "$TMP/$name.kd"
  "$KARDC" --no-cache -o "$TMP/$name" "$TMP/$name.kd" >/dev/null 2>&1 || { echo "FAIL [$name]: compile"; "$KARDC" "$TMP/$name.kd" 2>&1|head -3; exit 1; }
  set +e; "$TMP/$name"; local rc=$?; set -e
  [[ "$rc" -eq "$want" ]] || { echo "FAIL [$name]: exit $rc, want $want"; exit 1; }
  echo "PASS: $name (exit $rc)"; }
# JIT==AOT stdout for a printing program.
diff_run() { local name="$1" expect="$2" src="$3"
  local n; n=$(printf '%s\n' "$expect" | wc -l)
  printf '%s' "$src" > "$TMP/$name.kd"
  local jit; jit=$("$KARDC" "$TMP/$name.kd" 2>/dev/null | head -n "$n") || true
  [[ "$jit" == "$expect" ]] || { echo "FAIL [$name/jit]: expected '$expect' got '$jit'"; "$KARDC" "$TMP/$name.kd" 2>&1|head -4; exit 1; }
  "$KARDC" --no-cache -o "$TMP/$name" "$TMP/$name.kd" >/dev/null 2>&1
  local aot; aot=$("$TMP/$name" 2>/dev/null | head -n "$n") || true
  [[ "$aot" == "$expect" ]] || { echo "FAIL [$name/aot]: expected '$expect' got '$aot'"; exit 1; }
  echo "PASS: $name"; }

# main -> Result: Ok exits 0, Err exits 1 (via ?-propagation).
aot_exit main_ok 0 \
'fn step() -> Result<i64, i64> { Ok(42) }
fn main() -> Result<i64, i64> { let v = step()?; Ok(v) }'
aot_exit main_err 1 \
'fn step() -> Result<i64, i64> { Err(9) }
fn main() -> Result<i64, i64> { let v = step()?; Ok(v) }'
# main -> Result<(), E> form.
aot_exit main_unit_ok 0 \
'fn check(ok: bool) -> Result<i64, i64> { if ok { Ok(0) } else { Err(1) } }
fn main() -> Result<i64, i64> { check(true)?; Ok(0) }'

# #[allow(missing_effect)] under --effects=strict: a no-/empty-row fn doing io
# would normally error; the attribute silences it.
diff_run allow_strict_jit $'42' \
'#[allow(missing_effect)]
fn greet() -> i64 { print(42) }
fn main() -> i64 { greet(); 0 }'
# confirm the attribute is what silences strict mode: without it, strict errors.
printf '%s' 'fn greet() -> i64 ! { } { print(42) }
fn main() -> i64 { greet(); 0 }' > "$TMP/nostrict.kd"
e=$("$KARDC" --effects=strict "$TMP/nostrict.kd" 2>&1 >/dev/null || true)
echo "$e" | grep -qi 'uses effect' || { echo "FAIL: strict mode should error without #[allow]"; exit 1; }
echo "PASS(reject): strict errors without #[allow(missing_effect)]"
printf '%s' '#[allow(missing_effect)]
fn greet() -> i64 ! { } { print(42) }
fn main() -> i64 { greet(); 0 }' > "$TMP/yesallow.kd"
"$KARDC" --effects=strict "$TMP/yesallow.kd" >/dev/null 2>&1 || { echo "FAIL: #[allow] should silence strict mode"; exit 1; }
echo "PASS: #[allow(missing_effect)] silences strict mode"

# result_flatten / option_flatten.
diff_run flatten $'7\n9\n-1' \
'fn main() -> i64 ! { io } {
  match result_flatten(Ok(Ok(7))) { Ok(x) => print(x), Err(e) => print(0 - 1) }
  match option_flatten(Some(Some(9))) { Some(x) => print(x), None => print(0 - 1) }
  match option_flatten(some_none()) { Some(x) => print(x), None => print(0 - 1) }
  0 }
fn some_none() -> Option<Option<i64>> { Some(None) }'

echo "ALL RESULT-MAIN / v82 SMOKE TESTS PASSED"
