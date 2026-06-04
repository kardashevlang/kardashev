#!/usr/bin/env bash
# Roadmap v93 — write-capable `&mut [T]` slices + variadic-C FFI + C-backend
# slice-from-array. Builds on the v90 read-only slice support (all 3 backends).
#
# CORE (this gate):
#  1. `&mut [T]` is a DISTINCT slice type from `&[T]` (a sliceIsMut flag on the
#     slice Type). `&mut v[a..b]` / `&mut arr[a..b]` construct it; `&mut [T]`
#     freely coerces to `&[T]` (read-only), but NOT the reverse.
#  2. slice_set(s: &mut[T], i, v) -> () and slice_get_mut(s: &mut[T], i) -> &mut T
#     write through a slice. LLVM lowers a GEP+store / element-ptr; the C backend
#     lowers the same over `struct kdslice` (scalar i64/bool element only).
#     `*slice_get_mut(s,i) = v` works via the existing deref-assign path (LLVM).
#  3. Borrow-check: `&mut v[a..b]` takes a MUTABLE loan (the `&mut place`
#     exclusivity + v26 two-phase rules) — an aliasing `&[T]` read live across a
#     `slice_set` is rejected (E0502).
#  4. Variadic C externs (`extern "C" fn printf(fmt: &String, ...) -> i32`):
#     the extern FunctionType is isVarArg; trailing args pass through with C
#     default-argument promotion (f32->double, narrow int / bool -> i32).
#  5. slice-from-fixed-array (`&arr[a..b]` over a stack `[T; N]`) — LLVM (the v90
#     gap) AND the C backend.
#
# DIFFERENTIAL GATE: each correctness case is JIT == AOT (== C backend where in
# the C subset). The variadic leg is JIT == AOT only (the C backend has no extern
# lowering — it refuses ALL extern programs; that is a documented v94+ deferral).
#
# DEFERRALS (honest, no stubs):
#  - variadic in the C backend (--emit-c refuses externs entirely);
#  - non-scalar `&mut [String]` slices in the C backend (kdslice is int64_t*; the
#    LLVM backend supports it, the C backend cleanly refuses — matches v90);
#  - `*slice_get_mut(s,i) = v` deref-assign in the C backend (the C backend
#    refuses a non-variable/field assignment place — use slice_set there);
#  - mutable-slice ITERATION (`for x in &mut s`) -> v94.
set -uo pipefail
KARDC=""
for c in "${TEST_SRCDIR:-}/_main/compiler/kardc" "${RUNFILES_DIR:-}/_main/compiler/kardc" "./compiler/kardc" "./build.local/kardc"; do
  [[ -n "$c" && -x "$c" ]] && { KARDC="$c"; break; }; done
[[ -z "$KARDC" ]] && { echo "FAIL: kardc not found"; exit 1; }
echo "Using kardc at: $KARDC"
CC="$(command -v cc || command -v clang || true)"
TMP=$(mktemp -d); trap 'rm -rf "$TMP"' EXIT

