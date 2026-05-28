#!/usr/bin/env bash
# Phase 17b smoke test: de-i64-ifying the stdlib's generic containers.
#
# Proves the result/value type is threaded generically through the runtime,
# rather than hard-wired to i64, in both JIT and AOT:
#   (1) Future<T> — an `async fn -> T` whose result is computed from awaited
#       values + locals returns the right T through `.await` and `block_on`,
#       for T = bool (true AND false) and T = a small struct.
#   (2) The pre-existing i64 async path still works (no regression).
#   (3) HashMap<i64, V> — generic value type: V = bool and V = a struct,
#       insert + get back the right value (Some/None), len.
#   (4) The pre-existing HashMap<i64, i64> path still works (no regression).
#
# AOT runs use --no-cache so the executable is always freshly compiled by THIS
# kardc (the content-addressed AOT cache is keyed on source, not the compiler).
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

# jit_expect NAME FILE EXPECTED  — run JIT, compare full stdout.
jit_expect() {
    local name="$1" file="$2" expected="$3"
    local got
    got=$("$KARDC" "$file")
    if [[ "$got" != "$expected" ]]; then
        echo "FAIL [$name] JIT: expected:"; printf '%s\n' "$expected"
        echo "got:"; printf '%s\n' "$got"
        exit 1
    fi
}

# aot_expect NAME FILE EXPECTED_STDOUT EXPECTED_RC — build (no cache) + run.
aot_expect() {
    local name="$1" file="$2" expected_out="$3" expected_rc="$4"
    "$KARDC" --no-cache -o "$TMP/$name" "$file" 2>/dev/null
    local out rc
    set +e
    out=$("$TMP/$name")
    rc=$?
    set -e
    if [[ "$out" != "$expected_out" ]]; then
        echo "FAIL [$name] AOT stdout: expected:"; printf '%s\n' "$expected_out"
        echo "got:"; printf '%s\n' "$out"
        exit 1
    fi
    if [[ "$rc" != "$expected_rc" ]]; then
        echo "FAIL [$name] AOT exit: expected $expected_rc, got $rc"
        exit 1
    fi
}

# ===========================================================================
# 1. Future<bool>: an async fn returning bool, true AND false, via block_on.
# ===========================================================================
cat > "$TMP/future_bool.kd" <<'EOF'
async fn ab() -> bool { let x = yield_now(1).await; x == 1 }
async fn cd() -> bool { let x = yield_now(2).await; x == 1 }
fn main() -> i64 ! { io } {
    let r1 = block_on(ab());   // true  -> 1
    let r2 = block_on(cd());   // false -> 0
    let a = if r1 { 1 } else { 0 };
    let b = if r2 { 1 } else { 0 };
    print(a);
    print(b);
    0
}
EOF
# JIT also prints main's return value (0) on its own trailing line.
jit_expect "future_bool" "$TMP/future_bool.kd" $'1\n0\n0'
aot_expect "future_bool" "$TMP/future_bool.kd" $'1\n0' 0
echo "PASS [future_bool]: async fn -> bool; block_on(ab())=true, block_on(cd())=false (JIT + AOT)"

# ===========================================================================
# 2. Future<struct>: an async fn returning a struct built from two awaited
#    values; block_on returns it; read fields.
# ===========================================================================
cat > "$TMP/future_struct.kd" <<'EOF'
struct P { x: i64, y: i64 }
async fn mk() -> P {
    let a = yield_now(10).await;
    let b = yield_now(20).await;
    P { x: a, y: b }        // a survives the 2nd suspension (frame-promoted)
}
fn main() -> i64 ! { io } {
    let p = block_on(mk());
    print(p.x);             // 10
    print(p.y);             // 20
    p.x + p.y               // 30
}
EOF
jit_expect "future_struct" "$TMP/future_struct.kd" $'10\n20\n30'
aot_expect "future_struct" "$TMP/future_struct.kd" $'10\n20' 30
echo "PASS [future_struct]: async fn -> struct from 2 awaited values; block_on reads x=10,y=20 (JIT + AOT)"

# ===========================================================================
# 3. i64 async still works under the generalized machinery (no regression).
# ===========================================================================
cat > "$TMP/future_i64.kd" <<'EOF'
async fn add(a: i64, b: i64) -> i64 { a + b }
async fn double(n: i64) -> i64 { add(n, n).await }
fn main() -> i64 ! { io } {
    let v = block_on(double(21));
    print(v);               // 42
    v
}
EOF
jit_expect "future_i64" "$TMP/future_i64.kd" $'42\n42'
aot_expect "future_i64" "$TMP/future_i64.kd" $'42' 42
echo "PASS [future_i64]: existing i64 async chain still yields 42 (JIT + AOT)"

