#!/usr/bin/env bash
# Phase 14a smoke test: DWARF debug info behind the `-g` flag.
#
# Asserts:
#   (a) `kardc -g -o out prog.kd` produces an executable whose DWARF contains
#       a DW_TAG_compile_unit, at least one DW_TAG_subprogram, and line-table
#       rows.
#   (b) WITHOUT `-g`, the produced executable has NO debug info (clean
#       separation — the `-g` path is purely additive).
#   (c) the program runs to the same result with and without `-g`.
#
# DWARF is inspected with llvm-dwarfdump. If it isn't on PATH we locate it via
# `llvm-config --bindir` (the same toolchain the build used). If it still
# can't be found the test fails loudly rather than silently passing.
set -euo pipefail

find_bin() {
    local name=$1
    for candidate in \
        "${TEST_SRCDIR:-}/_main/compiler/$name" \
        "${TEST_SRCDIR:-}/kardashev/compiler/$name" \
        "${RUNFILES_DIR:-}/_main/compiler/$name" \
        "${RUNFILES_DIR:-}/kardashev/compiler/$name" \
        "./compiler/$name" \
        "./build.local/$name"; do
        if [[ -n "$candidate" && -x "$candidate" ]]; then
            echo "$candidate"
            return 0
        fi
    done
    return 1
}

KARDC=$(find_bin kardc) || { echo "FAIL: kardc binary not found"; exit 1; }
echo "Using kardc at: $KARDC"

# Locate llvm-dwarfdump: PATH first, then llvm-config --bindir.
DWARFDUMP=""
if command -v llvm-dwarfdump >/dev/null 2>&1; then
    DWARFDUMP=$(command -v llvm-dwarfdump)
elif command -v llvm-config >/dev/null 2>&1; then
    bindir=$(llvm-config --bindir 2>/dev/null || true)
    if [[ -n "$bindir" && -x "$bindir/llvm-dwarfdump" ]]; then
        DWARFDUMP="$bindir/llvm-dwarfdump"
    fi
fi
if [[ -z "$DWARFDUMP" ]]; then
    echo "FAIL: llvm-dwarfdump not found (PATH or llvm-config --bindir)"
    exit 1
fi
echo "Using llvm-dwarfdump at: $DWARFDUMP"

TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

cat > "$TMP/prog.kd" <<'EOF'
fn add(a: i64, b: i64) -> i64 {
    let s = a + b;
    s
}
fn main() -> i64 {
    let x = add(3, 4);
    let y = x + 1;
    y
}
EOF

# --- Build with and without -g. main() returns add(3,4)+1 = 8. ---
"$KARDC" -g -o "$TMP/prog_g"   "$TMP/prog.kd"
"$KARDC"    -o "$TMP/prog_nog" "$TMP/prog.kd"
for b in "$TMP/prog_g" "$TMP/prog_nog"; do
    if [[ ! -x "$b" ]]; then
        echo "FAIL: kardc did not produce executable $b"
        exit 1
    fi
done

# --- (c) Same runtime result with and without -g. ---
set +e
"$TMP/prog_g";   RC_G=$?
"$TMP/prog_nog"; RC_NOG=$?
set -e
if [[ "$RC_G" -ne 8 || "$RC_NOG" -ne 8 ]]; then
    echo "FAIL: expected both binaries to exit 8 (got -g=$RC_G, no-g=$RC_NOG)"
    exit 1
fi
echo "Runtime: both -g and no-g binaries exit 8 (identical result)"

# --- (a) -g binary carries a compile unit + subprogram + line table. ---
INFO_G=$("$DWARFDUMP" --debug-info "$TMP/prog_g" 2>/dev/null)
LINE_G=$("$DWARFDUMP" --debug-line "$TMP/prog_g" 2>/dev/null)

if ! grep -q "DW_TAG_compile_unit" <<<"$INFO_G"; then
    echo "FAIL: -g binary has no DW_TAG_compile_unit"
    exit 1
fi
echo "Found DW_TAG_compile_unit"

SUBPROG_COUNT=$(grep -c "DW_TAG_subprogram" <<<"$INFO_G" || true)
if [[ "$SUBPROG_COUNT" -lt 1 ]]; then
    echo "FAIL: -g binary has no DW_TAG_subprogram"
    exit 1
fi
echo "Found $SUBPROG_COUNT DW_TAG_subprogram entries"

# The user's `add` and `main` should both be present as subprograms.
if ! grep -qE 'DW_AT_name[[:space:]]*\("add"\)' <<<"$INFO_G"; then
    echo "FAIL: -g binary missing subprogram for 'add'"
    exit 1
fi
if ! grep -qE 'DW_AT_name[[:space:]]*\("main"\)' <<<"$INFO_G"; then
    echo "FAIL: -g binary missing subprogram for 'main'"
    exit 1
fi
echo "Subprograms include both 'add' and 'main'"

# Line table: at least one address->line row (rows look like `0x... <line> <col> ...`).
LINE_ROWS=$(grep -cE '^0x[0-9a-f]+[[:space:]]+[0-9]+[[:space:]]+[0-9]+' <<<"$LINE_G" || true)
if [[ "$LINE_ROWS" -lt 1 ]]; then
    echo "FAIL: -g binary has no line-table rows"
    echo "$LINE_G" | head
    exit 1
fi
echo "Found $LINE_ROWS line-table rows"

# --- (b) no-g binary must have NO debug info. ---
INFO_NOG=$("$DWARFDUMP" --debug-info "$TMP/prog_nog" 2>/dev/null)
NOG_SUBPROG=$(grep -c "DW_TAG_subprogram" <<<"$INFO_NOG" || true)
NOG_CU=$(grep -c "DW_TAG_compile_unit" <<<"$INFO_NOG" || true)
if [[ "$NOG_SUBPROG" -ne 0 || "$NOG_CU" -ne 0 ]]; then
    echo "FAIL: no-g binary unexpectedly contains debug info"
    echo "  compile_units=$NOG_CU subprograms=$NOG_SUBPROG"
    exit 1
fi
echo "Clean separation: no-g binary has zero compile units / subprograms"

echo "PASS: -g emits a DWARF compile unit + subprograms + line table; no-g stays debug-info-free"
