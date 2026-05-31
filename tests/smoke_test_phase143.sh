#!/usr/bin/env bash
# v26 Phase 143 smoke test: slice patterns `[a, b, _, ..]` in match arms. A
# slice-pattern match desugars to a length-checked if/else chain over slice_len
# / slice_get. Covers exact length, length dispatch, a prefix `[a, ..]`, the
# required catch-all, and confirms `&mut [T]` mutable slices. JIT + AOT.
set -uo pipefail
KARDC=""
for c in "${TEST_SRCDIR:-}/_main/compiler/kardc" "${TEST_SRCDIR:-}/kardashev/compiler/kardc" \
         "${RUNFILES_DIR:-}/_main/compiler/kardc" "${RUNFILES_DIR:-}/kardashev/compiler/kardc" \
         "./compiler/kardc" "./build.local/kardc"; do
    [[ -n "$c" && -x "$c" ]] && { KARDC="$c"; break; }; done
[[ -z "$KARDC" ]] && { echo "FAIL: kardc not found"; exit 1; }
echo "Using kardc at: $KARDC"
TMP=$(mktemp -d); trap 'rm -rf "$TMP"' EXIT
run_eq() { local jit; jit=$("$KARDC" "$2" 2>&1 | head -1)
    [[ "$jit" == "$3" ]] || { echo "FAIL [$1/jit]: want $3 got '$jit'"; exit 1; }
    "$KARDC" --no-cache -o "$TMP/b" "$2" >/dev/null 2>&1 || { echo "FAIL [$1/aot]: compile"; exit 1; }
    "$TMP/b" >/dev/null; local rc=$?; [[ "$rc" -eq "$3" ]] || { echo "FAIL [$1/aot]: exit $rc want $3"; exit 1; }; }

cat > "$TMP/a.kd" <<'EOF'
fn f(s: &[i64]) -> i64 { match s { [x] => x, [a, b] => a * 10 + b, _ => 99 } }
fn main() -> i64 ! { alloc } {
    let mut v1 = vec_new(); vec_push(&mut v1, 7);
    let mut v2 = vec_new(); vec_push(&mut v2, 2); vec_push(&mut v2, 3);
    let mut v3 = vec_new(); vec_push(&mut v3, 1); vec_push(&mut v3, 1); vec_push(&mut v3, 1);
    f(&v1[0..1]) + f(&v2[0..2]) + f(&v3[0..3])
}
EOF
run_eq dispatch "$TMP/a.kd" 129
echo "PASS [dispatch]: [x] / [a,b] / _ length dispatch (7 + 23 + 99 = 129)"

cat > "$TMP/p.kd" <<'EOF'
fn head(s: &[i64]) -> i64 { match s { [first, ..] => first, _ => 0 } }
fn main() -> i64 ! { alloc } { let mut v = vec_new(); vec_push(&mut v, 5); vec_push(&mut v, 6); head(&v[0..2]) }
EOF
run_eq prefix "$TMP/p.kd" 5
echo "PASS [prefix]: [first, ..] binds the head of a longer slice (5)"

cat > "$TMP/m.kd" <<'EOF'
fn len2(s: &mut [i64]) -> i64 { slice_len(s) }
fn main() -> i64 ! { alloc } { let mut v = vec_new(); vec_push(&mut v, 1); vec_push(&mut v, 2); len2(&mut v[0..2]) }
EOF
run_eq mutslice "$TMP/m.kd" 2
echo "PASS [mutslice]: &mut [T] mutable slice type works (len 2)"

# a slice-pattern match without a catch-all is rejected
cat > "$TMP/no.kd" <<'EOF'
fn f(s: &[i64]) -> i64 { match s { [a, b] => a + b } }
fn main() -> i64 ! { alloc } { let mut v = vec_new(); vec_push(&mut v, 1); vec_push(&mut v, 2); f(&v[0..2]) }
EOF
out=$("$KARDC" "$TMP/no.kd" 2>&1); rc=$?
[[ "$rc" -ne 0 ]] || { echo "FAIL [catchall]: a slice match without a catch-all should error"; exit 1; }
echo "PASS [catchall]: a slice-pattern match without a catch-all is rejected"

echo "PASS: Phase 143 — slice patterns + &mut [T]"
