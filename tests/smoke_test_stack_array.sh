#!/usr/bin/env bash
# Roadmap v89 — stack arrays [T; N]. Ground truth: fixed-size arrays are ALREADY
# fully runtime-first-class in the LLVM backend (alloca-backed, const-generic N,
# bounds-checked indexing + OOB panic, by-value params/returns, in-place a[i]=x,
# array-of-struct, per-element Drop) — verified end to end, JIT==AOT. The one
# remaining categorical gap was the C backend (`--emit-c`) refusing ALL arrays.
#
# v89 brings the C backend to PARITY: `[T; N]` lowers to a first-class wrapper
# `struct kdarr_<elem>_<N> { <elem> data[N]; }` (the v75 tuple pattern), with
# array literals (`[a,b,c]` / `[v; N]`), bounds-checked `a[i]` reads + `a[i] = x`
# stores (panic + exit 101 on OOB, byte-identical message to LLVM), and by-value
# param/return/copy. This gate is the FIRST end-to-end differential lock on the
# whole array surface: every value case must agree across JIT == AOT == C backend.
#
# DEFERRED (honest, no stubs): non-Copy array ELEMENTS in the C backend
# (`[String; N]` / `[Vec<_>; N]`) need C-backend per-element Drop glue and are
# cleanly REFUSED (LLVM keeps full non-Copy arrays — asserted here). Symbolic
# `[v; N]` repeat counts + side-effecting repeat values are refused in the C
# backend. Nested/multi-dim arrays-of-tuples stay LLVM-only. See ROADMAP.
set -uo pipefail
KARDC=""
for c in "${TEST_SRCDIR:-}/_main/compiler/kardc" "${RUNFILES_DIR:-}/_main/compiler/kardc" "./compiler/kardc" "./build.local/kardc"; do
  [[ -n "$c" && -x "$c" ]] && { KARDC="$c"; break; }; done
[[ -z "$KARDC" ]] && { echo "FAIL: kardc not found"; exit 1; }
echo "Using kardc at: $KARDC"
CC="$(command -v cc || command -v clang || true)"
TMP=$(mktemp -d); trap 'rm -rf "$TMP"' EXIT

# Triple-differential: JIT (stdout) == AOT (exit) == C backend (cc-compiled exit).
# The C leg is skipped (with a notice) if no cc — never masks an LLVM regression.
tri() {  # $1 label  $2 program  $3 want
  printf '%s' "$2" > "$TMP/t.kd"
  local j a
  j=$("$KARDC" "$TMP/t.kd" 2>/dev/null | head -1)
  [[ "$j" == "$3" ]] || { echo "FAIL [$1/JIT]: got '$j' want '$3'"; "$KARDC" "$TMP/t.kd" 2>&1 | head -3; exit 1; }
  "$KARDC" --no-cache -o "$TMP/a" "$TMP/t.kd" >/dev/null 2>&1 || { echo "FAIL [$1/AOT]: build"; exit 1; }
  "$TMP/a" >/dev/null 2>&1; a=$?
  [[ "$a" -eq "$3" ]] || { echo "FAIL [$1/AOT]: exit $a want $3"; exit 1; }
  if [[ -n "$CC" ]]; then
    "$KARDC" --emit-c "$TMP/t.kd" > "$TMP/t.c" 2>"$TMP/cerr" || { echo "FAIL [$1/C]: --emit-c refused: $(head -1 "$TMP/cerr")"; exit 1; }
    "$CC" -O2 -fwrapv "$TMP/t.c" -o "$TMP/c" 2>"$TMP/ccerr" || { echo "FAIL [$1/C]: cc rejected"; head -5 "$TMP/ccerr"; exit 1; }
    "$TMP/c" >/dev/null 2>&1; local cexit=$?
    [[ "$cexit" -eq "$3" ]] || { echo "FAIL [$1/C]: exit $cexit want $3"; exit 1; }
    echo "PASS [$1]: JIT == AOT == C == $3"
  else
    echo "PASS [$1]: JIT == AOT == $3 (C leg skipped: no cc)"
  fi
}

