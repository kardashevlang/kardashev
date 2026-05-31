#!/usr/bin/env bash
# v27 Phase 149: built-in `format!` / `print!` / `println!` formatting forms.
# There is no general macro system yet, so these are recognized in the parser
# and desugared to string-building over string_new / string_push_str and the
# Display trait's `to_string`. `{}` is a Display hole; `{{`/`}}` are literal
# braces; placeholder/argument count is checked. JIT + AOT.
set -uo pipefail
KARDC=""
for c in "${TEST_SRCDIR:-}/_main/compiler/kardc" "${TEST_SRCDIR:-}/kardashev/compiler/kardc" \
         "${RUNFILES_DIR:-}/_main/compiler/kardc" "${RUNFILES_DIR:-}/kardashev/compiler/kardc" \
         "./compiler/kardc" "./build.local/kardc"; do
    [[ -n "$c" && -x "$c" ]] && { KARDC="$c"; break; }; done
[[ -z "$KARDC" ]] && { echo "FAIL: kardc not found"; exit 1; }
echo "Using kardc at: $KARDC"
TMP=$(mktemp -d); trap 'rm -rf "$TMP"' EXIT
# The AOT binary's stdout is the program's true output (the JIT additionally
# prints main's return value), so we assert on the AOT artifact; the JIT is
# still built + run to confirm it compiles + executes.
out_eq() { "$KARDC" "$2" >/dev/null 2>&1 || { echo "FAIL [$1/jit]: run"; exit 1; }
    "$KARDC" --no-cache -o "$TMP/b" "$2" >/dev/null 2>&1 || { echo "FAIL [$1/aot]: compile"; exit 1; }
    local aot; aot=$("$TMP/b" 2>&1)
    [[ "$aot" == "$3" ]] || { echo "FAIL [$1/aot]: want '$3' got '$aot'"; exit 1; }
    echo "PASS [$1]: $4"; }
expect_err() { local out; out=$("$KARDC" "$2" 2>&1)
    [[ $? -ne 0 ]] || { echo "FAIL [$1]: expected error, compiled"; exit 1; }
    echo "$out" | grep -qiE "$3" || { echo "FAIL [$1]: want /$3/, got: $out"; exit 1; }
    echo "PASS [$1]: $4"; }

# 1) println! with mixed Display types (i64, String, char, bool, f64)
cat > "$TMP/a.kd" <<'EOF'
fn main() -> i64 ! { io, alloc } {
    println!("i={} s={} c={} b={} f={}", 42, "hi", 'Z', true, 2.5);
    0
}
EOF
out_eq mixed "$TMP/a.kd" "i=42 s=hi c=Z b=true f=2.5" "println! over i64/String/char/bool/f64 via Display"

# 2) format! returns a usable String value
cat > "$TMP/f.kd" <<'EOF'
fn main() -> i64 ! { io, alloc } {
    let s = format!("[{}|{}]", 7, "x");
    print_no_nl(&s);
    print_no_nl(&format!(" len={}", str_len(&s)));
    0
}
EOF
out_eq format "$TMP/f.kd" "[7|x] len=5" "format! builds a String reusable as a value"

# 3) escaped braces + a literal-only format
cat > "$TMP/b.kd" <<'EOF'
fn main() -> i64 ! { io, alloc } {
    println!("{{not a hole}} but {} is", 1);
    print!("no-newline");
    println!("");
    0
}
EOF
out_eq braces "$TMP/b.kd" "{not a hole} but 1 is
no-newline" "{{ }} escape to literal braces; print! has no newline"

# 4) placeholder/argument count mismatch is a compile error
cat > "$TMP/m1.kd" <<'EOF'
fn main() -> i64 ! { io, alloc } { println!("{} {}", 1); 0 }
EOF
expect_err too_few "$TMP/m1.kd" "placeholder|argument" "too few arguments for placeholders"
cat > "$TMP/m2.kd" <<'EOF'
fn main() -> i64 ! { io, alloc } { println!("{}", 1, 2); 0 }
EOF
expect_err too_many "$TMP/m2.kd" "placeholder|argument" "too many arguments for placeholders"

# 5) the first arg must be a string literal
cat > "$TMP/nl.kd" <<'EOF'
fn main() -> i64 ! { io, alloc } { let f = "x"; println!(f); 0 }
EOF
expect_err non_literal "$TMP/nl.kd" "must be a string literal" "non-literal format string rejected"

echo "PASS: Phase 149 — format! / print! / println!"
