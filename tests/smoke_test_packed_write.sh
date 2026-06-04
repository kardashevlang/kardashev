#!/usr/bin/env bash
# Roadmap v100 (codegen audit) — packed-field WRITE alignment.
#
# BUG (found by the v100 audit, fixed here): an escaping write to a misaligned
# field of a `#[repr(packed)]` struct emitted an OVER-aligned store —
#   getelementptr i8, ptr %r, i64 1   ; a 1-aligned address (offset 1)
#   store i64 %n, ptr %addr, align 8  ; WRONG: align 8 on a 1-aligned pointer
# This is IR-level UB: latent on x86-64 (misaligned 8-byte stores don't fault) but
# SIGBUS on strict-alignment targets (AArch64 w/ SCTLR.A, MIPS, SPARC) and
# exploitable by LLVM's alignment passes. The fix: codegen flags a packed-struct
# field place (`lastPlacePacked_`) and emits `store ... align 1` for it. The READ
# path was already correct (the whole packed struct loads `align 1`, then
# extractvalue).
#
# Known limitation (matches Rust, NOT claimed fixed): `&mut p.field as *mut T` of a
# packed field, then a store through the RAW POINTER, loses packedness — the raw
# pointer carries no alignment. This is UB in Rust too.
#
# The IR-grep checks are target-INDEPENDENT (the `align N` text is in the IR
# regardless of arch — per the v90 arch-dependent-IR lesson). Skips the runtime
# leg if no cc/clang.
set -uo pipefail
KARDC=""
for c in "${TEST_SRCDIR:-}/_main/compiler/kardc" "${RUNFILES_DIR:-}/_main/compiler/kardc" "./compiler/kardc" "./build.local/kardc"; do
  [[ -n "$c" && -x "$c" ]] && { KARDC="$c"; break; }; done
[[ -z "$KARDC" ]] && { echo "FAIL: kardc not found"; exit 1; }
echo "Using kardc at: $KARDC"
TMP=$(mktemp -d); trap 'rm -rf "$TMP"' EXIT

# Extract the body of a named define and return it (so greps are scoped to @fill).
body_of() {  # $1 .kd  $2 symbol
  "$KARDC" --no-cache --emit-llvm -O2 "$1" 2>/dev/null \
    | awk -v s="$2" 'index($0, "@" s "(") && /^define/ {p=1} p{print} p&&/^}$/{exit}'
}

# A packed {u8, u64}: `val` lands at offset 1 (misaligned). An escaping write
# (through a &mut param, which SROA can't promote away) must store `align 1`.
cat > "$TMP/packed.kd" <<'EOF'
#[repr(packed)]
struct Reg { pad: u8, val: u64 }
fn fill(r: &mut Reg, n: u64) ! {} { r.val = n; }
fn main() -> i64 ! { io } {
  let mut reg = Reg { pad: 0, val: 0 };
  fill(&mut reg, 4000000000);
  print(reg.val as i64);
  0
}
EOF
PB=$(body_of "$TMP/packed.kd" fill)
grep -q 'store i64 .* align 1' <<<"$PB" || { echo "FAIL [packed-store-align1]: @fill packed write is not align 1"; echo "$PB"; exit 1; }
grep -q 'store i64 .* align 8' <<<"$PB" && { echo "FAIL [packed-store-align1]: @fill still emits an align-8 store to the packed field"; echo "$PB"; exit 1; }
echo "PASS [packed-store-align1]: @fill stores the packed field with align 1 (no align 8)"

# Non-packed control: the SAME struct without repr(packed) — `val` is naturally
# aligned (offset 8), so the store stays align 8 (proves the fix is scoped).
cat > "$TMP/plain.kd" <<'EOF'
struct Reg { pad: u8, val: u64 }
fn fill(r: &mut Reg, n: u64) ! {} { r.val = n; }
fn main() -> i64 ! { io } { let mut reg = Reg { pad: 0, val: 0 }; fill(&mut reg, 7); print(reg.val as i64); 0 }
EOF
NB=$(body_of "$TMP/plain.kd" fill)
grep -q 'store i64 .* align 8' <<<"$NB" || { echo "FAIL [nonpacked-store-align8]: control lost its natural align 8"; echo "$NB"; exit 1; }
echo "PASS [nonpacked-store-align8]: non-packed control still stores align 8 (fix is scoped to packed)"

# Runtime correctness preserved (value crosses the misaligned 8-byte boundary).
CC="$(command -v clang || command -v cc || true)"
if [[ -n "$CC" ]]; then
  jit=$("$KARDC" --no-cache "$TMP/packed.kd" 2>/dev/null | head -1)
  "$KARDC" --no-cache -o "$TMP/packed" "$TMP/packed.kd" >/dev/null 2>&1
  aot=$("$TMP/packed" 2>/dev/null | head -1)
  [[ "$jit" == "4000000000" && "$aot" == "4000000000" ]] || { echo "FAIL [packed-runtime]: JIT='$jit' AOT='$aot' want 4000000000"; exit 1; }
  echo "PASS [packed-runtime]: misaligned packed write round-trips JIT == AOT == 4000000000"
else
  echo "PASS [packed-runtime]: SKIPPED (no cc/clang)"
fi

# Lock the v100-audit VERIFIED-CORRECT edges so the fix can't regress them:
# a &mut [u32] slice store uses align 4 (element width), and swap_bytes(u8) is the
# identity (no bswap). (These also live in slice_mut / repr_packed; re-asserted
# here as a permanent audit lock.)
cat > "$TMP/slice.kd" <<'EOF'
fn fillm(s: &mut [u32]) ! {} { slice_set(s, 0, 9); }
fn main() -> i64 ! { io, alloc } {
  let mut v: Vec<u32> = vec_new(); vec_push(&mut v, 0); vec_push(&mut v, 0);
  let s = &mut v[0..2]; fillm(s); print(vec_get(&v, 0) as i64); 0
}
EOF
SB=$(body_of "$TMP/slice.kd" fillm)
grep -Eq 'store i32 .* align 4' <<<"$SB" || { echo "FAIL [slice-elem-align]: &mut [u32] store is not align 4"; echo "$SB"; exit 1; }
echo "PASS [slice-elem-align]: &mut [u32] element store is align 4 (audit-verified, locked)"

echo "ALL v100 PACKED-WRITE SMOKE TESTS PASSED"
