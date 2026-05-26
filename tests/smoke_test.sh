#!/usr/bin/env bash
# Phase 0 smoke test: invoke //compiler:kardc and assert that stdout is "42".
set -euo pipefail

KARDC=""
for candidate in \
    "${TEST_SRCDIR:-}/_main/compiler/kardc" \
    "${TEST_SRCDIR:-}/kardashev/compiler/kardc" \
    "${RUNFILES_DIR:-}/_main/compiler/kardc" \
    "${RUNFILES_DIR:-}/kardashev/compiler/kardc" \
    "./compiler/kardc"; do
    if [[ -n "$candidate" && -x "$candidate" ]]; then
        KARDC="$candidate"
        break
    fi
done

if [[ -z "$KARDC" ]]; then
    echo "FAIL: kardc binary not found in runfiles"
    echo "  TEST_SRCDIR=${TEST_SRCDIR:-(unset)}"
    echo "  RUNFILES_DIR=${RUNFILES_DIR:-(unset)}"
    echo "  PWD=$(pwd)"
    echo "  Candidates discovered:"
    find . -name kardc -type f 2>/dev/null | head -5
    exit 1
fi

echo "Using kardc at: $KARDC"
OUTPUT=$("$KARDC")
echo "kardc stdout: $OUTPUT"
if [[ "$OUTPUT" == "42" ]]; then
    echo "PASS"
    exit 0
fi
echo "FAIL: expected '42', got '$OUTPUT'"
exit 1
