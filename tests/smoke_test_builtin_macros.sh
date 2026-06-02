#!/usr/bin/env bash
# v43 — built-in helper macros: stringify! / concat! / count! / cfg!.
set -euo pipefail
KARDC=""
for c in "${TEST_SRCDIR:-}/_main/compiler/kardc" "${RUNFILES_DIR:-}/_main/compiler/kardc" "./compiler/kardc" "./build.local/kardc"; do
  [[ -n "$c" && -x "$c" ]] && { KARDC="$c"; break; }; done
[[ -z "$KARDC" ]] && { echo "FAIL: kardc not found"; exit 1; }
echo "Using kardc at: $KARDC"
TMP=$(mktemp -d); trap 'rm -rf "$TMP"' EXIT
P='fn main() -> i64 ! { io } {
    print(count!(10, 20, 30, 40));
    print(count!());
    print(str_len(&stringify!(a + b)));
    print(str_len(&concat!("ab", 12, "z")));
    if cfg!(fast) { print(1); } else { print(0); }
    0
}'
printf '%s' "$P" > "$TMP/m.kd"
run() { "$KARDC" "$@" "$TMP/m.kd" 2>/dev/null | head -5 | tr '\n' ' '; }
# no flags: count=4,0 ; stringify "a + b"=5 ; concat "ab12z"=5 ; cfg!(fast)=0
got=$(run); exp="4 0 5 5 0 "; [[ "$got" == "$exp" ]] || { echo "FAIL[jit no-cfg]: exp '$exp' got '$got'"; exit 1; }; echo "PASS: builtin macros (no cfg)"
# --cfg fast: cfg!(fast) -> 1
got=$(run --cfg fast); exp="4 0 5 5 1 "; [[ "$got" == "$exp" ]] || { echo "FAIL[--cfg fast]: exp '$exp' got '$got'"; exit 1; }; echo "PASS: cfg! true under --cfg fast"
# AOT parity (no cfg)
"$KARDC" --no-cache -o "$TMP/m" "$TMP/m.kd" >/dev/null 2>&1; aot=$("$TMP/m" 2>/dev/null | head -5 | tr '\n' ' ')
[[ "$aot" == "4 0 5 5 0 " ]] || { echo "FAIL[aot]: got '$aot'"; exit 1; }; echo "PASS: AOT parity"
echo "ALL BUILTIN-MACRO SMOKE TESTS PASSED"
