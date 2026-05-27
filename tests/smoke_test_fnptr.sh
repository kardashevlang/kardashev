#!/usr/bin/env bash
# Phase 4.3 smoke test: first-class fn values flow through let bindings
# and conditional selection. Indirect call dispatches through the bound
# fn-pointer LLVM value at runtime.
set -euo pipefail

KARDC=""
for candidate in \
    "${TEST_SRCDIR:-}/_main/compiler/kardc" \
    "${TEST_SRCDIR:-}/kardashev/compiler/kardc" \
    "${RUNFILES_DIR:-}/_main/compiler/kardc" \
    "${RUNFILES_DIR:-}/kardashev/compiler/kardc" \
    "./compiler/kardc"; do
    if [[ -n "$candidate" && -x "$candidate" ]]; then
        KARDC="$candidate"
        break
    fi
done

if [[ -z "$KARDC" ]]; then
    echo "FAIL: kardc binary not found"
    exit 1
fi

echo "Using kardc at: $KARDC"

TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

cat > "$TMP/fnptr.kd" <<'EOF'
fn add(a: i64, b: i64) -> i64 { a + b }
fn mul(a: i64, b: i64) -> i64 { a * b }
fn main() -> i64 ! { io } {
    let plus = add;
    let times = mul;
    print(plus(3, 4));
    print(times(3, 4));
    let chosen = if 1 < 2 { plus } else { times };
    print(chosen(10, 20));
    0
}
EOF

OUT=$("$KARDC" "$TMP/fnptr.kd")
EXPECTED=$'7\n12\n30\n0'
if [[ "$OUT" != "$EXPECTED" ]]; then
    echo "FAIL: indirect-call output mismatch"
    echo "expected:"; echo "$EXPECTED"
    echo "got:"; echo "$OUT"
    exit 1
fi
echo "JIT: plus(3,4)=7, times(3,4)=12, chosen(10,20)=30"

"$KARDC" -o "$TMP/prog" "$TMP/fnptr.kd"
AOT_OUT=$("$TMP/prog")
EXPECTED_AOT=$'7\n12\n30'
if [[ "$AOT_OUT" != "$EXPECTED_AOT" ]]; then
    echo "FAIL: AOT indirect-call output mismatch"
    echo "expected:"; echo "$EXPECTED_AOT"
    echo "got:"; echo "$AOT_OUT"
    exit 1
fi

echo "PASS: first-class fn values + indirect call through let-binding"