# tri: JIT (stdout line 1) == AOT (exit) == C (exit). main returns a small i64
# that is BOTH printed (JIT) and the process exit code (AOT/C).
tri() {  # $1 label  $2 program  $3 want
  printf '%s' "$2" > "$TMP/t.kd"
  local j a
  j=$("$KARDC" "$TMP/t.kd" 2>/dev/null | head -1)
  [[ "$j" == "$3" ]] || { echo "FAIL [$1/JIT]: got '$j' want '$3'"; "$KARDC" "$TMP/t.kd" 2>&1 | head -4; exit 1; }
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

# ---- CORE 1+2: in-place bubble-sort over a `&mut [i64]` (slice_set writes) ----
# v = [5,3,1,4,2]; sort -> [1,2,3,4,5]; build the digits 12345; subtract 12340 = 5.
tri sort_mut 'fn bubble(s: &mut [i64]) -> i64 { let n = slice_len(s) ; let mut i = 0 ; while i < n { let mut k = 0 ; while k < n - 1 { let a = slice_get(s, k) ; let b = slice_get(s, k + 1) ; if a > b { slice_set(s, k, b) ; slice_set(s, k + 1, a) ; } else {} ; k = k + 1 ; } ; i = i + 1 ; } ; 0 } fn main() -> i64 ! { alloc } { let mut v = vec_new() ; vec_push(&mut v, 5) ; vec_push(&mut v, 3) ; vec_push(&mut v, 1) ; vec_push(&mut v, 4) ; vec_push(&mut v, 2) ; bubble(&mut v[0..5]) ; let mut acc = 0 ; let mut i = 0 ; while i < 5 { acc = acc * 10 + vec_get(&mut v, i) ; i = i + 1 ; } ; acc - 12340 }' 5

# ---- CORE 2: slice-fill via `*slice_get_mut(s,i) = v` (deref-assign; LLVM) ----
# Writes a[i] = i*2 for i in 0..4 -> 0+2+4+6 = 12. (C leg uses slice_set instead;
# this case keeps the deref-assign form, so it is JIT == AOT only — the C backend
# refuses the deref-place assignment, a documented deferral. Use trj for that.)
trj() {  # JIT==AOT only (no C leg): $1 label $2 program $3 want
  printf '%s' "$2" > "$TMP/t.kd"
  local j a
  j=$("$KARDC" "$TMP/t.kd" 2>/dev/null | head -1)
  [[ "$j" == "$3" ]] || { echo "FAIL [$1/JIT]: got '$j' want '$3'"; "$KARDC" "$TMP/t.kd" 2>&1 | head -4; exit 1; }
  "$KARDC" --no-cache -o "$TMP/a" "$TMP/t.kd" >/dev/null 2>&1 || { echo "FAIL [$1/AOT]: build"; exit 1; }
  "$TMP/a" >/dev/null 2>&1; a=$?
  [[ "$a" -eq "$3" ]] || { echo "FAIL [$1/AOT]: exit $a want $3"; exit 1; }
  echo "PASS [$1]: JIT == AOT == $3"
}
trj fill_getmut 'fn fill(s: &mut [i64]) -> i64 { let n = slice_len(s) ; let mut i = 0 ; while i < n { let r = slice_get_mut(s, i) ; *r = i * 2 ; i = i + 1 ; } ; 0 } fn main() -> i64 ! { alloc } { let mut v = vec_new() ; vec_push(&mut v, 0) ; vec_push(&mut v, 0) ; vec_push(&mut v, 0) ; vec_push(&mut v, 0) ; fill(&mut v[0..4]) ; slice_get(&v[0..4], 0) + slice_get(&v[0..4], 1) + slice_get(&v[0..4], 2) + slice_get(&v[0..4], 3) }' 12

# slice-fill via slice_set (in the C subset -> the full triple compare).
tri fill_set 'fn fill(s: &mut [i64]) -> i64 { let n = slice_len(s) ; let mut i = 0 ; while i < n { slice_set(s, i, i * 2) ; i = i + 1 ; } ; 0 } fn main() -> i64 ! { alloc } { let mut v = vec_new() ; vec_push(&mut v, 0) ; vec_push(&mut v, 0) ; vec_push(&mut v, 0) ; vec_push(&mut v, 0) ; fill(&mut v[0..4]) ; slice_get(&v[0..4], 0) + slice_get(&v[0..4], 1) + slice_get(&v[0..4], 2) + slice_get(&v[0..4], 3) }' 12

# ---- CORE 5: slice-from-fixed-array, read (LLVM gap + C backend) ----
# &a[1..4] over [1,2,3,4,5] -> 2+3+4 = 9.
tri arr_slice_read 'fn sm(s: &[i64]) -> i64 { let mut i = 0 ; let mut acc = 0 ; while i < slice_len(s) { acc = acc + slice_get(s, i) ; i = i + 1 ; } ; acc } fn main() -> i64 { let a: [i64; 5] = [1, 2, 3, 4, 5] ; sm(&a[1..4]) }' 9

# slice-from-fixed-array, WRITE through `&mut a[0..3]` (mutates the backing array).
# a = [0,0,0,9]; setall writes 7 to s[0..3]; sum -> 7+7+7+9 = 30.
tri arr_slice_write 'fn setall(s: &mut [i64]) -> i64 { let mut i = 0 ; while i < slice_len(s) { slice_set(s, i, 7) ; i = i + 1 ; } ; 0 } fn main() -> i64 { let mut a: [i64; 4] = [0, 0, 0, 9] ; setall(&mut a[0..3]) ; a[0] + a[1] + a[2] + a[3] }' 30

# ---- CORE 1: `&mut [T]` freely coerces to `&[T]` (a read-only borrow) ----
tri coerce_mut_to_shared 'fn rd(s: &[i64]) -> i64 { slice_get(s, 0) + slice_len(s) } fn main() -> i64 ! { alloc } { let mut v = vec_new() ; vec_push(&mut v, 10) ; vec_push(&mut v, 20) ; rd(&mut v[0..2]) }' 12

# ---- CORE 4: variadic C extern — printf("n=%d\n", 7) prints n=7 (JIT == AOT) ----
# The C backend has no extern lowering (refuses all extern programs), so the
# variadic gate is JIT == AOT, with the IR shape asserted as a portable guard.
printf '%s' 'extern "C" fn printf(fmt: &String, ...) -> i32 ; fn main() -> i64 ! { io } { let fmt = "n=%d\n" ; printf(&fmt, 7) ; 0 }' > "$TMP/var.kd"
JOUT=$("$KARDC" "$TMP/var.kd" 2>/dev/null)
echo "$JOUT" | grep -q "^n=7$" || { echo "FAIL [variadic/JIT]: stdout did not contain 'n=7'"; echo "got: $JOUT"; "$KARDC" "$TMP/var.kd" 2>&1 | head -4; exit 1; }
"$KARDC" --no-cache -o "$TMP/v" "$TMP/var.kd" >/dev/null 2>&1 || { echo "FAIL [variadic/AOT]: build"; exit 1; }
AOUT=$("$TMP/v" 2>/dev/null); ax=$?
[[ "$ax" -eq 0 ]] || { echo "FAIL [variadic/AOT]: exit $ax want 0"; exit 1; }
echo "$AOUT" | grep -q "^n=7$" || { echo "FAIL [variadic/AOT]: stdout did not contain 'n=7'"; echo "got: $AOUT"; exit 1; }
# IR-shape guard: the extern is declared variadic (`(ptr, ...)`). Emit to a file
# first (a `grep -q` on a pipe closes early -> SIGPIPE on kardc -> pipefail).
"$KARDC" --emit-llvm "$TMP/var.kd" > "$TMP/var.ll" 2>/dev/null
grep -qE 'declare.*@printf\(ptr.*, \.\.\.\)' "$TMP/var.ll" || { echo "FAIL [variadic/IR]: printf not declared variadic"; grep -i printf "$TMP/var.ll" | head; exit 1; }
# C backend refuses the extern program cleanly (documented deferral).
if [[ -n "$CC" ]]; then
  "$KARDC" --emit-c "$TMP/var.kd" >/dev/null 2>"$TMP/verr" && { echo "FAIL [variadic/C]: --emit-c should refuse an extern program"; exit 1; }
  echo "PASS [variadic]: JIT == AOT print 'n=7' + IR is variadic; C leg N/A (--emit-c does not support extern fns, documented)"
else
  echo "PASS [variadic]: JIT == AOT print 'n=7' + IR is variadic (C leg N/A by design)"
fi

# ---- CORE 3 NEGATIVE: an aliasing shared `&[T]` read live across a slice_set ----
# `let r = &v[0..2]` holds a shared borrow; `slice_set(&mut v[0..2], ...)` then
# takes a mutable borrow of the SAME `v` while `r` is still live -> E0502.
printf '%s' 'fn main() -> i64 ! { alloc } { let mut v = vec_new() ; vec_push(&mut v, 1) ; vec_push(&mut v, 2) ; let r = &v[0..2] ; slice_set(&mut v[0..2], 0, 9) ; slice_get(r, 1) }' > "$TMP/neg1.kd"
"$KARDC" "$TMP/neg1.kd" >/dev/null 2>"$TMP/neg1.err" && { echo "FAIL [neg-alias]: should reject an aliasing shared read across a slice_set"; exit 1; }
grep -qE 'E0502|cannot borrow .* mutably while shared borrows are active' "$TMP/neg1.err" || { echo "FAIL [neg-alias]: wrong diagnostic: $(head -1 "$TMP/neg1.err")"; exit 1; }
echo "PASS [neg-alias]: aliasing shared read live across a slice_set is rejected (E0502)"

# ---- B5 NEGATIVE: slice_set on a SHARED `&[T]` slice (not `&mut`) ----
# The soundness gate (unify ignores sliceIsMut, so this is an explicit check).
printf '%s' 'fn bad(s: &[i64]) -> i64 { slice_set(s, 0, 9) ; 0 } fn main() -> i64 ! { alloc } { let mut v = vec_new() ; vec_push(&mut v, 1) ; bad(&v[0..1]) }' > "$TMP/neg2.kd"
"$KARDC" "$TMP/neg2.kd" >/dev/null 2>"$TMP/neg2.err" && { echo "FAIL [neg-shared-write]: should reject slice_set on a shared &[T]"; exit 1; }
grep -qi 'requires a `&mut \[T\]` slice' "$TMP/neg2.err" || { echo "FAIL [neg-shared-write]: wrong diagnostic: $(head -1 "$TMP/neg2.err")"; exit 1; }
echo "PASS [neg-shared-write]: slice_set on a shared &[T] is rejected (mutability soundness gate)"

echo "ALL v93 SLICE-MUT / VARIADIC-FFI / SLICE-FROM-ARRAY SMOKE TESTS PASSED"
