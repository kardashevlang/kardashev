#!/usr/bin/env bash
# Roadmap v107 — self-hosted ENUM + MATCH (ARC C): the self-hosted LLVM-IR compiler
# (examples/selfhost/structgen.kd) now PARSES and LOWERS a generic two-variant enum
# `Opt<T> { Just(T), Nope }` and a value-producing `match o { Just(b) => .., Nope => .. }`.
#
# Representation (host-confirmed by the v107 research probe): an enum value is a flat
# tagged struct `{ i64 tag, i64 payload }` (Just=tag 0 carries the payload; Nope=tag 1
# leaves it undef). The constructor lowers to an `insertvalue` chain (like a struct
# literal); the match lowers to `extractvalue` of the tag + payload, the Just binder
# bound to the payload in a child env, both arm bodies lowered, then a `select` on
# `tag == 0` (mirroring the existing If-as-value lowering — so match/if expressions emit
# NO branches and compose, even nested). A param `o: Opt<i64>` resolves to the tagged
# struct, so an enum is passed by value as `{ i64, i64 }`.
#
# USE-GATED (Risk R0): a program that declares/uses NO enum emits BYTE-IDENTICAL IR —
# the seven prior self-host gates are the guard, plus the demo guard below asserts the
# default program is unchanged (no extractvalue, exit 7).
#
# Differential-gated vs the host: the self-hosted-emitted IR (clang -> native) must
# exit-match `kardc` on the equivalent program. Test programs keep
# `f(a: i64, b: i64) -> i64` so the host's `fn main() { f(a, b) }` wrapper works; the
# `enum Opt<T> { ... }` / `unwrap_or` are OTHER decls `f` uses. Exit codes mod 256.
# Skips if clang is unavailable.
#
# DEFERRALS (v107, honest): a FIXED 2-variant generic enum recognized by name
# (`Opt`/`Just`/`Nope`) exactly as the str_*/vec_* builtins are — arbitrary enum/variant
# names and >2 variants are deferred; single i64 payload only (multi-payload / non-i64
# / struct payloads deferred); the scrutinee must be an OWNED enum value (no `match &o`);
# value-producing arms only (side-effecting arms would need branch+phi, not select);
# `let x: Opt<i64> = ...` type annotations are not used (structgen has never supported
# `let` type annotations — a separate pre-existing limitation; bindings infer the type).
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
[[ -z "$CLANG" ]] && { echo "PASS [v107-enum-match]: SKIPPED (no clang to compile the emitted IR)"; exit 0; }
echo "Using kardc at: $KARDC ; clang at: $CLANG"
TMP=$(mktemp -d); trap 'rm -rf "$TMP"' EXIT

"$KARDC" --no-cache -o "$TMP/sg" "$SRC" >/dev/null 2>&1 || { echo "FAIL [v107-enum-match]: structgen did not build"; exit 1; }

# 1. BYTE-IDENTITY guard (Risk R0): the default (no-enum) demo (struct P, field sum —
#    no if/match) must still build its `{ i64, i64 }` struct, emit NO `select` (the
#    match/if marker; the demo has no conditional, so a select would mean codegen
#    drifted), and exit 7. (extractvalue { i64, i64 } is NOT an enum marker — it is the
#    demo's own `p.x`/`p.y` field reads.) The seven prior self-host gates carry the rest
#    of the byte-identity guarantee for no-enum programs.
"$TMP/sg" > "$TMP/d.ll" 2>/dev/null || { echo "FAIL [v107-enum-match]: demo did not run"; exit 1; }
grep -q 'insertvalue { i64, i64 }' "$TMP/d.ll" || { echo "FAIL [v107-enum-match]: { i64, i64 } struct regressed"; cat "$TMP/d.ll"; exit 1; }
grep -q 'select' "$TMP/d.ll" && { echo "FAIL [v107-enum-match]: no-conditional demo emitted a select (R0 codegen drift)"; cat "$TMP/d.ll"; exit 1; }
"$CLANG" "$TMP/d.ll" -o "$TMP/d" 2>/dev/null || { echo "FAIL [v107-enum-match]: clang rejected demo IR"; cat "$TMP/d.ll"; exit 1; }
"$TMP/d" >/dev/null 2>&1; rd=$?
[[ "$rd" -eq 7 ]] || { echo "FAIL [v107-enum-match]: demo exit $rd (want 7)"; exit 1; }
echo "PASS [byte-identity]: no-enum demo unchanged (struct insertvalue, no select); native exit 7"

