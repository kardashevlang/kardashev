#!/usr/bin/env bash
# Phase 28 smoke test: the Hash/Eq traits, multiple trait bounds (`T: A + B`),
# a generic HashMap<K,V> (i64 / String / user-type keys), and HashSet<T> —
# through JIT + AOT, plus the negative checks (non-hashable key rejected,
# unknown bound rejected).
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

# check <name> <file> <jit-stdout> <aot-exit>   (programs here print nothing)
check() {
    local name=$1 file=$2 jit_want=$3 aot_rc_want=$4
    local jit_out
    jit_out=$("$KARDC" "$file")
    if [[ "$jit_out" != "$jit_want" ]]; then
        echo "FAIL [$name/jit]: expected '$jit_want', got '$jit_out'"; exit 1
    fi
    "$KARDC" -o "$TMP/${name}_bin" "$file" >/dev/null
    set +e; "$TMP/${name}_bin" >/dev/null; local rc=$?; set -e
    if [[ "$rc" -ne "$aot_rc_want" ]]; then
        echo "FAIL [$name/aot]: exit was $rc (expected $aot_rc_want)"; exit 1
    fi
    echo "PASS [$name]: JIT + AOT"
}

# expect_error <name> <file> <substring>
expect_error() {
    local name=$1 file=$2 want=$3 out
    out=$("$KARDC" "$file" 2>&1) && { echo "FAIL [$name]: expected error, compiled OK"; exit 1; }
    if ! printf '%s' "$out" | grep -q "$want"; then
        echo "FAIL [$name]: error missing '$want'; got: $out"; exit 1
    fi
    echo "PASS [$name]: rejected ($want)"
}

# --- 1. multiple trait bounds: T: Foo + Bar, methods from both bounds. ---
cat > "$TMP/multibound.kd" <<'EOF'
trait Foo { fn foo(&self) -> i64; }
trait Bar { fn bar(&self) -> i64; }
struct P { x: i64 }
impl Foo for P { fn foo(&self) -> i64 { self.x } }
impl Bar for P { fn bar(&self) -> i64 { self.x + 1 } }
fn both<T: Foo + Bar>(t: &T) -> i64 { t.foo() + t.bar() }
fn main() -> i64 { let p = P { x: 10 }; both(&p) }     // 10 + 11 = 21
EOF
check multibound "$TMP/multibound.kd" 21 21

# --- 2. Hash + Eq traits over i64 and String, used through generic bounds. ---
cat > "$TMP/hasheq.kd" <<'EOF'
fn h<T: Hash>(x: &T) -> i64 { x.hash() }
fn same<T: Eq>(a: &T, b: &T) -> bool { a.eq(b) }
fn main() -> i64 ! { alloc } {
    let a = 5; let b = 5; let c = 6;
    let s1 = "key"; let s2 = "key"; let s3 = "no";
    (if same(&a, &b) { 1 } else { 0 })
  + (if same(&a, &c) { 1000 } else { 0 })
  + (if same(&s1, &s2) { 10 } else { 0 })
  + (if same(&s1, &s3) { 1000 } else { 0 })
  + h(&a)                                       // identity 5
  + (if h(&s1) == h(&s2) { 100 } else { 0 })    // equal strings hash equal
}                                                // 1+0+10+0+5+100 = 116
EOF
check hasheq "$TMP/hasheq.kd" 116 116

# --- 3. generic HashMap with String keys (+ i64-key regression). ---
cat > "$TMP/map.kd" <<'EOF'
fn main() -> i64 ! { alloc } {
    let m = hashmap_new();
    hashmap_insert(&mut m, "alpha", 10);
    hashmap_insert(&mut m, "beta", 20);
    hashmap_insert(&mut m, "alpha", 11);              // update
    let a = match hashmap_get(&m, "alpha") { Some(x)=>x, None=>0 };  // 11
    let b = match hashmap_get(&m, "beta")  { Some(x)=>x, None=>0 };  // 20
    let c = match hashmap_get(&m, "gamma") { Some(x)=>x, None=>0 };  // 0
    let m2 = hashmap_new();
    hashmap_insert(&mut m2, 5, 100);
    let d = match hashmap_get(&m2, 5) { Some(x)=>x, None=>0 };       // 100
    a + b + c + hashmap_len(&m) * 1000 + d            // 11+20+0+2000+100 = 2131
}
EOF
check map "$TMP/map.kd" 2131 83   # 2131 % 256 == 83

# --- 4. user struct key with its own Hash + Eq impls. ---
cat > "$TMP/userkey.kd" <<'EOF'
struct Pt { x: i64, y: i64 }
impl Hash for Pt { fn hash(&self) -> i64 { self.x * 31 + self.y } }
impl Eq for Pt {
    fn eq(&self, other: &Pt) -> bool {
        if self.x == other.x { self.y == other.y } else { false }
    }
}
fn main() -> i64 ! { alloc } {
    let m = hashmap_new();
    hashmap_insert(&mut m, Pt { x: 1, y: 2 }, 100);
    hashmap_insert(&mut m, Pt { x: 3, y: 4 }, 200);
    hashmap_insert(&mut m, Pt { x: 1, y: 2 }, 111);   // update same key
    let a = match hashmap_get(&m, Pt { x: 1, y: 2 }) { Some(v)=>v, None=>0 }; // 111
    let b = match hashmap_get(&m, Pt { x: 9, y: 9 }) { Some(v)=>v, None=>0 }; // 0
    a + b + hashmap_len(&m) * 1000     // 111 + 0 + 2000 = 2111
}
EOF
check userkey "$TMP/userkey.kd" 2111 63   # 2111 % 256 == 63

# --- 5. HashSet<T> over String and i64. ---
cat > "$TMP/set.kd" <<'EOF'
fn main() -> i64 ! { alloc } {
    let s = hashset_new();
    hashset_insert(&mut s, "apple");
    hashset_insert(&mut s, "banana");
    hashset_insert(&mut s, "apple");                 // dup
    let s2 = hashset_new();
    hashset_insert(&mut s2, 7);
    hashset_insert(&mut s2, 8);
    (if hashset_contains(&s, "apple")  { 1 } else { 0 })
  + (if hashset_contains(&s, "cherry") { 100 } else { 0 })
  + (if hashset_contains(&s2, 7) { 10 } else { 0 })
  + (if hashset_contains(&s2, 9) { 100 } else { 0 })
  + hashset_len(&s) * 1000                            // 1+0+10+0+2000 = 2011
}
EOF
check set "$TMP/set.kd" 2011 219   # 2011 % 256 == 219

# --- 6. negatives ---
cat > "$TMP/badkey.kd" <<'EOF'
fn main() -> i64 ! { alloc } { let m: HashMap<bool, i64> = hashmap_new(); 0 }
EOF
expect_error badkey "$TMP/badkey.kd" "must implement Hash + Eq"

cat > "$TMP/badbound.kd" <<'EOF'
trait Foo { fn foo(&self) -> i64; }
fn f<T: Foo + Bogus>(t: &T) -> i64 { t.foo() }
fn main() -> i64 { 0 }
EOF
expect_error badbound "$TMP/badbound.kd" "unknown trait bound 'Bogus'"

echo "PASS: Hash/Eq traits, multi-bound (T: A + B), generic HashMap<K,V> (i64/String/user keys), and HashSet<T> work in JIT + AOT"
