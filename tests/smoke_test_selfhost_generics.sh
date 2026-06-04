#!/usr/bin/env bash
# Roadmap v94 (PART 1) — self-hosted MONOMORPHIC GENERICS: the self-hosted LLVM-IR
# compiler (examples/selfhost/structgen.kd) now parses a single type-parameter on
# fn and struct declarations (`fn id<T>(x: T) -> T`, `struct Pair<T> { a: T, b: T }`)
# and MONOMORPHIZES it — one specialized copy is emitted per concrete type used at a
# call / struct-literal site, deduped by mangled name (`id__i64`, `Pair__i64`,
# `idp__Box2`), mirroring the host compiler's `emittedInstances_`. A generic decl
# emits NOTHING on its own; only its instances are emitted. The concrete tag bound
# to `T` is inferred from the first generic-typed argument / field value, and the
# generic param tag (-1) is substituted at emit time over the SHARED body AST (no
# deep clone). The host's i64-mono subset is the floor (struct instantiation works
# because the real tag is inferred).
#
# USE-GATED (Risk R0): a program that uses NO generics emits BYTE-IDENTICAL IR — the
# six existing self-host gates (phase117/118, selfhost_refs/calls/loops/vec) are the
# guard, and the demo guard below asserts the default program has no `__` mangling.
#
# Differential-gated vs the host: the self-hosted-emitted IR (clang -> native) must
# exit-match `kardc` on the equivalent program. Test programs keep
# `f(a: i64, b: i64) -> i64` so the host's `fn main() { f(a, b) }` wrapper works;
# generic fns/structs are OTHER decls that `f` uses. Exit codes are compared mod 256.
# Skips if clang is unavailable.
#
# DEFERRALS (v94 PART 1): generic TRAIT DISPATCH (vtables -> v98); const-generics;
# multi-param generics (`<A, B>` — only a single `T` is supported); nested
# instantiation (a generic fn calling another generic, or a generic-over-generic).
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
[[ -z "$CLANG" ]] && { echo "PASS [v94-generics]: SKIPPED (no clang to compile the emitted IR)"; exit 0; }
echo "Using kardc at: $KARDC ; clang at: $CLANG"
TMP=$(mktemp -d); trap 'rm -rf "$TMP"' EXIT

"$KARDC" --no-cache -o "$TMP/sg" "$SRC" >/dev/null 2>&1 || { echo "FAIL [v94-generics]: structgen did not build"; exit 1; }

# 1. BYTE-IDENTITY guard (Risk R0): the default (no-generics) demo must emit NO
#    monomorphization mangling, still build a `{ i64, i64 }` struct, and exit 7.
"$TMP/sg" > "$TMP/d.ll" 2>/dev/null || { echo "FAIL [v94-generics]: demo did not run"; exit 1; }
grep -q 'insertvalue { i64, i64 }' "$TMP/d.ll" || { echo "FAIL [v94-generics]: { i64, i64 } struct regressed"; cat "$TMP/d.ll"; exit 1; }
grep -q '__'    "$TMP/d.ll" && { echo "FAIL [v94-generics]: no-generics demo emitted a mangled instance (R0)"; cat "$TMP/d.ll"; exit 1; }
grep -q '@id__' "$TMP/d.ll" && { echo "FAIL [v94-generics]: no-generics demo emitted @id__ (R0)"; cat "$TMP/d.ll"; exit 1; }
"$CLANG" "$TMP/d.ll" -o "$TMP/d" 2>/dev/null || { echo "FAIL [v94-generics]: clang rejected demo IR"; cat "$TMP/d.ll"; exit 1; }
"$TMP/d" >/dev/null 2>&1; rd=$?
[[ "$rd" -eq 7 ]] || { echo "FAIL [v94-generics]: demo exit $rd (want 7)"; exit 1; }
echo "PASS [byte-identity]: no-generics demo has no mangled instance; native exit 7"

# DIFFERENTIAL helper: emit IR from the self-hosted compiler, clang -> native, and
# compare its exit to the host kardc on `<prog>\nfn main() { f(a, b) }`.
diff_case() {  # $1 source, $2 a, $3 b, $4 label
    "$TMP/sg" "$1" "$2" "$3" > "$TMP/s.ll" 2>/dev/null || { echo "FAIL [v94-generics/$4]: selfcc errored"; exit 1; }
    "$CLANG" "$TMP/s.ll" -o "$TMP/s" 2>/dev/null || { echo "FAIL [v94-generics/$4]: clang rejected IR"; cat "$TMP/s.ll"; exit 1; }
    "$TMP/s" >/dev/null 2>&1; local r_self=$?
    printf '%s\nfn main() -> i64 { f(%s, %s) }\n' "$1" "$2" "$3" > "$TMP/h.kd"
    "$KARDC" --no-cache -o "$TMP/h" "$TMP/h.kd" >/dev/null 2>&1 || { echo "FAIL [v94-generics/$4]: host rejected program"; exit 1; }
    "$TMP/h" >/dev/null 2>&1; local r_host=$?
    [[ "$r_self" -eq "$r_host" ]] || { echo "FAIL [v94-generics/$4]: self=$r_self != host=$r_host"; exit 1; }
    echo "PASS [$4]: self == host == $r_self"
}
# Same as diff_case but also asserts the self-hosted IR contains a regex.
diff_case_ir() {  # $1 source, $2 a, $3 b, $4 label, $5 ir-grep-regex
    diff_case "$1" "$2" "$3" "$4"
    grep -Eq "$5" "$TMP/s.ll" || { echo "FAIL [v94-generics/$4]: IR missing /$5/"; cat "$TMP/s.ll"; exit 1; }
    echo "PASS [$4-ir]: IR contains /$5/"
}

