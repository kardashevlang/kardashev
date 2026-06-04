#!/usr/bin/env bash
# Roadmap v88 — #[repr(C)] struct layout + struct FFI by pointer + narrow-int ABI.
# Builds on v87's sized-int FFI widths. The honest, portable cut:
#   * `#[repr(C)]` on a struct decl -> a guaranteed C layout (declaration field
#     order + host C alignment via the already-set datalayout). `repr(packed)` /
#     `repr(transparent)` are rejected (not silently ignored).
#   * An `extern "C"` signature may pass/return a `#[repr(C)]` struct BY POINTER
#     (`&T` / `&mut T`); a pointer to a NON-repr(C) user struct is rejected (no
#     layout guarantee), and struct BY VALUE is rejected with an actionable
#     message (the System V by-value register ABI is the deferred mega-arc).
#   * signext/zeroext on narrow (i8/i16) extern params + returns (the v87
#     deferral) — a C `unsigned char`/`signed char` boundary is value-correct.
#   * `kardc --emit-obj <file.o>` emits a native object so a build/test can link
#     it with a C object for REAL FFI interop.
#
# DEFERRED (honest, no stubs): struct BY VALUE + `sret` struct returns need the
# per-platform System V eightbyte register classifier (~2000 lines, the
# by-value-ABI / WASM+Windows mega-arc) — clang lowers `int sum(struct{int x,y})`
# to `i32 @sum(i64)`, not an LLVM aggregate param, so a half-implementation would
# silently miscompile. Rejected with a clear "pass &T" message, not stubbed.
#
# Real C interop requires clang; the test skips-with-pass if clang is absent
# (mirrors smoke_test_ffi). IR-shape assertions also need clang for --emit-llvm.
set -uo pipefail
KARDC=""
for c in "${TEST_SRCDIR:-}/_main/compiler/kardc" "${RUNFILES_DIR:-}/_main/compiler/kardc" "./compiler/kardc" "./build.local/kardc"; do
  [[ -n "$c" && -x "$c" ]] && { KARDC="$c"; break; }; done
[[ -z "$KARDC" ]] && { echo "FAIL: kardc not found"; exit 1; }
CLANG="$(command -v clang || true)"
[[ -z "$CLANG" ]] && { echo "PASS [repr-c-ffi]: SKIPPED (no clang)"; exit 0; }
echo "Using kardc at: $KARDC ; clang at: $CLANG"
TMP=$(mktemp -d); trap 'rm -rf "$TMP"' EXIT

# --- Gate A: repr(C) layout + by-pointer declaration shape (IR, -O0) ---
cat > "$TMP/p.kd" <<'EOF'
#[repr(C)] struct Point { x: i32, y: i32 }
extern "C" fn point_sum(p: &Point) -> i32;
fn main() -> i64 ! { io } { let p = Point { x: 3i32, y: 4i32 } ; point_sum(&p) as i64 }
EOF
"$KARDC" --emit-llvm -O0 "$TMP/p.kd" 2>/dev/null > "$TMP/p.ll"
grep -aqF '{ i32, i32 }' "$TMP/p.ll" || { echo "FAIL [layout]: repr(C) Point not laid out as { i32, i32 }"; exit 1; }
grep -aqE 'declare i32 @point_sum\(ptr' "$TMP/p.ll" || { echo "FAIL [by-pointer]: extern struct param not a C pointer"; grep -a point_sum "$TMP/p.ll"; exit 1; }
echo "PASS [layout]: #[repr(C)] Point -> { i32, i32 }; extern takes it by pointer (ptr)"

# --- Gate B: REAL C interop by pointer (the load-bearing layout cross-check) ---
cat > "$TMP/helper.c" <<'EOF'
struct Point { int x; int y; };
int point_sum(const struct Point* p) { return p->x + p->y; }
void point_scale(struct Point* p, int k) { p->x *= k; p->y *= k; }
unsigned char low_byte(unsigned char x) { return x; }
signed char neg_sc(signed char x) { return -x; }
EOF
"$CLANG" -c "$TMP/helper.c" -o "$TMP/helper.o" 2>/dev/null || { echo "FAIL [helper]: clang could not compile helper.c"; exit 1; }
cat > "$TMP/use.kd" <<'EOF'
#[repr(C)] struct Point { x: i32, y: i32 }
extern "C" fn point_sum(p: &Point) -> i32;
extern "C" fn point_scale(p: &mut Point, k: i32);
fn main() -> i64 ! { io } {
    let mut p = Point { x: 3i32, y: 4i32 } ;
    point_scale(&mut p, 10) ;
    point_sum(&p) as i64
}
EOF
"$KARDC" --emit-obj "$TMP/use.o" "$TMP/use.kd" >/dev/null 2>&1 || { echo "FAIL [emit-obj]: kardc --emit-obj failed"; "$KARDC" --emit-obj "$TMP/use.o" "$TMP/use.kd" 2>&1 | head -3; exit 1; }
"$CLANG" "$TMP/use.o" "$TMP/helper.o" -o "$TMP/use" -lpthread -lm 2>/dev/null || { echo "FAIL [link]: clang could not link kardc.o + helper.o"; exit 1; }
"$TMP/use" >/dev/null 2>&1; r=$?
[[ "$r" -eq 70 ]] || { echo "FAIL [real-c-ptr]: exit $r (want 70 = (3*10 + 4*10)); struct layout mismatch?"; exit 1; }
echo "PASS [real-c-ptr]: kardc-built repr(C) struct read/written by clang-compiled C; exit 70"

