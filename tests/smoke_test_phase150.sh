#!/usr/bin/env bash
# v27 Phase 150: the `Debug` trait + `{:?}` format spec, wired to format-args.
# Debug is distinct from Display: a String is QUOTED + escaped, a char is
# single-quoted. Built-in impls for the scalars + String; `#[derive(Debug)]`
# synthesizes one for a struct (`Name { f: <dbg>, ... }`) / enum
# (`Variant(<dbg>, ...)`). `{:?}` in format!/println! routes to fmt_debug. AOT
# (the binary's stdout is the program's true output).
set -uo pipefail
KARDC=""
for c in "${TEST_SRCDIR:-}/_main/compiler/kardc" "${TEST_SRCDIR:-}/kardashev/compiler/kardc" \
         "${RUNFILES_DIR:-}/_main/compiler/kardc" "${RUNFILES_DIR:-}/kardashev/compiler/kardc" \
         "./compiler/kardc" "./build.local/kardc"; do
    [[ -n "$c" && -x "$c" ]] && { KARDC="$c"; break; }; done
[[ -z "$KARDC" ]] && { echo "FAIL: kardc not found"; exit 1; }
echo "Using kardc at: $KARDC"
TMP=$(mktemp -d); trap 'rm -rf "$TMP"' EXIT
out_eq() { "$KARDC" "$2" >/dev/null 2>&1 || { echo "FAIL [$1/jit]: run"; exit 1; }
    "$KARDC" --no-cache -o "$TMP/b" "$2" >/dev/null 2>&1 || { echo "FAIL [$1/aot]: compile"; exit 1; }
    local aot; aot=$("$TMP/b" 2>&1)
    [[ "$aot" == "$3" ]] || { echo "FAIL [$1/aot]: want '$3' got '$aot'"; exit 1; }
    echo "PASS [$1]: $4"; }

# 1) built-in Debug: String quoted+escaped, char single-quoted, scalars plain
cat > "$TMP/a.kd" <<'EOF'
fn main() -> i64 ! { io, alloc } {
    println!("{:?} {:?} {:?} {:?} {:?}", "a\tb", 'Q', 42, true, 1.5);
    0
}
EOF
out_eq builtins "$TMP/a.kd" '"a\tb" '"'Q'"' 42 true 1.5' "Debug: String quoted+escaped, char quoted, scalars"

# 2) #[derive(Debug)] for a struct
cat > "$TMP/s.kd" <<'EOF'
#[derive(Debug)]
struct Point { x: i64, y: i64 }
fn main() -> i64 ! { io, alloc } {
    println!("{:?}", Point { x: 3, y: 4 });
    0
}
EOF
out_eq derive_struct "$TMP/s.kd" "Point { x: 3, y: 4 }" "#[derive(Debug)] struct -> Name { f: v, ... }"

# 3) #[derive(Debug)] for an enum (unit / single / multi payload)
cat > "$TMP/e.kd" <<'EOF'
#[derive(Debug)]
enum Shape { Dot, Circle(i64), Rect(i64, i64) }
fn main() -> i64 ! { io, alloc } {
    println!("{:?} {:?} {:?}", Shape::Dot, Shape::Circle(5), Shape::Rect(2, 3));
    0
}
EOF
out_eq derive_enum "$TMP/e.kd" "Dot Circle(5) Rect(2, 3)" "#[derive(Debug)] enum -> Variant(payload...)"

# 4) Debug + Display differ (Display unquoted, Debug quoted) for a String
cat > "$TMP/m.kd" <<'EOF'
fn main() -> i64 ! { io, alloc } {
    let s = "hi";
    println!("display={} debug={:?}", s, s);
    0
}
EOF
out_eq vs_display "$TMP/m.kd" 'display=hi debug="hi"' "Display unquoted vs Debug quoted for the same String"

# 5) nested: a struct field that is itself derive(Debug)
cat > "$TMP/n.kd" <<'EOF'
#[derive(Debug)]
struct Inner { v: i64 }
#[derive(Debug)]
struct Outer { name: String, inner: Inner }
fn main() -> i64 ! { io, alloc } {
    println!("{:?}", Outer { name: "z", inner: Inner { v: 9 } });
    0
}
EOF
out_eq nested "$TMP/n.kd" 'Outer { name: "z", inner: Inner { v: 9 } }' "nested derive(Debug) recurses (String field quoted)"

echo "PASS: Phase 150 — Debug trait + {:?} + #[derive(Debug)]"
