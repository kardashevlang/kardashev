#!/usr/bin/env bash
# Roadmap v87 — sized integers across all surfaces. Sized integers (i8/i16/i32/i64,
# u8/u16/u32/u64) and f32/f64 were made first-class in the LLVM backend back in
# v11 (type system, signedness-correct arithmetic, full casts), but had no
# dedicated end-to-end RUNTIME differential gate, and two boundaries still
# assumed i64. v87:
#   * EXTERN FFI boundary: `cAbiType` now maps each sized int to its REAL C width
#     (u8 -> i8, u32 -> i32, …) instead of collapsing to i64 — the v88
#     repr(C)-by-value prerequisite. (i32 keeps its historical i64-sugar.)
#   * This gate PINS the sized-int semantics (JIT == AOT): unsigned wrap, signed
#     vs unsigned division / shift / compare, cast round-trips (trunc/sext/zext/
#     fp), a sized struct field read at -O2 (datalayout-before-opt guard), a sized
#     array element, and f32 arithmetic; plus the FFI all-width declaration shape,
#     the mixed-width negative, and the C-backend's clean refusal.
#
# DEFERRED (honest, no stubs): the C backend (--emit-c) continues to cleanly
# REFUSE sized ints — faithful support would need a width-cast after EVERY op to
# match LLVM's wrap-at-width (C integer promotion computes `uint8_t + uint8_t` in
# `int`), so refusing is sound, not a stub. print/print_f64 arg-widening (so a
# sized int prints without `as i64`) and signext/zeroext narrow-arg ABI attrs ride
# on later versions (v88 FFI hardening / v89 stdlib formatting). See ROADMAP.
set -uo pipefail
KARDC=""
for c in "${TEST_SRCDIR:-}/_main/compiler/kardc" "${RUNFILES_DIR:-}/_main/compiler/kardc" "./compiler/kardc" "./build.local/kardc"; do
  [[ -n "$c" && -x "$c" ]] && { KARDC="$c"; break; }; done
[[ -z "$KARDC" ]] && { echo "FAIL: kardc not found"; exit 1; }
echo "Using kardc at: $KARDC"
TMP=$(mktemp -d); trap 'rm -rf "$TMP"' EXIT

# JIT prints main's return value; AOT uses it as the process exit code. Each case
# asserts BOTH equal the expected value (so JIT and AOT agree).
jit_aot() {  # $1 program  $2 want  $3 label  [$4 extra kardc flags]
  printf '%s' "$1" > "$TMP/t.kd"
  local j; j=$("$KARDC" "$TMP/t.kd" 2>/dev/null | head -1)
  [[ "$j" == "$2" ]] || { echo "FAIL [$3]: JIT printed '$j', want '$2'"; "$KARDC" "$TMP/t.kd" 2>&1 | head -3; exit 1; }
  "$KARDC" --no-cache ${4:-} -o "$TMP/t" "$TMP/t.kd" >/dev/null 2>&1 || { echo "FAIL [$3]: AOT build failed"; exit 1; }
  "$TMP/t" >/dev/null 2>&1; local a=$?
  [[ "$a" -eq "$2" ]] || { echo "FAIL [$3]: AOT exit $a, want $2"; exit 1; }
  echo "PASS [$3]: JIT == AOT == $2"
}

