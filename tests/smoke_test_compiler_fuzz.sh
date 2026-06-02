#!/usr/bin/env bash
# v46 — compiler-hardening fuzz: feed adversarial / malformed source through the
# kardc front-end and assert it NEVER crashes (no SIGSEGV/SIGABRT) — a clean
# diagnostic + non-zero exit is fine, a signal is a failure. This is the
# compiler's own DoS/robustness surface, distinct from the differential fuzzing
# of GENERATED programs. (A crash on bad input must fail CI.)
#
# Caught + fixed in this phase: deeply-nested `(((…`, `[[[…`, `Vec<Vec<…>>`,
# `&&&…`, and unary chains `----…` / `!!!!…` stack-overflowed the recursive
# descent parser → now bounded by a parse recursion-depth guard.
set -euo pipefail
KARDC=""
for c in "${TEST_SRCDIR:-}/_main/compiler/kardc" "${RUNFILES_DIR:-}/_main/compiler/kardc" "./compiler/kardc" "./build.local/kardc"; do
  [[ -n "$c" && -x "$c" ]] && { KARDC="$c"; break; }; done
[[ -z "$KARDC" ]] && { echo "FAIL: kardc not found"; exit 1; }
echo "Using kardc at: $KARDC"
TMP=$(mktemp -d); trap 'rm -rf "$TMP"' EXIT

crashes=0; n=0
check() { # run kardc on the file; a >128 exit (killed by a signal) is a crash
  local rc=0
  "$KARDC" "$1" >/dev/null 2>&1 </dev/null || rc=$?   # `|| rc=$?` so set -e ignores a clean diagnostic
  n=$((n+1))
  if [[ "$rc" -gt 128 ]]; then crashes=$((crashes+1)); echo "CRASH (rc=$rc) on $(basename "$1")"; fi
}

# 1. Curated adversarial inputs (the deep-nesting DoS class + malformed tokens).
mk() { printf '%s' "$2" > "$TMP/$1.kd"; check "$TMP/$1.kd"; }
mk parens   "fn main()->i64{$(python3 -c 'print("("*9000)')}"
mk brackets "fn main()->i64{$(python3 -c 'print("["*9000)')0$(python3 -c 'print("]"*9000)')}"
mk gentypes "fn f(x: $(python3 -c 'print("Vec<"*4000)')i64$(python3 -c 'print(">"*4000)')) {}"
mk refs     "fn f(x: $(python3 -c 'print("&"*9000)')i64) {}"
mk negs     "fn main()->i64{$(python3 -c 'print("-"*9000)')5}"
mk nots     "fn main()->i64{$(python3 -c 'print("!"*9000)')true}"
mk derefs   "fn main()->i64{$(python3 -c 'print("*"*9000)')p}"
mk fns      "$(python3 -c 'print("fn f(){"*4000)')"
mk truncfn  "fn main() -> i64 {"
mk lonelyfn "fn"
mk bigenum  "enum E { $(python3 -c 'print("A,"*8000)') }"
mk bigmac   "macro_rules! m { ($(python3 -c 'print("$x:expr,"*1000)')) => {} }"
mk unstr    '"unterminated'
mk uncomment 'fn main()->i64{ /* nope'
mk badnum   'fn main()->i64{ 0xZZ }'
mk soup     'fn ! :: < > => | & * impl trait struct { ( [ , ; match'
echo "PASS: curated adversarial corpus ($n inputs)"

# 2. Random token soup: mutate a valid program by injecting random punctuation.
TOKS='( ) { } [ ] < > , ; : :: ! & * | + - / % = == fn let if else match -> => struct enum impl trait for while 0 1 x'
read -r -a TARR <<< "$TOKS"
seed=12345
rand() { seed=$(( (seed * 1103515245 + 12345) & 0x7fffffff )); echo $(( seed % $1 )); }
for it in $(seq 1 250); do
  len=$(( $(rand 40) + 1 )); prog=""
  for k in $(seq 1 "$len"); do prog+="${TARR[$(rand ${#TARR[@]})]} "; done
  printf '%s' "$prog" > "$TMP/r.kd"; check "$TMP/r.kd"
done
echo "PASS: random token soup (250 inputs)"

echo "total inputs fuzzed: $n ; crashes: $crashes"
[[ "$crashes" -eq 0 ]] || { echo "FAIL: $crashes compiler crashes on adversarial input"; exit 1; }
echo "ALL COMPILER-FUZZ TESTS PASSED (zero crashes)"
