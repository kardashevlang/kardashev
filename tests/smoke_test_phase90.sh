#!/usr/bin/env bash
# Phase 90 (Roadmap v15 — "self-hosting"): a parser for actual KARDASHEV syntax,
# written in kardashev (examples/selfhost/parser.kd). It lexes a function
# SIGNATURE into the Phase-89 token stream and parses it into a structured AST —
# FnSig { name, params: Vec<Param>, ret } — recovering the names/types by
# str_substring over each token's span. Over `fn add(a: i64, b: i64) -> i64` it
# yields: name == "add" (1), 2 params, first param is `a: i64` (1), return type
# i64 (1), witness 1211. (Arithmetic-expression parsing is already shown by
# examples/calc; this parses the language's own grammar.) Built JIT + AOT.
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
    "${TEST_SRCDIR:-}/_main/examples/selfhost/parser.kd" \
    "${TEST_SRCDIR:-}/kardashev/examples/selfhost/parser.kd" \
    "${RUNFILES_DIR:-}/_main/examples/selfhost/parser.kd" \
    "${RUNFILES_DIR:-}/kardashev/examples/selfhost/parser.kd" \
    "examples/selfhost/parser.kd"; do
    if [[ -f "$cand" ]]; then SRC="$cand"; break; fi
done
[[ -z "$SRC" ]] && { echo "FAIL: examples/selfhost/parser.kd not found"; exit 1; }

TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

want=$'1\n2\n1\n1\n1211'
for run in 1 2 3; do
    got=$("$KARDC" "$SRC" 2>/dev/null)
    [[ "$got" == "$want" ]] || { echo "FAIL [parser/jit run $run]:"; diff <(echo "$want") <(echo "$got"); exit 1; }
done
echo "PASS [parser/jit]: fn add(a: i64, b: i64) -> i64 -> name \"add\", 2 params, p0 a:i64, ret i64; witness 1211"

"$KARDC" --no-cache -o "$TMP/parser" "$SRC" >/dev/null 2>&1
for run in 1 2 3; do
    set +e; aout=$("$TMP/parser"); rc=$?; set -e
    [[ "$rc" -eq 187 && "$aout" == $'1\n2\n1\n1' ]] || { echo "FAIL [parser/aot run $run]: exit $rc out '$aout'"; exit 1; }
done
echo "PASS [parser/aot]: same parsed signature, exit 187 (= 1211 & 255), deterministic"

echo "ALL PHASE 90 SMOKE TESTS PASSED"
