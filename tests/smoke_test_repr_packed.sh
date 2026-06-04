#!/usr/bin/env bash
# Roadmap v97 — repr(packed) + endianness intrinsics + volatile intrinsics.
# The "parse-a-packet-header / touch-a-device-register / read-a-binary-file"
# version. Ground-truth probing corrected the surveys: raw pointers + enforced
# `unsafe` blocks ALREADY exist (so volatile is cheap + must be unsafe-gated like
# ptr_write), and `reverse_bytes` was HARDCODED to i64 (so the sized-int
# endianness intrinsics need WIDTH-AWARE lowering, not a bswap alias).
#
# v97 ships:
#   * #[repr(packed)] — LLVM packed struct (no inter-field padding); size_of
#     shrinks, unaligned field load/store stay correct.
#   * width-aware swap_bytes / to_le / to_be / from_le / from_be (T -> T, byte
#     swap at the argument's real width; endianness from the module DataLayout).
#   * volatile_load / volatile_store — `setVolatile(true)`, unsafe-gated.
#
# DEFERRED (honest, no stub): bit-fields (`field: uN : W`). It is a genuine L
# feature — a parallel single-i64-backing struct representation special-cased
# across emitStructLit, the three field-access/lvalue codegen paths, field-assign
# read-modify-write, struct-body declaration and size_of, plus a typecheck/borrow
# ban on `&`-of-a-sub-byte-field. repr(packed) already covers byte-granular
# packet/register access; bit-fields are the sub-byte refinement (designed,
# in-source note at parser.cpp parseParam; tracked in ROADMAP-v91-v100). The C
# backend cleanly REFUSES packed / volatile / endianness (never miscompiles).
set -uo pipefail
KARDC=""
for c in "${TEST_SRCDIR:-}/_main/compiler/kardc" "${RUNFILES_DIR:-}/_main/compiler/kardc" "./compiler/kardc" "./build.local/kardc"; do
  [[ -n "$c" && -x "$c" ]] && { KARDC="$c"; break; }; done
[[ -z "$KARDC" ]] && { echo "FAIL: kardc not found"; exit 1; }
echo "Using kardc at: $KARDC"
TMP=$(mktemp -d); trap 'rm -rf "$TMP"' EXIT

# Full-stdout differential (exit codes are mod 256, so large values must be
# compared via printed stdout). Asserts JIT stdout == AOT stdout == expected.
diff_jit_aot() {  # $1 program  $2 expected-stdout (may be multi-line)  $3 label
  # The JIT echoes main's return value as a trailing line; AOT uses it as the
  # exit code. Take exactly the expected number of lines from both so the prints
  # line up (the established diff_run pattern).
  printf '%s' "$1" > "$TMP/t.kd"
  local n; n=$(printf '%s\n' "$2" | wc -l)
  local j; j=$("$KARDC" --no-cache "$TMP/t.kd" 2>/dev/null | head -n "$n")
  [[ "$j" == "$2" ]] || { echo "FAIL [$3/jit]: got '$j' want '$2'"; "$KARDC" "$TMP/t.kd" 2>&1 | head -4; exit 1; }
  "$KARDC" --no-cache -o "$TMP/t" "$TMP/t.kd" >/dev/null 2>&1 || { echo "FAIL [$3]: AOT build failed"; "$KARDC" "$TMP/t.kd" 2>&1 | head -4; exit 1; }
  local a; a=$("$TMP/t" 2>/dev/null | head -n "$n")
  [[ "$a" == "$2" ]] || { echo "FAIL [$3/aot]: got '$a' want '$2'"; exit 1; }
  echo "PASS [$3]"
}

reject() {  # $1 program  $2 needle  $3 label
  printf '%s' "$1" > "$TMP/r.kd"
  local e; e=$("$KARDC" "$TMP/r.kd" 2>&1 >/dev/null || true)
  grep -qiE "$2" <<<"$e" || { echo "FAIL [$3]: want /$2/, got:"; echo "$e" | head -3; exit 1; }
  echo "PASS [$3]"
}

# --- (a) #[repr(packed)] removes inter-field padding (size_of shrinks) ---
diff_jit_aot \
'#[repr(packed)]
struct Packed { a: u8, b: u64 }
struct Plain { a: u8, b: u64 }
fn main() -> i64 ! { io } { print(size_of!(Packed)); print(size_of!(Plain)); 0 }' \
$'9\n16' packed_no_padding

# --- (b) packed byte round-trip: a {u8,u64,u8} header reads back exactly ---
diff_jit_aot \
'#[repr(packed)]
struct Hdr { ver: u8, len: u64, flag: u8 }
fn main() -> i64 ! { io } {
  let h = Hdr { ver: 7, len: 4000000000, flag: 1 };
  print(h.ver as i64); print(h.len as i64); print(h.flag as i64); print(size_of!(Hdr)); 0
}' \
$'7\n4000000000\n1\n10' packed_roundtrip

