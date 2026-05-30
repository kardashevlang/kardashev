#!/usr/bin/env bash
# Phase 92 (Roadmap v15 — "self-hosting"): a scope/semantic CHECKER written in
# kardashev (examples/selfhost/checker.kd). After parsing a function signature
# into the FnSig AST, it builds a HashMap<String,String> SYMBOL TABLE and runs
# real semantic checks: it RESOLVES a parameter's type by name and REJECTS a
# duplicate parameter name. On `fn add(a: i64, b: i64) -> i64` it reports no
# duplicate (0), `b` resolves to i64 (1); on `fn bad(a: i64, a: i64) -> i64` it
# flags the duplicate (1); witness 11. Built JIT + AOT.
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
    "${TEST_SRCDIR:-}/_main/examples/selfhost/checker.kd" "${TEST_SRCDIR:-}/kardashev/examples/selfhost/checker.kd" \
    "${RUNFILES_DIR:-}/_main/examples/selfhost/checker.kd" "${RUNFILES_DIR:-}/kardashev/examples/selfhost/checker.kd" \
    "examples/selfhost/checker.kd"; do
    if [[ -f "$cand" ]]; then SRC="$cand"; break; fi
done
[[ -z "$SRC" ]] && { echo "FAIL: examples/selfhost/checker.kd not found"; exit 1; }
TMP=$(mktemp -d); trap 'rm -rf "$TMP"' EXIT
want=$'0\n1\n1\n11'
for run in 1 2 3; do
    got=$("$KARDC" "$SRC" 2>/dev/null)
    [[ "$got" == "$want" ]] || { echo "FAIL [checker/jit run $run]:"; diff <(echo "$want") <(echo "$got"); exit 1; }
done
echo "PASS [checker/jit]: symbol table — b:i64 resolved, no dup in add, duplicate 'a' flagged in bad"
"$KARDC" --no-cache -o "$TMP/checker" "$SRC" >/dev/null 2>&1
for run in 1 2 3; do
    set +e; aout=$("$TMP/checker"); rc=$?; set -e
    [[ "$rc" -eq 11 && "$aout" == $'0\n1\n1' ]] || { echo "FAIL [checker/aot run $run]: exit $rc out '$aout'"; exit 1; }
done
echo "PASS [checker/aot]: same checks, exit 11, deterministic"
echo "ALL PHASE 92 SMOKE TESTS PASSED"
