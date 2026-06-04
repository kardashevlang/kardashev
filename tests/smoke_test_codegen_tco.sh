#!/usr/bin/env bash
# Roadmap v106 — tail-call lowering + monotone bounds-check elision, LOCKED.
#
# Like v95 (the fib perf gate), this version ships NO codegen change: live probing
# proved the DEFAULT build (-O2) already (a) lowers self-tail-recursion to a loop /
# closed form so deep recursion does not blow the stack, and (b) elides the
# per-iteration bounds check in monotone index loops where that is sound. Making a
# codegen change would be a no-op stub or a regression risk (the v95 lesson). So
# v106 ships this PERMANENT gate that LOCKS the wins (so a future PassBuilder /
# codegen refactor can't silently regress them), plus honest deferrals.
#
# All BLOCKING checks are deterministic — structural IR-greps (identical on x86-64
# AND arm64, since the relevant transforms [TailCallElim/IndVars/SCEV] are
# target-independent) and a runtime exit-code proof. The single target-DEPENDENT
# check (vectorization, a cost-model decision) is x86-64-enforce / arm64-soft per
# the v90 lesson. Zero wall-time anywhere.
#
# DEFERRED (honest): TCO at explicit `-O0` (the user opted out of optimization);
# a `become`/`musttail` language guarantee (vs the optimizer's best-effort);
# general/mutual tail-call elimination guarantee; the vec_get null-data branch that
# blocks by-value-Vec-loop vectorization (correctness-neutral + off the benchmark
# surface — the by-ref vec_get_ref path already vectorizes).
set -uo pipefail
KARDC=""
for c in "${TEST_SRCDIR:-}/_main/compiler/kardc" "${RUNFILES_DIR:-}/_main/compiler/kardc" "./compiler/kardc" "./build.local/kardc"; do
  [[ -n "$c" && -x "$c" ]] && { KARDC="$c"; break; }; done
[[ -z "$KARDC" ]] && { echo "FAIL: kardc not found"; exit 1; }
ARCH=$(uname -m 2>/dev/null || echo unknown)
echo "Using kardc at: $KARDC ; arch: $ARCH"
TMP=$(mktemp -d); trap 'rm -rf "$TMP"' EXIT

# Extract a named define's body from -O2 IR. grep -c (not -q) so the whole stream
# is consumed — never SIGPIPEs kardc into a false pipefail failure.
body() {  # $1 .kd  $2 symbol
  "$KARDC" --no-cache --emit-llvm -O2 "$1" 2>/dev/null \
    | awk -v s="$2" 'index($0, "@" s "(") && /^define/ {p=1} p{print} p&&/^}$/{exit}'
}

# ---- fixture 1: self-tail-recursive sum (every fixture uses `fn main() -> i64`
#      to avoid the unit-main AOT wrapper crash) ----
cat > "$TMP/tco.kd" <<'EOF'
fn sum(n: i64, acc: i64) -> i64 { if n == 0 { acc } else { sum(n - 1, acc + n) } }
fn main() -> i64 ! { io } { print(sum(1000000, 0)); 0 }
EOF

# BLOCKING 1: the self-tail-recursive call is gone at -O2 (lowered to a loop or
# solved to closed form) — no surviving `call @sum`. Target-independent.
sc=$(body "$TMP/tco.kd" sum | grep -c 'call i64 @sum' || true)
[[ "$sc" -eq 0 ]] || { echo "FAIL [tco-lowered]: @sum still has $sc recursive 'call @sum' at -O2 (TailCallElim/IndVars regressed → stack-growth risk)"; exit 1; }
echo "PASS [tco-lowered]: self-tail-recursion eliminated at -O2 (0 surviving recursive calls)"

# BLOCKING 2: runtime no-overflow proof + correctness oracle — sum(1e6,0) at the
# default opt level must complete (exit != 139 SIGSEGV) and print the right total.
"$KARDC" --no-cache -o "$TMP/tco" "$TMP/tco.kd" >/dev/null 2>&1 || { echo "FAIL [tco-runtime]: AOT build failed"; exit 1; }
out=$("$TMP/tco" 2>/dev/null); rc=$?
[[ "$rc" -ne 139 ]] || { echo "FAIL [tco-runtime]: sum(1e6,0) stack-overflowed (exit 139) — TCO not applied in the default build"; exit 1; }
[[ "$out" == "500000500000" ]] || { echo "FAIL [tco-runtime]: wrong result '$out' (want 500000500000) — codegen changed the answer"; exit 1; }
echo "PASS [tco-runtime]: sum(1_000_000,0) completes (exit $rc, no overflow) = 500000500000"

