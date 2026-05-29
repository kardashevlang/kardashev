#!/usr/bin/env bash
# Phase 27 smoke test: the string toolkit — str_eq, str_substring,
# int_to_string, print_no_nl, println — round-trips through JIT + AOT.
#
# Covers: byte-exact equality (equal / unequal / different-length / empty),
# substring extraction with start/len CLAMPING (in-range and past-the-end),
# decimal formatting of positive and negative i64, composing a single output
# line from print_no_nl + println, and that int_to_string returns a real
# heap-owned String a later string_push_str can grow.
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
    echo "FAIL: kardc binary not found in runfiles"
    exit 1
fi

echo "Using kardc at: $KARDC"

TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

# Run a program through JIT (kardc prints stdout then main's return) and AOT
# (stdout, with main's return as the process exit code), and check both.
#   check <name> <file> <jit-stdout> <aot-stdout> <aot-exit>
check() {
    local name=$1 file=$2 jit_want=$3 aot_stdout_want=$4 aot_rc_want=$5
    local jit_out
    jit_out=$("$KARDC" "$file")
    if [[ "$jit_out" != "$jit_want" ]]; then
        echo "FAIL [$name/jit]: output mismatch"
        echo "expected:"; printf '%s\n' "$jit_want"
        echo "got:";      printf '%s\n' "$jit_out"
        exit 1
    fi
    "$KARDC" -o "$TMP/${name}_bin" "$file" >/dev/null
    set +e
    local aot_stdout rc
    aot_stdout=$("$TMP/${name}_bin")
    "$TMP/${name}_bin" >/dev/null
    rc=$?
    set -e
    if [[ "$aot_stdout" != "$aot_stdout_want" ]]; then
        echo "FAIL [$name/aot]: stdout mismatch"
        echo "expected:"; printf '%s\n' "$aot_stdout_want"
        echo "got:";      printf '%s\n' "$aot_stdout"
        exit 1
    fi
    if [[ "$rc" -ne "$aot_rc_want" ]]; then
        echo "FAIL [$name/aot]: exit code was $rc (expected $aot_rc_want)"
        exit 1
    fi
    echo "PASS [$name]: JIT + AOT"
}

# --- 1. comprehensive: eq, formatting, substring (in-range + clamped), and
#        print_no_nl composing one line with the following println. ---
cat > "$TMP/toolkit.kd" <<'EOF'
fn main() -> i64 ! { io, alloc } {
    let a = "hello";
    let b = "hello";
    let c = "world";
    let eq1 = str_eq(&a, &b);             // true
    let eq2 = str_eq(&a, &c);             // false
    let s = int_to_string(42);
    print_no_nl(&s);                       // "42" (no newline)
    let bang = "!";
    println(&bang);                        // "!\n"   => first line "42!"
    let sub = str_substring(&a, 1, 3);     // "ell"
    println(&sub);
    let exp = "ell";
    let eqs = str_eq(&sub, &exp);          // true
    let neg = int_to_string(0 - 7);        // "-7"
    println(&neg);
    let oob = str_substring(&a, 3, 100);   // clamps to "lo"
    println(&oob);
    let r = (if eq1 { 1 } else { 0 })
          + (if eq2 { 1000 } else { 0 })
          + (if eqs { 10 } else { 0 })
          + str_len(&s) + str_len(&neg) + str_len(&oob);
    r                                       // 1 + 0 + 10 + 2 + 2 + 2 = 17
}
EOF
check toolkit "$TMP/toolkit.kd" $'42!\nell\n-7\nlo\n17' $'42!\nell\n-7\nlo' 17

# --- 2. str_eq edge cases (pure — no effect row needed). ---
cat > "$TMP/eq.kd" <<'EOF'
fn main() -> i64 {
    let e1 = "";
    let e2 = "";
    let x = "x";
    let xy = "xy";
    (if str_eq(&e1, &e2) { 1 } else { 0 })       // empty == empty -> 1
  + (if str_eq(&e1, &x)  { 1000 } else { 0 })    // "" == "x"      -> 0
  + (if str_eq(&x, &xy)  { 1000 } else { 0 })    // len mismatch   -> 0
  + (if str_eq(&x, &x)   { 10 } else { 0 })      // x == x         -> 10
}
EOF
check eq "$TMP/eq.kd" $'11' '' 11

# --- 3. int_to_string returns a heap-owned String string_push_str can grow. ---
cat > "$TMP/grow.kd" <<'EOF'
fn main() -> i64 ! { io, alloc } {
    let mut s = int_to_string(12);   // "12"
    string_push_str(&mut s, "34");   // "1234"
    println(&s);                      // "1234"
    let want = "1234";
    let ok = str_eq(&s, &want);       // true
    (if ok { 100 } else { 0 }) + str_len(&s)   // 100 + 4 = 104
}
EOF
check grow "$TMP/grow.kd" $'1234\n104' $'1234' 104

echo "PASS: string toolkit (str_eq / str_substring / int_to_string / print_no_nl / println) works in JIT + AOT, with clamping, signed formatting, line composition, and heap-owned growth"
