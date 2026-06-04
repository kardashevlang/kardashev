#!/usr/bin/env bash
# v96 — coherence E0119 + generalized negative impls.
#
# WHAT v96 ADDS (ground-truthed — three roadmap premises were already met):
#   * Overlapping blanket impls were ALREADY rejected; concrete-beats-blanket
#     ALREADY worked; a duplicate concrete impl was ALREADY a clean error. v96
#     therefore (1) attaches a STABLE error code E0119 (+ `--explain`) to the
#     existing coherence diagnostic, and (2) GENERALIZES negative impls from the
#     v31 Send/Sync-only restriction to any declared trait: `impl !Tr for X {}`
#     opts X out of a blanket `impl<T> Tr for T`.
#
# The #1 design constraint is NO FALSE POSITIVES: concrete-beats-blanket
# (specialization) and derives must still compile + dispatch correctly. Cases
# (b)/(c2)/(e) RUN the produced binary and assert the result to lock that in.
set -uo pipefail

KARDC=""
for candidate in \
    "${TEST_SRCDIR:-}/_main/compiler/kardc" \
    "${TEST_SRCDIR:-}/kardashev/compiler/kardc" \
    "${RUNFILES_DIR:-}/_main/compiler/kardc" \
    "${RUNFILES_DIR:-}/kardashev/compiler/kardc" \
    "./compiler/kardc" "./build.local/kardc"; do
    if [[ -n "$candidate" && -x "$candidate" ]]; then KARDC="$candidate"; break; fi
done
[[ -z "$KARDC" ]] && { echo "FAIL: kardc binary not found"; exit 1; }
echo "Using kardc at: $KARDC"

TMP=$(mktemp -d); trap 'rm -rf "$TMP"' EXIT

# reject NAME NEEDLE SRC  — compile must fail, stderr must match NEEDLE (grep -E).
reject() {
    local name="$1" needle="$2" src="$3" out rc
    printf '%s' "$src" > "$TMP/$name.kd"
    out=$("$KARDC" "$TMP/$name.kd" 2>&1 >/dev/null); rc=$?
    [[ "$rc" -ne 0 ]] || { echo "FAIL [$name]: expected a compile error, got none"; echo "$out"; exit 1; }
    grep -qE "$needle" <<<"$out" || { echo "FAIL [$name]: want /$needle/, got:"; echo "$out" | head -3; exit 1; }
    echo "PASS [$name]"
}

# run NAME WANT SRC  — must compile AND the binary must exit with code WANT.
run() {
    local name="$1" want="$2" src="$3" rc
    printf '%s' "$src" > "$TMP/$name.kd"
    "$KARDC" --no-cache -o "$TMP/$name" "$TMP/$name.kd" >/dev/null 2>&1 || {
        echo "FAIL [$name]: expected to compile, did not:"; "$KARDC" "$TMP/$name.kd" 2>&1 | head -3; exit 1; }
    "$TMP/$name"; rc=$?
    [[ "$rc" -eq "$want" ]] || { echo "FAIL [$name]: want exit $want, got $rc"; exit 1; }
    echo "PASS [$name] (exit $rc)"
}

# ---------------------------------------------------------------------------
# (a) a TRUE overlap (two blankets of the same trait) still errors — now E0119.
# ---------------------------------------------------------------------------
reject overlap_blankets 'error\[E0119\].*|conflicting implementations' \
'trait Foo { fn f(&self) -> i64; }
struct Take { x: i64 }
impl<T> Foo for T { fn f(&self) -> i64 { 1 } }
impl<U> Foo for U { fn f(&self) -> i64 { 2 } }
fn main() -> i64 { 0 }'

# ---------------------------------------------------------------------------
# (b) FALSE-POSITIVE GUARD #1: concrete-beats-blanket still compiles AND
#     dispatches to the concrete impl (exit 111, not the blanket's 222).
# ---------------------------------------------------------------------------
run concrete_wins 111 \
'trait Foo { fn f(&self) -> i64; }
struct W { x: i64 }
impl Foo for W { fn f(&self) -> i64 { 111 } }
impl<T: Clone> Foo for T { fn f(&self) -> i64 { 222 } }
fn main() -> i64 { let w = W { x: 0 }; w.f() }'

