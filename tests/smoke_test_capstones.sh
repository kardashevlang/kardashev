#!/usr/bin/env bash
# Phase 31 smoke test: the two v5 capstones — real tools written in kardashev,
# built by the real kardc, exercising the v5 stdlib end to end.
#
#   1. examples/json/main.kd — a JSON object parser. Scans bytes with
#      str_char_at, slices keys with str_substring, builds a
#      HashMap<String, i64> (the Phase 28 String-keyed map), and answers
#      lookups. For {"width":800,"height":600,"depth":-3,"scale":2} it prints
#      "members = 4" / "width*height = 480000" and returns 800+600-3+2 = 1399.
#
#   2. examples/kdlex/main.kd — a lexer for a kardashev subset. Tokenizes
#      source into a Vec<Tok> (keyword vs identifier via str_substring+str_eq),
#      counts `fn NAME` declarations, and checks brace balance. For a 2-fn
#      source it prints "tokens = 32" / "fn decls = 2" / "balanced = 1" and
#      returns 2*100 + 1*10 + 1 = 211.
#
# Each is JIT-run (asserting printed lines + the printed result) and AOT-built
# + run (asserting the exit code). Builds the SHIPPED example files, so this
# doubles as a build check of the examples.
set -euo pipefail

KARDC=""
for candidate in \
    "${TEST_SRCDIR:-}/_main/compiler/kardc" \
    "${TEST_SRCDIR:-}/kardashev/compiler/kardc" \
    "${RUNFILES_DIR:-}/_main/compiler/kardc" \
    "${RUNFILES_DIR:-}/kardashev/compiler/kardc" \
    "./compiler/kardc"; do
    if [[ -n "$candidate" && -x "$candidate" ]]; then KARDC="$candidate"; break; fi
done
[[ -z "$KARDC" ]] && { echo "FAIL: kardc binary not found"; exit 1; }
echo "Using kardc at: $KARDC"

# Locate the example sources (Bazel runfiles or the source tree).
find_src() {
    for c in \
        "${TEST_SRCDIR:-}/_main/$1" "${TEST_SRCDIR:-}/kardashev/$1" \
        "${RUNFILES_DIR:-}/_main/$1" "${RUNFILES_DIR:-}/kardashev/$1" \
        "./$1"; do
        if [[ -n "$c" && -f "$c" ]]; then echo "$c"; return; fi
    done
    echo ""
}
JSON_SRC=$(find_src "examples/json/main.kd")
KDLEX_SRC=$(find_src "examples/kdlex/main.kd")
[[ -z "$JSON_SRC" || -z "$KDLEX_SRC" ]] && { echo "FAIL: capstone sources not found"; exit 1; }

TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

# run <name> <src> <expected-jit-stdout> <expected-aot-exit>
run() {
    local name=$1 src=$2 want=$3 rc_want=$4
    local jit
    jit=$("$KARDC" "$src")
    if [[ "$jit" != "$want" ]]; then
        echo "FAIL [$name/jit]: output mismatch"
        echo "expected:"; printf '%s\n' "$want"
        echo "got:";      printf '%s\n' "$jit"
        exit 1
    fi
    "$KARDC" --no-cache -o "$TMP/$name" "$src" >/dev/null
    set +e; "$TMP/$name" >/dev/null; local rc=$?; set -e
    if [[ "$rc" -ne "$rc_want" ]]; then
        echo "FAIL [$name/aot]: exit $rc (expected $rc_want)"; exit 1
    fi
    echo "PASS [$name]: JIT output + AOT exit ($rc_want)"
}

run json "$JSON_SRC" \
    $'json: members = 4\njson: width*height = 480000\n1399' 119  # 1399 % 256

run kdlex "$KDLEX_SRC" \
    $'kdlex: tokens = 32\nkdlex: fn decls = 2\nkdlex: braces balanced = 1\n211' 211

echo "PASS: both v5 capstones (JSON parser + kardashev-subset lexer) build and run in JIT + AOT"
