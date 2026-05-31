#!/usr/bin/env bash
# v18 Phase 109: a UNIT-returning async fn (`async fn f(..) ! { .. } { stmt; }`,
# no `-> T`) drove the compiler to a SIGTRAP (rc 133) when its future was
# consumed: block_on / .await / spawn+join all read the Poll<T> value slot as T,
# and for the unit result T maps to LLVM void — `load void` (and a named void
# call) is invalid IR, which crashed the in-process JIT. Fixed: a void result
# yields the unit placeholder (i64 0) without loading, and the block_on call is
# left unnamed. Verifies all three drivers compile AND run (JIT + AOT).
set -euo pipefail
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

# 1. block_on a unit-result future.
cat > "$TMP/bo.kd" <<'EOF'
async fn noop(n: i64) ! { io } { print(n); }
fn main() -> i64 ! { io } { block_on(noop(7)); 0 }
EOF
got=$("$KARDC" "$TMP/bo.kd" 2>/dev/null); rc=$?
[[ "$rc" -eq 0 && "$got" == $'7\n0' ]] || { echo "FAIL [block_on/jit]: rc=$rc out='$got'"; exit 1; }
"$KARDC" --no-cache -o "$TMP/bo" "$TMP/bo.kd" >/dev/null 2>&1
set +e; aout=$("$TMP/bo"); arc=$?; set -e
[[ "$arc" -eq 0 && "$aout" == "7" ]] || { echo "FAIL [block_on/aot]: rc=$arc out='$aout'"; exit 1; }
echo "PASS [block_on]: block_on of a unit-result future (was SIGTRAP) — JIT + AOT"

# 2. .await a unit-result future inside another async fn.
cat > "$TMP/aw.kd" <<'EOF'
async fn leaf(n: i64) ! { io } { print(n); }
async fn outer() -> i64 ! { io } { leaf(3).await; leaf(4).await; 99 }
fn main() -> i64 ! { io } { block_on(outer()) }
EOF
got=$("$KARDC" "$TMP/aw.kd" 2>/dev/null); rc=$?
[[ "$rc" -eq 0 && "$got" == $'3\n4\n99' ]] || { echo "FAIL [await/jit]: rc=$rc out='$got'"; exit 1; }
"$KARDC" --no-cache -o "$TMP/aw" "$TMP/aw.kd" >/dev/null 2>&1
set +e; aout=$("$TMP/aw"); arc=$?; set -e
[[ "$arc" -eq 99 && "$aout" == $'3\n4' ]] || { echo "FAIL [await/aot]: rc=$arc out='$aout'"; exit 1; }
echo "PASS [await]: .await a unit-result future (was SIGTRAP) — JIT + AOT"

# 3. spawn + join a unit-result future.
cat > "$TMP/sj.kd" <<'EOF'
async fn noop(n: i64) ! { io } { print(n); }
fn main() -> i64 ! { io } { let h = spawn(noop(5)); join(h); 0 }
EOF
got=$("$KARDC" "$TMP/sj.kd" 2>/dev/null); rc=$?
[[ "$rc" -eq 0 && "$got" == $'5\n0' ]] || { echo "FAIL [spawn-join/jit]: rc=$rc out='$got'"; exit 1; }
echo "PASS [spawn-join]: spawn + join a unit-result future — JIT"

echo "ALL UNIT-ASYNC SMOKE TESTS PASSED"
