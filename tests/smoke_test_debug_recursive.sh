#!/usr/bin/env bash
# Roadmap v102 — recursive container `Debug` (`{:?}`).
#
# Before v102, `Debug` had impls only for i64/f64/bool/char/String, so
# `println!("{:?}", v)` over a `Vec<T>`/`HashMap`/`Option`/… was impossible. v102
# adds blanket prelude impls over each element's own `fmt_debug` (resolved via the
# v101 generic-impl fix — no codegen change): Vec, Option, Result, BTreeMap,
# BTreeSet, HashMap, VecDeque. The headline DX win: `#[derive(Debug)]` on a struct
# with `Vec`/`Option` fields now recurses correctly. `Box<T>` Debug already works
# via deref. All under the existing `trait Debug` opt-out gate (no collision with a
# user `Debug`, no bloat for programs that don't use it).
#
# Determinism: BTreeMap/BTreeSet iterate in ASCENDING key order, so their
# multi-entry output is deterministic; HashMap bucket order is NOT, so it is tested
# SINGLE-ENTRY only. Quoting/escaping of String elements reuses the v27 str_escape.
#
# DEFERRED (honest): tuple `Debug`, `&[T]` slice Debug, format-spec dispatch
# (`{:x}`/`{:04d}`), and `{:#?}` pretty-printing — see ROADMAP-v101-v110.md (v104+).
set -uo pipefail
KARDC=""
for c in "${TEST_SRCDIR:-}/_main/compiler/kardc" "${RUNFILES_DIR:-}/_main/compiler/kardc" "./compiler/kardc" "./build.local/kardc"; do
  [[ -n "$c" && -x "$c" ]] && { KARDC="$c"; break; }; done
[[ -z "$KARDC" ]] && { echo "FAIL: kardc not found"; exit 1; }
echo "Using kardc at: $KARDC"
TMP=$(mktemp -d); trap 'rm -rf "$TMP"' EXIT

# JIT==AOT differential over the first N expected lines (JIT echoes main's return
# value as a trailing line; AOT uses it as the exit code).
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

# (a) Vec<i64> + (b) Vec<String> with an embedded quote (escaping)
diff_jaot vec $'[1, 2, 3]\n["a\\"b", "c"]' \
'fn main() -> i64 ! { alloc, io } {
  let mut v: Vec<i64> = vec_new(); vec_push(&mut v, 1); vec_push(&mut v, 2); vec_push(&mut v, 3);
  println!("{:?}", v);
  let mut s: Vec<String> = vec_new(); vec_push(&mut s, "a\"b"); vec_push(&mut s, "c");
  println!("{:?}", s); 0
}'

# (c) Option/Result, incl. a nested Option<Vec<i64>>
diff_jaot optres $'Some([1, 2])\nNone\nOk(7)\nErr("bad")' \
'fn main() -> i64 ! { alloc, io } {
  let mut v: Vec<i64> = vec_new(); vec_push(&mut v, 1); vec_push(&mut v, 2);
  let o: Option<Vec<i64>> = Some(v); println!("{:?}", o);
  let n: Option<i64> = None; println!("{:?}", n);
  let ok: Result<i64, String> = Ok(7); println!("{:?}", ok);
  let er: Result<i64, String> = Err("bad"); println!("{:?}", er); 0
}'

# (d) BTreeMap + BTreeSet — ORDERED, deterministic multi-entry
diff_jaot btree $'{1: "a", 2: "b", 3: "c"}\n{1, 2, 3}' \
'fn main() -> i64 ! { alloc, io } {
  let mut m: BTreeMap<i64, String> = btreemap_new();
  btreemap_insert(&mut m, 3, "c"); btreemap_insert(&mut m, 1, "a"); btreemap_insert(&mut m, 2, "b");
  println!("{:?}", m);
  let mut s: BTreeSet<i64> = btreeset_new();
  btreeset_insert(&mut s, 2); btreeset_insert(&mut s, 3); btreeset_insert(&mut s, 1);
  println!("{:?}", s); 0
}'

# (e) HashMap — SINGLE entry only (bucket order non-deterministic)
diff_jaot hashmap $'{42: "z"}' \
'fn main() -> i64 ! { alloc, io } {
  let mut h: HashMap<i64, String> = hashmap_new(); hashmap_insert(&mut h, 42, "z");
  println!("{:?}", h); 0
}'

# (f) VecDeque — front->back order
diff_jaot vecdeque $'[0, 1, 2]' \
'fn main() -> i64 ! { alloc, io } {
  let mut d: VecDeque<i64> = vecdeque_new();
  vecdeque_push_back(&mut d, 1); vecdeque_push_back(&mut d, 2); vecdeque_push_front(&mut d, 0);
  println!("{:?}", d); 0
}'

# (g) THE HEADLINE: #[derive(Debug)] over a struct with container fields recurses
diff_jaot derive_container $'Widget { id: 7, tags: ["a", "b"], parent: Some(3) }' \
'#[derive(Debug)]
struct Widget { id: i64, tags: Vec<String>, parent: Option<i64> }
fn main() -> i64 ! { alloc, io } {
  let mut t: Vec<String> = vec_new(); vec_push(&mut t, "a"); vec_push(&mut t, "b");
  let w = Widget { id: 7, tags: t, parent: Some(3) };
  println!("{:?}", w); 0
}'

# (h) Box<T> Debug works via deref (no explicit impl needed).
diff_jaot box_debug $'99' \
'fn main() -> i64 ! { alloc, io } { let b: Box<i64> = Box::new(99); println!("{:?}", b); 0 }'

# (i) REGRESSION LOCK: a scalar #[derive(Debug)] still prints exactly as before.
diff_jaot derive_scalar $'P { x: 1, y: 2 }' \
'#[derive(Debug)]
struct P { x: i64, y: i64 }
fn main() -> i64 ! { alloc, io } { let p = P { x: 1, y: 2 }; println!("{:?}", p); 0 }'

echo "ALL v102 (recursive container Debug) SMOKE TESTS PASSED"
