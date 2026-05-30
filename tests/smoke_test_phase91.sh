#!/usr/bin/env bash
# Phase 91 (Roadmap v15 — "self-hosting"): an AST PRINTER written in kardashev
# (examples/selfhost/printer.kd). It lexes + parses a function signature into the
# Phase-90 FnSig AST, reprints the AST back to source, and checks it ROUND-TRIPS:
# source -> tokens -> AST -> source is byte-identical. A lossless round-trip
# proves the AST captures everything the surface syntax carries. Over the
# canonical `fn add(a: i64, b: i64) -> i64` (29 chars) it reports length 29 and
# round_trips = 1. Built JIT + AOT.
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
    "${TEST_SRCDIR:-}/_main/examples/selfhost/printer.kd" \
    "${TEST_SRCDIR:-}/kardashev/examples/selfhost/printer.kd" \
    "${RUNFILES_DIR:-}/_main/examples/selfhost/printer.kd" \
    "${RUNFILES_DIR:-}/kardashev/examples/selfhost/printer.kd" \
    "examples/selfhost/printer.kd"; do
    if [[ -f "$cand" ]]; then SRC="$cand"; break; fi
done
[[ -z "$SRC" ]] && { echo "FAIL: examples/selfhost/printer.kd not found"; exit 1; }

TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

want=$'29\n1\n1'
for run in 1 2 3; do
    got=$("$KARDC" "$SRC" 2>/dev/null)
    [[ "$got" == "$want" ]] || { echo "FAIL [printer/jit run $run]:"; diff <(echo "$want") <(echo "$got"); exit 1; }
done
echo "PASS [printer/jit]: fn add(a: i64, b: i64) -> i64 round-trips (source -> AST -> source, len 29, identical)"

"$KARDC" --no-cache -o "$TMP/printer" "$SRC" >/dev/null 2>&1
for run in 1 2 3; do
    set +e; aout=$("$TMP/printer"); rc=$?; set -e
    [[ "$rc" -eq 1 && "$aout" == $'29\n1' ]] || { echo "FAIL [printer/aot run $run]: exit $rc out '$aout'"; exit 1; }
done
echo "PASS [printer/aot]: same round-trip, exit 1, deterministic"

echo "ALL PHASE 91 SMOKE TESTS PASSED"
