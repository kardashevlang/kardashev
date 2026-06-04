#!/usr/bin/env bash
# Roadmap v92 — self-hosted scalar Vec<i64> + growable strings: the self-hosted
# LLVM-IR compiler (examples/selfhost/structgen.kd) now emits a real growable
# heap `Vec<i64>` (vec_new/push/get/len/set) and growable owned strings
# (str_concat, str_char_at) into its OWN module preamble — a self-contained
# runtime (libc `declare`s + `@kdvec_*`/`@kdstr_*` `define`s) that clang links
# against libc with no kardashev runtime. The runtime is emitted ONLY when the
# program actually uses Vec / str_concat / str_char_at, so every prior (no-Vec)
# program's IR stays BYTE-IDENTICAL (the v84-v91 gates: phase117/118,
# selfhost_refs/calls/loops). Non-escaping owned heap locals are freed once at
# the function exit (Drop discipline — v91 gave us a real exit block).
#
# Differential-gated vs the host: the self-hosted-emitted IR (clang -> native)
# must exit-match `kardc` on the equivalent program. Test programs keep
# `f(a: i64, b: i64) -> i64` so the host's `fn main() { f(a, b) }` wrapper works.
# Exit codes are compared mod 256 (Unix exit-status width) on BOTH sides.
# Skips if clang is unavailable.
set -uo pipefail
KARDC=""
for candidate in \
    "${TEST_SRCDIR:-}/_main/compiler/kardc" "${TEST_SRCDIR:-}/kardashev/compiler/kardc" \
    "${RUNFILES_DIR:-}/_main/compiler/kardc" "${RUNFILES_DIR:-}/kardashev/compiler/kardc" \
    "./compiler/kardc" "./build.local/kardc"; do
    if [[ -n "$candidate" && -x "$candidate" ]]; then KARDC="$candidate"; break; fi
done
[[ -z "$KARDC" ]] && { echo "FAIL: kardc binary not found"; exit 1; }
SRC=""
for cand in \
    "${TEST_SRCDIR:-}/_main/examples/selfhost/structgen.kd" "${TEST_SRCDIR:-}/kardashev/examples/selfhost/structgen.kd" \
    "${RUNFILES_DIR:-}/_main/examples/selfhost/structgen.kd" "${RUNFILES_DIR:-}/kardashev/examples/selfhost/structgen.kd" \
    "examples/selfhost/structgen.kd"; do
    if [[ -f "$cand" ]]; then SRC="$cand"; break; fi
done
[[ -z "$SRC" ]] && { echo "FAIL: examples/selfhost/structgen.kd not found"; exit 1; }
CLANG="$(command -v clang || true)"
[[ -z "$CLANG" ]] && { echo "PASS [v92-vec]: SKIPPED (no clang to compile the emitted IR)"; exit 0; }
echo "Using kardc at: $KARDC ; clang at: $CLANG"
TMP=$(mktemp -d); trap 'rm -rf "$TMP"' EXIT

"$KARDC" --no-cache -o "$TMP/sg" "$SRC" >/dev/null 2>&1 || { echo "FAIL [v92-vec]: structgen did not build"; exit 1; }

