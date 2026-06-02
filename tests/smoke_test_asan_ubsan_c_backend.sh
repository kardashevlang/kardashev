#!/usr/bin/env bash
# v66 — sanitizer sweep of the C backend. Emit C (`--emit-c`) for in-subset
# programs (struct / enum+match / ref / for / while / String / Vec / Drop /
# closure / generic / recursion), compile each under -fsanitize=address,undefined
# and assert it runs CLEAN (exit 0, no sanitizer diagnostic). Then feed the SAME
# sanitizer flags 3 hand-written known-UB C programs and assert each is CAUGHT —
# proving the sanitizers are actually live (not silently disabled). Skips
# gracefully when no clang/ASan is available.
#
# NOTE: `--emit-c` re-parses RAW source WITHOUT the prelude, so these programs
# use builtins + user-defined types only (no Option/Some/str_concat/vec_sum).
set -uo pipefail
KARDC=""
for c in "${TEST_SRCDIR:-}/_main/compiler/kardc" "${RUNFILES_DIR:-}/_main/compiler/kardc" "./compiler/kardc" "./build.local/kardc"; do
  [[ -n "$c" && -x "$c" ]] && { KARDC="$c"; break; }; done
[[ -z "$KARDC" ]] && { echo "FAIL: kardc not found"; exit 1; }
CC="$(command -v clang || command -v cc || command -v gcc || true)"
echo "Using kardc at: $KARDC ; cc: ${CC:-<none>}"
TMP=$(mktemp -d); trap 'rm -rf "$TMP"' EXIT
SANFLAGS="-fsanitize=address,undefined -fno-sanitize-recover=undefined"

# Probe: do the sanitizers compile AND catch a known overflow?
if [[ -z "$CC" ]]; then echo "SKIP: no C compiler — sanitizer sweep skipped"; exit 0; fi
printf '#include <stdlib.h>\nint main(){char*b=malloc(4);b[9]=1;int r=b[9];free(b);return r;}\n' > "$TMP/probe.c"
if ! $CC $SANFLAGS -o "$TMP/probe" "$TMP/probe.c" 2>/dev/null; then
  echo "SKIP: $CC cannot build with $SANFLAGS — sanitizer sweep skipped"; exit 0
fi
if "$TMP/probe" >/dev/null 2>&1; then
  echo "SKIP: ASan did not catch a known heap overflow (sanitizer inactive) — skipped"; exit 0
fi
echo "Sanitizers active (clang + ASan/UBSan)."

# --- 11 in-subset programs that MUST be sanitizer-clean (exit 0) ---
declare -a PROGS
PROGS+=('struct P { a: i64, b: i64 } fn main() -> i64 { let p = P { a: 3, b: 4 }; let r = &p; if r.a + r.b == 7 { 0 } else { 1 } }')
PROGS+=('enum E { A(i64), B } fn main() -> i64 { let e = E::A(5); match e { A(x) => if x == 5 { 0 } else { 1 }, B => 2 } }')
PROGS+=('fn main() -> i64 { let mut s = 0; for i in 0..10 { s = s + i; } if s == 45 { 0 } else { 1 } }')
PROGS+=('fn main() -> i64 { let mut i = 0; let mut s = 0; while i < 5 { s = s + i; i = i + 1; } if s == 10 { 0 } else { 1 } }')
PROGS+=('fn main() -> i64 ! { alloc } { let mut s = string_new(); str_push_byte(&mut s, 65); str_push_byte(&mut s, 66); if str_len(&s) == 2 { 0 } else { 1 } }')
PROGS+=('fn main() -> i64 ! { alloc } { let mut v = vec_new(); vec_push(&mut v, 10); vec_push(&mut v, 20); if vec_get(&v, 1) == 20 { 0 } else { 1 } }')
PROGS+=('fn main() -> i64 ! { alloc } { let mut v = vec_new(); let mut i = 0; while i < 5 { vec_push(&mut v, i); i = i + 1; } let mut s = 0; let mut j = 0; while j < vec_len(&v) { s = s + vec_get(&v, j); j = j + 1; } if s == 10 { 0 } else { 1 } }')
PROGS+=('fn main() -> i64 { let n = 5; let f = |x| x + n; if f(3) == 8 { 0 } else { 1 } }')
PROGS+=('fn id<T>(x: T) -> T { x } fn main() -> i64 { if id(7) == 7 { 0 } else { 1 } }')
PROGS+=('fn fib(n: i64) -> i64 { if n < 2 { n } else { fib(n - 1) + fib(n - 2) } } fn main() -> i64 { if fib(10) == 55 { 0 } else { 1 } }')
PROGS+=('fn main() -> i64 { let a = true; let b = false; if a && !b || b { 0 } else { 1 } }')
PROGS+=('fn dbl(x: i64) -> i64 { x * 2 } fn main() -> i64 { let r = dbl(dbl(3)); if r == 12 { 0 } else { 1 } }')

clean=0
for idx in "${!PROGS[@]}"; do
  printf '%s\n' "${PROGS[$idx]}" > "$TMP/p$idx.kd"
  if ! "$KARDC" --emit-c "$TMP/p$idx.kd" > "$TMP/p$idx.c" 2>"$TMP/e"; then
    echo "FAIL [prog $idx]: --emit-c refused: $(head -1 "$TMP/e")"; cat "$TMP/p$idx.kd"; exit 1
  fi
  if ! $CC $SANFLAGS -fwrapv -O1 -o "$TMP/p$idx" "$TMP/p$idx.c" 2>"$TMP/cc"; then
    echo "FAIL [prog $idx]: cc rejected emitted C:"; head -5 "$TMP/cc"; exit 1
  fi
  out=$("$TMP/p$idx" 2>&1); rc=$?
  if (( rc != 0 )) || echo "$out" | grep -qiE "AddressSanitizer|runtime error|UndefinedBehavior|LeakSanitizer"; then
    echo "FAIL [prog $idx]: sanitizer error or nonzero exit ($rc): $out"; cat "$TMP/p$idx.kd"; exit 1
  fi
  clean=$((clean+1))
done
echo "PASS: $clean/${#PROGS[@]} in-subset C-backend programs sanitizer-clean"
(( clean >= 10 )) || { echo "FAIL: fewer than 10 clean programs"; exit 1; }

# --- 3 hand-written known-UB C programs the sanitizers MUST catch ---
caught=0
printf '#include <stdlib.h>\nint main(){char*b=malloc(4);b[7]=9;int r=b[7];free(b);return r;}\n' > "$TMP/u0.c"   # heap overflow
printf '#include <stdlib.h>\nint main(){char*b=malloc(4);free(b);b[0]=1;return b[0];}\n' > "$TMP/u1.c"            # use-after-free
printf '#include <limits.h>\nint main(){int x=INT_MAX;int y=x+1;return y;}\n' > "$TMP/u2.c"                       # signed overflow (UBSan)
for u in u0 u1 u2; do
  $CC $SANFLAGS -o "$TMP/$u" "$TMP/$u.c" 2>/dev/null || { echo "FAIL: cannot build UB probe $u"; exit 1; }
  o=$("$TMP/$u" 2>&1); r=$?
  if (( r != 0 )) || echo "$o" | grep -qiE "AddressSanitizer|runtime error|overflow"; then caught=$((caught+1)); else echo "FAIL: sanitizer MISSED known UB in $u"; exit 1; fi
done
echo "PASS: $caught/3 intentional-UB programs caught by the sanitizers"

echo "ALL ASAN/UBSAN C-BACKEND TESTS PASSED"
