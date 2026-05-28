#!/usr/bin/env bash
# Phase 20b smoke test: kard-lsp answers the two editing requests
# (find-all-references + rename) on top of the Phase 14b occurrence index.
#
# We drive a small two-function program over stdio:
#   line 0: fn greet(n: i64) -> i64 ! { io } { print(n); n }
#   line 1: fn main() -> i64 ! { io } { let x = greet(7); greet(x) }
# The symbol `greet` appears 3 times: its declaration (line 0) + two call
# sites (line 1). The local `x` appears twice: its `let` binding + one use.
#
# Asserts:
#   0. initialize advertises referencesProvider + renameProvider.
#   (a) references on a `greet` call with includeDeclaration=true returns 3
#       Locations (decl + 2 uses); with includeDeclaration=false returns 2.
#   (b) rename of `greet` -> `salute` returns a WorkspaceEdit whose `changes`
#       cover all 3 occurrences, each with newText "salute".
#   (c) references on the local `x` returns 2 Locations (binding + use),
#       scoped to that local (does not bleed into `greet`).
#
# Each LSP message is Content-Length framed (CRLF + blank line + JSON body),
# mirroring smoke_test_lsp_rich.sh.
set -euo pipefail

LSP=""
for candidate in \
    "${TEST_SRCDIR:-}/_main/compiler/kard-lsp" \
    "${TEST_SRCDIR:-}/kardashev/compiler/kard-lsp" \
    "${RUNFILES_DIR:-}/_main/compiler/kard-lsp" \
    "${RUNFILES_DIR:-}/kardashev/compiler/kard-lsp" \
    "./compiler/kard-lsp"; do
    if [[ -n "$candidate" && -x "$candidate" ]]; then
        LSP="$candidate"
        break
    fi
done

if [[ -z "$LSP" ]]; then
    echo "FAIL: kard-lsp binary not found"
    exit 1
fi

echo "Using kard-lsp at: $LSP"

