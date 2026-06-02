#!/usr/bin/env bash
# v41 — deref-assignment (`*p = v`) + copy_nonoverlapping (in-language unsafe
# surface). Write through `&mut T` (safe) and `*mut T` (unsafe); a memcpy of n
# elements between raw pointers. Retires the long-standing "deref-assign
# unsupported language-wide" gap. Differential JIT vs AOT.
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

TMP=$(mktemp -d); trap 'rm -rf "$TMP"' EXIT
diff_run() {
    local name="$1" expect="$2" src="$3"
    local n; n=$(printf '%s\n' "$expect" | wc -l | tr -d ' ')
    printf '%s' "$src" > "$TMP/$name.kd"
    local jit; jit=$("$KARDC" "$TMP/$name.kd" 2>/dev/null | head -n "$n") || true
    [[ "$jit" == "$expect" ]] || { echo "FAIL [$name/jit]: expected '$expect' got '$jit'"; exit 1; }
    "$KARDC" --no-cache -o "$TMP/$name" "$TMP/$name.kd" >/dev/null 2>&1
    local aot; aot=$("$TMP/$name" 2>/dev/null | head -n "$n") || true
    [[ "$aot" == "$expect" ]] || { echo "FAIL [$name/aot]: expected '$expect' got '$aot'"; exit 1; }
    echo "PASS: $name"
}
expect_err() {
    local name="$1" needle="$2" src="$3"
    printf '%s' "$src" > "$TMP/$name.kd"
    local err; err=$("$KARDC" "$TMP/$name.kd" 2>&1 >/dev/null || true)
    echo "$err" | grep -qi "$needle" || { echo "FAIL [$name]: expected '$needle', got: $err"; exit 1; }
    echo "PASS (negative): $name"
}

# Deref-assign through &mut (safe) and *mut (unsafe).
diff_run deref_assign $'105\n42\n77' '
fn bump(r: &mut i64) { *r = *r + 100; }
fn main() -> i64 ! { io } {
    let mut x = 5; bump(&mut x); print(x);          // 105
    let mut y = 10; let p = &mut y; *p = 42; print(y);  // 42
    let mut z = 1; let rp = &mut z as *mut i64;
    let u = unsafe { *rp = 77; 0 }; print(z);        // 77
    0
}
'

# copy_nonoverlapping memcpy of n elements between raw pointers.
diff_run copy_nono $'10\n3' '
fn main() -> i64 ! { io } {
    let src = [1, 2, 3, 4];
    let mut dst = [0, 0, 0, 0];
    let sp = &src[0] as *const i64;
    let dp = &mut dst[0] as *mut i64;
    let u = unsafe { copy_nonoverlapping(sp, dp, 4); 0 };
    print(dst[0] + dst[1] + dst[2] + dst[3]);   // 10
    print(dst[2]);                               // 3
    0
}
'

# NEGATIVE: write through an immutable &T.
expect_err immut 'not mutable' '
fn f(r: &i64) { *r = 9; }
fn main() -> i64 { 0 }
'
# NEGATIVE: raw deref-assign outside unsafe.
expect_err raw_safe 'unsafe' '
fn main() -> i64 { let mut z=1; let rp = &mut z as *mut i64; *rp = 5; z }
'
# NEGATIVE: copy_nonoverlapping outside unsafe.
expect_err copy_safe 'unsafe' '
fn main() -> i64 { let a=[1,2]; let mut b=[0,0]; let s=&a[0] as *const i64; let d=&mut b[0] as *mut i64; copy_nonoverlapping(s, d, 2); 0 }
'

echo "ALL DEREF-ASSIGN SMOKE TESTS PASSED"