# 1. BYTE-IDENTITY guard (Risk R0): a NO-Vec program emits NO runtime — the
#    use-gated emission keeps prior outputs byte-for-byte unchanged. The default
#    immutable demo must STILL have no alloca, no kdvec_/kdstr_, no `declare`, and
#    exit 7. A v91 while program (mutable locals + loop, but no Vec) likewise emits
#    no runtime lines.
"$TMP/sg" > "$TMP/d.ll" 2>/dev/null || { echo "FAIL [v92-vec]: demo did not run"; exit 1; }
grep -q 'insertvalue { i64, i64 }' "$TMP/d.ll" || { echo "FAIL [v92-vec]: { i64, i64 } insertvalue regressed"; cat "$TMP/d.ll"; exit 1; }
grep -q 'alloca'  "$TMP/d.ll" && { echo "FAIL [v92-vec]: immutable demo unexpectedly used an alloca"; cat "$TMP/d.ll"; exit 1; }
grep -q 'kdvec'   "$TMP/d.ll" && { echo "FAIL [v92-vec]: no-Vec demo emitted Vec runtime (R0)"; cat "$TMP/d.ll"; exit 1; }
grep -q 'kdstr'   "$TMP/d.ll" && { echo "FAIL [v92-vec]: no-Vec demo emitted String runtime (R0)"; cat "$TMP/d.ll"; exit 1; }
grep -q 'declare' "$TMP/d.ll" && { echo "FAIL [v92-vec]: no-Vec demo emitted libc declares (R0)"; cat "$TMP/d.ll"; exit 1; }
"$CLANG" "$TMP/d.ll" -o "$TMP/d" 2>/dev/null || { echo "FAIL [v92-vec]: clang rejected demo IR"; cat "$TMP/d.ll"; exit 1; }
"$TMP/d" >/dev/null 2>&1; [[ $? -eq 7 ]] || { echo "FAIL [v92-vec]: demo exit != 7"; exit 1; }
WSUM='fn f(a: i64, b: i64) -> i64 { let mut r = 0 ; let mut i = 1 ; while i <= a { r = r + i ; i = i + 1 ; } ; r }'
"$TMP/sg" "$WSUM" 10 0 > "$TMP/w.ll" 2>/dev/null || { echo "FAIL [v92-vec]: v91 while program errored"; exit 1; }
grep -q 'kdvec\|kdstr\|declare' "$TMP/w.ll" && { echo "FAIL [v92-vec]: v91 while program emitted runtime (R0)"; cat "$TMP/w.ll"; exit 1; }
echo "PASS [byte-identity]: use-gated runtime — no-Vec programs emit zero runtime lines; demo exit 7"

# 2. IR SHAPE: a Vec program emits the self-contained runtime (libc declares +
#    kdvec_* defines with a doubling-grow block) and frees the local at exit.
VPROG='fn f(a: i64, b: i64) -> i64 { let mut v = vec_new() ; for i in 0 .. a { vec_push(&mut v, i) ; } ; vec_len(&v) }'
"$TMP/sg" "$VPROG" 5 0 > "$TMP/v.ll" 2>/dev/null || { echo "FAIL [v92-vec]: vec program errored"; exit 1; }
grep -q 'declare ptr @malloc(i64)'                  "$TMP/v.ll" || { echo "FAIL [v92-vec]: no malloc declare"; cat "$TMP/v.ll"; exit 1; }
grep -q 'declare ptr @realloc'                      "$TMP/v.ll" || { echo "FAIL [v92-vec]: no realloc declare"; cat "$TMP/v.ll"; exit 1; }
grep -q 'define { ptr, i64, i64 } @kdvec_new_i64'   "$TMP/v.ll" || { echo "FAIL [v92-vec]: no kdvec_new define"; cat "$TMP/v.ll"; exit 1; }
grep -q '@kdvec_push_i64'                            "$TMP/v.ll" || { echo "FAIL [v92-vec]: no kdvec_push"; cat "$TMP/v.ll"; exit 1; }
grep -q 'call ptr @realloc'                          "$TMP/v.ll" || { echo "FAIL [v92-vec]: push has no realloc grow"; cat "$TMP/v.ll"; exit 1; }
grep -q 'call void @kdvec_drop_i64'                  "$TMP/v.ll" || { echo "FAIL [v92-vec]: no drop call at fn exit"; cat "$TMP/v.ll"; exit 1; }
grep -q 'call void @free'                            "$TMP/v.ll" || { echo "FAIL [v92-vec]: drop has no free"; cat "$TMP/v.ll"; exit 1; }
"$CLANG" "$TMP/v.ll" -o "$TMP/v" 2>/dev/null || { echo "FAIL [v92-vec]: clang rejected vec IR"; cat "$TMP/v.ll"; exit 1; }
"$TMP/v" >/dev/null 2>&1; [[ $? -eq 5 ]] || { echo "FAIL [v92-vec]: vec_len after 5 pushes != 5"; exit 1; }
echo "PASS [vec-ir]: self-contained kdvec runtime (declares + doubling-grow + drop/free); exit 5"