# ---- fixture 2: monotone array loop with a statically-provable bound ----
cat > "$TMP/arr.kd" <<'EOF'
fn work(n: i64) -> i64 { let mut a: [i64; 256] = [0; 256]; let mut i = 0; while i < n { a[i] = i * 2; i = i + 1; } let mut t = 0; let mut j = 0; while j < n { t = t + a[j]; j = j + 1; } t }
fn main() -> i64 ! { io } { print(work(200)); 0 }
EOF
ab=$(body "$TMP/arr.kd" main)
po=$(grep -c 'panic_oob' <<<"$ab" || true); ip=$(grep -cE 'idx\.panic|idx_panic' <<<"$ab" || true)
[[ "$po" -eq 0 && "$ip" -eq 0 ]] || { echo "FAIL [array-boundselide]: monotone provable-bound loop kept bounds checks (panic_oob=$po idx.panic=$ip; want 0/0)"; exit 1; }
echo "PASS [array-boundselide]: monotone provable-bound array loop has 0 bounds checks (SCEV/Phase196 lock)"

# SOFT(arm64)/BLOCKING(x86-64): the elided array loop still vectorizes (v51/v90 consistency).
vc=$(grep -cE '<[0-9]+ x i(64|32)>' <<<"$ab" || true)
case "$ARCH" in
  x86_64|amd64) [[ "$vc" -gt 0 ]] || { echo "FAIL [array-vectorized]: 0 vector ops at -O2 on x86-64 (v51 TTI regressed)"; exit 1; }
    echo "PASS [array-vectorized]: $vc vector ops at -O2 (v51 TTI locked)" ;;
  *) echo "PASS [array-vectorized]: $vc vector ops on $ARCH (enforced on x86-64; arm64 cost model target-dependent)" ;;
esac

# ---- fixture 3: vec_get monotone loop — range+sign+panic checks SCEV-elided ----
cat > "$TMP/vg.kd" <<'EOF'
fn sumvec(v: &Vec<i64>) -> i64 ! { alloc } { let mut t = 0; let mut i = 0; while i < vec_len(v) { t = t + vec_get(v, i); i = i + 1; } t }
fn main() -> i64 ! { io, alloc } { let mut v: Vec<i64> = vec_new(); let mut i = 0; while i < 100 { vec_push(&mut v, i); i = i + 1; } print(sumvec(&v)); 0 }
EOF
vb=$(body "$TMP/vg.kd" sumvec)
slt=$(grep -c 'icmp slt' <<<"$vb" || true); sge=$(grep -c 'icmp sge' <<<"$vb" || true); pn=$(grep -c 'panic' <<<"$vb" || true)
[[ "$slt" -eq 0 && "$sge" -eq 0 && "$pn" -eq 0 ]] || { echo "FAIL [vec-boundselide]: vec_get monotone loop kept checks (icmp slt=$slt sge=$sge panic=$pn; want 0/0/0)"; exit 1; }
echo "PASS [vec-boundselide]: vec_get monotone loop range/sign/panic checks SCEV-elided (0/0/0)"

# Correctness oracle for the loops (a codegen change must never change results).
"$KARDC" --no-cache -o "$TMP/arr" "$TMP/arr.kd" >/dev/null 2>&1 && a=$("$TMP/arr" 2>/dev/null) || a=ERR
"$KARDC" --no-cache -o "$TMP/vg" "$TMP/vg.kd" >/dev/null 2>&1 && g=$("$TMP/vg" 2>/dev/null) || g=ERR
[[ "$a" == "39800" && "$g" == "4950" ]] || { echo "FAIL [loop-correctness]: work(200)='$a' (want 39800) / sumvec='$g' (want 4950)"; exit 1; }
echo "PASS [loop-correctness]: work(200)=39800, sumvec(0..100)=4950 (oracle)"

echo "ALL v106 (codegen TCO + bounds-elision lock) SMOKE TESTS PASSED"
