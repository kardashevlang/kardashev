#!/usr/bin/env bash
# Phase 89 (Roadmap v15 — "self-hosting"): a TOKEN-STREAM lexer in kardashev
# (examples/selfhost/tokens.kd). Grows Phase 88's classifier into a real lexer
# that produces a `Vec<Token>` — each token carrying its KIND and its SPAN
# (start + len) into the source — the interface a parser sits on. Over
# `fn add(a: i64, b: i64) -> i64 { a + b + 42 }` it yields 20 tokens; the first
# reconstructs (via str_substring over the span) to "fn" and the arrow token to
# "->", proving the spans are correct; plus a deterministic stream fingerprint
# (4512001309659388475) and a final witness 2011 (= 20*100 + fn_ok*10 + arrow_ok).
# Built JIT + AOT, deterministic.
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
    "${TEST_SRCDIR:-}/_main/examples/selfhost/tokens.kd" \
    "${TEST_SRCDIR:-}/kardashev/examples/selfhost/tokens.kd" \
    "${RUNFILES_DIR:-}/_main/examples/selfhost/tokens.kd" \
    "${RUNFILES_DIR:-}/kardashev/examples/selfhost/tokens.kd" \
    "examples/selfhost/tokens.kd"; do
    if [[ -f "$cand" ]]; then SRC="$cand"; break; fi
done
[[ -z "$SRC" ]] && { echo "FAIL: examples/selfhost/tokens.kd not found"; exit 1; }

TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

want=$'20\n1\n1\n4512001309659388475\n2011'
for run in 1 2 3; do
    got=$("$KARDC" "$SRC" 2>/dev/null)
    [[ "$got" == "$want" ]] || { echo "FAIL [tokens/jit run $run]:"; diff <(echo "$want") <(echo "$got"); exit 1; }
done
echo "PASS [tokens/jit]: 20-token stream; first span -> \"fn\", arrow span -> \"->\"; fingerprint 4512001309659388475"

"$KARDC" --no-cache -o "$TMP/tokens" "$SRC" >/dev/null 2>&1
for run in 1 2 3; do
    set +e; aout=$("$TMP/tokens"); rc=$?; set -e
    [[ "$rc" -eq 219 && "$aout" == $'20\n1\n1\n4512001309659388475' ]] || { echo "FAIL [tokens/aot run $run]: exit $rc out '$aout'"; exit 1; }
done
echo "PASS [tokens/aot]: same stream, exit 219 (= 2011 & 255), deterministic"

echo "ALL PHASE 89 SMOKE TESTS PASSED"
