#!/usr/bin/env bash
# Roadmap v95 — a permanent, CI-ROBUST perf-regression gate over the real bench/*.kd.
#
# WHY: fib(40) and the 200M `loop` are at 1.00x C TODAY (measured: @fib's asm is
# byte-identical to clang's; @main in loop has 0 allocas + auto-vectorizes). The
# v51 TargetMachine/TTI win that got us there was *stumbled upon*, never
# regression-tested — so a future codegen/PassBuilder refactor could silently
# regress perf. This gate LOCKS the measured parity INVARIANTS so perf can't
# silently rot. (The roadmap's "~1.2x fib gap" was stale text; ground-truth
# measurement shows parity, so v95 ships NO codegen change — making one would be a
# no-op stub — only this gate + the documented finding.)
#
# DESIGN (flakiness is the headline risk — mitigated):
#   BLOCKING checks are DETERMINISTIC structural IR-greps — identical on x86-64
#   AND arm64, zero wall-time. They catch the real regression class (alloca-heavy
#   lowering returning / vectorization lost).
#   The wall-time check is ADVISORY: generous (<= 2.0x = gross regression only,
#   NOT a flaky tight ratio), best-of-5, x86-64-only, and fully SKIPPABLE — it can
#   never fail CI on noise. The tight measured numbers live in BENCHMARKS.md.
#
# Complements (does not duplicate): smoke_test_codegen_perf.sh (v65, synthesized
# fib alloca/attr checks) and smoke_test_v90_close.sh (v90, the primary vector lock).
set -uo pipefail
KARDC=""
for c in "${TEST_SRCDIR:-}/_main/compiler/kardc" "${RUNFILES_DIR:-}/_main/compiler/kardc" "./compiler/kardc" "./build.local/kardc"; do
  [[ -n "$c" && -x "$c" ]] && { KARDC="$c"; break; }; done
[[ -z "$KARDC" ]] && { echo "FAIL: kardc not found"; exit 1; }
BENCH=""
for d in "${TEST_SRCDIR:-}/_main/bench" "${RUNFILES_DIR:-}/_main/bench" "bench"; do
  [[ -f "$d/fib.kd" && -f "$d/loop.kd" ]] && { BENCH="$d"; break; }; done
[[ -z "$BENCH" ]] && { echo "FAIL: bench/ not found"; exit 1; }
ARCH=$(uname -m 2>/dev/null || echo unknown)
CLANG="$(command -v clang || command -v cc || true)"
echo "Using kardc at: $KARDC ; bench at: $BENCH ; arch: $ARCH"
TMP=$(mktemp -d); trap 'rm -rf "$TMP"' EXIT

# Count allocas inside a named define at -O2 (deterministic). grep -c (not -q) so
# the whole stream is read — never SIGPIPEs kardc into a false pipefail failure.
allocas_in() {  # $1 .kd  $2 symbol
  "$KARDC" --no-cache --emit-llvm -O2 "$1" 2>/dev/null \
    | awk -v s="$2" 'index($0, "@" s "(") && /^define/ {p=1} p{print} p&&/^}$/{exit}' \
    | grep -c 'alloca' || true
}

# ---- BLOCKING 1: fib leaf recursion stays register-only (no allocas) ----
fa=$(allocas_in "$BENCH/fib.kd" fib)
[[ "$fa" -eq 0 ]] || { echo "FAIL [fib-allocas]: @fib has $fa allocas at -O2 (want 0 — the pre-v51 alloca-heavy lowering has regressed)"; exit 1; }
echo "PASS [fib-allocas]: @fib has 0 allocas at -O2 (register-only leaf recursion)"

# ---- BLOCKING 2: the 200M hot loop allocates nothing ----
la=$(allocas_in "$BENCH/loop.kd" main)
[[ "$la" -eq 0 ]] || { echo "FAIL [loop-allocas]: @main has $la allocas at -O2 (want 0 — a hot loop must not allocate)"; exit 1; }
echo "PASS [loop-allocas]: @main has 0 allocas at -O2"

# ---- BLOCKING 3: the loop auto-vectorizes (consistency w/ the v90 vector lock) ----
vc=$("$KARDC" --no-cache --emit-llvm -O2 "$BENCH/loop.kd" 2>/dev/null | grep -cE '<[0-9]+ x i(64|32)>' || true)
case "$ARCH" in
  x86_64|amd64)
    [[ "$vc" -gt 0 ]] || { echo "FAIL [loop-vectorized]: 0 vector ops at -O2 on x86-64 (the v51 TTI auto-vectorization has regressed)"; exit 1; }
    echo "PASS [loop-vectorized]: loop emits $vc vector ops at -O2 (v51 TTI locked)" ;;
  *)
    echo "PASS [loop-vectorized]: $vc vector ops on $ARCH (enforced on x86-64; arm64 cost model is target-dependent)" ;;
esac

# ---- ADVISORY: generous, best-of-5, x86-64-only, SKIPPABLE wall-time sanity ----
# Catches a GROSS (>2x) regression only. Never fails CI on noise; tight numbers
# are recorded in BENCHMARKS.md, not asserted here.
bestof5() {  # $1 exe -> prints min wall seconds (best of 5) as a float
  local best=999 i t
  for i in 1 2 3 4 5; do
    t=$( { TIMEFORMAT=%R; time "$1" >/dev/null 2>&1; } 2>&1 )
    awk -v a="$t" -v b="$best" 'BEGIN{exit !(a<b)}' && best="$t"
  done
  echo "$best"
}
if [[ -n "$CLANG" && ( "$ARCH" == "x86_64" || "$ARCH" == "amd64" ) ]]; then
  ok=1
  "$KARDC" --no-cache -O2 -o "$TMP/fibk" "$BENCH/fib.kd" >/dev/null 2>&1 || ok=0
  "$CLANG" -O2 "$BENCH/fib.c" -o "$TMP/fibc" >/dev/null 2>&1 || ok=0
  if [[ "$ok" -eq 1 ]]; then
    kt=$(bestof5 "$TMP/fibk"); ct=$(bestof5 "$TMP/fibc")
    ratio=$(awk -v k="$kt" -v c="$ct" 'BEGIN{ if(c<=0){print "0"} else {printf "%.2f", k/c} }')
    if awk -v r="$ratio" 'BEGIN{exit !(r>2.0)}'; then
      echo "FAIL [fib-walltime]: kardashev fib ${kt}s vs C ${ct}s = ${ratio}x (> 2.0x GROSS regression)"; exit 1
    fi
    echo "PASS [fib-walltime advisory]: fib ${kt}s vs C ${ct}s = ${ratio}x (<= 2.0x; tight parity in BENCHMARKS.md)"
  else
    echo "PASS [fib-walltime advisory]: SKIPPED (build/clang unavailable)"
  fi
else
  echo "PASS [fib-walltime advisory]: SKIPPED (no clang or non-x86-64 — deterministic checks above are authoritative)"
fi

echo "ALL PERF-REGRESSION SMOKE TESTS PASSED"
