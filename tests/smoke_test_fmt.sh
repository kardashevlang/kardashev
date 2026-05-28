#!/usr/bin/env bash
# Phase 14a smoke test: the `kardfmt` source formatter.
#
# Exercises a deliberately messy-but-valid kardashev program covering many
# constructs (struct/impl/trait, dyn dispatch, enum + match, for/while loops,
# a closure, an effect row, nested expressions) and asserts:
#   (a) idempotency  — formatting twice is byte-for-byte identical.
#   (b) round-trip   — the formatted output compiles with kardc and runs to
#                       the SAME result as the original messy source.
#   (c) --check      — exits 0 on already-formatted input, non-zero on messy.
set -euo pipefail

# Locate kardc (re-compiles formatted output) and kardfmt (under test). Both
# binaries are symlinked into compiler/ by the Makefile.local / Bazel harness.
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
KARDFMT=$(find_bin kardfmt) || { echo "FAIL: kardfmt binary not found"; exit 1; }
echo "Using kardc at:   $KARDC"
echo "Using kardfmt at: $KARDFMT"

TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

# A messy program: collapsed whitespace, multiple stmts per line, irregular
# indentation. Semantically valid and runnable.
#   describe(&sq)=25, describe(&r)=12, classify(B(7))=7,
#   sumloop(4): for 0..4 => 0+1+2+3 = 6; while k<3 => 1+2+3 = 6; total = 12.
#   main returns 25 + 12 + 7 + 12 = 56  (also prints 12 from sumloop).
cat > "$TMP/messy.kd" <<'EOF'
trait Shape{fn area(&self)->i64;}
struct Sq{side:i64}
struct Rect{w:i64,h:i64}
impl Shape for Sq{fn area(&self)->i64{self.side*self.side}}
impl Shape for Rect{fn area(&self)->i64{self.w*self.h}}
fn describe(s:&dyn Shape)->i64{s.area()}
enum Tag{A,B(i64)}
fn classify(t:Tag)->i64{match t{A=>1,B(n)=>n,}}
fn sumloop(n:i64)->i64!{io}{
let mut total=0;
for i in 0..n{total=total+i;}
let mut k=0;
while k<3{k=k+1;total=total+k;}
let bump=|x:i64|x+10;
print(total);
total
}
fn main()->i64!{io}{
let sq=Sq{side:5};let r=Rect{w:3,h:4};
let a=describe(&sq);let b=describe(&r);
let c=classify(B(7));
let d=sumloop(4);
a+b+c+d
}
EOF

# --- Baseline: run the original messy program. ---
ORIG_OUT=$("$KARDC" "$TMP/messy.kd")
ORIG_RC=$?
if [[ "$ORIG_RC" -ne 0 ]]; then
    echo "FAIL: original program did not compile/run (rc=$ORIG_RC)"
    echo "$ORIG_OUT"
    exit 1
fi
echo "Original output: $(echo "$ORIG_OUT" | tr '\n' ' ')"

# --- (c) --check must reject the messy input. ---
set +e
"$KARDFMT" --check "$TMP/messy.kd" >/dev/null 2>&1
CHECK_MESSY_RC=$?
set -e
if [[ "$CHECK_MESSY_RC" -eq 0 ]]; then
    echo "FAIL: --check returned 0 on a messy (unformatted) file"
    exit 1
fi
echo "--check on messy input: exit $CHECK_MESSY_RC (non-zero, correct)"

# --- Format once. ---
"$KARDFMT" "$TMP/messy.kd" > "$TMP/fmt1.kd"

# --- (a) Idempotency: formatting the formatted output is identical. ---
"$KARDFMT" "$TMP/fmt1.kd" > "$TMP/fmt2.kd"
if ! diff -u "$TMP/fmt1.kd" "$TMP/fmt2.kd"; then
    echo "FAIL: formatter is not idempotent (fmt1 != fmt2)"
    exit 1
fi
echo "Idempotency: fmt(fmt(src)) == fmt(src) (byte-identical)"

# --- (c) --check must accept the already-formatted file. ---
set +e
"$KARDFMT" --check "$TMP/fmt1.kd"
CHECK_FMT_RC=$?
set -e
if [[ "$CHECK_FMT_RC" -ne 0 ]]; then
    echo "FAIL: --check returned $CHECK_FMT_RC on already-formatted input (expected 0)"
    exit 1
fi
echo "--check on formatted input: exit 0 (correct)"

# --- (b) Round-trip: formatted output compiles + runs to the same result. ---
FMT_OUT=$("$KARDC" "$TMP/fmt1.kd")
FMT_RC=$?
if [[ "$FMT_RC" -ne 0 ]]; then
    echo "FAIL: formatted program did not compile/run (rc=$FMT_RC)"
    echo "$FMT_OUT"
    exit 1
fi
if [[ "$FMT_OUT" != "$ORIG_OUT" ]]; then
    echo "FAIL: formatted output differs from original"
    echo "  original:  $(echo "$ORIG_OUT" | tr '\n' ' ')"
    echo "  formatted: $(echo "$FMT_OUT" | tr '\n' ' ')"
    exit 1
fi
echo "Round-trip: formatted program runs to the identical result ($(echo "$FMT_OUT" | tr '\n' ' '))"

# --- AOT round-trip too: the formatted source links + exits with main()'s value. ---
"$KARDC" -o "$TMP/fmt_prog" "$TMP/fmt1.kd"
set +e
"$TMP/fmt_prog" >/dev/null
AOT_RC=$?
set -e
if [[ "$AOT_RC" -ne 56 ]]; then
    echo "FAIL: AOT of formatted program exited $AOT_RC (expected 56)"
    exit 1
fi
echo "AOT round-trip: formatted program exits 56"

echo "PASS: kardfmt is idempotent, round-trips through compile+run, and --check works"