# --- Gate C: signext/zeroext narrow-int boundary correctness ---
"$KARDC" --emit-llvm -O0 "$TMP/use.kd" >/dev/null 2>&1  # warm (no-op)
cat > "$TMP/sx.kd" <<'EOF'
extern "C" fn low_byte(x: u8) -> u8;
extern "C" fn neg_sc(x: i8) -> i8;
fn dummy() -> i64 ! { io } { (low_byte(200u8) as i64) }
fn main() -> i64 ! { io } { if (low_byte(255u8) == 255u8) { if (neg_sc(5i8) == (0i8 - 5i8)) { 1 } else { 0 } } else { 0 } }
EOF
"$KARDC" --emit-llvm -O0 "$TMP/sx.kd" 2>/dev/null > "$TMP/sx.ll"
grep -aqE 'declare zeroext i8 @low_byte\(i8 zeroext' "$TMP/sx.ll" || { echo "FAIL [zeroext-attr]: low_byte not declared zeroext"; grep -a low_byte "$TMP/sx.ll"; exit 1; }
grep -aqE 'declare signext i8 @neg_sc\(i8 signext' "$TMP/sx.ll" || { echo "FAIL [signext-attr]: neg_sc not declared signext"; grep -a neg_sc "$TMP/sx.ll"; exit 1; }
echo "PASS [ext-attrs]: narrow extern params/returns get zeroext/signext (matches clang)"
"$KARDC" --emit-obj "$TMP/sx.o" "$TMP/sx.kd" >/dev/null 2>&1 || { echo "FAIL [sx emit-obj]"; exit 1; }
"$CLANG" "$TMP/sx.o" "$TMP/helper.o" -o "$TMP/sx" -lpthread -lm 2>/dev/null || { echo "FAIL [sx link]"; exit 1; }
"$TMP/sx" >/dev/null 2>&1; r=$?
[[ "$r" -eq 1 ]] || { echo "FAIL [narrow-int]: exit $r (want 1: low_byte(255)==255, neg_sc(5)==-5 across real C)"; exit 1; }
echo "PASS [narrow-int]: u8 255 stays 255 (zero-ext), i8 neg stays -5 (sign-ext) across real C"

# --- Negatives ---
printf '%s' 'struct Plain { a: i64 } extern "C" fn f(p: &Plain) -> i64; fn main() -> i64 { 0 }' > "$TMP/n1.kd"
"$KARDC" "$TMP/n1.kd" >/dev/null 2>"$TMP/n1.err" && { echo "FAIL [neg-non-reprc-ptr]: &non-repr(C) struct should be rejected"; exit 1; }
grep -qi "repr(C)" "$TMP/n1.err" || { echo "FAIL [neg-non-reprc-ptr]: wrong message"; cat "$TMP/n1.err"; exit 1; }
echo "PASS [neg-non-reprc-ptr]: a pointer to a non-repr(C) struct is rejected"
printf '%s' '#[repr(C)] struct Q { a: i32 } extern "C" fn g(p: Q) -> i64; fn main() -> i64 { 0 }' > "$TMP/n2.kd"
"$KARDC" "$TMP/n2.kd" >/dev/null 2>"$TMP/n2.err" && { echo "FAIL [neg-by-value]: struct-by-value should be rejected"; exit 1; }
grep -qi "by value" "$TMP/n2.err" || { echo "FAIL [neg-by-value]: wrong message"; cat "$TMP/n2.err"; exit 1; }
echo "PASS [neg-by-value]: struct-by-value across extern \"C\" is rejected (pass &T)"
# v97: `#[repr(packed)]` is now SUPPORTED (no inter-field padding) — see
# smoke_test_repr_packed.sh. The remaining unsupported repr kind is
# `repr(transparent)`, which is still rejected (not silently ignored).
printf '%s' '#[repr(transparent)] struct Wrapper { a: i32 } fn main() -> i64 { 0 }' > "$TMP/n3.kd"
"$KARDC" "$TMP/n3.kd" >/dev/null 2>"$TMP/n3.err" && { echo "FAIL [neg-repr-transparent]: repr(transparent) should be rejected"; exit 1; }
grep -qi "repr(C)" "$TMP/n3.err" || { echo "FAIL [neg-repr-transparent]: wrong message"; cat "$TMP/n3.err"; exit 1; }
echo "PASS [neg-repr-transparent]: repr(transparent) is rejected (not silently ignored)"
# v97: `#[repr(packed)]` now COMPILES (positive).
printf '%s' '#[repr(packed)] struct Hdr { a: u8, b: u32 } fn main() -> i64 { 0 }' > "$TMP/p3.kd"
"$KARDC" "$TMP/p3.kd" >/dev/null 2>"$TMP/p3.err" || { echo "FAIL [pos-repr-packed]: repr(packed) should compile now"; cat "$TMP/p3.err"; exit 1; }
echo "PASS [pos-repr-packed]: repr(packed) compiles (v97)"

echo "ALL REPR(C) FFI SMOKE TESTS PASSED"
