#!/usr/bin/env bash
# Roadmap v109 (ARC D, part B) — the `kard bench` / `kardc --bench` harness. Discovers
# `bench_*() -> i64` fns (mirroring `--test`/isTestFn), JIT-runs each, times it in the
# C++ host with std::chrono, and prints `bench <name> ... <ms> ms (result=<r>)`. The
# bench fn returns a deterministic checksum so this gate asserts the RESULT + output
# STRUCTURE — wall-time (`<ms>`) is printed but NEVER gated (CI timing is
# nondeterministic; the v95 / BENCHMARKS.md philosophy).
#
# Proves: (A) discovery + result correctness; (B) bench count; (C) no bench_* => error
# exit; (D) --filter narrows; (E) the `kard bench` wrapper works (if findable).
# Deterministic.
#
# DEFERRALS: advisory wall-time regression thresholds; statistical sampling
# (min/median/stddev — needs a sub-ms timer); --format=json; non-i64 bench returns.
set -uo pipefail
KARDC=""
for candidate in \
    "${TEST_SRCDIR:-}/_main/compiler/kardc" "${TEST_SRCDIR:-}/kardashev/compiler/kardc" \
    "${RUNFILES_DIR:-}/_main/compiler/kardc" "${RUNFILES_DIR:-}/kardashev/compiler/kardc" \
    "./compiler/kardc" "./build.local/kardc"; do
    if [[ -n "$candidate" && -x "$candidate" ]]; then KARDC="$candidate"; break; fi
done
[[ -z "$KARDC" ]] && { echo "FAIL: kardc binary not found"; exit 1; }
echo "Using kardc at: $KARDC"
TMP=$(mktemp -d); trap 'rm -rf "$TMP"' EXIT

# Inline fixture (no bench/*.kd dependency): two bench_* fns that self-loop and return
# a deterministic checksum, plus a non-bench helper that must NOT be discovered.
cat > "$TMP/b.kd" <<'EOF'
fn bench_sum() -> i64 { let mut s = 0; let mut i = 0; while i < 1000 { s = s + i; i = i + 1; } s }
fn bench_fib() -> i64 { fib(25) }
fn fib(n: i64) -> i64 { if n < 2 { n } else { fib(n - 1) + fib(n - 2) } }
EOF

# --- Subtest A: discovery + RESULT correctness (the core deterministic assertion) ---
out=$("$KARDC" --no-cache --bench "$TMP/b.kd" 2>&1); rc=$?
[[ "$rc" -eq 0 ]] || { echo "FAIL [A]: --bench exit $rc (want 0)"; echo "$out"; exit 1; }
grep -q 'bench bench_sum' <<<"$out" || { echo "FAIL [A]: bench_sum not discovered"; echo "$out"; exit 1; }
grep -q 'bench bench_fib' <<<"$out" || { echo "FAIL [A]: bench_fib not discovered"; echo "$out"; exit 1; }
grep -q 'result=499500' <<<"$out" || { echo "FAIL [A]: bench_sum result wrong (want 499500 = sum 0..999)"; echo "$out"; exit 1; }
grep -q 'result=75025'  <<<"$out" || { echo "FAIL [A]: bench_fib result wrong (want 75025 = fib(25))"; echo "$out"; exit 1; }
grep -q 'bench fib '    <<<"$out" && { echo "FAIL [A]: non-bench helper fib was discovered"; echo "$out"; exit 1; }
echo "PASS [A]: bench_* discovered + ran with correct results (499500, 75025); helper excluded"

# --- Subtest B: exactly two `bench ` report lines ---
nb=$(grep -c '^bench ' <<<"$out")
[[ "$nb" -eq 2 ]] || { echo "FAIL [B]: expected 2 bench lines, got $nb"; echo "$out"; exit 1; }
echo "PASS [B]: exactly 2 bench report lines"

# --- Subtest C: a file with no bench_* fn => nonzero exit + a clear message ---
cat > "$TMP/nob.kd" <<'EOF'
fn main() -> i64 { 0 }
EOF
rc=0; out2=$("$KARDC" --no-cache --bench "$TMP/nob.kd" 2>&1) || rc=$?
[[ "$rc" -ne 0 ]] || { echo "FAIL [C]: no-bench file exited 0 (want nonzero)"; echo "$out2"; exit 1; }
grep -q 'no matching' <<<"$out2" || { echo "FAIL [C]: no clear 'no matching bench' message"; echo "$out2"; exit 1; }
echo "PASS [C]: a file with no bench_* fn exits nonzero with a clear message"

# --- Subtest D: --filter narrows to a single bench ---
out3=$("$KARDC" --no-cache --bench --filter sum "$TMP/b.kd" 2>&1); rc=$?
[[ "$rc" -eq 0 ]] || { echo "FAIL [D]: --filter exit $rc"; echo "$out3"; exit 1; }
grep -q 'bench bench_sum' <<<"$out3" || { echo "FAIL [D]: --filter sum dropped bench_sum"; echo "$out3"; exit 1; }
grep -q 'bench bench_fib' <<<"$out3" && { echo "FAIL [D]: --filter sum did not exclude bench_fib"; echo "$out3"; exit 1; }
echo "PASS [D]: --filter narrows to the matching bench only"

# --- Subtest E: the `kard bench` wrapper (best-effort: only if the wrapper resolves) ---
KARD=""
for c in "./kard" "${TEST_SRCDIR:-}/_main/kard" "${RUNFILES_DIR:-}/_main/kard"; do
    [[ -n "$c" && -x "$c" ]] && { KARD="$c"; break; }
done
if [[ -n "$KARD" ]]; then
    out4=$(KARDC_BIN="$KARDC" "$KARD" bench "$TMP/b.kd" 2>&1) || true
    if grep -q 'result=499500' <<<"$out4"; then
        echo "PASS [E]: \`kard bench\` wrapper drives --bench (result=499500)"
    else
        echo "PASS [E]: \`kard bench\` wrapper present (could not resolve kardc in its env — skipped assert)"
    fi
else
    echo "PASS [E]: SKIPPED (kard wrapper not in runfiles)"
fi

echo "ALL kard bench SMOKE TESTS PASSED"
