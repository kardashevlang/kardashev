#!/usr/bin/env bash
# Phase 32 doc-lint: guard the "truth pass" against regression. It needs no
# kardc — it's a grep over the shipped docs + a few source files, asserting
# that (a) the stale "Phase 6 (stub)" async markers don't come back, (b) the
# v1-era stdlib claims that have since shipped (Vec is i64-only / String needs
# a byte type / Vec leaks / combinators are blocked) don't reappear in the
# docs, and (c) the docs actually mention the v5 stdlib (a positive check so an
# accidental revert to the old text is caught).
set -euo pipefail

# Locate a repo file in the source tree (Makefile: ./) or Bazel runfiles.
find_file() {
    for c in \
        "${TEST_SRCDIR:-}/_main/$1" "${TEST_SRCDIR:-}/kardashev/$1" \
        "${RUNFILES_DIR:-}/_main/$1" "${RUNFILES_DIR:-}/kardashev/$1" \
        "./$1"; do
        if [[ -n "$c" && -f "$c" ]]; then echo "$c"; return; fi
    done
    echo ""
}

fail=0
note() { echo "FAIL [doclint]: $1"; fail=1; }

# --- (a) no "(stub)" markers in these source files (async is real since P12/18). ---
for src in \
    compiler/include/kardashev/ast.hpp \
    compiler/src/parser.cpp \
    compiler/src/borrow_check.cpp; do
    f=$(find_file "$src")
    if [[ -z "$f" ]]; then echo "INFO [doclint]: $src not found (skipped)"; continue; fi
    if grep -nq '(stub)' "$f"; then
        note "'$src' still contains a '(stub)' marker:"; grep -n '(stub)' "$f"
    fi
done

# --- (b) no stale v1-era claims in the docs (these features all shipped). ---
STALE_STDLIB="element type is fixed at|needs a byte type|backing buffer leaks|blocked on the same first-class function|growable buffer of .i64. elements"
for doc in docs/stdlib.md docs/language-reference.md docs/effects.md docs/architecture.md; do
    f=$(find_file "$doc")
    if [[ -z "$f" ]]; then note "doc '$doc' not found"; continue; fi
    if grep -nEq "$STALE_STDLIB" "$f"; then
        note "doc '$doc' contains a stale v1-era claim:"; grep -nE "$STALE_STDLIB" "$f"
    fi
    if grep -nq 'Phase 6 (stub)' "$f"; then
        note "doc '$doc' contains a stale 'Phase 6 (stub)' marker"
    fi
done

# --- (c) the stdlib doc must mention the headline v5 stdlib (positive check). ---
STDLIB=$(find_file docs/stdlib.md)
if [[ -n "$STDLIB" ]]; then
    for needle in "HashMap" "str_substring" "fs_read_to_string"; do
        if ! grep -q "$needle" "$STDLIB"; then
            note "docs/stdlib.md no longer documents '$needle' (v5 stdlib)"
        fi
    done
fi

if [[ "$fail" -ne 0 ]]; then exit 1; fi
echo "PASS: doc-lint — no '(stub)' markers, no stale v1-era stdlib claims, v5 stdlib documented"
