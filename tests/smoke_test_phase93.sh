#!/usr/bin/env bash
# Phase 93 CAPSTONE (Roadmap v15 — "self-hosting"): "the front-end, end to end."
# One kardashev program (examples/selfhost/front.kd) runs the WHOLE compiler
# front it built across Phases 88-92 — lex -> parse -> check -> reprint — over a
# function signature, scoring it params*100 + round_trips*10 + (no-dup). Run on a
# 2-param and a 3-param signature to show it generalizes: r1 = 211, r2 = 311,
# and a single pipeline witness 211311. A self-hosted compiler FRONT-END written
# in the language it compiles. Built JIT + AOT, deterministic.
set -euo pipefail
KARDC=""
for candidate in \
    "${TEST_SRCDIR:-}/_main/compiler/kardc" "${TEST_SRCDIR:-}/kardashev/compiler/kardc" \
    "${RUNFILES_DIR:-}/_main/compiler/kardc" "${RUNFILES_DIR:-}/kardashev/compiler/kardc" \
    "./compiler/kardc" "./build.local/kardc"; do
    if [[ -n "$candidate" && -x "$candidate" ]]; then KARDC="$candidate"; break; fi
done
[[ -z "$KARDC" ]] && { echo "FAIL: kardc binary not found"; exit 1; }
echo "Using kardc at: $KARDC"
SRC=""
for cand in \
    "${TEST_SRCDIR:-}/_main/examples/selfhost/front.kd" "${TEST_SRCDIR:-}/kardashev/examples/selfhost/front.kd" \
    "${RUNFILES_DIR:-}/_main/examples/selfhost/front.kd" "${RUNFILES_DIR:-}/kardashev/examples/selfhost/front.kd" \
    "examples/selfhost/front.kd"; do
    if [[ -f "$cand" ]]; then SRC="$cand"; break; fi
done
[[ -z "$SRC" ]] && { echo "FAIL: examples/selfhost/front.kd not found"; exit 1; }
TMP=$(mktemp -d); trap 'rm -rf "$TMP"' EXIT
want=$'211\n311\n211311'
for run in 1 2 3; do
    got=$("$KARDC" "$SRC" 2>/dev/null)
    [[ "$got" == "$want" ]] || { echo "FAIL [front/jit run $run]:"; diff <(echo "$want") <(echo "$got"); exit 1; }
done
echo "PASS [front/jit]: full front-end (lex->parse->check->print) on 2- and 3-param sigs -> 211, 311; witness 211311"
"$KARDC" --no-cache -o "$TMP/front" "$SRC" >/dev/null 2>&1
for run in 1 2 3; do
    set +e; aout=$("$TMP/front"); rc=$?; set -e
    [[ "$rc" -eq 111 && "$aout" == $'211\n311' ]] || { echo "FAIL [front/aot run $run]: exit $rc out '$aout'"; exit 1; }
done
echo "PASS [front/aot]: same pipeline, exit 111 (= 211311 & 255), deterministic"
echo "ALL PHASE 93 SMOKE TESTS PASSED"