# Frame a single LSP message with its Content-Length header.
encode() {
    local body="$1"
    local len=${#body}
    printf 'Content-Length: %d\r\n\r\n%s' "$len" "$body"
}

# Count non-overlapping occurrences of a fixed substring in a string.
count_occurrences() {
    local hay="$1" needle="$2" n=0 rest="$1"
    while [[ "$rest" == *"$needle"* ]]; do
        rest="${rest#*"$needle"}"
        n=$((n + 1))
    done
    echo "$n"
}

# Two-line program. The newline is a literal "\n" escape inside the JSON
# string; the server decodes it to a real newline. Columns are 0-based.
#   line 1: `fn main() -> i64 ! { io } { let x = greet(7); greet(x) }`
#            col 36 = first `greet`; col 32 = the `let x` binding.
SRC='fn greet(n: i64) -> i64 ! { io } { print(n); n }\nfn main() -> i64 ! { io } { let x = greet(7); greet(x) }'
GREET_CALL_COL=36   # a `greet` call site on line 1
X_DECL_COL=32       # the `let x` binding on line 1

INIT='{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"capabilities":{}}}'
INITED='{"jsonrpc":"2.0","method":"initialized","params":{}}'
DIDOPEN=$(printf '{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":"file:///tmp/t.kd","languageId":"kardashev","version":1,"text":"%s"}}}' "$SRC")

REFS_INCL=$(printf '{"jsonrpc":"2.0","id":30,"method":"textDocument/references","params":{"textDocument":{"uri":"file:///tmp/t.kd"},"position":{"line":1,"character":%d},"context":{"includeDeclaration":true}}}' "$GREET_CALL_COL")
REFS_EXCL=$(printf '{"jsonrpc":"2.0","id":31,"method":"textDocument/references","params":{"textDocument":{"uri":"file:///tmp/t.kd"},"position":{"line":1,"character":%d},"context":{"includeDeclaration":false}}}' "$GREET_CALL_COL")
RENAME=$(printf '{"jsonrpc":"2.0","id":32,"method":"textDocument/rename","params":{"textDocument":{"uri":"file:///tmp/t.kd"},"position":{"line":1,"character":%d},"newName":"salute"}}' "$GREET_CALL_COL")
REFS_LOCAL=$(printf '{"jsonrpc":"2.0","id":33,"method":"textDocument/references","params":{"textDocument":{"uri":"file:///tmp/t.kd"},"position":{"line":1,"character":%d},"context":{"includeDeclaration":true}}}' "$X_DECL_COL")
SHUT='{"jsonrpc":"2.0","id":2,"method":"shutdown","params":null}'
EXIT='{"jsonrpc":"2.0","method":"exit","params":null}'

INPUT="$(encode "$INIT")$(encode "$INITED")$(encode "$DIDOPEN")$(encode "$REFS_INCL")$(encode "$REFS_EXCL")$(encode "$RENAME")$(encode "$REFS_LOCAL")$(encode "$SHUT")$(encode "$EXIT")"

OUT=$(printf '%s' "$INPUT" | "$LSP")
# Normalise CRLF so grep can scan line-by-line.
OUT_NL=$(printf '%s' "$OUT" | tr '\r' '\n')

# 0. Capabilities must advertise both editing providers.
for cap in referencesProvider renameProvider; do
    if ! grep -q "\"$cap\":true" <<< "$OUT_NL"; then
        echo "FAIL: initialize result missing capability $cap"
        echo "$OUT_NL"
        exit 1
    fi
done

# (a) references with includeDeclaration=true: 3 Locations (decl + 2 calls).
REFS_INCL_RESP=$(grep '"id":30' <<< "$OUT_NL" || true)
N_INCL=$(count_occurrences "$REFS_INCL_RESP" '"uri":"file:///tmp/t.kd"')
if [[ "$N_INCL" -lt 2 ]]; then
    echo "FAIL: references (includeDeclaration) returned <2 Locations ($N_INCL)"
    echo "$OUT_NL"
    exit 1
fi
if [[ "$N_INCL" -ne 3 ]]; then
    echo "FAIL: references (includeDeclaration) expected 3 Locations, got $N_INCL"
    echo "$OUT_NL"
    exit 1
fi
# The declaration is on line 0; it must be present when includeDeclaration=true.
if ! grep -q '"start":{"line":0' <<< "$REFS_INCL_RESP"; then
    echo "FAIL: references (includeDeclaration) did not include the decl on line 0"
    echo "$OUT_NL"
    exit 1
fi
echo "references(includeDeclaration=true): $N_INCL Locations (decl + 2 uses)"

# (a') references with includeDeclaration=false: 2 Locations (the 2 calls only).
REFS_EXCL_RESP=$(grep '"id":31' <<< "$OUT_NL" || true)
N_EXCL=$(count_occurrences "$REFS_EXCL_RESP" '"uri":"file:///tmp/t.kd"')
if [[ "$N_EXCL" -ne 2 ]]; then
    echo "FAIL: references (no declaration) expected 2 Locations, got $N_EXCL"
    echo "$OUT_NL"
    exit 1
fi
# With the declaration excluded, line 0 must NOT appear.
if grep -q '"start":{"line":0' <<< "$REFS_EXCL_RESP"; then
    echo "FAIL: references (includeDeclaration=false) still listed the decl line 0"
    echo "$OUT_NL"
    exit 1
fi
echo "references(includeDeclaration=false): $N_EXCL Locations (uses only)"

# (b) rename greet -> salute: WorkspaceEdit changing all 3 occurrences.
RENAME_RESP=$(grep '"id":32' <<< "$OUT_NL" || true)
if ! grep -q '"changes"' <<< "$RENAME_RESP"; then
    echo "FAIL: rename did not return a WorkspaceEdit with a 'changes' map"
    echo "$OUT_NL"
    exit 1
fi
if ! grep -q '"file:///tmp/t.kd"' <<< "$RENAME_RESP"; then
    echo "FAIL: rename WorkspaceEdit did not key the document uri"
    echo "$OUT_NL"
    exit 1
fi
N_EDITS=$(count_occurrences "$RENAME_RESP" '"newText":"salute"')
if [[ "$N_EDITS" -ne 3 ]]; then
    echo "FAIL: rename expected 3 edits (decl + 2 uses), got $N_EDITS"
    echo "$OUT_NL"
    exit 1
fi
echo "rename greet->salute: $N_EDITS edits, each newText=salute"

# (c) references on the local `x`: exactly 2 (binding + single use), scoped to
#     the local — must not pick up any `greet` occurrence.
REFS_LOCAL_RESP=$(grep '"id":33' <<< "$OUT_NL" || true)
N_LOCAL=$(count_occurrences "$REFS_LOCAL_RESP" '"uri":"file:///tmp/t.kd"')
if [[ "$N_LOCAL" -ne 2 ]]; then
    echo "FAIL: references on local x expected 2 Locations, got $N_LOCAL"
    echo "$OUT_NL"
    exit 1
fi
# The local `x` lives only on line 1; a stray line-0 hit would mean the symbol
# resolution bled across symbols.
if grep -q '"start":{"line":0' <<< "$REFS_LOCAL_RESP"; then
    echo "FAIL: references on local x leaked onto line 0 (wrong symbol)"
    echo "$OUT_NL"
    exit 1
fi
echo "references(local x): $N_LOCAL Locations (binding + use), scoped"

echo "PASS: kard-lsp serves find-all-references (incl/excl decl) and rename"
