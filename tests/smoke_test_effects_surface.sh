#!/usr/bin/env bash
# v83 — collapse the effect surface. The niche `div` (may-not-terminate) label is
# gated behind `--effects=extended`; the recognized default surface is io / alloc
# / panic / async / unwind / share. `kardc --explain effects` prints a
# consolidated guide. (share stays a recognized core-adjacent concurrency label.)
set -uo pipefail
KARDC=""
for c in "${TEST_SRCDIR:-}/_main/compiler/kardc" "${RUNFILES_DIR:-}/_main/compiler/kardc" "./compiler/kardc" "./build.local/kardc"; do
  [[ -n "$c" && -x "$c" ]] && { KARDC="$c"; break; }; done
[[ -z "$KARDC" ]] && { echo "FAIL: kardc not found"; exit 1; }
echo "Using kardc at: $KARDC"
TMP=$(mktemp -d); trap 'rm -rf "$TMP"' EXIT

# `! { div }` is rejected by default...
printf '%s' 'fn f() -> i64 ! { div } { 0 }
fn main() -> i64 { 0 }' > "$TMP/div.kd"
e=$("$KARDC" "$TMP/div.kd" 2>&1 >/dev/null || true)
echo "$e" | grep -qi "div" || { echo "FAIL: `! { div }` should be rejected by default; got: $e"; exit 1; }
echo "PASS(reject): \`! { div }\` rejected by default"

# ...but compiles under --effects=extended.
"$KARDC" --effects=extended "$TMP/div.kd" >/dev/null 2>&1 || { echo "FAIL: \`! { div }\` should compile under --effects=extended"; exit 1; }
echo "PASS: \`! { div }\` compiles under --effects=extended"

# share stays a recognized label by default (concurrency code still type-checks).
printf '%s' 'fn w() -> i64 { 7 }
fn main() -> i64 ! { io, share } { let h = thread_spawn(w); print(thread_join(h)); 0 }' > "$TMP/share.kd"
OUT=$("$KARDC" "$TMP/share.kd" 2>/dev/null | head -1)
[[ "$OUT" == "7" ]] || { echo "FAIL: share concurrency program (default mode) got '$OUT', want 7"; "$KARDC" "$TMP/share.kd" 2>&1|head -3; exit 1; }
echo "PASS: explicit \`! { ..., share }\` still type-checks by default"

# a no-row concurrency fn works (opt-in; share auto-inferred + unchecked).
printf '%s' 'fn w() -> i64 { 5 }
fn main() -> i64 { let h = thread_spawn(w); print(thread_join(h)); 0 }' > "$TMP/norow.kd"
OUT=$("$KARDC" "$TMP/norow.kd" 2>/dev/null | head -1)
[[ "$OUT" == "5" ]] || { echo "FAIL: no-row concurrency program got '$OUT', want 5"; exit 1; }
echo "PASS: no-row concurrency program compiles (opt-in)"

# --explain effects prints the consolidated guide.
EX=$("$KARDC" --explain effects 2>&1)
echo "$EX" | grep -qi "OPT-IN" || { echo "FAIL: --explain effects missing opt-in summary"; exit 1; }
echo "$EX" | grep -qi "Result" || { echo "FAIL: --explain effects missing Result guidance"; exit 1; }
echo "$EX" | grep -qi "effects=strict" || { echo "FAIL: --explain effects missing modes"; exit 1; }
echo "PASS: --explain effects prints the consolidated guide"

echo "ALL EFFECTS-SURFACE SMOKE TESTS PASSED"