jit_aot 'fn main() -> i64 { (200u8 + 100u8) as i64 }' 44 "u8-overflow-wrap"
jit_aot 'fn main() -> i64 { let r = (0i32 - 7i32) / 2i32 ; if r == (0i32 - 3i32) { 1 } else { 0 } }' 1 "i32-signed-div"
jit_aot 'fn main() -> i64 { if (4294967289u32 / 2u32) == 2147483644u32 { 1 } else { 0 } }' 1 "u32-unsigned-div"
jit_aot 'fn main() -> i64 { if ((0i32 - 2i32) >> 1i32) == (0i32 - 1i32) { 1 } else { 0 } }' 1 "i32-arith-shift"
jit_aot 'fn main() -> i64 { if (4294967294u32 >> 1u32) == 2147483647u32 { 1 } else { 0 } }' 1 "u32-logical-shift"
jit_aot 'fn main() -> i64 { if 4294967295u32 < 1u32 { 0 } else { 1 } }' 1 "u32-unsigned-compare"
jit_aot 'fn main() -> i64 { if (255u8 as i32) == 255i32 { 1 } else { 0 } }' 1 "zext-cast"
jit_aot 'fn main() -> i64 { if ((0i8 - 1i8) as i32) == (0i32 - 1i32) { 1 } else { 0 } }' 1 "sext-cast"
jit_aot 'fn main() -> i64 { (300i32 as u8) as i64 }' 44 "trunc-cast"
jit_aot 'fn main() -> i64 { let x: f32 = 1.5f32 ; let y = x + x ; y as i64 }' 3 "f32-arith"
jit_aot 'fn main() -> i64 { if (3.9f64 as i32) == 3i32 { if (3.9f32 as u32) == 3u32 { 1 } else { 0 } } else { 0 } }' 1 "float-to-int-cast"
# Sized struct field read at -O2 — guards the datalayout-before-opt rule for
# mixed-width aggregate reads-through-pointer.
jit_aot 'struct S { x: i32, y: u16 } fn main() -> i64 { let s = S { x: 0i32 - 5i32, y: 60000u16 } ; if s.x == (0i32 - 5i32) { if s.y == 60000u16 { 1 } else { 0 } } else { 0 } }' 1 "sized-struct-field-O2" "-O2"
jit_aot 'fn main() -> i64 { let a: [u16; 3] = [10u16, 60000u16, 30000u16] ; if a[1] == 60000u16 { 1 } else { 0 } }' 1 "sized-array-elem"

# FFI all-width boundary: an extern declares each sized int at its real C width.
cat > "$TMP/ffi.kd" <<'EOF'
extern "C" fn fw(a: u8, b: u16, c: u32, d: u64) -> u8;
fn use_it() -> u8 { fw(1u8, 2u16, 3u32, 4u64) }
fn main() -> i64 { use_it() as i64 }
EOF
"$KARDC" --emit-llvm "$TMP/ffi.kd" 2>/dev/null > "$TMP/ffi.ll"
grep -aqF 'declare i8 @fw(i8, i16, i32, i64)' "$TMP/ffi.ll" || { echo "FAIL [ffi-widths]: extern not declared at real C widths"; grep -a 'declare.*@fw' "$TMP/ffi.ll"; exit 1; }
echo "PASS [ffi-widths]: extern \"C\" maps sized ints to real C widths (i8/i16/i32/i64)"
# i32 keeps its historical i64-sugar: abs(0 - 7) == 7.
printf '%s' 'extern "C" fn abs(x: i32) -> i32; fn main() -> i64 { abs(0 - 7) }' > "$TMP/abs.kd"
"$KARDC" --no-cache -o "$TMP/abs" "$TMP/abs.kd" >/dev/null 2>&1 && "$TMP/abs" >/dev/null 2>&1; [[ $? -eq 7 ]] || { echo "FAIL [ffi-i32-sugar]: abs(0-7) != 7"; exit 1; }
echo "PASS [ffi-i32-sugar]: i32 extern sugar preserved (abs(0 - 7) == 7)"

# NEGATIVE: mixed-width arithmetic is a compile error (no implicit widening).
printf '%s' 'fn main() -> i64 { let x: u8 = 1u8 ; let y: u32 = 5u32 ; (x + y) as i64 }' > "$TMP/mix.kd"
"$KARDC" "$TMP/mix.kd" >/dev/null 2>"$TMP/mixerr" && { echo "FAIL [neg-mixed-width]: u8 + u32 should be rejected"; exit 1; }
grep -qi "same integer type" "$TMP/mixerr" || { echo "FAIL [neg-mixed-width]: wrong error"; cat "$TMP/mixerr"; exit 1; }
echo "PASS [neg-mixed-width]: u8 + u32 is a compile error (no implicit widening)"

# C backend cleanly REFUSES sized ints (sound — C integer promotion would
# silently diverge from LLVM wrap-at-width; refusing is not a stub).
printf '%s' 'fn add(a: u8, b: u8) -> u8 { a + b } fn main() -> i64 { add(200u8, 100u8) as i64 }' > "$TMP/cb.kd"
"$KARDC" --emit-c "$TMP/cb.kd" >/dev/null 2>"$TMP/cberr" && { echo "FAIL [c-backend-refuse]: --emit-c should refuse sized ints"; exit 1; }
grep -qi "outside the C-backend subset" "$TMP/cberr" || { echo "FAIL [c-backend-refuse]: wrong refusal message"; cat "$TMP/cberr"; exit 1; }
echo "PASS [c-backend-refuse]: --emit-c cleanly refuses sized ints (no silent miscompile)"

echo "ALL SIZED-INTEGER RUNTIME SMOKE TESTS PASSED"