# ---------------------------------------------------------------------------
# (c2) WITHOUT a negative impl the blanket applies — H{}.g() returns 7.
# ---------------------------------------------------------------------------
run blanket_applies 7 \
'trait Greet { fn g(&self) -> i64; }
impl<T: Clone> Greet for T { fn g(&self) -> i64 { 7 } }
struct H { x: i64 }
impl Clone for H { fn clone(&self) -> H { H { x: self.x } } }
fn main() -> i64 { let h = H { x: 0 }; h.g() }'

# ---------------------------------------------------------------------------
# (c) a generalized negative impl OPTS H OUT of the blanket — the same program
#     plus `impl !Greet for H {}` now fails: no impl provides `g` for H.
# ---------------------------------------------------------------------------
reject negative_blocks 'no impl provides method|no method|does not implement' \
'trait Greet { fn g(&self) -> i64; }
impl<T: Clone> Greet for T { fn g(&self) -> i64 { 7 } }
struct H { x: i64 }
impl Clone for H { fn clone(&self) -> H { H { x: self.x } } }
impl !Greet for H {}
fn main() -> i64 { let h = H { x: 0 }; h.g() }'

# ---------------------------------------------------------------------------
# (d) a positive `impl Tr` and a negative `impl !Tr` for the same type conflict.
# ---------------------------------------------------------------------------
reject pos_neg_conflict 'error\[E0119\].*|conflicting `impl' \
'trait Greet { fn g(&self) -> i64; }
struct H { x: i64 }
impl Greet for H { fn g(&self) -> i64 { 1 } }
impl !Greet for H {}
fn main() -> i64 { 0 }'

# ---------------------------------------------------------------------------
# (d2) a duplicate negative impl is rejected.
# ---------------------------------------------------------------------------
reject dup_negative 'duplicate negative impl' \
'trait Greet { fn g(&self) -> i64; }
struct H { x: i64 }
impl !Greet for H {}
impl !Greet for H {}
fn main() -> i64 { 0 }'

# ---------------------------------------------------------------------------
# (e) a negative impl of an UNKNOWN trait is rejected (trait must be declared).
# ---------------------------------------------------------------------------
reject neg_unknown 'unknown trait' \
'struct W { x: i64 }
impl !Bogus for W {}
fn main() -> i64 { 0 }'

# ---------------------------------------------------------------------------
# (f) a negative impl with a method body is rejected (must be method-less).
# ---------------------------------------------------------------------------
reject neg_with_method 'must have an empty body|must have no methods' \
'trait Greet { fn g(&self) -> i64; }
struct W { x: i64 }
impl !Greet for W { fn g(&self) -> i64 { 1 } }
fn main() -> i64 { 0 }'

# ---------------------------------------------------------------------------
# (g) FALSE-POSITIVE GUARD #2: derives still work — derive(Clone) over a Vec
#     field deep-copies, derive(Debug) over a scalar struct formats. Both lean
#     on blanket/derive machinery v96 must not disturb.
# ---------------------------------------------------------------------------
run derive_clone_vec 7 \
'#[derive(Clone)]
struct P { a: i64, b: Vec<i64> }
fn main() -> i64 ! { io, alloc } {
  let mut v: Vec<i64> = vec_new();
  vec_push(&mut v, 7);
  let p = P { a: 5, b: v };
  let q = p.clone();
  vec_get(&q.b, 0)
}'

run derive_debug 0 \
'#[derive(Debug)]
struct Q { a: i64, b: bool }
fn main() -> i64 ! { io, alloc } {
  let q = Q { a: 5, b: true };
  println!("{:?}", q);
  0
}'

# ---------------------------------------------------------------------------
# (h) `--explain E0119` prints the curated coherence text.
# ---------------------------------------------------------------------------
"$KARDC" --explain E0119 2>&1 | grep -q "conflicting trait implementations" \
    || { echo "FAIL [explain_e0119]: --explain E0119 missing"; exit 1; }
echo "PASS [explain_e0119]"

echo "ALL COHERENCE (v96) SMOKE TESTS PASSED"
