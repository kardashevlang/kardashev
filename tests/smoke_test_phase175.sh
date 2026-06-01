#!/usr/bin/env bash
# v32 Phase 175 smoke test — effect SUBTYPING (subsumption).
#
# A function value that performs FEWER effects is usable where one with MORE
# effects is expected: calling it can only do at most what the expected
# signature permits, so the substitution is sound. Concretely, a pure
# `fn()->R` coerces into a `fn()->R ! {io}` parameter; the reverse (an actual
# that performs MORE than the expected allows) is rejected. Effect inference for
# closures (their row is inferred from the body) continues to work, and the
# `! {e}` effect-ROW-var threading of vec_map / future_map is unchanged (the
# subsumption rule fires only for a CLOSED actual row vs a strictly-larger /
# open expected; everything else keeps the exact symmetric unify).
#
# Pure typecheck behavior — checked on JIT exit/stdout and (positives) AOT.
set -euo pipefail

KARDC=""
for candidate in \
    "${TEST_SRCDIR:-}/_main/compiler/kardc" \
    "${TEST_SRCDIR:-}/kardashev/compiler/kardc" \
    "${RUNFILES_DIR:-}/_main/compiler/kardc" \
    "${RUNFILES_DIR:-}/kardashev/compiler/kardc" \
    "./compiler/kardc" \
    "./build.local/kardc"; do
    if [[ -n "$candidate" && -x "$candidate" ]]; then KARDC="$candidate"; break; fi
done
[[ -z "$KARDC" ]] && { echo "FAIL: kardc binary not found"; exit 1; }
echo "Using kardc at: $KARDC"

TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

diff_run() {
    local name="$1" expect="$2" src="$3"
    local n; n=$(printf '%s\n' "$expect" | wc -l | tr -d ' ')
    printf '%s' "$src" > "$TMP/$name.kd"
    local jit; jit=$("$KARDC" "$TMP/$name.kd" 2>/dev/null | head -n "$n")
    [[ "$jit" == "$expect" ]] || { echo "FAIL [$name/jit]: expected '$expect' got '$jit'"; exit 1; }
    "$KARDC" --no-cache -o "$TMP/$name" "$TMP/$name.kd" >/dev/null 2>&1
    local aot; aot=$("$TMP/$name" 2>/dev/null | head -n "$n")
    [[ "$aot" == "$expect" ]] || { echo "FAIL [$name/aot]: expected '$expect' got '$aot'"; exit 1; }
    echo "PASS: $name"
}
expect_err() {
    local name="$1" needle="$2" src="$3"
    printf '%s' "$src" > "$TMP/$name.kd"
    local err; err=$("$KARDC" "$TMP/$name.kd" 2>&1 >/dev/null || true)
    echo "$err" | grep -qi "$needle" || {
        echo "FAIL [$name]: expected error containing '$needle', got: $err"; exit 1; }
    echo "PASS (negative): $name"
}

# 1. A pure top-level fn passed where an io-effecting fn param is expected.
diff_run sub_pure_to_io $'42' '
fn apply(f: fn(i64) -> i64 ! { io }, x: i64) -> i64 ! { io } { f(x) }
fn pure_inc(n: i64) -> i64 { n + 1 }
fn main() -> i64 ! { io } { print(apply(pure_inc, 41)); 0 }
'

# 2. A pure closure where a multi-effect ({io, alloc}) param is expected.
diff_run sub_pure_closure $'42' '
fn apply(f: fn(i64) -> i64 ! { io, alloc }, x: i64) -> i64 ! { io, alloc } { f(x) }
fn main() -> i64 ! { io, alloc } { print(apply(|n| n * 2, 21)); 0 }
'

# 3. A fn with a STRICT SUBSET of effects ({io}) where {io, alloc} is expected.
diff_run sub_subset $'9' '
fn run(f: fn() -> i64 ! { io, alloc }) -> i64 ! { io, alloc } { f() }
fn only_io() -> i64 ! { io } { print(9); 9 }
fn main() -> i64 ! { io, alloc } { run(only_io); 0 }
'

# 4. NEGATIVE: an io fn where a PURE fn param is expected — actual does MORE
#    than the expected permits, so it must be rejected (subsumption is one-way).
expect_err sub_neg_more 'expected fn(i64) -> i64' '
fn apply_pure(f: fn(i64) -> i64, x: i64) -> i64 { f(x) }
fn ioic(n: i64) -> i64 ! { io } { print(n); n + 1 }
fn main() -> i64 ! { io } { print(apply_pure(ioic, 41)); 0 }
'

# 5. Exact-match (no subsumption needed) still type-checks and runs.
diff_run sub_exact $'5' '
fn apply(f: fn(i64) -> i64 ! { io }, x: i64) -> i64 ! { io } { f(x) }
fn ioic(n: i64) -> i64 ! { io } { print(n); n }
fn main() -> i64 ! { io } { apply(ioic, 5); 0 }
'

echo "ALL PHASE 175 SMOKE TESTS PASSED"
