#!/usr/bin/env bash
# Roadmap v110 (ARC D, part B, FINAL) — the kard-lsp `textDocument/codeAction` quick-fix.
# The LSP server now advertises `codeActionProvider` and, for each v110 trait-bound
# diagnostic (an unsatisfied `Type: Trait` whose message carries the `add `impl …``
# hint), offers a quick-fix CodeAction whose WorkspaceEdit inserts a skeleton `impl`
# block at the end of the file.
#
# Feeds JSON-RPC over stdio (initialize, didOpen of a file with a missing impl,
# codeAction) and asserts: the capability is advertised; the response is a CodeAction
# array with the right title + a quickfix kind + a WorkspaceEdit inserting the impl stub.
#
# DEFERRAL (honest): the inserted `impl` body is an empty block — the user fills in the
# method signatures/bodies (auto-generating them is future work).
set -uo pipefail
LSP=""
for candidate in \
    "${TEST_SRCDIR:-}/_main/compiler/kard-lsp" "${TEST_SRCDIR:-}/kardashev/compiler/kard-lsp" \
    "${RUNFILES_DIR:-}/_main/compiler/kard-lsp" "${RUNFILES_DIR:-}/kardashev/compiler/kard-lsp" \
    "./compiler/kard-lsp" "./build.local/kard-lsp"; do
    if [[ -n "$candidate" && -x "$candidate" ]]; then LSP="$candidate"; break; fi
done
[[ -z "$LSP" ]] && { echo "FAIL: kard-lsp binary not found"; exit 1; }
echo "Using kard-lsp at: $LSP"

encode() { local body="$1"; local len=${#body}; printf 'Content-Length: %d\r\n\r\n%s' "$len" "$body"; }

INIT='{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"capabilities":{}}}'
INITED='{"jsonrpc":"2.0","method":"initialized","params":{}}'
# A file with a missing trait impl -> the v110 bound diagnostic fires.
SRC='trait Show { fn show(&self) -> i64; } struct Widget { v: i64 } fn main() -> i64 { let w = Widget { v: 5 }; w.show() }'
DIDOPEN=$(printf '{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":"file:///tmp/t.kd","languageId":"kardashev","version":1,"text":"%s"}}}' "$SRC")
CODEACT='{"jsonrpc":"2.0","id":3,"method":"textDocument/codeAction","params":{"textDocument":{"uri":"file:///tmp/t.kd"},"range":{"start":{"line":0,"character":90},"end":{"line":0,"character":96}},"context":{"diagnostics":[]}}}'
SHUT='{"jsonrpc":"2.0","id":2,"method":"shutdown","params":null}'
EXIT='{"jsonrpc":"2.0","method":"exit","params":null}'

INPUT="$(encode "$INIT")$(encode "$INITED")$(encode "$DIDOPEN")$(encode "$CODEACT")$(encode "$SHUT")$(encode "$EXIT")"
OUT=$(printf '%s' "$INPUT" | "$LSP")

grep -q '"codeActionProvider":true' <<<"$OUT" || { echo "FAIL: codeActionProvider not advertised"; echo "$OUT"; exit 1; }
echo "PASS [capability]: initialize advertises codeActionProvider"

# The id:3 response must be a CodeAction array with the impl quick-fix.
RESP=$(grep -o '{"jsonrpc":"2.0","id":3,"result":\[.*\]}' <<<"$OUT")
[[ -n "$RESP" ]] || { echo "FAIL: no id:3 codeAction response"; echo "$OUT"; exit 1; }
grep -q '"title":"Add `impl Show for Widget`"' <<<"$RESP" || { echo "FAIL: wrong/absent code-action title"; echo "$RESP"; exit 1; }
grep -q '"kind":"quickfix"' <<<"$RESP" || { echo "FAIL: not a quickfix kind"; echo "$RESP"; exit 1; }
grep -q '"newText":"\\n\\nimpl Show for Widget {\\n}\\n"' <<<"$RESP" || { echo "FAIL: WorkspaceEdit does not insert the impl stub"; echo "$RESP"; exit 1; }
grep -q '"changes":{"file:///tmp/t.kd"' <<<"$RESP" || { echo "FAIL: edit not keyed to the document uri"; echo "$RESP"; exit 1; }
echo "PASS [codeAction]: quick-fix offers 'Add \`impl Show for Widget\`' with a WorkspaceEdit inserting the stub"

# Negative: a clean file yields NO code actions (empty array).
SRC2='fn main() -> i64 { 0 }'
DIDOPEN2=$(printf '{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":"file:///tmp/u.kd","languageId":"kardashev","version":1,"text":"%s"}}}' "$SRC2")
CODEACT2='{"jsonrpc":"2.0","id":4,"method":"textDocument/codeAction","params":{"textDocument":{"uri":"file:///tmp/u.kd"},"range":{"start":{"line":0,"character":0},"end":{"line":0,"character":1}},"context":{"diagnostics":[]}}}'
INPUT2="$(encode "$INIT")$(encode "$INITED")$(encode "$DIDOPEN2")$(encode "$CODEACT2")$(encode "$SHUT")$(encode "$EXIT")"
OUT2=$(printf '%s' "$INPUT2" | "$LSP")
grep -q '{"jsonrpc":"2.0","id":4,"result":\[\]}' <<<"$OUT2" || { echo "FAIL: a clean file should yield [] code actions"; echo "$OUT2"; exit 1; }
echo "PASS [no-fix]: a clean file yields an empty code-action array"

echo "ALL v110 LSP codeAction SMOKE TESTS PASSED"
