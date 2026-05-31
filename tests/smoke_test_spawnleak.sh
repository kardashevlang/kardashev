#!/usr/bin/env bash
# Phase 121 (Roadmap v21 — "close the leaks"): the async executor's `spawn` +
# `join` path leaked a heap frame per spawned task (the executor's task array
# grew unbounded — RSS ballooned over a spawn+join loop), because `join` drove +
# read the result but never reclaimed the task (unlike `block_on`, which reaps).
# A naive "reap-if-idle after join" is WRONG: driving one handle to completion
# also completes sibling tasks (the executor interleaves), so reaping all-done
# tasks frees a sibling's result before its own `join` reads it. The fix is a
# PER-HANDLE release (`__kd_exec_release(h)`): free only task h's frame+slot, and
# reset the executor only when every task is released. This test pins: (1) a
# spawn+join loop is RSS-flat; (2) multi-handle joins return the correct distinct
# results; (3) heap-clean under MALLOC_CHECK_=3.
set -uo pipefail
KARDC=""
for candidate in \
    "${TEST_SRCDIR:-}/_main/compiler/kardc" "${TEST_SRCDIR:-}/kardashev/compiler/kardc" \
    "${RUNFILES_DIR:-}/_main/compiler/kardc" "${RUNFILES_DIR:-}/kardashev/compiler/kardc" \
    "./compiler/kardc" "./build.local/kardc"; do
    if [[ -n "$candidate" && -x "$candidate" ]]; then KARDC="$candidate"; break; fi
done
[[ -z "$KARDC" ]] && { echo "FAIL: kardc binary not found"; exit 1; }
echo "Using kardc at: $KARDC"
TMP=$(mktemp -d); trap 'rm -rf "$TMP"' EXIT

# 1. Multi-handle correctness: outstanding handles keep their results until joined.
cat > "$TMP/mh.kd" <<'EOF'
async fn add(a: i64, b: i64) -> i64 { a + b }
fn main() -> i64 ! { async, io } {
    let h1 = spawn(add(10, 20));
    let h2 = spawn(add(3, 4));
    let h3 = spawn(add(100, 1));
    print(join(h1)); print(join(h2)); print(join(h3)); 0
}
EOF
got=$("$KARDC" "$TMP/mh.kd" 2>/dev/null)
[[ "$got" == $'30\n7\n101\n0' ]] || { echo "FAIL [multi-handle]: got '$got' (want 30,7,101,0 — a join freed a sibling's result early)"; exit 1; }
echo "PASS [multi-handle]: spawn h1/h2/h3 then join each -> 30,7,101 (no sibling freed early)"

# 2. RSS-flat: a spawn+join loop must not leak a frame per task.
cat > "$TMP/loop.kd" <<'EOF'
async fn add(a: i64, b: i64) -> i64 { a + b }
fn main() -> i64 ! { async, alloc } {
    let mut s = 0; let mut i = 0;
    while i < 500000 { let h = spawn(add(i, 1)); s = s + join(h); i = i + 1; }
    s
}
EOF
"$KARDC" --no-cache -o "$TMP/loop" "$TMP/loop.kd" >/dev/null 2>&1 || { echo "FAIL [spawnleak]: build failed"; exit 1; }
# heap-clean under MALLOC_CHECK_=3 (a double-free of a released frame would abort).
bad=0
for r in 1 2 3; do
    set +e; MALLOC_CHECK_=3 "$TMP/loop" >/dev/null 2>"$TMP/e"; rc=$?; set -e
    if [[ "$rc" -eq 134 ]] || grep -qi 'free\|corrupt' "$TMP/e"; then bad=$((bad+1)); fi
done
[[ "$bad" -eq 0 ]] || { echo "FAIL [spawnleak]: $bad/3 runs corrupted the heap"; exit 1; }
# RSS gate: 500k spawn+join must stay flat (the leak ballooned it to tens of MB).
rss=""
if command -v /usr/bin/time >/dev/null 2>&1; then
    set +e; /usr/bin/time -v "$TMP/loop" >/dev/null 2>"$TMP/t"; set -e
    rss=$(grep -oE 'Maximum resident set size \(kbytes\): [0-9]+' "$TMP/t" 2>/dev/null | grep -oE '[0-9]+$' || true)
fi
if [[ -n "$rss" ]]; then
    [[ "$rss" -lt 32768 ]] || { echo "FAIL [spawnleak]: RSS $rss KB over 500k spawn+join — frames leak"; exit 1; }
    echo "PASS [spawnleak]: 500k spawn+join RSS-flat (${rss} KB), heap-clean (MALLOC_CHECK_=3)"
else
    echo "PASS [spawnleak]: 500k spawn+join heap-clean (MALLOC_CHECK_=3; RSS gate skipped — no GNU time)"
fi

echo "ALL SPAWN-LEAK SMOKE TESTS PASSED"