# --- (c) swap_bytes WIDTH-AWARE (this fails with the old i64-bswap bug) ---
diff_jit_aot \
'fn main() -> i64 ! { io } {
  let a: u16 = 0x1122; print(swap_bytes(a) as i64);
  let b: u32 = 0x11223344; print(swap_bytes(b) as i64);
  let c: u8 = 0x55; print(swap_bytes(c) as i64);
  let d: u64 = 0x0102030405060708; print(swap_bytes(d) as i64);
  0
}' \
$'8721\n1144201745\n85\n578437695752307201' swap_bytes_width_aware

# --- (d) to_le/to_be/from_le/from_be round-trip (identity composition) ---
diff_jit_aot \
'fn main() -> i64 ! { io } {
  let a: u16 = 4660; let b: u32 = 305419896; let c: u64 = 1311768467463790320;
  print(from_le(to_le(a)) as i64); print(from_be(to_be(a)) as i64);
  print(from_le(to_le(b)) as i64); print(from_be(to_be(b)) as i64);
  print(from_le(to_le(c)) as i64); print(from_be(to_be(c)) as i64);
  0
}' \
$'4660\n4660\n305419896\n305419896\n1311768467463790320\n1311768467463790320' endian_roundtrip

# --- (e) volatile round-trip through a raw pointer, in an unsafe block ---
diff_jit_aot \
'fn main() -> i64 ! { io } {
  let mut x: i64 = 0;
  let p: *mut i64 = &mut x as *mut i64;
  let v = unsafe { volatile_store(p, 42); volatile_load(p) };
  print(v); 0
}' \
$'42' volatile_roundtrip

# --- (e2) sized volatile keeps the pointee width (u32) ---
diff_jit_aot \
'fn main() -> i64 ! { io } {
  let mut x: u32 = 0;
  let p: *mut u32 = &mut x as *mut u32;
  let v = unsafe { volatile_store(p, 4000000000); volatile_load(p) };
  print(v as i64); 0
}' \
$'4000000000' volatile_sized_width

# --- (f) volatile OUTSIDE unsafe is rejected ---
reject \
'fn main() -> i64 ! { io } {
  let mut x: i64 = 0;
  let p: *mut i64 = &mut x as *mut i64;
  volatile_store(p, 42); 0
}' \
'requires an .unsafe. block' volatile_needs_unsafe

# --- (g) the IR shows `volatile` (target-INDEPENDENT keyword grep, per v90) ---
printf '%s' 'fn main() -> i64 ! { io } {
  let mut x: i64 = 0;
  let p: *mut i64 = &mut x as *mut i64;
  let v = unsafe { volatile_store(p, 42); volatile_load(p) };
  print(v); 0
}' > "$TMP/g.kd"
vc=$("$KARDC" --no-cache --emit-llvm "$TMP/g.kd" 2>/dev/null | grep -c 'volatile' || true)
[[ "$vc" -ge 2 ]] || { echo "FAIL [volatile_ir]: expected >=2 volatile ops in IR, got $vc"; exit 1; }
echo "PASS [volatile_ir]: $vc volatile load/store ops in --emit-llvm"

# --- (h) C backend REFUSES what it cannot represent (never miscompiles) ---
# NB: packed/volatile/endianness all compile fine on the LLVM path; the refusal
# is on the --emit-c path only, so these drive --emit-c explicitly.
# NB: capture-then-grep (NOT `| grep -q`): under `set -o pipefail`, grep -q closes
# the pipe on first match and the producer dies with SIGPIPE → false failure.
printf '%s' '#[repr(packed)] struct Hdr { a: u8, b: u32 } fn main() -> i64 { 0 }' > "$TMP/cp.kd"
cpout=$("$KARDC" --emit-c "$TMP/cp.kd" 2>&1 || true)
grep -qi "outside the C-backend subset" <<<"$cpout" || { echo "FAIL [c_refuses_packed_emitc]: --emit-c did not refuse packed: $cpout"; exit 1; }
echo "PASS [c_refuses_packed_emitc]"
printf '%s' 'fn main() -> i64 ! { io } { let x: i64 = 100; print(swap_bytes(x)); 0 }' > "$TMP/cs.kd"
csout=$("$KARDC" --emit-c "$TMP/cs.kd" 2>&1 || true)
grep -qi "outside the C-backend subset" <<<"$csout" || { echo "FAIL [c_refuses_swap]: --emit-c did not refuse swap_bytes: $csout"; exit 1; }
echo "PASS [c_refuses_swap]"

# --- (i) repr(transparent) still rejected (only C/packed supported) ---
reject \
'#[repr(transparent)] struct Wrapper { a: i32 } fn main() -> i64 { 0 }' \
'repr' repr_transparent_rejected

echo "ALL REPR(PACKED) / ENDIANNESS / VOLATILE SMOKE TESTS PASSED"