# 3. DIFFERENTIAL: self-hosted-emitted IR exit == host exit (both mod 256).
diff_case() {  # $1 src  $2 a  $3 b  $4 label
    "$TMP/sg" "$1" "$2" "$3" > "$TMP/s.ll" 2>/dev/null || { echo "FAIL [v92-vec/$4]: self errored"; exit 1; }
    "$CLANG" "$TMP/s.ll" -o "$TMP/s" 2>/dev/null || { echo "FAIL [v92-vec/$4]: clang rejected IR"; cat "$TMP/s.ll"; exit 1; }
    "$TMP/s" >/dev/null 2>&1; local r_self=$?
    printf '%s\nfn main() -> i64 { f(%s, %s) }\n' "$1" "$2" "$3" > "$TMP/h.kd"
    "$KARDC" --no-cache -o "$TMP/h" "$TMP/h.kd" >/dev/null 2>&1 || { echo "FAIL [v92-vec/$4]: host rejected program"; cat "$TMP/h.kd"; exit 1; }
    "$TMP/h" >/dev/null 2>&1; local r_host=$?
    [[ "$r_self" -eq "$r_host" ]] || { echo "FAIL [v92-vec/$4]: self=$r_self != host=$r_host"; exit 1; }
    echo "PASS [$4]: self == host == $r_self"
}
# (a) build a Vec via a for-loop, return its length.  a=5 -> 5
diff_case "$VPROG" 5 0 "vec-len-after-push"
# empty Vec: 0 pushes -> len 0; vec_get OOB -> 0
diff_case 'fn f(a: i64, b: i64) -> i64 { let mut v = vec_new() ; vec_get(&v, 0) }' 0 0 "empty-vec-oob"
# growth across the first realloc boundary (cap 0->4) and a second doubling (->8):
diff_case "$VPROG" 4 0 "vec-grow-boundary-4"
diff_case "$VPROG" 5 0 "vec-grow-boundary-5"
# (b) build a Vec, SUM it in a loop -> exit == sum(0..a).  a=10 -> 45
diff_case 'fn f(a: i64, b: i64) -> i64 { let mut v = vec_new() ; for i in 0 .. a { vec_push(&mut v, i) ; } ; let mut s = 0 ; for j in 0 .. a { s = s + vec_get(&v, j) ; } ; s }' 10 0 "vec-sum"
# growth through >=5 doublings (4->8->16->32->64->128); a=100 -> len 100 (mod 256)
diff_case "$VPROG" 100 0 "vec-grow-100"

# vec_set is SELF-ONLY: the host stdlib has no `vec_set` builtin (it mutates by
# index via `vec_get_ref` / `*r = x`), so this op can't be differentially gated.
# Verify it standalone: push 0..a, set index b to 100, read it back -> 100.
self_case() {  # $1 src  $2 a  $3 b  $4 want  $5 label
    "$TMP/sg" "$1" "$2" "$3" > "$TMP/so.ll" 2>/dev/null || { echo "FAIL [v92-vec/$5]: self errored"; exit 1; }
    "$CLANG" "$TMP/so.ll" -o "$TMP/so" 2>/dev/null || { echo "FAIL [v92-vec/$5]: clang rejected IR"; cat "$TMP/so.ll"; exit 1; }
    "$TMP/so" >/dev/null 2>&1; local r=$?
    [[ "$r" -eq "$4" ]] || { echo "FAIL [v92-vec/$5]: self=$r != want=$4"; exit 1; }
    echo "PASS [$5]: self == $r (self-only — no host vec_set)"
}
self_case 'fn f(a: i64, b: i64) -> i64 { let mut v = vec_new() ; for i in 0 .. a { vec_push(&mut v, i) ; } ; vec_set(&mut v, b, 100) ; vec_get(&v, b) }' 5 2 100 "vec-set-get"
# vec_set OOB is a no-op (returns 0, no store): index 99 of a 5-elem vec, read idx 2 unchanged -> 2.
self_case 'fn f(a: i64, b: i64) -> i64 { let mut v = vec_new() ; for i in 0 .. a { vec_push(&mut v, i) ; } ; vec_set(&mut v, 99, 100) ; vec_get(&v, b) }' 5 2 2 "vec-set-oob-noop"

