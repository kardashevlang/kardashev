#!/usr/bin/env bash
# v14 Phase 87 (hardening): a JIT-vs-AOT DIFFERENTIAL sweep over the real
# capstone programs. Each example is run through BOTH backends and the results
# are asserted CONSISTENT — a single place that any future codegen change must
# keep green, catching a divergence between the ORC-JIT and the clang-linked AOT
# path on real, diverse programs (parsing, the numeric tower, generics, traits,
# collections, recursion, threads/channels, const-generics, Drop).
#
# The two backends differ ONLY in how `main`'s `i64` return is surfaced: the JIT
# PRINTS it as a trailing line; the AOT process EXITS with it (& 255). So the
# invariant for a program that prints P lines and returns R is:
#   - JIT stdout = the P printed lines + R as one extra line   (P+1 lines)
#   - AOT stdout = the P printed lines                         (P lines)
#   - AOT exit code = R & 255
# i.e. AOT stdout is exactly the JIT stdout minus its last line, and that last
# line mod 256 equals the AOT exit code. We assert all three.
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

# Locate the examples/ tree (Bazel runfiles or the source checkout).
find_src() {
    local rel="$1" c
    for c in \
        "${TEST_SRCDIR:-}/_main/$rel" "${TEST_SRCDIR:-}/kardashev/$rel" \
        "${RUNFILES_DIR:-}/_main/$rel" "${RUNFILES_DIR:-}/kardashev/$rel" \
        "$rel"; do
        if [[ -f "$c" ]]; then echo "$c"; return 0; fi
    done
    return 1
}

# Single-file, self-contained capstones (hello is multi-module, so excluded).
EXAMPLES="calc checksum csvstats json kdlex matrix parstats rpn wordfreq"
n_ok=0
for ex in $EXAMPLES; do
    SRC=$(find_src "examples/$ex/main.kd") || { echo "FAIL [$ex]: main.kd not found"; exit 1; }

    # JIT: stdout = printed lines + the returned i64 as a trailing line.
    jit=$("$KARDC" "$SRC" 2>/dev/null)
    # AOT: build, run; stdout = printed lines, exit code = return & 255.
    "$KARDC" --no-cache -o "$TMP/$ex" "$SRC" >/dev/null 2>&1 || { echo "FAIL [$ex]: AOT compile failed"; exit 1; }
    set +e; aot=$("$TMP/$ex" 2>/dev/null); aot_rc=$?; set -e

    nj=$(printf '%s\n' "$jit" | wc -l)
    na=$(printf '%s\n' "$aot" | wc -l)
    # Empty AOT output is reported as 1 line by wc on the trailing newline; treat
    # a truly-empty AOT (matrix prints nothing) as 0 printed lines.
    [[ -z "$aot" ]] && na=0
    [[ -z "$jit" ]] && nj=0

    # JIT must have exactly one more line than AOT (the printed return value).
    if [[ "$nj" -ne $((na + 1)) ]]; then
        echo "FAIL [$ex]: JIT printed $nj lines, AOT $na (expected JIT = AOT+1 for the returned value)"
        echo "--- JIT ---"; printf '%s\n' "$jit"
        echo "--- AOT ---"; printf '%s\n' "$aot"
        exit 1
    fi

    # The first AOT-many lines of the JIT output must equal the AOT output.
    jit_printed=$(printf '%s\n' "$jit" | head -n "$na")
    if [[ "$na" -gt 0 && "$jit_printed" != "$aot" ]]; then
        echo "FAIL [$ex]: JIT/AOT printed output diverges"
        diff <(printf '%s\n' "$jit_printed") <(printf '%s\n' "$aot") || true
        exit 1
    fi

    # The JIT's trailing line (main's return) mod 256 must equal the AOT exit.
    ret=$(printf '%s\n' "$jit" | tail -n 1)
    if [[ ! "$ret" =~ ^-?[0-9]+$ ]]; then
        echo "FAIL [$ex]: JIT return line '$ret' is not an integer"; exit 1
    fi
    want=$(( (ret % 256 + 256) % 256 ))
    if [[ "$aot_rc" -ne "$want" ]]; then
        echo "FAIL [$ex]: JIT return $ret -> & 255 = $want, but AOT exited $aot_rc"
        exit 1
    fi

    echo "PASS [$ex]: JIT/AOT consistent — $na printed lines match, return $ret (AOT exit $aot_rc)"
    n_ok=$((n_ok + 1))
done

echo "ALL DIFFERENTIAL TESTS PASSED ($n_ok/9 capstones: JIT and AOT agree)"
