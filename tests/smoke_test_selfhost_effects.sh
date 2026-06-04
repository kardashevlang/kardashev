#!/usr/bin/env bash
# Roadmap v99 — self-hosted EFFECT ROWS: the self-hosted LLVM-IR compiler
# (examples/selfhost/structgen.kd) now PARSES + PROPAGATES the opt-in effect rows
# the host uses — `! { alloc }`, `! { io }`, `! { io, alloc }` — after a fn (or impl
# method) return type. Before v99 the subset lexer had no `!` token, so any
# effectful program emitted `; TYPE ERROR`. v99 adds `!` (token kind 27), an
# optional-effect-row consumer in parse_fn / parse_impl_method, and an `effects`
# bitset field on the `Fn` registry record (1 = alloc, 2 = io). Effects are opt-in
# METADATA — codegen IGNORES the bitset, so a row-free program emits BYTE-IDENTICAL
# IR (matching the host's v81 opt-in default). This makes the subset able to parse
# its OWN style of signatures (structgen.kd uses `! { alloc }` throughout).
#
# Differential-gated vs the host: the self-hosted-emitted IR (clang -> native) must
# exit-match `kardc` on the equivalent program (`<prog>\nfn main(){f(a,b)}`), mod 256.
# Skips if clang is unavailable.
set -uo pipefail
KARDC=""
for candidate in \
    "${TEST_SRCDIR:-}/_main/compiler/kardc" "${TEST_SRCDIR:-}/kardashev/compiler/kardc" \
    "${RUNFILES_DIR:-}/_main/compiler/kardc" "${RUNFILES_DIR:-}/kardashev/compiler/kardc" \
    "./compiler/kardc" "./build.local/kardc"; do
    if [[ -n "$candidate" && -x "$candidate" ]]; then KARDC="$candidate"; break; fi
done
[[ -z "$KARDC" ]] && { echo "FAIL: kardc binary not found"; exit 1; }
SRC=""
for cand in \
    "${TEST_SRCDIR:-}/_main/examples/selfhost/structgen.kd" "${TEST_SRCDIR:-}/kardashev/examples/selfhost/structgen.kd" \
    "${RUNFILES_DIR:-}/_main/examples/selfhost/structgen.kd" "${RUNFILES_DIR:-}/kardashev/examples/selfhost/structgen.kd" \
    "examples/selfhost/structgen.kd"; do
    if [[ -f "$cand" ]]; then SRC="$cand"; break; fi
done
[[ -z "$SRC" ]] && { echo "FAIL: examples/selfhost/structgen.kd not found"; exit 1; }
CLANG="$(command -v clang || true)"
[[ -z "$CLANG" ]] && { echo "PASS [v99-effects]: SKIPPED (no clang to compile the emitted IR)"; exit 0; }
echo "Using kardc at: $KARDC ; clang at: $CLANG"
TMP=$(mktemp -d); trap 'rm -rf "$TMP"' EXIT

"$KARDC" --no-cache -o "$TMP/sg" "$SRC" >/dev/null 2>&1 || { echo "FAIL [v99-effects]: structgen did not build"; exit 1; }

diff_case() {  # $1 source, $2 a, $3 b, $4 label
    "$TMP/sg" "$1" "$2" "$3" > "$TMP/s.ll" 2>/dev/null || { echo "FAIL [v99-effects/$4]: selfcc errored"; exit 1; }
    "$CLANG" "$TMP/s.ll" -o "$TMP/s" 2>/dev/null || { echo "FAIL [v99-effects/$4]: clang rejected IR"; cat "$TMP/s.ll"; exit 1; }
    "$TMP/s" >/dev/null 2>&1; local r_self=$?
    printf '%s\nfn main() -> i64 { f(%s, %s) }\n' "$1" "$2" "$3" > "$TMP/h.kd"
    "$KARDC" --no-cache -o "$TMP/h" "$TMP/h.kd" >/dev/null 2>&1 || { echo "FAIL [v99-effects/$4]: host rejected program"; exit 1; }
    "$TMP/h" >/dev/null 2>&1; local r_host=$?
    [[ "$r_self" -eq "$r_host" ]] || { echo "FAIL [v99-effects/$4]: self=$r_self != host=$r_host"; exit 1; }
    echo "PASS [$4]: self == host == $r_self"
}

# 1-4. effect rows parse + the program runs, self == host.
diff_case "fn f(a: i64, b: i64) -> i64 ! { alloc } { a + b }" 3 4 "alloc-row"
diff_case "fn f(a: i64, b: i64) -> i64 ! { io } { a + b }" 5 6 "io-row"
diff_case "fn f(a: i64, b: i64) -> i64 ! { io, alloc } { a * b }" 5 6 "two-effects"
# an effectful program that also exercises CFG + a struct (note the `;` after the
# while statement before the result — a structgen requirement).
diff_case "struct Widget { v: i64 } fn f(a: i64, b: i64) -> i64 ! { io, alloc } { let w = Widget { v: a } ; let mut s = 0 ; let mut i = 0 ; while i < b { s = s + w.v ; i = i + 1 ; } ; s }" 5 4 "effectful-cfg"
# 5. an effectful program calling another effectful fn (rows propagate across calls).
diff_case "fn helper(x: i64) -> i64 ! { alloc } { x + x } fn f(a: i64, b: i64) -> i64 ! { alloc } { helper(a) + b }" 3 4 "effectful-call"
# NB: structgen parse_impl_method also consumes an effect row on an impl method, but
# the HOST enforces "impl effects must be a SUBSET of the trait's", so a clean
# differential test would need the trait method to declare the row too — out of scope
# for this gate (the fn-level rows above are the v99 deliverable).

# 6. BYTE-IDENTITY: the IR for `f ! { alloc } { body }` is byte-for-byte identical
#    to the row-free `f { body }` (effects are pure metadata — no codegen drift).
"$TMP/sg" "fn f(a: i64, b: i64) -> i64 ! { alloc } { a + b }" 3 4 > "$TMP/with.ll" 2>/dev/null
"$TMP/sg" "fn f(a: i64, b: i64) -> i64 { a + b }" 3 4 > "$TMP/without.ll" 2>/dev/null
diff -q "$TMP/with.ll" "$TMP/without.ll" >/dev/null || { echo "FAIL [byte-identity]: effect row perturbed the emitted IR"; diff "$TMP/with.ll" "$TMP/without.ll" | head; exit 1; }
echo "PASS [byte-identity]: an effect row emits byte-identical IR to a row-free fn"

# 7. NEGATIVE robustness: an effect row whose body is still in-subset must not be a
#    TYPE ERROR (regression guard for the pre-v99 behavior).
"$TMP/sg" "fn f(a: i64, b: i64) -> i64 ! { io } { a + b }" 3 4 2>/dev/null | grep -q 'TYPE ERROR' && { echo "FAIL [no-false-type-error]: effect row wrongly rejected"; exit 1; }
echo "PASS [no-false-type-error]: an effectful in-subset program is accepted"

echo "ALL v99 (self-hosted effect rows) SMOKE TESTS PASSED"
