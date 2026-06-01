#!/usr/bin/env bash
# v33 Phase 177 smoke test — raw pointers + `unsafe` blocks (the systems escape
# hatch).
#
#   *const T / *mut T   raw-pointer types (NOT borrow-checked, nullable)
#   unsafe { … }        block permitting unchecked ops
#   &x as *const T      create a raw pointer from a reference (safe)
#   *p   (in unsafe)    dereference a raw pointer (read)
#   p as i64 / a as *mut T / *const T as *mut U   pointer<->int / reinterpret casts
#
# A raw deref OUTSIDE `unsafe` is a type error. Raw pointers lower to the same
# opaque LLVM pointer as `&T`. Differentially gated JIT vs AOT.
#
# Documented Phase-177 scope: raw-pointer WRITE (`*p = v`) is deferred — it needs
# deref-assignment, which the language does not support yet (the same limitation
# as lock-guard `*g = v`); pointer ARITHMETIC is also deferred. The read +
# create + cast escape hatch (what FFI needs to pass/observe C pointers) is here.
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

# Each program prints its result (so we compare stdout, not the exit code) and
# returns 0. JIT echoes the i64 return as a trailing line; we slice the first N.
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
    echo "$err" | grep -qi "$needle" || {
        echo "FAIL [$name]: expected error containing '$needle', got: $err"; exit 1; }
    echo "PASS (negative): $name"
}

# 1. Create a raw pointer from a reference and dereference it (read) in unsafe.
diff_run deref_read $'42' '
fn main() -> i64 ! { io } {
    let x = 42;
    let p = &x as *const i64;
    print(unsafe { *p });
    0
}
'

# 2. Round-trip a pointer through an integer address (ptrtoint / inttoptr).
diff_run ptr_int_roundtrip $'7' '
fn main() -> i64 ! { io } {
    let x = 7;
    let p = &x as *const i64;
    let addr = p as i64;
    let q = addr as *const i64;
    print(unsafe { *q });
    0
}
'

# 3. Reinterpret between raw pointers (*const <-> *mut) and read.
diff_run rawptr_reinterpret $'5' '
fn main() -> i64 ! { io } {
    let x = 5;
    let p = &x as *const i64;
    let q = p as *mut i64;
    let r = q as *const i64;
    print(unsafe { *r });
    0
}
'

# 4. A raw pointer to a struct field, dereferenced in unsafe.
diff_run rawptr_struct $'9' '
struct P { x: i64, y: i64 }
fn main() -> i64 ! { io } {
    let pt = P { x: 9, y: 1 };
    let p = &pt.x as *const i64;
    print(unsafe { *p });
    0
}
'

# 5. Raw pointers across a function boundary (read in unsafe).
diff_run rawptr_arg $'13' '
fn read_ptr(p: *const i64) -> i64 { unsafe { *p } }
fn main() -> i64 ! { io } { let x = 13; print(read_ptr(&x as *const i64)); 0 }
'

# 6. NEGATIVE: dereferencing a raw pointer OUTSIDE `unsafe` is rejected.
expect_err deref_outside_unsafe 'unsafe' '
fn main() -> i64 { let x = 1; let p = &x as *const i64; *p }
'

# 7. NEGATIVE: a raw pointer and a reference do not unify (distinct types).
expect_err rawptr_not_ref 'expected' '
fn want_ref(r: &i64) -> i64 { *r }
fn main() -> i64 { let x = 1; let p = &x as *const i64; want_ref(p) }
'

# 8. The raw-pointer syntax does not break ordinary multiplication (`a * b`).
diff_run star_is_mul $'20' '
fn main() -> i64 ! { io } { let a = 4; let b = 5; print(a * b); 0 }
'

echo "ALL PHASE 177 SMOKE TESTS PASSED"