# (c) GROWABLE STRING: concat builds an owned (cap>0) string; str_len of it.
#     str_concat(&String,&String): a bare literal needs `&` (matches host).
SPROG='fn f(a: i64, b: i64) -> i64 { let mut s = "" ; s = str_concat(&s, &"ab") ; s = str_concat(&s, &"cd") ; str_len(&s) }'
"$TMP/sg" "$SPROG" 0 0 > "$TMP/sc.ll" 2>/dev/null || { echo "FAIL [v92-vec]: str program errored"; exit 1; }
grep -q '@kdstr_concat'   "$TMP/sc.ll" || { echo "FAIL [v92-vec]: no kdstr_concat define"; cat "$TMP/sc.ll"; exit 1; }
grep -q 'call ptr @memcpy' "$TMP/sc.ll" || { echo "FAIL [v92-vec]: concat has no memcpy"; cat "$TMP/sc.ll"; exit 1; }
grep -q 'call void @kdstr_drop' "$TMP/sc.ll" || { echo "FAIL [v92-vec]: no owned-string drop at exit"; cat "$TMP/sc.ll"; exit 1; }
diff_case "$SPROG" 0 0 "str-concat-len"

# (d) CAPSTONE: tokenize a fixed source string char-by-char into a Vec of token
#     kinds (0 = space, 1 = non-space), then sum the kinds -> the non-space count.
#     A real loop-driven data program over both String and Vec; self == host.
CAP='fn f(a: i64, b: i64) -> i64 { let s = "ab cd ef gh" ; let mut v = vec_new() ; let n = str_len(&s) ; for i in 0 .. n { let ch = str_char_at(&s, i) ; let kind = if ch == 32 { 0 } else { 1 } ; vec_push(&mut v, kind) ; } ; let mut cnt = 0 ; for j in 0 .. n { cnt = cnt + vec_get(&v, j) ; } ; cnt }'
diff_case "$CAP" 0 0 "capstone-tokenizer"

# 4. NEGATIVE: ill-typed Vec/String use must be a type error (no invalid IR).
"$TMP/sg" 'fn f(a: i64, b: i64) -> i64 { let mut v = vec_new() ; vec_push(v, a) ; vec_len(&v) }' 5 0 2>/dev/null > "$TMP/n1.ll"
grep -q 'TYPE ERROR' "$TMP/n1.ll" || { echo "FAIL [neg-vec-push-no-mut]: vec_push without &mut not rejected"; cat "$TMP/n1.ll"; exit 1; }
"$TMP/sg" 'fn f(a: i64, b: i64) -> i64 { let mut v = vec_new() ; vec_get(&v) }' 5 0 2>/dev/null > "$TMP/n2.ll"
grep -q 'TYPE ERROR' "$TMP/n2.ll" || { echo "FAIL [neg-vec-get-arity]: vec_get arity mismatch not rejected"; cat "$TMP/n2.ll"; exit 1; }
echo "PASS [neg-typecheck]: ill-typed vec_push (missing &mut) / vec_get (arity) are type errors"

