#!/usr/bin/env bash
# Phase 62 capstone (Roadmap v10): a fixed-size linear-algebra library in
# kardashev — Matrix<const R, const C> with compile-time-checked shapes.
#   1. Builds the real examples/matrix/main.kd: transpose() -> Matrix<C,R> and a
#      dimension-checked matmul Matrix<R,K> x Matrix<K,C> -> Matrix<R,C>. JIT +
#      AOT both compute 21.
#   2. A DIMENSION MISMATCH is a COMPILE error, not a runtime crash:
#      matmul(Matrix<2,3>, Matrix<4,5>) — the shared inner dim K can't be both 3
#      and 4; assigning a transpose result Matrix<3,2> to Matrix<2,3> is a type
#      error.
#   3. Array-repeat `[value; N]` (the matrix's `[[0; C]; R]`) works with a
#      symbolic const-generic length.
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

# Locate the capstone source (Bazel runfiles vs. local tree).
SRC=""
for cand in \
    "${TEST_SRCDIR:-}/_main/examples/matrix/main.kd" \
    "${TEST_SRCDIR:-}/kardashev/examples/matrix/main.kd" \
    "${RUNFILES_DIR:-}/_main/examples/matrix/main.kd" \
    "${RUNFILES_DIR:-}/kardashev/examples/matrix/main.kd" \
    "examples/matrix/main.kd"; do
    if [[ -f "$cand" ]]; then SRC="$cand"; break; fi
done
[[ -z "$SRC" ]] && { echo "FAIL: examples/matrix/main.kd not found"; exit 1; }

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

# 1. the capstone computes 21 (transpose 6 + matmul 4 + 11), JIT + AOT.
jit=$("$KARDC" "$SRC" 2>/dev/null | tail -1)
[[ "$jit" == "21" ]] || { echo "FAIL [matrix/jit]: expected 21 got '$jit'"; exit 1; }
"$KARDC" --no-cache -o "$TMP/matrix" "$SRC" >/dev/null 2>&1
set +e; "$TMP/matrix" >/dev/null; rc=$?; set -e
[[ "$rc" -eq 21 ]] || { echo "FAIL [matrix/aot]: exit $rc expected 21"; exit 1; }
echo "PASS [matrix-capstone]: transpose + dimension-checked matmul, JIT 21, AOT 21"

# 2a. dimension mismatch on the shared inner dim K.
cat > "$TMP/mism.kd" <<'EOF'
struct Matrix<const R: i64, const C: i64> { data: [[i64; C]; R] }
fn matmul<const R: i64, const K: i64, const C: i64>(
    a: &Matrix<R, K>, b: &Matrix<K, C>) -> Matrix<R, C> {
    let out: [[i64; C]; R] = [[0; C]; R];
    Matrix { data: out }
}
fn main() -> i64 {
    let a: Matrix<2, 3> = Matrix { data: [[1, 2, 3], [4, 5, 6]] };
    let b: Matrix<4, 5> = Matrix {
        data: [[1,1,1,1,1],[2,2,2,2,2],[3,3,3,3,3],[4,4,4,4,4]] };
    let c = matmul(&a, &b);
    0
}
EOF
rejects dim-mismatch-matmul "$TMP/mism.kd" "dimension mismatch"

# 2b. transpose Matrix<2,3> -> Matrix<3,2> can't be assigned to Matrix<2,3>.
cat > "$TMP/tbad.kd" <<'EOF'
struct Matrix<const R: i64, const C: i64> { data: [[i64; C]; R] }
fn transpose<const R: i64, const C: i64>(m: &Matrix<R, C>) -> Matrix<C, R> {
    let out: [[i64; R]; C] = [[0; R]; C];
    Matrix { data: out }
}
fn main() -> i64 {
    let a: Matrix<2, 3> = Matrix { data: [[1,2,3],[4,5,6]] };
    let bad: Matrix<2, 3> = transpose(&a);
    0
}
EOF
rejects transpose-shape-mismatch "$TMP/tbad.kd" "annotated type"

# 3. array-repeat `[value; N]` — concrete here; the matrix capstone above
# exercises the SYMBOLIC form (`[[0; C]; R]` with const-generic R/C).
cat > "$TMP/rep.kd" <<'EOF'
fn main() -> i64 {
    let c = [7; 3];
    c[0] + c[1] + c[2]
}
EOF
jit=$("$KARDC" "$TMP/rep.kd" 2>/dev/null | tail -1)
[[ "$jit" == "21" ]] || { echo "FAIL [repeat]: expected 21 got '$jit'"; exit 1; }
echo "PASS [array-repeat]: [7; 3] -> 21 (the matrix exercises the symbolic form)"

echo "ALL PHASE 62 SMOKE TESTS PASSED"
