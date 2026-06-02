#!/usr/bin/env bash
# v47 — totality: `#[total]` is a checked termination assertion. A sound
# conservative call-graph analysis accepts a fn only if it (and every fn it
# transitively calls) has no `while`/`loop` and the reachable call graph is
# acyclic (no recursion); `for`-over-a-range is bounded and fine. Anything that
# might diverge is rejected, naming the cause. (The full halting oracle + a
# `div` effect row is the deferred 6/6 work.)
set -euo pipefail
KARDC=""
for c in "${TEST_SRCDIR:-}/_main/compiler/kardc" "${RUNFILES_DIR:-}/_main/compiler/kardc" "./compiler/kardc" "./build.local/kardc"; do
  [[ -n "$c" && -x "$c" ]] && { KARDC="$c"; break; }; done
[[ -z "$KARDC" ]] && { echo "FAIL: kardc not found"; exit 1; }
echo "Using kardc at: $KARDC"
TMP=$(mktemp -d); trap 'rm -rf "$TMP"' EXIT
accept() { printf '%s' "$2" > "$TMP/$1.kd"; if "$KARDC" "$TMP/$1.kd" >/dev/null 2>&1; then echo "PASS(total): $1"; else echo "FAIL[accept $1]"; "$KARDC" "$TMP/$1.kd" 2>&1 | head -2; exit 1; fi; }
reject() { printf '%s' "$3" > "$TMP/$1.kd"; local e; e=$("$KARDC" "$TMP/$1.kd" 2>&1 >/dev/null || true); echo "$e" | grep -qi "$2" || { echo "FAIL[reject $1]: want '$2' got: $e"; exit 1; }; echo "PASS(partial): $1"; }

# A library of total building blocks (used by the accept cases).
LIB='#[total] fn add(a: i64, b: i64) -> i64 { a + b }
#[total] fn dbl(x: i64) -> i64 { x * 2 }
#[total] fn maxi(a: i64, b: i64) -> i64 { if a > b { a } else { b } }
#[total] fn sign(n: i64) -> i64 { if n > 0 { 1 } else { if n < 0 { 0 - 1 } else { 0 } } }
'
# ---- accept: total fns (straight-line / if / match / for-range / total calls) ----
accept t_add        "$LIB"'fn main() -> i64 { add(1,2) }'
accept t_dbl        "$LIB"'fn main() -> i64 { dbl(21) }'
accept t_max        "$LIB"'fn main() -> i64 { maxi(3,9) }'
accept t_sign       "$LIB"'fn main() -> i64 { sign(0-4) }'
accept t_compose    "$LIB"'#[total] fn f(x: i64) -> i64 { add(dbl(x), maxi(x, 1)) } fn main() -> i64 { f(5) }'
accept t_match      "$LIB"'enum E { A, B } #[total] fn g(e: E) -> i64 { match e { A => 1, B => 2 } } fn main() -> i64 { g(E::A) }'
accept t_forrange   "$LIB"'#[total] fn s(n: i64) -> i64 ! { alloc } { let mut t = 0; for i in 0..n { t = t + i; } t } fn main() -> i64 ! { alloc } { s(5) }'
accept t_nested     "$LIB"'#[total] fn h(x: i64) -> i64 { let y = add(x, 1); let z = dbl(y); maxi(y, z) } fn main() -> i64 { h(3) }'
accept t_const      "$LIB"'#[total] fn k() -> i64 { 42 } fn main() -> i64 { k() }'
accept t_chain3     "$LIB"'#[total] fn a1(x: i64) -> i64 { add(x,1) } #[total] fn a2(x: i64) -> i64 { a1(a1(x)) } #[total] fn a3(x: i64) -> i64 { a2(a2(x)) } fn main() -> i64 { a3(0) }'

# ---- reject: partial fns wrongly declared #[total] ----
reject r_while      'while'  '#[total] fn f(n: i64) -> i64 { let mut i = 0; while i < n { i = i + 1; } i } fn main() -> i64 { f(3) }'
reject r_loop       'loop'   '#[total] fn f() -> i64 { loop { } } fn main() -> i64 { f() }'
reject r_selfrec    'recursive' '#[total] fn fact(n: i64) -> i64 { if n <= 1 { 1 } else { n * fact(n-1) } } fn main() -> i64 { fact(5) }'
reject r_mutualrec  'recursive' '#[total] fn ev(n: i64) -> i64 { if n == 0 { 1 } else { od(n-1) } } #[total] fn od(n: i64) -> i64 { if n == 0 { 0 } else { ev(n-1) } } fn main() -> i64 { ev(4) }'
reject r_callsloop  'while'  'fn loops(n: i64) -> i64 { let mut i=0; while i<n { i=i+1; } i } #[total] fn t(n: i64) -> i64 { loops(n) } fn main() -> i64 { t(3) }'
reject r_deepcall   'while'  'fn lp() -> i64 { let mut i=0; while i<3 { i=i+1; } i } fn mid() -> i64 { lp() } #[total] fn top() -> i64 { mid() } fn main() -> i64 { top() }'

echo "ALL TOTALITY SMOKE TESTS PASSED"
