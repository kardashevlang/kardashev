#!/usr/bin/env bash
# v60 — effect-row-variable / param-name collision soundness.
#
# An effect-polymorphic higher-order FREE function (e.g. the prelude
# `option_map(o, f: fn(i64)->i64 ! {e}) -> Option<i64> ! {e}`) calls its
# fn-typed PARAMETER `f`. Before v60 the free-fn body checker forgot to bind
# the effect-row-var name into its generic env, so `f`'s param type got a
# mismatched row var; the call's per-site effect set came out empty and
# `collectEffects` fell back to a TOP-LEVEL fn of the same name — wrongly
# attributing that fn's effects to the higher-order fn. Any program that
# merely DEFINED a top-level `fn f ! {io}` (or `! {panic}`, etc.) — extremely
# common single-letter names — then failed to compile with a spurious
# "function 'option_map' uses effect `io` but does not declare it".
#
# This pins the fix: such programs compile and run; option_map stays effect-
# polymorphic; differential JIT==AOT.
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

# A top-level `fn f ! { io }` no longer contaminates the prelude option_map.
diff_run collide_io $'2\n1' 'fn f() -> i64 ! { io } { print(2); 0 }
fn main() -> i64 ! { io } { let r = f(); print(1); 0 }'

# Same with a panic-effect collision (the panic-as-tail case).
diff_run collide_panic $'7' 'fn f() -> i64 ! { panic } { panic("x") }
fn main() -> i64 ! { io } { print(7); 0 }'

# And `g` — the other common combinator param name (future_and_then's `g`).
diff_run collide_g $'9' 'fn g(x: i64) -> i64 ! { io } { print(x); 0 }
fn main() -> i64 ! { io } { g(9); 0 }'

# option_map is STILL genuinely effect-polymorphic: a pure mapper keeps the
# caller pure; an io mapper makes the call site io. Here we actually USE it.
diff_run polymap $'5\n6' 'fn dbl(x: i64) -> i64 { x * 2 }
fn main() -> i64 ! { io } {
    let r = option_map(Some(3), dbl);
    match r { Some(v) => { print(v - 1); print(v); 0 }, None => 0 }
}'

# A user higher-order effect-polymorphic fn whose param collides with a
# top-level fn of the same name AND a different effect — must stay sound:
# apply must report exactly the mapper effect, named via its own row var.
diff_run user_hof $'4\n8' 'fn apply(x: i64, f: fn(i64) -> i64 ! {e}) -> i64 ! {e} { f(x) }
fn f(x: i64) -> i64 ! { io } { print(x); x * 2 }
fn main() -> i64 ! { io } { let r = apply(4, f); print(r); 0 }'

# Negative: a pure-declared fn that calls option_map with an IO mapper must
# still be REJECTED (the row var carries io to the caller) — proves the fix
# did not blanket-suppress effect propagation.
printf '%s' 'fn shout(x: i64) -> i64 ! { io } { print(x); x }
fn pure_caller() -> i64 ! { } { let r = option_map(Some(1), shout); 0 }
fn main() -> i64 ! { io } { print(0); 0 }' > "$TMP/neg.kd"
e=$("$KARDC" "$TMP/neg.kd" 2>&1 >/dev/null || true)
echo "$e" | grep -qi "io" || { echo "FAIL[neg]: pure caller of io-mapper option_map should be rejected; got: $e"; exit 1; }
echo "PASS(reject): pure caller of io-mapper option_map"

echo "ALL EFFECT-PARAM-COLLISION SMOKE TESTS PASSED"
