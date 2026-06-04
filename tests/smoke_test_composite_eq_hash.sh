#!/usr/bin/env bash
# Roadmap v105 — generic Eq/Hash for Option/Result (+ the derive/HashMap-key it
# unlocks).
#
# Eq/Hash were impl'd only for scalars + String. v105 adds generic blanket impls
# `impl<T: Eq> Eq for Option<T>` / `impl<T: Hash> Hash for Option<T>` (and the
# Result<T,E> pair), verified post-v101 resolver. This makes Option/Result usable
# in `==`, as Vec/Box elements, and — the headline — lets `#[derive(Eq, Hash)]` on
# a struct with an Option/Result field resolve, so that struct keys a HashMap.
# Hash mixes a per-variant seed (527+ordinal, fold payload *31) so equal values
# hash equal (the commute property a HashMap relies on).
#
# DEFERRED (honest, probe-confirmed): a TUPLE is not a registrable impl head
# (`impl Eq for (T,U)` -> "impl for unsupported type"), so the composite-key path
# is a nominal `#[derive(Eq,Hash)]` struct, not `HashMap<(K1,K2),V>`. Using a
# GENERIC type directly as a key (`HashMap<Option<i64>,V>` / `HashMap<Pair<T>,V>`)
# is blocked at codegen (the eager-emit pass skips monomorphized generic-impl
# methods, so the bare hash/eq symbol the key machinery looks up isn't emitted) —
# a codegen-dispatch item for a later version. Concrete derive'd struct keys work.
set -uo pipefail
KARDC=""
for c in "${TEST_SRCDIR:-}/_main/compiler/kardc" "${RUNFILES_DIR:-}/_main/compiler/kardc" "./compiler/kardc" "./build.local/kardc"; do
  [[ -n "$c" && -x "$c" ]] && { KARDC="$c"; break; }; done
[[ -z "$KARDC" ]] && { echo "FAIL: kardc not found"; exit 1; }
echo "Using kardc at: $KARDC"
TMP=$(mktemp -d); trap 'rm -rf "$TMP"' EXIT

diff_jaot() {  # $1 name  $2 expected(multiline)  $3 src
  local n; n=$(printf '%s\n' "$2" | wc -l)
  printf '%s' "$3" > "$TMP/$1.kd"
  local j; j=$("$KARDC" --no-cache "$TMP/$1.kd" 2>/dev/null | head -n "$n")
  [[ "$j" == "$2" ]] || { echo "FAIL [$1/jit]: got '$j' want '$2'"; "$KARDC" "$TMP/$1.kd" 2>&1 | head -4; exit 1; }
  "$KARDC" --no-cache -o "$TMP/$1" "$TMP/$1.kd" >/dev/null 2>&1 || { echo "FAIL [$1]: AOT build failed"; "$KARDC" "$TMP/$1.kd" 2>&1 | head -4; exit 1; }
  local a; a=$("$TMP/$1" 2>/dev/null | head -n "$n")
  [[ "$a" == "$2" ]] || { echo "FAIL [$1/aot]: got '$a' want '$2'"; exit 1; }
  echo "PASS [$1]"
}

# (a) Option / Result value-level Eq.
diff_jaot option_result_eq $'1\n1\n0\n1\n0' \
'fn main() -> i64 ! { alloc, io } {
  let a: Option<i64> = Some(3); let b: Option<i64> = Some(3);
  let n: Option<i64> = None; let m: Option<i64> = None;
  if a.eq(&b) { print(1); } else { print(0); }      // Some(3)==Some(3) -> 1
  if n.eq(&m) { print(1); } else { print(0); }      // None==None -> 1
  if a.eq(&n) { print(1); } else { print(0); }      // Some!=None -> 0
  let ok: Result<i64, String> = Ok(7); let ok2: Result<i64, String> = Ok(7);
  let er: Result<i64, String> = Err("x");
  if ok.eq(&ok2) { print(1); } else { print(0); }   // Ok(7)==Ok(7) -> 1
  if ok.eq(&er) { print(1); } else { print(0); }    // Ok!=Err -> 0
  0
}'

# (b) Hash COMMUTES: equal Option/Result values hash equal (HashMap correctness).
diff_jaot hash_commute $'1\n1' \
'fn main() -> i64 ! { alloc, io } {
  let a: Option<String> = Some("hello"); let b: Option<String> = Some("hello");
  if a.hash() == b.hash() { print(1); } else { print(0); }   // equal -> hash equal
  let x: Result<i64, i64> = Ok(5); let y: Result<i64, i64> = Ok(5);
  if x.hash() == y.hash() { print(1); } else { print(0); }
  0
}'

# (c) THE HEADLINE: #[derive(Eq, Hash)] over an Option field -> a HashMap key.
#     Insert with one allocation, retrieve with a freshly-built equal key (across
#     distinct String allocations) -> proves eq+hash commute end-to-end.
diff_jaot derive_option_key $'100\n200\n-1' \
'#[derive(Eq, Hash)]
struct Key { id: i64, tag: Option<String> }
fn main() -> i64 ! { alloc, io } {
  let mut m: HashMap<Key, i64> = hashmap_new();
  hashmap_insert(&mut m, Key { id: 1, tag: Some("x") }, 100);
  hashmap_insert(&mut m, Key { id: 2, tag: None }, 200);
  match hashmap_get_ref(&m, Key { id: 1, tag: Some("x") }) { Some(v) => { print(*v); }, None => { print(0 - 1); } }   // 100
  match hashmap_get_ref(&m, Key { id: 2, tag: None }) { Some(v) => { print(*v); }, None => { print(0 - 1); } }       // 200
  match hashmap_get_ref(&m, Key { id: 1, tag: Some("y") }) { Some(v) => { print(*v); }, None => { print(0 - 1); } }  // absent -> -1
  0
}'

# (d) Option/Result as Vec elements via vec_contains (uses the new Eq).
diff_jaot option_in_vec $'1\n0' \
'fn main() -> i64 ! { alloc, io } {
  let mut v: Vec<Option<i64>> = vec_new();
  vec_push(&mut v, Some(1)); vec_push(&mut v, None); vec_push(&mut v, Some(3));
  let needle: Option<i64> = Some(3);
  if vec_contains(&v, &needle) { print(1); } else { print(0); }    // present -> 1
  let absent: Option<i64> = Some(9);
  if vec_contains(&v, &absent) { print(1); } else { print(0); }    // absent -> 0
  0
}'

echo "ALL v105 (Option/Result Eq/Hash + composite keys) SMOKE TESTS PASSED"