# 5. LEAK GATE: a 100k-push loop frees its Vec at fn exit. Run under MALLOC_CHECK_=3
#    (aborts on heap corruption / double-free) and assert exit == host (both mod
#    256). Then drive the alloc+free 500 times in one process and assert peak RSS
#    stays flat (< 64 MB): if drop didn't run, 500 x ~800 KB = ~400 MB.
LEAK='fn f(a: i64, b: i64) -> i64 { let mut v = vec_new() ; let mut i = 0 ; while i < a { vec_push(&mut v, i) ; i = i + 1 ; } ; vec_len(&v) }'
"$TMP/sg" "$LEAK" 100000 0 > "$TMP/leak.ll" 2>/dev/null || { echo "FAIL [v92-vec]: leak program errored"; exit 1; }
"$CLANG" "$TMP/leak.ll" -o "$TMP/leak" 2>/dev/null || { echo "FAIL [v92-vec]: clang rejected leak IR"; exit 1; }
MALLOC_CHECK_=3 "$TMP/leak" 2>"$TMP/mc.err"; r_self=$?
[[ -s "$TMP/mc.err" ]] && { echo "FAIL [leak-mallochk]: MALLOC_CHECK_=3 reported heap errors"; cat "$TMP/mc.err"; exit 1; }
printf '%s\nfn main() -> i64 { f(100000, 0) }\n' "$LEAK" > "$TMP/hl.kd"
"$KARDC" --no-cache -o "$TMP/hl" "$TMP/hl.kd" >/dev/null 2>&1 || { echo "FAIL [leak]: host rejected leak program"; exit 1; }
"$TMP/hl" >/dev/null 2>&1; r_host=$?
[[ "$r_self" -eq "$r_host" ]] || { echo "FAIL [leak]: self=$r_self != host=$r_host (vec_len after 100k pushes)"; exit 1; }
echo "PASS [leak-mallochk]: 100k-push under MALLOC_CHECK_=3 clean; self == host == $r_self"
# RSS-flat: 500 allocate-and-free cycles of an ~800 KB Vec inside one process.
RLEAK='fn g(a: i64) -> i64 { let mut v = vec_new() ; let mut i = 0 ; while i < a { vec_push(&mut v, i) ; i = i + 1 ; } ; vec_len(&v) } fn f(a: i64, b: i64) -> i64 { let mut acc = 0 ; let mut k = 0 ; while k < b { acc = acc + g(a) ; k = k + 1 ; } ; acc }'
"$TMP/sg" "$RLEAK" 100000 500 > "$TMP/rl.ll" 2>/dev/null || { echo "FAIL [v92-vec]: rss program errored"; exit 1; }
"$CLANG" "$TMP/rl.ll" -o "$TMP/rl" 2>/dev/null || { echo "FAIL [v92-vec]: clang rejected rss IR"; exit 1; }
if command -v /usr/bin/time >/dev/null 2>&1; then
    /usr/bin/time -v "$TMP/rl" 2>"$TMP/t.out" >/dev/null
    RSS=$(grep 'Maximum resident' "$TMP/t.out" | grep -oE '[0-9]+' | tail -1)
    if [[ -n "$RSS" ]]; then
        [[ "$RSS" -lt 65536 ]] || { echo "FAIL [leak-rss]: peak RSS ${RSS} KB >= 64 MB — drop-at-exit not freeing (500 x ~800 KB would be ~400 MB)"; exit 1; }
        echo "PASS [leak-rss]: 500 x ~800 KB alloc+free cycles, peak RSS ${RSS} KB (< 64 MB) — Drop discipline holds"
    else
        echo "PASS [leak-rss]: SKIPPED (could not read Maximum resident set size)"
    fi
else
    echo "PASS [leak-rss]: SKIPPED (no /usr/bin/time -v available)"
fi

echo "ALL v92 (vec + growable strings) SMOKE TESTS PASSED"
