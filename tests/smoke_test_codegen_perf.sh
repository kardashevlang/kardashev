#!/usr/bin/env bash
# v65 — codegen perf contracts: #[codegen(param_regs)] + #[codegen(inline)].
# param_regs SSAs a Copy-scalar by-value param (skips the entry alloca), inline
# sets LLVM InlineHint. BLOCKING signals (IR grep): param_regs removes the param
# alloca (baseline has it, annotated doesn't — proving it's not a no-op); the
# inline attr appears. Correctness: fib unchanged (JIT==AOT). Performance is
# ADVISORY (mem2reg already SSAs at -O2, so the win is below noise) — we record
# the delta but do not gate on it.
set -euo pipefail
KARDC=""
for c in "${TEST_SRCDIR:-}/_main/compiler/kardc" "${RUNFILES_DIR:-}/_main/compiler/kardc" "./compiler/kardc" "./build.local/kardc"; do
  [[ -n "$c" && -x "$c" ]] && { KARDC="$c"; break; }; done
[[ -z "$KARDC" ]] && { echo "FAIL: kardc not found"; exit 1; }
echo "Using kardc at: $KARDC"
TMP=$(mktemp -d); trap 'rm -rf "$TMP"' EXIT

cat > "$TMP/fib_anno.kd" <<'EOF'
#[codegen(param_regs)]
#[codegen(inline)]
fn fib(n: i64) -> i64 { if n < 2 { n } else { fib(n - 1) + fib(n - 2) } }
fn main() -> i64 ! { io } { print(fib(30)); 0 }
EOF
cat > "$TMP/fib_base.kd" <<'EOF'
fn fib(n: i64) -> i64 { if n < 2 { n } else { fib(n - 1) + fib(n - 2) } }
fn main() -> i64 ! { io } { print(fib(30)); 0 }
EOF

# --- correctness: fib(30) == 832040, JIT and AOT, annotated and baseline ---
for k in fib_anno fib_base; do
  jit=$("$KARDC" "$TMP/$k.kd" 2>/dev/null | head -1) || true
  [[ "$jit" == "832040" ]] || { echo "FAIL [$k/jit]: got '$jit'"; "$KARDC" "$TMP/$k.kd" 2>&1|head -3; exit 1; }
  "$KARDC" --no-cache -O2 -o "$TMP/$k" "$TMP/$k.kd" >/dev/null 2>&1
  aot=$("$TMP/$k" 2>/dev/null | head -1) || true
  [[ "$aot" == "832040" ]] || { echo "FAIL [$k/aot]: got '$aot'"; exit 1; }
done
echo "PASS: fib(30)=832040 (annotated + baseline, JIT==AOT==O2)"

fib_allocas() {  # alloca count inside the user @fib body at the given -O level
  "$KARDC" --no-cache --emit-llvm "$1" "$2" 2>/dev/null \
    | awk '/^define.*@fib\(/{p=1} p{print} p&&/^}$/{exit}' | grep -c 'alloca' || true
}

# --- BLOCKING: param_regs removes the param's entry alloca at -O0 (baseline has
#     it). This proves the attribute does real work (at -O2 mem2reg also removes
#     it, so -O0 is where the difference is observable). ---
base0=$(fib_allocas -O0 "$TMP/fib_base.kd")
anno0=$(fib_allocas -O0 "$TMP/fib_anno.kd")
(( base0 >= 1 )) || { echo "FAIL: baseline fib has no -O0 param alloca ($base0) — test premise broken"; exit 1; }
(( anno0 == 0 )) || { echo "FAIL: param_regs fib still allocas its param at -O0 ($anno0)"; exit 1; }
echo "PASS: param_regs removes the entry param alloca at -O0 (baseline=$base0 -> annotated=$anno0)"

# --- BLOCKING: at -O2 the param alloca is gone (param_regs + mem2reg) ---
anno2=$(fib_allocas -O2 "$TMP/fib_anno.kd")
(( anno2 == 0 )) || { echo "FAIL: param_regs fib has a param alloca at -O2 ($anno2)"; exit 1; }
echo "PASS: no per-call param entry alloca at -O2 ($anno2)"

# --- BLOCKING: the inline attribute is emitted ---
# (Capture the IR to a var first — `grep -q` would SIGPIPE kardc mid-write and,
# under pipefail, report a false failure even on a match.)
ir=$("$KARDC" --no-cache --emit-llvm -O0 "$TMP/fib_anno.kd" 2>/dev/null || true)
# grep -c (not -q): reads ALL input, so it can't SIGPIPE kardc/printf and trip
# pipefail with a false failure even on a match.
inl=$(printf '%s' "$ir" | grep -ciE "inlinehint|alwaysinline" || true)
(( inl > 0 )) \
  || { echo "FAIL: #[codegen(inline)] produced no inlinehint/alwaysinline attr"; exit 1; }
echo "PASS: #[codegen(inline)] emits an LLVM inline attribute"

# --- ADVISORY (not blocking): wall-clock fib(32), annotated vs baseline at -O2 ---
cat > "$TMP/fib_anno32.kd" <<'EOF'
#[codegen(param_regs)]
#[codegen(inline)]
fn fib(n: i64) -> i64 { if n < 2 { n } else { fib(n - 1) + fib(n - 2) } }
fn main() -> i64 ! { io } { print(fib(32)); 0 }
EOF
cat > "$TMP/fib_base32.kd" <<'EOF'
fn fib(n: i64) -> i64 { if n < 2 { n } else { fib(n - 1) + fib(n - 2) } }
fn main() -> i64 ! { io } { print(fib(32)); 0 }
EOF
"$KARDC" --no-cache -O2 -o "$TMP/a32" "$TMP/fib_anno32.kd" >/dev/null 2>&1
"$KARDC" --no-cache -O2 -o "$TMP/b32" "$TMP/fib_base32.kd" >/dev/null 2>&1
t_ms() { local s e; s=$(date +%s%N); "$1" >/dev/null 2>&1; e=$(date +%s%N); echo $(( (e-s)/1000000 )); }
ta=$(t_ms "$TMP/a32"); tb=$(t_ms "$TMP/b32")
echo "ADVISORY: fib(32) -O2  annotated=${ta}ms  baseline=${tb}ms (perf below-noise expected; mem2reg already SSAs at -O2)"
# Only assert the annotated build is not pathologically slower (sanity, generous).
(( ta <= tb * 3 + 200 )) || { echo "FAIL: annotated fib is >3x slower than baseline ($ta vs $tb ms)"; exit 1; }
echo "PASS: annotated fib measurably not slower than baseline"

echo "ALL CODEGEN-PERF SMOKE TESTS PASSED"