# ===========================================================================
# 4. HashMap<i64, bool>: insert (1->true),(2->false); get(1)=Some(true),
#    get(2)=Some(false), get(9)=None.
# ===========================================================================
cat > "$TMP/hashmap_bool.kd" <<'EOF'
fn main() -> i64 ! { alloc, io } {
    let m = hashmap_new();
    hashmap_insert(&mut m, 1, true);
    hashmap_insert(&mut m, 2, false);
    let a = match hashmap_get(&m, 1) { Some(v) => if v { 1 } else { 0 }, None => 0 - 1 };
    let b = match hashmap_get(&m, 2) { Some(v) => if v { 1 } else { 0 }, None => 0 - 1 };
    let c = match hashmap_get(&m, 9) { Some(v) => if v { 1 } else { 0 }, None => 0 - 1 };
    print(a);   // 1  (Some(true))
    print(b);   // 0  (Some(false))
    print(c);   // -1 (None)
    hashmap_len(&m)   // 2
}
EOF
jit_expect "hashmap_bool" "$TMP/hashmap_bool.kd" $'1\n0\n-1\n2'
aot_expect "hashmap_bool" "$TMP/hashmap_bool.kd" $'1\n0\n-1' 2
echo "PASS [hashmap_bool]: HashMap<i64,bool> get(1)=Some(true), get(2)=Some(false), get(9)=None, len 2 (JIT + AOT)"

# ===========================================================================
# 5. HashMap<i64, P> for a struct P{x,y}: insert + get back a struct, fields.
# ===========================================================================
cat > "$TMP/hashmap_struct.kd" <<'EOF'
struct P { x: i64, y: i64 }
fn main() -> i64 ! { alloc, io } {
    let m = hashmap_new();
    hashmap_insert(&mut m, 5, P { x: 50, y: 51 });
    hashmap_insert(&mut m, 6, P { x: 60, y: 61 });
    hashmap_insert(&mut m, 5, P { x: 500, y: 501 });   // overwrite
    match hashmap_get(&m, 5) { Some(p) => { print(p.x); print(p.y); 0 }, None => { print(0 - 1); 0 } };
    match hashmap_get(&m, 6) { Some(p) => { print(p.x); print(p.y); 0 }, None => { print(0 - 1); 0 } };
    let miss = match hashmap_get(&m, 9) { Some(p) => p.x, None => 0 - 1 };
    print(miss);          // -1 (None)
    hashmap_len(&m)       // 2
}
EOF
jit_expect "hashmap_struct" "$TMP/hashmap_struct.kd" $'500\n501\n60\n61\n-1\n2'
aot_expect "hashmap_struct" "$TMP/hashmap_struct.kd" $'500\n501\n60\n61\n-1' 2
echo "PASS [hashmap_struct]: HashMap<i64,P> insert/overwrite/get struct fields + None + len (JIT + AOT)"

# A type annotation in the two-arg HashMap<i64, V> surface form resolves.
cat > "$TMP/hashmap_annot.kd" <<'EOF'
fn build() -> HashMap<i64, bool> ! { alloc } {
    let m = hashmap_new();
    hashmap_insert(&mut m, 1, true);
    m
}
fn main() -> i64 ! { alloc, io } {
    let m = build();
    let a = match hashmap_get(&m, 1) { Some(v) => if v { 7 } else { 0 }, None => 0 - 1 };
    print(a);   // 7
    0
}
EOF
jit_expect "hashmap_annot" "$TMP/hashmap_annot.kd" $'7\n0'
aot_expect "hashmap_annot" "$TMP/hashmap_annot.kd" $'7' 0
echo "PASS [hashmap_annot]: HashMap<i64, bool> two-arg type annotation resolves + round-trips (JIT + AOT)"

# ===========================================================================
# 6. HashMap<i64, i64> still works (no regression) — incl. rehash.
# ===========================================================================
cat > "$TMP/hashmap_i64.kd" <<'EOF'
fn main() -> i64 ! { alloc, io } {
    let m = hashmap_new();
    let mut i = 0;
    while i < 50 { hashmap_insert(&mut m, i, i * 10); i = i + 1; }
    print(hashmap_len(&m));   // 50
    let mut bad = 0;
    let mut j = 0;
    while j < 50 {
        let got = match hashmap_get(&m, j) { Some(v) => v, None => 0 - 1 };
        let delta = if got == j * 10 { 0 } else { 1 };
        bad = bad + delta;
        j = j + 1;
    }
    print(bad);               // 0
    bad
}
EOF
jit_expect "hashmap_i64" "$TMP/hashmap_i64.kd" $'50\n0\n0'
aot_expect "hashmap_i64" "$TMP/hashmap_i64.kd" $'50\n0' 0
echo "PASS [hashmap_i64]: existing HashMap<i64,i64> 50-key rehash still retrievable, len 50 (JIT + AOT)"

echo "PASS: generic Future<T> (bool/struct/i64) + generic HashMap<i64,V> (bool/struct/i64) work in JIT + AOT"
