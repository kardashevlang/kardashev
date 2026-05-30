#!/usr/bin/env bash
# Phase 80 CAPSTONE (Roadmap v13 — concurrency): a PARALLEL MAP-REDUCE in
# kardashev (examples/parstats), safe by construction. The series
# data(i) = (i*7+13) mod 1000 over 0..10000 is split across 4 worker THREADS;
# each reduces its chunk to a `Stats` struct and SENDS it on a shared channel;
# the main thread gathers the 4 partials over the MPSC channel and MERGES them.
#   1. Builds the real examples/parstats/main.kd, JIT + AOT, DETERMINISTIC: the
#      merged stats are sum=4995000, count=10000, min=0, max=999, and the
#      witness 6004000 matches the sequential answer (exit = 6004000 & 255 = 32).
#   2. Exercises the whole v13 line: thread_spawn (`share`), channels MOVING a
#      `Stats` struct across threads, the Send rule (Stats is Send), fork-join,
#      and the v12 i64_min/i64_max helpers.
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

SRC=""
for cand in \
    "${TEST_SRCDIR:-}/_main/examples/parstats/main.kd" \
    "${TEST_SRCDIR:-}/kardashev/examples/parstats/main.kd" \
    "${RUNFILES_DIR:-}/_main/examples/parstats/main.kd" \
    "${RUNFILES_DIR:-}/kardashev/examples/parstats/main.kd" \
    "examples/parstats/main.kd"; do
    if [[ -f "$cand" ]]; then SRC="$cand"; break; fi
done
[[ -z "$SRC" ]] && { echo "FAIL: examples/parstats/main.kd not found"; exit 1; }

TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

want=$'4995000\n10000\n0\n999\n6004000'
# JIT: the four printed stats + the witness, deterministic over several runs.
for run in 1 2 3 4 5; do
    got=$("$KARDC" "$SRC" 2>/dev/null)
    [[ "$got" == "$want" ]] || { echo "FAIL [parstats/jit run $run]:"; diff <(echo "$want") <(echo "$got"); exit 1; }
done
echo "PASS [parstats/jit]: parallel map-reduce -> sum 4995000, count 10000, min 0, max 999; witness 6004000 (5 runs deterministic)"

# AOT: same four stats, exit = 6004000 & 255 = 32.
"$KARDC" --no-cache -o "$TMP/parstats" "$SRC" >/dev/null 2>&1
for run in 1 2 3; do
    set +e; aout=$("$TMP/parstats"); rc=$?; set -e
    [[ "$rc" -eq 32 && "$aout" == $'4995000\n10000\n0\n999' ]] || { echo "FAIL [parstats/aot run $run]: exit $rc out '$aout'"; exit 1; }
done
echo "PASS [parstats/aot]: same merged stats, exit 32 (= witness & 255), deterministic (3 runs)"

echo "ALL PHASE 80 SMOKE TESTS PASSED"