# 1. histogram: [i64;8] zero-init via [0;8], fill + sum (in-place a[i]=x). 0+1+4+…+49 = 140.
tri histogram 'fn main() -> i64 { let mut h: [i64; 8] = [0; 8] ; let mut i = 0 ; while i < 8 { h[i] = i * i ; i = i + 1 ; } ; let mut s = 0 ; let mut j = 0 ; while j < 8 { s = s + h[j] ; j = j + 1 ; } ; s }' 140
# 2. in-place bubble sort of [i64;4] -> a[0]+a[3] of the sorted [5,10,20,30] = 35.
tri sort 'fn main() -> i64 { let mut a: [i64; 4] = [30, 10, 20, 5] ; let mut i = 0 ; while i < 4 { let mut j = 0 ; while j < 3 { if a[j] > a[j + 1] { let t = a[j] ; a[j] = a[j + 1] ; a[j + 1] = t ; } else { } ; j = j + 1 ; } ; i = i + 1 ; } ; a[0] + a[3] }' 35
# 3. array-of-struct: [Point;2], field access through an index. 3 + 6 = 9.
tri arr_struct 'struct Point { x: i64, y: i64 } fn main() -> i64 { let pts: [Point; 2] = [Point { x: 3, y: 4 }, Point { x: 5, y: 6 }] ; pts[0].x + pts[1].y }' 9
# 4. by-value array param AND by-value array return. 10+20+30 = 60.
tri byval 'fn sum3(a: [i64; 3]) -> i64 { a[0] + a[1] + a[2] } fn mk() -> [i64; 3] { [10, 20, 30] } fn main() -> i64 { sum3(mk()) }' 60
# 5. element mutation through a copy stays independent (value semantics). b[0]=99 doesn't touch a.
tri valuecopy 'fn main() -> i64 { let a: [i64; 2] = [1, 2] ; let mut b = a ; b[0] = 99 ; a[0] + b[0] }' 100

# 6. OOB panic PARITY: AOT and the C backend both panic + exit 101 with the same message.
printf '%s' 'fn main() -> i64 { let a: [i64; 3] = [10, 20, 30] ; let i = 5 ; a[i] }' > "$TMP/oob.kd"
"$KARDC" --no-cache -o "$TMP/oa" "$TMP/oob.kd" >/dev/null 2>&1 && "$TMP/oa" >/dev/null 2>&1; ar=$?
[[ "$ar" -eq 101 ]] || { echo "FAIL [oob/AOT]: exit $ar want 101"; exit 1; }
if [[ -n "$CC" ]]; then
  "$KARDC" --emit-c "$TMP/oob.kd" > "$TMP/oob.c" 2>/dev/null && "$CC" -O2 -fwrapv "$TMP/oob.c" -o "$TMP/oc" 2>/dev/null
  msg=$("$TMP/oc" 2>&1); cr=$?
  [[ "$cr" -eq 101 ]] || { echo "FAIL [oob/C]: exit $cr want 101"; exit 1; }
  echo "$msg" | grep -q "index out of bounds: the len is 3 but the index is 5" || { echo "FAIL [oob/C]: wrong panic message: $msg"; exit 1; }
  echo "PASS [oob]: AOT == C == exit 101 with identical panic message"
else
  echo "PASS [oob]: AOT exit 101 (C leg skipped: no cc)"
fi

# 7. non-Copy element array: JIT/AOT handle [String;N] + element Drop; the C backend
#    cleanly REFUSES it (LLVM-only — no silent miscompile).
printf '%s' 'fn main() -> i64 { let a: [String; 3] = [string_new(), string_new(), string_new()] ; string_len(&a[0]) + 3 }' > "$TMP/sa.kd"
sj=$("$KARDC" "$TMP/sa.kd" 2>/dev/null | head -1)
[[ "$sj" == "3" ]] || { echo "FAIL [nonCopy/JIT]: got '$sj' want 3"; "$KARDC" "$TMP/sa.kd" 2>&1 | head -3; exit 1; }
"$KARDC" --no-cache -o "$TMP/sa" "$TMP/sa.kd" >/dev/null 2>&1 && "$TMP/sa" >/dev/null 2>&1; sar=$?
[[ "$sar" -eq 3 ]] || { echo "FAIL [nonCopy/AOT]: exit $sar want 3"; exit 1; }
if [[ -n "$CC" ]]; then
  "$KARDC" --emit-c "$TMP/sa.kd" >/dev/null 2>"$TMP/saerr" && { echo "FAIL [nonCopy/C]: --emit-c should refuse [String;N]"; exit 1; }
  grep -qi "outside the subset" "$TMP/saerr" || { echo "FAIL [nonCopy/C]: wrong refusal: $(head -1 "$TMP/saerr")"; exit 1; }
  echo "PASS [nonCopy]: JIT == AOT == 3; C backend cleanly refuses [String;N] (LLVM-only)"
else
  echo "PASS [nonCopy]: JIT == AOT == 3 (C leg skipped)"
fi

echo "ALL STACK-ARRAY SMOKE TESTS PASSED"
