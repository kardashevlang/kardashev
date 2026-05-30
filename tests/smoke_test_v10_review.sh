#!/usr/bin/env bash
# v10 adversarial-review regression guard. A 6-dimension multi-agent review of
# the const-generic line found 5 blockers + majors the green smoke suite had
# missed (every one with a verified repro). This pins each fix so they can't
# silently regress — the v10 analogue of smoke_test_soundness.sh.
set -euo pipefail

KARDC=""
for candidate in \
    "${TEST_SRCDIR:-}/_main/compiler/kardc" \
    "${TEST_SRCDIR:-}/kardashev/compiler/kardc" \
    "${RUNFILES_DIR:-}/_main/compiler/kardc" \
    "${RUNFILES_DIR:-}/kardashev/compiler/kardc" \
    "./compiler/kardc" \
    "./build.local/kardc"; do
    if [[ -n "$candidate" && -x "$candidate" ]]; then KARDC="$candidate"; break; fi
done
[[ -z "$KARDC" ]] && { echo "FAIL: kardc binary not found"; exit 1; }
echo "Using kardc at: $KARDC"

TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

runs() { local n=$1 f=$2 want=$3 got; got=$("$KARDC" "$f" 2>/dev/null | tail -1)
    [[ "$got" == "$want" ]] || { echo "FAIL [$n]: expected $want got '$got'"; exit 1; }
    echo "PASS [$n]: $got"; }
rejects() { local n=$1 f=$2 needle=$3 out; set +e; out=$("$KARDC" "$f" 2>&1); set -e
    if "$KARDC" "$f" >/dev/null 2>&1; then echo "FAIL [$n]: compiled, expected error"; exit 1; fi
    grep -qi "$needle" <<<"$out" || { echo "FAIL [$n]: missing '$needle'; got: $out"; exit 1; }
    echo "PASS [$n]: rejected"; }

# B1 (blocker): a const param threaded into a NESTED struct field's typeArgs
# (Inner<N> field of Outer<N>) — used to mangle Inner__c0 and fail LLVM verify.
printf 'struct Inner<const N: i64> { data: [i64; N] }\nstruct Outer<const N: i64> { inner: Inner<N> }\nfn main() -> i64 { let i: Inner<3> = Inner { data: [1,2,3] }; let o: Outer<3> = Outer { inner: i }; o.inner.data[2] }\n' > "$TMP/b1.kd"
runs B1-nested-const-field "$TMP/b1.kd" 3

# M1 (major): a const-generic ENUM's variant payload `[i64; N]` (was rejected).
printf 'enum Buf<const N: i64> { Full([i64; N]), Empty }\nfn main() -> i64 { 0 }\n' > "$TMP/m1.kd"
runs M1-const-enum "$TMP/m1.kd" 0

# B2 (blocker): a BARE `b.clone()` on a const-generic struct (no annotation) —
# used to leave CAP symbolic (mangled c0) and type-confuse the result.
cat > "$TMP/b2.kd" <<'EOF'
#[derive(Clone)]
struct Buf<T, const CAP: i64> { data: [T; CAP], tag: Vec<i64> }
fn main() -> i64 ! { alloc } {
    let mut t = vec_new(); vec_push(&mut t, 7);
    let b: Buf<String, 2> = Buf { data: [int_to_string(1), int_to_string(2)], tag: t };
    let b2 = b.clone();
    vec_get(&b2.tag, 0)
}
EOF
runs B2-bare-clone "$TMP/b2.kd" 7

# B3 (blocker): Drop is NOT exempt from the effect-subset rule — a `dyn Drop`
# dispatch can't launder io through a pure-declared Drop trait.
cat > "$TMP/b3.kd" <<'EOF'
trait Drop { fn drop(&self) -> i64; }
struct Sneaky {}
impl Drop for Sneaky { fn drop(&self) -> i64 ! { io } { print(7); 7 } }
fn pure_dispatch(x: &dyn Drop) -> i64 ! { } { x.drop() }
fn main() -> i64 ! { } { let s = Sneaky {}; pure_dispatch(&s) }
EOF
rejects B3-dyn-drop-launder "$TMP/b3.kd" "subset of the trait"