# DIFFERENTIAL helper: emit IR from the self-hosted compiler, clang -> native, and
# compare its exit to the host kardc on `<prog>\nfn main() { f(a, b) }`.
diff_case() {  # $1 source, $2 a, $3 b, $4 label
    "$TMP/sg" "$1" "$2" "$3" > "$TMP/s.ll" 2>/dev/null || { echo "FAIL [v107-enum-match/$4]: selfcc errored"; exit 1; }
    "$CLANG" "$TMP/s.ll" -o "$TMP/s" 2>/dev/null || { echo "FAIL [v107-enum-match/$4]: clang rejected IR"; cat "$TMP/s.ll"; exit 1; }
    "$TMP/s" >/dev/null 2>&1; local r_self=$?
    printf '%s\nfn main() -> i64 { f(%s, %s) }\n' "$1" "$2" "$3" > "$TMP/h.kd"
    "$KARDC" --no-cache -o "$TMP/h" "$TMP/h.kd" >/dev/null 2>&1 || { echo "FAIL [v107-enum-match/$4]: host rejected program"; exit 1; }
    "$TMP/h" >/dev/null 2>&1; local r_host=$?
    [[ "$r_self" -eq "$r_host" ]] || { echo "FAIL [v107-enum-match/$4]: self=$r_self != host=$r_host"; exit 1; }
    echo "PASS [$4]: self == host == $r_self"
}
# Same as diff_case but also asserts the self-hosted IR contains a regex.
diff_case_ir() {  # $1 source, $2 a, $3 b, $4 label, $5 ir-grep-regex
    diff_case "$1" "$2" "$3" "$4"
    grep -Eq "$5" "$TMP/s.ll" || { echo "FAIL [v107-enum-match/$4]: IR missing /$5/"; cat "$TMP/s.ll"; exit 1; }
    echo "PASS [$4-ir]: IR contains /$5/"
}

ENUM='enum Opt<T> { Just(T), Nope }'
UNWRAP="$ENUM fn unwrap_or(o: Opt<i64>, d: i64) -> i64 { match o { Just(x) => x, Nope => d } }"

# 2. The Just path: unwrap_or(Just(a), b) == a. Assert the enum is passed by value as
#    { i64, i64 } and the match lowers to extractvalue + select (no branches).
diff_case_ir "$UNWRAP fn f(a: i64, b: i64) -> i64 { unwrap_or(Just(a), b) }" \
    7 9 "just-path" 'unwrap_or\(\{ i64, i64 \}'
"$TMP/sg" "$UNWRAP fn f(a: i64, b: i64) -> i64 { unwrap_or(Just(a), b) }" 7 9 > "$TMP/m.ll" 2>/dev/null
grep -q 'extractvalue { i64, i64 }' "$TMP/m.ll" || { echo "FAIL [just-path]: match did not lower to extractvalue"; cat "$TMP/m.ll"; exit 1; }
grep -Eq '= select i1 .*, i64 ' "$TMP/m.ll" || { echo "FAIL [just-path]: match did not lower to a select"; cat "$TMP/m.ll"; exit 1; }
echo "PASS [just-path-ir]: enum by-value { i64, i64 } + extractvalue + select"

# 3. The Nope path: unwrap_or(Nope, b) == b. Nope builds {1, undef}.
diff_case_ir "$UNWRAP fn f(a: i64, b: i64) -> i64 { unwrap_or(Nope, b) }" \
    7 9 "nope-path" 'insertvalue \{ i64, i64 \} undef, i64 1, 0'

# 4. Both arms exercised in one program (Just + Nope), and arm-ORDER independence
#    (`Nope => .. , Just(x) => ..`).
diff_case "$UNWRAP fn f(a: i64, b: i64) -> i64 { unwrap_or(Just(a), b) + unwrap_or(Nope, a) }" \
    7 9 "both-arms"
diff_case "$ENUM fn uo(o: Opt<i64>, d: i64) -> i64 { match o { Nope => d, Just(x) => x } } fn f(a: i64, b: i64) -> i64 { uo(Just(a), b) }" \
    5 3 "nope-first-order"

# 5. The Just binder USED in an expression (x + x), and a let-bound enum value whose
#    type is INFERRED (Nope -> Opt<i64>) then matched inline.
diff_case "$ENUM fn dbl(o: Opt<i64>) -> i64 { match o { Just(x) => x + x, Nope => 0 } } fn f(a: i64, b: i64) -> i64 { dbl(Just(a)) }" \
    21 0 "binder-in-expr"
diff_case "$ENUM fn f(a: i64, b: i64) -> i64 { let s = Just(a); let n = Nope; match s { Just(x) => x, Nope => b } + match n { Just(y) => y, Nope => b } }" \
    6 4 "let-bound-inferred"

# 6. NEGATIVE: mismatched arm body types (Just => i64, Nope => bool) must be rejected
#    by the self-hosted type checker.
"$TMP/sg" "$ENUM fn g(o: Opt<i64>) -> i64 { match o { Just(x) => x, Nope => true } } fn f(a: i64, b: i64) -> i64 { g(Just(a)) }" 3 4 > "$TMP/neg.ll" 2>/dev/null
grep -q 'TYPE ERROR' "$TMP/neg.ll" || { echo "FAIL [v107-enum-match/neg-arm-mismatch]: mismatched match arms were not rejected"; cat "$TMP/neg.ll"; exit 1; }
echo "PASS [neg-arm-mismatch]: a match with mismatched arm result types is a type error"

# 7. NEGATIVE: Just with the wrong payload type (bool, not i64) must be rejected.
"$TMP/sg" "$ENUM fn h(o: Opt<i64>) -> i64 { match o { Just(x) => x, Nope => 0 } } fn f(a: i64, b: i64) -> i64 { h(Just(a < b)) }" 3 4 > "$TMP/neg2.ll" 2>/dev/null
grep -q 'TYPE ERROR' "$TMP/neg2.ll" || { echo "FAIL [v107-enum-match/neg-payload-type]: a bool payload to Just(i64) was not rejected"; cat "$TMP/neg2.ll"; exit 1; }
echo "PASS [neg-payload-type]: Just(bool) where i64 expected is a type error"

echo "ALL v107 (self-hosted enum + match) SMOKE TESTS PASSED"
