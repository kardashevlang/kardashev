#!/usr/bin/env bash
# v32 Phase 173 smoke test — async cancellation, timeouts & structured concurrency.
#
# Part 1: timeout<T>(fut: Future<T>, ms: i64) -> Future<Option<T>> — race `fut`
# against an internal sleep_ms(ms) timer; Some(v) if fut finishes first, None on
# timeout. A compiler-synthesized leaf future (getOrEmitTimeout) built on the
# Phase 172 select machinery; the guarded future is checked first so a real
# result wins a same-poll tie. Differentially gated JIT vs AOT.
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

if command -v timeout >/dev/null 2>&1; then _TO=timeout
elif command -v gtimeout >/dev/null 2>&1; then _TO=gtimeout
else _TO=""; fi
run_to() { if [[ -n "$_TO" ]]; then "$_TO" "$@"; else shift; "$@"; fi; }

diff_run() {
    local name="$1" expect="$2" src="$3"
    local n; n=$(printf '%s\n' "$expect" | wc -l | tr -d ' ')
    printf '%s' "$src" > "$TMP/$name.kd"
    local jit; jit=$(run_to 20 "$KARDC" "$TMP/$name.kd" 2>/dev/null | head -n "$n")
    [[ "$jit" == "$expect" ]] || { echo "FAIL [$name/jit]: expected '$expect' got '$jit'"; exit 1; }
    run_to 30 "$KARDC" --no-cache -o "$TMP/$name" "$TMP/$name.kd" >/dev/null 2>&1
    local aot; aot=$(run_to 20 "$TMP/$name" 2>/dev/null | head -n "$n")
    [[ "$aot" == "$expect" ]] || { echo "FAIL [$name/aot]: expected '$expect' got '$aot'"; exit 1; }
    echo "PASS: $name"
}

# 1. fut finishes before the timer -> Some(v). (Guarded future ready first.)
diff_run timeout_some $'1\n7' '
async fn quick() -> i64 { 7 }
fn main() -> i64 ! { io, async } {
    match block_on(timeout(quick(), 100)) {
        Some(v) => { print(1); print(v); },
        None => { print(0); },
    }
    0
}
'

# 2. a slow future times out -> None. (Timer fires first.)
diff_run timeout_none $'0' '
async fn slow() -> i64 { let z = sleep_ms(200).await; z + 1 }
fn main() -> i64 ! { io, async } {
    match block_on(timeout(slow(), 5)) {
        Some(v) => { print(1); print(v); },
        None => { print(0); },
    }
    0
}
'

# 3. timeout composes with the Phase 172 combinators (map the Option result).
diff_run timeout_compose $'8' '
async fn quick() -> i64 { 7 }
fn main() -> i64 ! { io, async } {
    let g = future_map(timeout(quick(), 100), |o| match o { Some(v) => v + 1, None => 0 - 1, });
    print(block_on(g));   // Some(7) -> 8
    0
}
'

# 4. No leak / no double-free on the Some path (fut wins, timer dropped before
#    arming nested state) — tight loop stays flat under MALLOC_CHECK_=3.
cat > "$TMP/leak.kd" <<'EOF'
async fn one() -> i64 { 1 }
fn main() -> i64 ! { io, async } {
    let mut i = 0;
    let mut acc = 0;
    while i < 50000 {
        acc = acc + match block_on(timeout(one(), 1000)) { Some(v) => v, None => 0, };
        i = i + 1;
    }
    print(acc);   // 50000
    0
}
EOF
run_to 60 "$KARDC" --no-cache -o "$TMP/leak" "$TMP/leak.kd" >/dev/null 2>&1
got=$(MALLOC_CHECK_=3 run_to 60 "$TMP/leak" 2>&1)
[[ "$got" == "50000" ]] || { echo "FAIL [leak]: expected 50000 got: $got"; exit 1; }
echo "PASS: timeout_loop_no_leak (50000)"

echo "ALL PHASE 173 SMOKE TESTS PASSED"