# B4 (blocker): a BOUNDED-GENERIC method call attributes the trait's effects
# (used to attribute ZERO — a pure-declared generic caller of an io method).
cat > "$TMP/b4.kd" <<'EOF'
trait Boom { fn boom(&self) -> i64 ! { io }; }
struct S {}
impl Boom for S { fn boom(&self) -> i64 ! { io } { print(1); 1 } }
fn run<T: Boom>(x: T) -> i64 ! { } { x.boom() }
fn main() -> i64 { let s = S {}; run(s) }
EOF
rejects B4-bounded-generic-effect "$TMP/b4.kd" "does not declare"

# B5 (blocker): forwarding a SYMBOLIC array length alongside a concrete one is a
# dimension mismatch (used to be accepted ill-typed -> LLVM verifier failure).
cat > "$TMP/b5.kd" <<'EOF'
fn dot<const N: i64>(a: [i64; N], b: [i64; N]) -> i64 { a[0] }
fn caller<const M: i64>(a: [i64; M]) -> i64 { let b: [i64; 2] = [10, 20]; dot(a, b) }
fn main() -> i64 { let p: [i64; 4] = [1,2,3,4]; caller(p) }
EOF
rejects B5-symbolic-vs-concrete "$TMP/b5.kd" "dimension mismatch"

# M5 (major): LEGITIMATE forwarding of a const-generic array into another
# const-generic fn (used to wrongly say "cannot infer").
cat > "$TMP/m5.kd" <<'EOF'
fn sum<const N: i64>(a: [i64; N]) -> i64 { let mut s=0; let mut i=0; while i<N { s=s+a[i]; i=i+1; } s }
fn fwd<const M: i64>(a: [i64; M]) -> i64 { sum(a) }
fn main() -> i64 { let x: [i64; 3] = [1, 2, 3]; fwd(x) }
EOF
runs M5-symbolic-forwarding "$TMP/m5.kd" 6

# M2 (major): a monomorphization name colliding with a user fn is a clear error
# (used to silently resolve to the user fn / fail LLVM verify).
printf 'fn g<T>(x: T) -> T { x }\nfn g__i64(x: i64) -> i64 { x * 1000 }\nfn main() -> i64 { let a = g(5); let b = g__i64(7); a + b }\n' > "$TMP/m2.kd"
rejects M2-mangling-collision "$TMP/m2.kd" "collides with a user-defined"

# M3 (major): assigning to a non-Copy array element `a[i] = x` (was rejected by
# the index move-out check).
printf 'fn main() -> i64 ! { alloc } { let mut a: [String; 2] = [int_to_string(1), int_to_string(2)]; a[0] = int_to_string(99999); str_len(&a[0]) }\n' > "$TMP/m3.kd"
runs M3-noncopy-array-assign "$TMP/m3.kd" 5

# M4 (major): array-repeat length respects a LOCAL that shadows a const param —
# it must NOT silently use the const param's value. A runtime local isn't a
# compile-time length, so `[9; N]` with `let N = 2` shadowing is a clear error
# (the bug silently sized the array by the const param instead).
cat > "$TMP/m4.kd" <<'EOF'
fn shadow<const N: i64>(probe: [i64; N]) -> i64 {
    let N = 2;       // a runtime local shadows the const param N
    let a = [9; N];  // must NOT be sized by the const param
    a[0]
}
fn main() -> i64 { let p: [i64; 5] = [0,0,0,0,0]; shadow(p) }
EOF
rejects M4-array-repeat-shadow "$TMP/m4.kd" "compile-time constant"

echo "ALL V10 REVIEW-REGRESSION TESTS PASSED"
