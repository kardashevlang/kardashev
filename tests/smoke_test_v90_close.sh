#!/usr/bin/env bash
# Roadmap v90 (the CLOSING pass of the v81-v90 arc). Two real, CI-robust deliverables:
#
#  PART 1 — read-only slices in the C backend. The LLVM backend already had
#  read-only slices (&v[a..b] over a Vec<i64>/<bool>: slice_len/slice_get/
#  slice_get_ref, a `{ptr,len}` fat pointer); the C backend (--emit-c) refused
#  ALL slices. v90 lowers `&[T]` to `struct kdslice { int64_t* ptr; int64_t len; }`
#  (mirroring the LLVM `{i8*,i64}`), with bounds-checked-return `kd_slice_*`,
#  scalar-element only. Differentially gated JIT == AOT == C backend. (Slice
#  MUTATION — slice_set/slice_get_mut — exists in NO backend today, so this is
#  parity-preserving, not a regression; genuine mutation is the v91 line.)
#
#  PART 3 — vectorization regression lock. The v51 TargetMachine/TTI-in-PassBuilder
#  fix makes loops auto-vectorize; it runs in ALL emit paths (JIT/AOT/--emit-llvm)
#  because they share codegen()->finish(). This gate IR-greps for vector ops so a
#  future PassBuilder refactor can't silently drop TTI again (the v51 regression).
#
# DEFERRED to a documented v91 line (honest, no stubs): a user-replaceable
# `GlobalAlloc` allocator (L/XL: ~63 hardcoded malloc/realloc/free sites + free-glue
# routing + not CI-safely-observable without fragile LD_PRELOAD), and genuine slice
# MUTATION. The optimization "pass" is verify-and-lock: vectorization is already
# complete (not a fix). Skips the C leg with a notice if no cc.
set -uo pipefail
KARDC=""
for c in "${TEST_SRCDIR:-}/_main/compiler/kardc" "${RUNFILES_DIR:-}/_main/compiler/kardc" "./compiler/kardc" "./build.local/kardc"; do
  [[ -n "$c" && -x "$c" ]] && { KARDC="$c"; break; }; done
[[ -z "$KARDC" ]] && { echo "FAIL: kardc not found"; exit 1; }
echo "Using kardc at: $KARDC"
CC="$(command -v cc || command -v clang || true)"
TMP=$(mktemp -d); trap 'rm -rf "$TMP"' EXIT

# ---- PART 1: read-only C-backend slices, JIT == AOT == C ----
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
    "$TMP/c" >/dev/null 2>&1; local cx=$?
    [[ "$cx" -eq "$3" ]] || { echo "FAIL [$1/C]: exit $cx want $3"; exit 1; }
    echo "PASS [$1]: JIT == AOT == C == $3"
  else
    echo "PASS [$1]: JIT == AOT == $3 (C leg skipped: no cc)"
  fi
}
tri slice_read 'fn rd(s: &[i64]) -> i64 { slice_get(s, 0) + slice_len(s) } fn main() -> i64 ! { alloc } { let mut v = vec_new() ; vec_push(&mut v, 10) ; vec_push(&mut v, 20) ; rd(&v[0..2]) }' 12
tri slice_subrange 'fn sm(s: &[i64]) -> i64 { let mut i = 0 ; let mut acc = 0 ; while i < slice_len(s) { acc = acc + slice_get(s, i) ; i = i + 1 ; } ; acc } fn main() -> i64 ! { alloc } { let mut v = vec_new() ; vec_push(&mut v, 1) ; vec_push(&mut v, 2) ; vec_push(&mut v, 3) ; vec_push(&mut v, 4) ; sm(&v[1..3]) }' 5
tri slice_ref 'fn first(s: &[i64]) -> i64 { let r = slice_get_ref(s, 0) ; *r } fn main() -> i64 ! { alloc } { let mut v = vec_new() ; vec_push(&mut v, 42) ; first(&v[0..1]) }' 42

# PART 1 negative: a slice of a non-scalar element is cleanly refused by the C backend.
printf '%s' 'fn rd(s: &[String]) -> i64 { slice_len(s) } fn main() -> i64 ! { alloc } { let mut v = vec_new() ; vec_push(&mut v, string_new()) ; rd(&v[0..1]) }' > "$TMP/n.kd"
nj=$("$KARDC" "$TMP/n.kd" 2>/dev/null | head -1)  # JIT/LLVM still supports it
[[ "$nj" == "1" ]] || { echo "FAIL [slice-nonscalar/JIT]: got '$nj' want 1"; exit 1; }
if [[ -n "$CC" ]]; then
  "$KARDC" --emit-c "$TMP/n.kd" >/dev/null 2>"$TMP/nerr" && { echo "FAIL [slice-nonscalar/C]: should refuse &[String]"; exit 1; }
  grep -qi "non-scalar element is outside the C-backend subset" "$TMP/nerr" || { echo "FAIL [slice-nonscalar/C]: wrong message: $(head -1 "$TMP/nerr")"; exit 1; }
  echo "PASS [slice-nonscalar]: LLVM supports &[String]; C backend cleanly refuses it"
else
  echo "PASS [slice-nonscalar]: LLVM supports &[String] (C leg skipped)"
fi

# ---- PART 3: vectorization regression lock (IR-grep, both platforms) ----
SRC=""
for cand in "${TEST_SRCDIR:-}/_main/bench/loop.kd" "${RUNFILES_DIR:-}/_main/bench/loop.kd" "bench/loop.kd"; do
  [[ -f "$cand" ]] && { SRC="$cand"; break; }; done
if [[ -n "$SRC" ]]; then
  vc=$("$KARDC" --emit-llvm "$SRC" 2>/dev/null | grep -cE '<[0-9]+ x i(64|32)>' || true)
  [[ "$vc" -gt 0 ]] || { echo "FAIL [vectorize]: bench/loop.kd produced 0 vector ops (the v51 TTI fix may have regressed)"; exit 1; }
  echo "PASS [vectorize]: bench/loop.kd emits $vc vector ops (v51 TTI/auto-vectorization locked)"
else
  # Fall back to a self-contained hot loop so the guard runs even without bench/.
  printf '%s' 'fn main() -> i64 { let mut a: [i64; 256] = [0; 256] ; let mut i = 0 ; while i < 256 { a[i] = i * 2 ; i = i + 1 ; } ; let mut s = 0 ; let mut j = 0 ; while j < 256 { s = s + a[j] ; j = j + 1 ; } ; s }' > "$TMP/vec.kd"
  vc=$("$KARDC" --emit-llvm "$TMP/vec.kd" 2>/dev/null | grep -cE '<[0-9]+ x i(64|32)>' || true)
  [[ "$vc" -gt 0 ]] || { echo "FAIL [vectorize]: hot loop produced 0 vector ops"; exit 1; }
  echo "PASS [vectorize]: a hot loop emits $vc vector ops (v51 TTI/auto-vectorization locked)"
fi

echo "ALL v90 CLOSING-PASS SMOKE TESTS PASSED"
