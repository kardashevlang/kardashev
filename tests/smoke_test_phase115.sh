#!/usr/bin/env bash
# Phase 115 (Roadmap v20 — "toward a real bootstrap"): the self-hosted compiler
# (examples/selfhost/llvmgen.kd) now emits REAL textual LLVM IR — a compilable
# `.ll` artifact — instead of running an in-process stack VM. The full chain:
#   kardashev source (llvmgen.kd) --host kardc--> native self-hosted compiler
#     --run--> emits `.ll` for `fn f(a,b){ let s=a+b; if a<b { s*2 } else { s+1 } }`
#       --clang--> native binary --run--> exit code = f(3,4) = 14.
# DIFFERENTIAL GATE: the same function compiled by the HOST kardc must produce the
# SAME result, so the self-hosted compiler's codegen agrees with the host's.
# (Skips cleanly if clang is unavailable — the whole point is to compile the IR.)
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
    "${TEST_SRCDIR:-}/_main/examples/selfhost/llvmgen.kd" "${TEST_SRCDIR:-}/kardashev/examples/selfhost/llvmgen.kd" \
    "${RUNFILES_DIR:-}/_main/examples/selfhost/llvmgen.kd" "${RUNFILES_DIR:-}/kardashev/examples/selfhost/llvmgen.kd" \
    "examples/selfhost/llvmgen.kd"; do
    if [[ -f "$cand" ]]; then SRC="$cand"; break; fi
done
[[ -z "$SRC" ]] && { echo "FAIL: examples/selfhost/llvmgen.kd not found"; exit 1; }
CLANG="$(command -v clang || true)"
[[ -z "$CLANG" ]] && { echo "PASS [phase115]: SKIPPED (no clang to compile the emitted IR)"; exit 0; }
echo "Using kardc at: $KARDC ; clang at: $CLANG"
TMP=$(mktemp -d); trap 'rm -rf "$TMP"' EXIT

# 1. Build the self-hosted compiler (host kardc -> native) and run it to emit IR.
"$KARDC" --no-cache -o "$TMP/selfcc" "$SRC" >/dev/null 2>&1 || { echo "FAIL [phase115]: self-hosted compiler did not build"; exit 1; }
"$TMP/selfcc" > "$TMP/out.ll" 2>/dev/null || { echo "FAIL [phase115]: self-hosted compiler did not run"; exit 1; }

# 2. Sanity-check the emitted module is real LLVM IR (not stack-VM bytecode).
grep -q 'define i64 @f(' "$TMP/out.ll" || { echo "FAIL [phase115]: no 'define i64 @f' in emitted IR"; cat "$TMP/out.ll"; exit 1; }
grep -q 'select i1' "$TMP/out.ll"     || { echo "FAIL [phase115]: no 'select' (if-lowering) in emitted IR"; cat "$TMP/out.ll"; exit 1; }
grep -q 'define i64 @main(' "$TMP/out.ll" || { echo "FAIL [phase115]: no 'define i64 @main' in emitted IR"; exit 1; }
echo "PASS [emit]: self-hosted compiler emitted valid-shaped LLVM IR ($(wc -l < "$TMP/out.ll") lines)"

# 3. Compile the emitted IR with clang -> native, and run it.
"$CLANG" "$TMP/out.ll" -o "$TMP/prog" 2>/dev/null || { echo "FAIL [phase115]: clang could not compile the emitted IR"; cat "$TMP/out.ll"; exit 1; }
"$TMP/prog" >/dev/null 2>&1; r_self=$?
[[ "$r_self" -eq 14 ]] || { echo "FAIL [phase115]: emitted-IR native binary exit $r_self (want 14)"; exit 1; }
echo "PASS [native]: clang-compiled emitted IR runs natively, exit $r_self (= f(3,4))"

# 4. DIFFERENTIAL GATE: the host compiler on the equivalent kardashev program.
cat > "$TMP/host.kd" <<'EOF'
fn f(a: i64, b: i64) -> i64 { let s = a + b ; if a < b { s * 2 } else { s + 1 } }
fn main() -> i64 { f(3, 4) }
EOF
"$KARDC" --no-cache -o "$TMP/host" "$TMP/host.kd" >/dev/null 2>&1 || { echo "FAIL [phase115]: host program did not build"; exit 1; }
"$TMP/host" >/dev/null 2>&1; r_host=$?
[[ "$r_self" -eq "$r_host" ]] || { echo "FAIL [phase115]: self-hosted result $r_self != host result $r_host"; exit 1; }
echo "PASS [differential]: self-hosted IR result == host compiler result ($r_self == $r_host)"

echo "ALL PHASE 115 SMOKE TESTS PASSED"
