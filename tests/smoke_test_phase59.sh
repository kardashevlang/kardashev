#!/usr/bin/env bash
# Phase 59 smoke test (Roadmap v10): const-generic FUNCTIONS + compile-time
# dimension unification.
#   1. `fn dot<const N>(a: [i64; N], b: [i64; N]) -> i64` — N is inferred from
#      the argument array lengths, usable as a VALUE in the body (`while i < N`),
#      and the fn is monomorphized per size (`@dot__c3` over `[3 x i64]` and
#      `@dot__c2` over `[2 x i64]` are distinct). JIT + AOT.
#   2. A dimension MISMATCH (`dot([i64;3], [i64;2])`) is a compile error
#      (N can't be both 3 and 2).
#   3. A const param that appears in no argument array type can't be inferred —
#      a clear error, not a silent 0.
set -euo pipefail

KARDC=""
for candidate in \
    "${TEST_SRCDIR:-}/_main/compiler/kardc" \
    "${TEST_SRCDIR:-}/kardashev/compiler/kardc" \
    "${RUNFILES_DIR:-}/_main/compiler/kardc" \
    "${RUNFILES_DIR:-}/kardashev/compiler/kardc" \
    "./compiler/kardc" \
    "./build.local/kardc"; do
    if [[ -n "$candidate" && -x "$candidate" ]]; then KARDC="$candidate"; break; fi
done
[[ -z "$KARDC" ]] && { echo "FAIL: kardc binary not found"; exit 1; }
echo "Using kardc at: $KARDC"

TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

rejects() { # name file needle
    local n=$1 f=$2 needle=$3 out
    set +e; out=$("$KARDC" "$f" 2>&1); set -e
    if "$KARDC" "$f" >/dev/null 2>&1; then
        echo "FAIL [$n]: expected REJECTION, but it compiled"; exit 1
    fi
    if [[ -n "$needle" ]] && ! grep -qi "$needle" <<<"$out"; then
        echo "FAIL [$n]: rejected but missing '$needle'; got: $out"; exit 1
    fi
    echo "PASS [$n]: rejected as expected"
}

# 1. acceptance: dot<const N>, N inferred + used as a value, two monomorphs.
cat > "$TMP/dot.kd" <<'EOF'
fn dot<const N: i64>(a: [i64; N], b: [i64; N]) -> i64 {
    let mut acc = 0;
    let mut i = 0;
    while i < N {
        acc = acc + a[i] * b[i];
        i = i + 1;
    }
    acc
}
fn main() -> i64 {
    let x: [i64; 3] = [1, 2, 3];
    let y: [i64; 3] = [4, 5, 6];
    let p: [i64; 2] = [10, 20];
    let q: [i64; 2] = [3, 4];
    dot(x, y) + dot(p, q)
}
EOF
jit=$("$KARDC" "$TMP/dot.kd" 2>/dev/null | tail -1)
[[ "$jit" == "142" ]] || { echo "FAIL [dot/jit]: expected 142 got '$jit'"; exit 1; }
ll=$("$KARDC" --emit-llvm "$TMP/dot.kd" 2>/dev/null)
grep -q "define i64 @dot__c3(\[3 x i64\]" <<<"$ll" || {
    echo "FAIL [dot/llvm]: missing distinct @dot__c3 over [3 x i64]"; exit 1; }
grep -q "define i64 @dot__c2(\[2 x i64\]" <<<"$ll" || {
    echo "FAIL [dot/llvm]: missing distinct @dot__c2 over [2 x i64]"; exit 1; }
"$KARDC" --no-cache -o "$TMP/dot" "$TMP/dot.kd" >/dev/null 2>&1
set +e; "$TMP/dot" >/dev/null; rc=$?; set -e
[[ "$rc" -eq 142 ]] || { echo "FAIL [dot/aot]: exit $rc expected 142"; exit 1; }
echo "PASS [const-generic-fn]: dot<N> inferred + N-as-value, @dot__c3/@dot__c2 distinct, JIT 142, AOT 142"

# 2. dimension mismatch is a compile error.
cat > "$TMP/mism.kd" <<'EOF'
fn dot<const N: i64>(a: [i64; N], b: [i64; N]) -> i64 { a[0] }
fn main() -> i64 {
    let x: [i64; 3] = [1, 2, 3];
    let y: [i64; 2] = [4, 5];
    dot(x, y)
}
EOF
rejects dim-mismatch "$TMP/mism.kd" "dimension mismatch"

# 3. a const param not appearing in any argument array type can't be inferred.
printf 'fn mk<const N: i64>() -> i64 { N }\nfn main() -> i64 { mk() }\n' > "$TMP/noinfer.kd"
rejects cannot-infer "$TMP/noinfer.kd" "cannot infer const generic parameter"

echo "ALL PHASE 59 SMOKE TESTS PASSED"