# 2. Generic fn specialized at i64 -> one @id__i64 instance; the generic decl @id
#    is NOT emitted.
diff_case_ir "fn id<T>(x: T) -> T { x } fn f(a: i64, b: i64) -> i64 { id(a) + id(b) }" \
    3 4 "fn-at-i64" 'define i64 @id__i64\('
"$TMP/sg" "fn id<T>(x: T) -> T { x } fn f(a: i64, b: i64) -> i64 { id(a) + id(b) }" 3 4 > "$TMP/g.ll" 2>/dev/null
grep -q 'define i64 @id(' "$TMP/g.ll" && { echo "FAIL [v94-generics/fn-at-i64]: generic decl @id was emitted (should only emit instances)"; exit 1; }
echo "PASS [fn-at-i64-no-generic-decl]: the generic @id decl emits nothing; only @id__i64"

# 3. Generic fn applied to a STRUCT value, returning a field -> @idp__Box2 over
#    `{ i64 }` (the real concrete tag, not collapsed to i64).
diff_case_ir "struct Box2 { v: i64 } fn idp<T>(x: T) -> T { x } fn f(a: i64, b: i64) -> i64 { let p = Box2 { v: a + b } ; let q = idp(p) ; q.v }" \
    3 4 "fn-at-struct" '@idp__Box2\(\{ i64 \}'

# 4. Generic STRUCT built + summed -> Pair<T> at i64 lowers to `{ i64, i64 }`.
diff_case_ir "struct Pair<T> { a: T, b: T } fn f(a: i64, b: i64) -> i64 { let p = Pair { a: a, b: b } ; p.a + p.b }" \
    3 4 "struct-build-sum" 'insertvalue \{ i64, i64 \}'
diff_case "struct Pair<T> { a: T, b: T } fn f(a: i64, b: i64) -> i64 { let p = Pair { a: a, b: b } ; if p.a < p.b { p.b } else { p.a } }" \
    9 2 "struct-field-in-if"

# 5. (optional) A generic fn called at TWO different types in one program -> dedup
#    by mangled name => exactly TWO emitted instances (@id__i64 and @id__Wrap).
PROG_D='struct Wrap { v: i64 } fn id<T>(x: T) -> T { x } fn f(a: i64, b: i64) -> i64 { let w = Wrap { v: b } ; let r = id(w) ; id(a) + r.v }'
diff_case "$PROG_D" 3 4 "fn-two-types"
"$TMP/sg" "$PROG_D" 3 4 > "$TMP/dd.ll" 2>/dev/null
ninst=$(grep -Ec 'define .*@id__' "$TMP/dd.ll")
[[ "$ninst" -eq 2 ]] || { echo "FAIL [v94-generics/fn-two-types]: expected 2 deduped instances, got $ninst"; cat "$TMP/dd.ll"; exit 1; }
echo "PASS [fn-two-types-dedup]: 2 mangled instances (@id__i64, @id__Wrap)"

# 6. (optional) The SAME generic fn called twice at i64 dedups to ONE instance.
"$TMP/sg" "fn id<T>(x: T) -> T { x } fn f(a: i64, b: i64) -> i64 { id(a) + id(b) }" 3 4 > "$TMP/dd2.ll" 2>/dev/null
n1=$(grep -Ec 'define .*@id__' "$TMP/dd2.ll")
[[ "$n1" -eq 1 ]] || { echo "FAIL [v94-generics/same-type-dedup]: expected 1 instance, got $n1"; cat "$TMP/dd2.ll"; exit 1; }
echo "PASS [same-type-dedup]: id(a)+id(b) at i64 emits exactly one @id__i64"

# 7. NEGATIVE: an ill-typed generic call (T inferred i64 from arg 0, arg 1 is bool)
#    must be rejected by the self-hosted type checker.
"$TMP/sg" "fn pick<T>(x: T, y: T) -> T { x } fn f(a: i64, b: i64) -> i64 { pick(a, a < b) }" 3 4 > "$TMP/neg.ll" 2>/dev/null
grep -q 'TYPE ERROR' "$TMP/neg.ll" || { echo "FAIL [v94-generics/neg-mismatch]: ill-typed generic call was not rejected"; cat "$TMP/neg.ll"; exit 1; }
echo "PASS [neg-mismatch]: a generic call with mismatched arg types is a type error"

echo "ALL v94 (self-hosted generics) SMOKE TESTS PASSED"
