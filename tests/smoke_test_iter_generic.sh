#!/usr/bin/env bash
# Roadmap v101 — ELEMENT-GENERIC iterator adaptors.
#
# The v61/v78 lazy adaptor tower was `i64`-only because a generic impl could not
# bind a generic param as the trait's type-arg (`impl<I: Iterator<T>, T>
# Iterator<T> for GTake<I,T>` errored `unknown type: T`). v101 fixes the resolver
# (`bindTraitParamsForImpl` seeds impl params referenced by a trait type-arg) and
# adds a parallel ELEMENT-GENERIC prelude tower under `g*` names — `gvec_iter` /
# `gtake` / `gskip` / `gmap` / `gfilter` — that fuses lazily over ANY element type
# (i64, structs, owned String), drained by the already-generic `iter_collect`.
#
# The existing i64 tower (`iter_take`/`iter_map`/…) is FROZEN for byte-identity;
# its struct mangles are locked and the g* tower lives under distinct names. This
# gate proves: the generic tower works over i64/struct/String (JIT==AOT), nested
# chains specialize with distinct instantiated types (no PHI crash, IR-grep), and
# an i64-only program emits NO g* symbols (the g* tower is fully monomorphize-on-
# use, so it never bloats programs that don't use it).
#
# DEFERRED (honest): element-generic `zip`/`enumerate` — their element is a
# COMPUTED pair (`TwoTup<T,U>`) only in the impl trait-arg, so `iter_collect`'s
# `Vec<T>` cannot infer it (the associated-output-through-a-bound gap); the i64
# `iter_zip`/`iter_enumerate` remain. `--emit-c` refuses generic-trait programs.
set -uo pipefail
KARDC=""
for c in "${TEST_SRCDIR:-}/_main/compiler/kardc" "${RUNFILES_DIR:-}/_main/compiler/kardc" "./compiler/kardc" "./build.local/kardc"; do
  [[ -n "$c" && -x "$c" ]] && { KARDC="$c"; break; }; done
[[ -z "$KARDC" ]] && { echo "FAIL: kardc not found"; exit 1; }
echo "Using kardc at: $KARDC"
TMP=$(mktemp -d); trap 'rm -rf "$TMP"' EXIT

# JIT==AOT differential over the first N expected lines (the JIT echoes main's
# return value as a trailing line; AOT uses it as the exit code).
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

# 1. i64 generic chain: gvec_iter -> gskip(1) -> gfilter(odd) -> gmap(+1) -> gtake(2) -> collect
diff_jaot i64_chain $'2\n4' \
'fn isodd(x: &i64) -> bool { (*x % 2) == 1 }
fn inc(x: &i64) -> i64 { *x + 1 }
fn main() -> i64 ! { alloc, io } {
  let mut v: Vec<i64> = vec_new(); let mut i = 0; while i < 10 { vec_push(&mut v, i); i = i + 1; }
  let mut ch: GTake<GMap<GFilter<GSkip<GVecIter<i64>, i64>, i64>, i64, i64>, i64> = gtake(gmap(gfilter(gskip(gvec_iter(v), 1), isodd), inc), 2);
  let out = iter_collect(&mut ch);
  let mut j = 0; while j < vec_len(&out) { print(vec_get(&out, j)); j = j + 1; } 0
}'

# 2. struct elements: gfilter(&Item) -> gmap(&Item -> i64) -> collect
diff_jaot struct_chain $'9\n7' \
'struct Item { id: i64, w: i64 }
fn keepbig(it: &Item) -> bool { it.w > 5 }
fn getw(it: &Item) -> i64 { it.w }
fn main() -> i64 ! { alloc, io } {
  let mut items: Vec<Item> = vec_new(); vec_push(&mut items, Item{id:1,w:3}); vec_push(&mut items, Item{id:2,w:9}); vec_push(&mut items, Item{id:3,w:7});
  let mut ic: GMap<GFilter<GVecIter<Item>, Item>, Item, i64> = gmap(gfilter(gvec_iter(items), keepbig), getw);
  let wo = iter_collect(&mut ic);
  let mut k = 0; while k < vec_len(&wo) { print(vec_get(&wo, k)); k = k + 1; } 0
}'

# 3. owned String elements: gmap(str_len) -> collect
diff_jaot string_chain $'2\n4' \
'fn main() -> i64 ! { alloc, io } {
  let mut sv: Vec<String> = vec_new(); vec_push(&mut sv, "ab"); vec_push(&mut sv, "cdef");
  let mut sm: GMap<GVecIter<String>, String, i64> = gmap(gvec_iter(sv), str_len);
  let smo = iter_collect(&mut sm);
  print(vec_get(&smo, 0)); print(vec_get(&smo, 1)); 0
}'

# 4. NESTED over a struct element specializes with DISTINCT instantiated types
#    (no PHI-type-mismatch crash). IR-grep is mangling-based -> host-independent.
cat > "$TMP/nest.kd" <<'EOF'
struct Pair { a: i64, b: i64 }
fn keep(p: &Pair) -> bool { p.a < 2 }
fn main() -> i64 ! { alloc, io } {
  let mut v: Vec<Pair> = vec_new(); vec_push(&mut v, Pair{a:0,b:10}); vec_push(&mut v, Pair{a:1,b:11}); vec_push(&mut v, Pair{a:5,b:12});
  let mut t: GTake<GFilter<GVecIter<Pair>, Pair>, Pair> = gtake(gfilter(gvec_iter(v), keep), 5);
  let o: Vec<Pair> = iter_collect(&mut t);
  let mut i = 0; while i < vec_len(&o) { let p = vec_get(&o, i); print(p.b); i = i + 1; } 0
}
EOF
nestir=$("$KARDC" --no-cache --emit-llvm "$TMP/nest.kd" 2>/dev/null)
grep -q 'Option__Pair' <<<"$nestir" || { echo "FAIL [nested-ir]: no Option<Pair> instantiation (element not specialized to the struct)"; exit 1; }
grep -qE 'GFilter__GVecIter|GTake__GFilter' <<<"$nestir" || { echo "FAIL [nested-ir]: no distinct nested adaptor instantiations"; exit 1; }
echo "PASS [nested-ir]: nested struct-element adaptors specialize (Option__Pair + distinct GTake/GFilter instances, no PHI crash)"
diff_jaot nested_run $'10\n11' "$(cat "$TMP/nest.kd")"

# 5. USE-GATED: an i64-only program (the FROZEN iter_* tower) emits NO g* symbols
#    (the generic tower is fully monomorphize-on-use; zero bloat when unused).
cat > "$TMP/i64only.kd" <<'EOF'
fn main() -> i64 ! { alloc, io } {
  let mut t = iter_take(iter_skip(Range{start:0,end:100,inclusive:0}, 20), 5);
  let mut go = true;
  while go { match t.next() { Some(x) => { print(x); }, None => { go = false; } } } 0
}
EOF
i64ir=$("$KARDC" --no-cache --emit-llvm "$TMP/i64only.kd" 2>/dev/null)
grep -qE '@?GTake|@?GMap|@?GFilter|@?GVecIter|@?GSkip' <<<"$i64ir" && { echo "FAIL [use-gated]: an i64-only program emitted g* symbols (the g* tower is not monomorphize-on-use)"; exit 1; }
echo "PASS [use-gated]: i64-only program emits no g* symbols (zero bloat)"

echo "ALL v101 (element-generic iterator) SMOKE TESTS PASSED"
