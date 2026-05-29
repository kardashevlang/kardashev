#!/usr/bin/env bash
# Phase 37 smoke test: Display/`to_string` + de-i64'd iteration + generic slices.
#   1. The prelude `Display` trait formats i64 / bool / String uniformly through
#      a bounded generic `show<T: Display>`.
#   2. A user `impl Display for Json` RECURSIVELY serializes a nested enum to a
#      String (the capstone serializer shape) — built on str builders.
#   3. de-i64'd iteration: a custom `Iterator<String>` drives a `for s in it`
#      whose loop variable is a real `String` (not i64).
#   4. A generic `&[String]` slice is read (len + element borrow).
# JIT + AOT.
set -euo pipefail

KARDC=""
for candidate in \
    "${TEST_SRCDIR:-}/_main/compiler/kardc" \
    "${TEST_SRCDIR:-}/kardashev/compiler/kardc" \
    "${RUNFILES_DIR:-}/_main/compiler/kardc" \
    "${RUNFILES_DIR:-}/kardashev/compiler/kardc" \
    "./compiler/kardc"; do
    if [[ -n "$candidate" && -x "$candidate" ]]; then KARDC="$candidate"; break; fi
done
[[ -z "$KARDC" ]] && { echo "FAIL: kardc binary not found"; exit 1; }
echo "Using kardc at: $KARDC"

TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

check() { # name file expected
    local n=$1 f=$2 w=$3 jit
    jit=$("$KARDC" "$f")
    [[ "$jit" != "$w" ]] && { echo "FAIL [$n/jit]: expected $w got $jit"; exit 1; }
    "$KARDC" --no-cache -o "$TMP/$n" "$f" >/dev/null
    set +e; "$TMP/$n" >/dev/null; local r=$?; set -e
    local wm=$(( ( (w % 256) + 256 ) % 256 ))
    [[ "$r" -ne "$wm" ]] && { echo "FAIL [$n/aot]: exit $r expected $wm"; exit 1; }
    echo "PASS [$n]: JIT=$jit, AOT matches"
}

# --- 1. prelude Display over i64 / bool / String via a bounded generic ---
cat > "$TMP/disp.kd" <<'EOF'
fn width<T: Display>(x: &T) -> i64 ! { alloc } { let s = x.to_string(); str_len(&s) }
fn main() -> i64 ! { alloc } {
    let n = 42; let b = true; let st = int_to_string(999);
    width(&n) + width(&b) + width(&st)     // 2 + 4 + 3 = 9
}
EOF
check disp "$TMP/disp.kd" 9

# --- 2. user impl Display for a recursive enum: RECURSIVE serialize ---
cat > "$TMP/json.kd" <<'EOF'
enum Json { JInt(i64), JArr(Vec<Json>) }
impl Display for Json {
    fn to_string(&self) -> String ! { alloc } {
        match self {
            JInt(n) => int_to_string(*n),
            JArr(items) => {
                let mut out = string_new();
                string_push_str(&mut out, "[");
                let mut i = 0;
                let len = vec_len(items);
                while i < len {
                    if i > 0 { string_push_str(&mut out, ","); } else {}
                    let s = vec_get_ref(items, i).to_string();   // recurse
                    string_push_str(&mut out, s);
                    i = i + 1;
                }
                string_push_str(&mut out, "]");
                out
            }
        }
    }
}
fn main() -> i64 ! { alloc } {
    let mut a = vec_new();
    vec_push(&mut a, JInt(1));
    vec_push(&mut a, JInt(22));
    let mut b = vec_new();
    vec_push(&mut b, JInt(3));
    vec_push(&mut b, JArr(a));      // nested: [3,[1,22]]
    let j = JArr(b);
    let s = j.to_string();           // "[3,[1,22]]" — 10 chars
    str_len(&s)
}
EOF
check json "$TMP/json.kd" 10

# --- 3. de-i64'd iteration: custom Iterator<String>, for-in binds String ---
cat > "$TMP/iter.kd" <<'EOF'
struct SI { v: Vec<String>, i: i64 }
impl Iterator<String> for SI {
    fn next(&mut self) -> Option<String> ! { alloc } {
        if self.i < vec_len(&self.v) {
            let s = clone(vec_get_ref(&self.v, self.i));
            self.i = self.i + 1;
            Some(s)
        } else { None }
    }
}
fn main() -> i64 ! { alloc } {
    let mut v = vec_new();
    vec_push(&mut v, int_to_string(11));
    vec_push(&mut v, int_to_string(222));
    let it = SI { v: v, i: 0 };
    let mut total = 0;
    for s in it { total = total + str_len(&s); }   // s : String (not i64)
    total                                            // 2 + 3 = 5
}
EOF
check iter "$TMP/iter.kd" 5

# --- 4. generic &[String] slice read ---
cat > "$TMP/slice.kd" <<'EOF'
fn main() -> i64 ! { alloc } {
    let mut v = vec_new();
    vec_push(&mut v, int_to_string(11));
    vec_push(&mut v, int_to_string(2222));
    vec_push(&mut v, int_to_string(333));
    let sl = &v[0..3];
    let a = slice_len(sl);                     // 3
    let b = str_len(slice_get_ref(sl, 1));     // "2222" -> 4
    a + b                                        // 7
}
EOF
check slice "$TMP/slice.kd" 7

echo "PASS: Phase 37 — Display + de-i64 iteration + generic slices (JIT + AOT)"
