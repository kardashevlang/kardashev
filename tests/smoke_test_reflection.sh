#!/usr/bin/env bash
# v49 — compile-time reflection intrinsics. The typechecker resolves a type
# argument and reflects a constant: `field_count!(S)` / `variant_count!(E)` /
# `size_of!(T)` -> i64, `type_name!(T)` -> String. field_count/variant_count and
# type_name are computed from the static type info; size_of! is computed in
# codegen from the lowered type's real DataLayout alloc size. Differentially
# gated JIT vs AOT (both must print the same expected output). The full
# field-iterating TypeInfo API + proc-macros that build on it are deferred.
set -euo pipefail
KARDC=""
for c in "${TEST_SRCDIR:-}/_main/compiler/kardc" "${RUNFILES_DIR:-}/_main/compiler/kardc" "./compiler/kardc" "./build.local/kardc"; do
  [[ -n "$c" && -x "$c" ]] && { KARDC="$c"; break; }; done
[[ -z "$KARDC" ]] && { echo "FAIL: kardc not found"; exit 1; }
echo "Using kardc at: $KARDC"
TMP=$(mktemp -d); trap 'rm -rf "$TMP"' EXIT

# JIT==AOT==expected over the program's stdout. The JIT appends the main return
# value as a trailing line (AOT uses it as the exit code), so both are limited to
# the first N lines, where N is the expected line count.
diff_run() {
    local name="$1" expect="$2" src="$3"
    local n; n=$(printf '%s\n' "$expect" | wc -l)
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
    echo "$err" | grep -qi "$needle" || { echo "FAIL [$name]: want '$needle' got: $err"; exit 1; }
    echo "PASS (negative): $name"
}

# ---- field_count!(struct) — equals the declared field count ----
diff_run fc3 '3' 'struct P { x: i64, y: i64, z: i64 } fn main() -> i64 ! { io } { print(field_count!(P)); 0 }'
diff_run fc1 '1' 'struct One { only: bool } fn main() -> i64 ! { io } { print(field_count!(One)); 0 }'

# ---- variant_count!(enum) — equals the declared variant count ----
diff_run vc4 '4' 'enum Color { Red, Green, Blue, Alpha } fn main() -> i64 ! { io } { print(variant_count!(Color)); 0 }'
diff_run vc2 '2' 'enum Bit { Zero, One } fn main() -> i64 ! { io } { print(variant_count!(Bit)); 0 }'

# ---- size_of!(T) — real DataLayout alloc size ----
diff_run sz_i64  '8'  'fn main() -> i64 ! { io } { print(size_of!(i64)); 0 }'
diff_run sz_i32  '4'  'fn main() -> i64 ! { io } { print(size_of!(i32)); 0 }'
diff_run sz_bool '1'  'fn main() -> i64 ! { io } { print(size_of!(bool)); 0 }'
diff_run sz_3i64 '24' 'struct T3 { a: i64, b: i64, c: i64 } fn main() -> i64 ! { io } { print(size_of!(T3)); 0 }'

# ---- type_name!(T) — the type's display name as a String ----
diff_run tn_struct 'Widget' 'struct Widget { a: i64 } fn main() -> i64 ! { io } { print_str(&type_name!(Widget)); 0 }'
diff_run tn_prim   'i64'    'fn main() -> i64 ! { io } { print_str(&type_name!(i64)); 0 }'

# ---- reflection composes in ordinary expressions / control flow ----
diff_run compose '2' 'struct Pair { a: i64, b: i64 } enum Tri { X, Y, Z } fn main() -> i64 ! { io } { let d = variant_count!(Tri) - field_count!(Pair); print(d + 1); 0 }'

# ---- negative: wrong kind of type for the intrinsic ----
expect_err err_fc_enum   'struct' 'enum E { A, B } fn main() -> i64 { field_count!(E) }'
expect_err err_vc_struct 'enum'   'struct S { a: i64 } fn main() -> i64 { variant_count!(S) }'

echo "ALL REFLECTION SMOKE TESTS PASSED"
