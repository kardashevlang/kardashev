#!/usr/bin/env bash
# v81 — effects are OPT-IN. A fn with NO `! { ... }` row is unchecked (may
# perform any effect); only a fn that wrote an EXPLICIT row (incl. `! { }`, an
# asserted-pure) must declare everything it performs. Backward-compatible: all
# existing `! { io }`-annotated code is still strictly checked. `--effects=strict`
# restores the old "absent row => pure" rule. Differential JIT==AOT.
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
reject_flag() { local name="$1" flag="$2" needle="$3" src="$4"; printf '%s' "$src" > "$TMP/$name.kd"
  local e; e=$("$KARDC" $flag "$TMP/$name.kd" 2>&1 >/dev/null || true)
  echo "$e" | grep -qi "$needle" || { echo "FAIL[reject $name]: want '$needle' got: $e"; exit 1; }
  echo "PASS(reject): $name"; }

# (a) a fn with NO effect row may call print; the program runs.
diff_run no_row $'42' \
'fn greet() -> i64 { print(42) }
fn main() -> i64 { greet(); 0 }'

# (b) an annotated caller of an un-annotated effectful fn still type-checks.
diff_run annotated_caller $'7' \
'fn emit() -> i64 { print(7) }
fn g() -> i64 ! { io } { emit() }
fn main() -> i64 ! { io } { g() }'

# (c) a no-row main may both compute (pure) and print (io) under opt-in.
diff_run pure_no_row $'5' \
'fn add(a: i64, b: i64) -> i64 { a + b }
fn main() -> i64 { print(add(2, 3)); 0 }'

# (d) an EXPLICIT empty row `! { }` is an asserted-pure -> performing io errors.
reject_flag explicit_empty "" 'uses effect `io`' \
'fn f() -> i64 ! { } { print(42) }
fn main() -> i64 ! { io } { f() }'

# (e) an EXPLICIT row that UNDER-declares (does alloc, says only io) still errors
#     (explicit rows are always strictly checked -> backward compatible).
reject_flag under_declared "" 'uses effect `alloc`' \
'fn f() -> i64 ! { io } { print(1); let v = vec_new(); vec_push(&mut v, 1); vec_len(&v) }
fn main() -> i64 ! { io, alloc } { f() }'

# (f) --effects=strict restores the old rule: a no-row fn doing io errors.
reject_flag strict_mode "--effects=strict" 'uses effect `io`' \
'fn greet() -> i64 { print(42) }
fn main() -> i64 { greet(); 0 }'

# (g) under --effects=strict, a correctly-annotated program still compiles+runs.
diff_run strict_ok $'9' \
'fn g() -> i64 ! { io } { print(9) }
fn main() -> i64 ! { io } { g() }'

echo "ALL EFFECTS-OPT-IN SMOKE TESTS PASSED"
