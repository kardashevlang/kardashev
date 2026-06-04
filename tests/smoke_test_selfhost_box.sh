#!/usr/bin/env bash
# Roadmap v108 — self-hosted Box<i64> heap indirection (ARC C): the self-hosted LLVM-IR
# compiler (examples/selfhost/structgen.kd) now PARSES and LOWERS a heap-allocated
# single i64 — `Box::new(e)` (constructor) and `*b` (deref) — to real, self-contained
# IR, freed once at the function exit. self == host gated + AddressSanitizer-clean.
#
# Representation (host-confirmed by the v108 research probe): a Box<i64> is a bare ptr
# (type tag 600). `Box::new(e)` lowers to `call ptr @malloc(i64 8)` + `store i64 <e>`;
# `*b` lowers to a `load i64`. A `let mut` Box owns its heap and is freed once at the
# single fn exit (`load ptr` from its slot + `call void @free`) — sound because the
# subset has no early return. The lexer gained a `::` token (Box::new is the only host
# spelling — `box`/`box_new` are rejected by the host) and parse_factor a prefix-`*`
# deref (kind 11 at a factor start, vs infix multiply after a left operand).
#
# USE-GATED (Risk R0): a program that uses NO Box emits BYTE-IDENTICAL IR — the eight
# prior self-host gates are the guard, plus the demo guard below asserts the default
# program has no `@malloc`/`@free` and exits 7.
#
# Differential self == host: the self-hosted-emitted IR (clang -> native) must
# exit-match `kardc` on the equivalent program. Each test program keeps
# `f(a: i64, b: i64) -> i64` so the host's `fn main() { f(a, b) }` wrapper works. Exit
# codes mod 256. AddressSanitizer (when available) confirms the freed Box has no leak
# / use-after-free / double-free. Skips if clang is unavailable.
#
# DEFERRALS (v108, honest): Box<i64> ONLY (no Box of struct / String / bool / generic
# T — mirrors Opt<i64>=500 / Vec<i64>=4); read-only deref (the host has no
# deref-assign); no returning a Box and no Box-typed params (check_fn rejects a return
# tag >= 200; the subset keeps a Box a within-fn `let mut` local); no nested Box-of-Box
# (BoxNew requires an i64 payload); only the FINAL value of a reassigned `let mut` box
# is freed (drop-on-reassign needs liveness — same documented limit as owned String);
# a plain immutable `let p = Box::new(..)` lowers + typechecks but is not freed (no
# slot) so the gate uses `let mut`.
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
[[ -z "$CLANG" ]] && { echo "PASS [v108-box]: SKIPPED (no clang to compile the emitted IR)"; exit 0; }
echo "Using kardc at: $KARDC ; clang at: $CLANG"
TMP=$(mktemp -d); trap 'rm -rf "$TMP"' EXIT

"$KARDC" --no-cache -o "$TMP/sg" "$SRC" >/dev/null 2>&1 || { echo "FAIL [v108-box]: structgen did not build"; exit 1; }

# 1. BYTE-IDENTITY guard (Risk R0): the default (no-Box) demo must build its struct,
#    emit NO @malloc / @free (the Box runtime markers), and exit 7.
"$TMP/sg" > "$TMP/d.ll" 2>/dev/null || { echo "FAIL [v108-box]: demo did not run"; exit 1; }
grep -q 'insertvalue { i64, i64 }' "$TMP/d.ll" || { echo "FAIL [v108-box]: { i64, i64 } struct regressed"; cat "$TMP/d.ll"; exit 1; }
grep -q '@malloc' "$TMP/d.ll" && { echo "FAIL [v108-box]: no-Box demo emitted @malloc (R0)"; cat "$TMP/d.ll"; exit 1; }
grep -q '@free'   "$TMP/d.ll" && { echo "FAIL [v108-box]: no-Box demo emitted @free (R0)"; cat "$TMP/d.ll"; exit 1; }
"$CLANG" "$TMP/d.ll" -o "$TMP/d" 2>/dev/null || { echo "FAIL [v108-box]: clang rejected demo IR"; cat "$TMP/d.ll"; exit 1; }
"$TMP/d" >/dev/null 2>&1; rd=$?
[[ "$rd" -eq 7 ]] || { echo "FAIL [v108-box]: demo exit $rd (want 7)"; exit 1; }
echo "PASS [byte-identity]: no-Box demo has no @malloc/@free; native exit 7"

# DIFFERENTIAL helper: self-hosted IR (clang -> native, AND clang+ASan -> native) must
# exit-match the host kardc on `<prog>\nfn main() { f(a, b) }`. ASan asserts no
# leak/UAF/double-free on the freed Box.
diff_case() {  # $1 source, $2 a, $3 b, $4 label
    "$TMP/sg" "$1" "$2" "$3" > "$TMP/s.ll" 2>/dev/null || { echo "FAIL [v108-box/$4]: selfcc errored"; exit 1; }
    "$CLANG" "$TMP/s.ll" -o "$TMP/s" 2>/dev/null || { echo "FAIL [v108-box/$4]: clang rejected IR"; cat "$TMP/s.ll"; exit 1; }
    "$TMP/s" >/dev/null 2>&1; local r_self=$?
    printf '%s\nfn main() -> i64 { f(%s, %s) }\n' "$1" "$2" "$3" > "$TMP/h.kd"
    "$KARDC" --no-cache -o "$TMP/h" "$TMP/h.kd" >/dev/null 2>&1 || { echo "FAIL [v108-box/$4]: host rejected program"; exit 1; }
    "$TMP/h" >/dev/null 2>&1; local r_host=$?
    [[ "$r_self" -eq "$r_host" ]] || { echo "FAIL [v108-box/$4]: self=$r_self != host=$r_host"; exit 1; }
    # ASan (best-effort: only assert if the ASan build succeeds). LeakSanitizer
    # (detect_leaks) is Linux-only — passing it on macOS aborts ASan (Abort trap: 6),
    # so enable leak detection only on Linux; macOS still catches UAF/double-free with
    # plain ASan (and the Box is freed once at exit, so there is nothing to leak anyway).
    if "$CLANG" -fsanitize=address "$TMP/s.ll" -o "$TMP/sa" 2>/dev/null; then
        local r_asan
        if [[ "$(uname -s)" == "Linux" ]]; then
            ASAN_OPTIONS=detect_leaks=1 "$TMP/sa" >/dev/null 2>&1; r_asan=$?
        else
            "$TMP/sa" >/dev/null 2>&1; r_asan=$?
        fi
        [[ "$r_asan" -eq "$r_host" ]] || { echo "FAIL [v108-box/$4]: ASan exit $r_asan != $r_host (leak/UAF/double-free)"; exit 1; }
        echo "PASS [$4]: self == host == ASan == $r_self"
    else
        echo "PASS [$4]: self == host == $r_self (ASan build unavailable)"
    fi
}

# 2. Box::new(a + b) then *p — assert the IR has malloc(8) + store i64 + load i64 + free.
diff_case 'fn f(a: i64, b: i64) -> i64 { let mut p = Box::new(a + b); *p }' 20 22 "box-add-deref"
"$TMP/sg" 'fn f(a: i64, b: i64) -> i64 { let mut p = Box::new(a + b); *p }' 20 22 > "$TMP/m.ll" 2>/dev/null
grep -q 'call ptr @malloc(i64 8)' "$TMP/m.ll" || { echo "FAIL [box-add-deref]: no malloc(8)"; cat "$TMP/m.ll"; exit 1; }
grep -Eq 'store i64 .*, ptr' "$TMP/m.ll"    || { echo "FAIL [box-add-deref]: no store i64"; cat "$TMP/m.ll"; exit 1; }
grep -Eq '= load i64, ptr' "$TMP/m.ll"      || { echo "FAIL [box-add-deref]: no load i64 (deref)"; cat "$TMP/m.ll"; exit 1; }
grep -q 'call void @free(ptr' "$TMP/m.ll"   || { echo "FAIL [box-add-deref]: Box not freed at exit"; cat "$TMP/m.ll"; exit 1; }
echo "PASS [box-add-deref-ir]: malloc(8) + store i64 + load i64 + free"

# 3. Box::new(a - b); two independent boxes (two malloc + two free); box value reused.
diff_case 'fn f(a: i64, b: i64) -> i64 { let mut p = Box::new(a - b); *p }' 50 8 "box-sub"
diff_case 'fn f(a: i64, b: i64) -> i64 { let mut p = Box::new(a); let mut q = Box::new(b); *p + *q }' 17 25 "two-boxes"
"$TMP/sg" 'fn f(a: i64, b: i64) -> i64 { let mut p = Box::new(a); let mut q = Box::new(b); *p + *q }' 17 25 > "$TMP/m2.ll" 2>/dev/null
nm=$(grep -c 'call ptr @malloc(i64 8)' "$TMP/m2.ll"); nf=$(grep -c 'call void @free(ptr' "$TMP/m2.ll")
[[ "$nm" -eq 2 && "$nf" -eq 2 ]] || { echo "FAIL [two-boxes]: expected 2 malloc + 2 free, got $nm/$nf"; cat "$TMP/m2.ll"; exit 1; }
echo "PASS [two-boxes-balance]: 2 malloc + 2 free (no leak / double-free)"

# 4. A boxed value combined with a plain arg; a Box created inside a helper fn.
diff_case 'fn f(a: i64, b: i64) -> i64 { let mut p = Box::new(a * a); *p + b }' 6 6 "box-then-use"
diff_case 'fn ub(a: i64) -> i64 { let mut p = Box::new(a + a); *p } fn f(a: i64, b: i64) -> i64 { ub(a) + ub(b) }' 10 11 "box-in-helper-fn"

# 5. NEGATIVE: Box::new(bool) — payload must be i64.
"$TMP/sg" 'fn f(a: i64, b: i64) -> i64 { let mut p = Box::new(a < b); *p }' 3 4 > "$TMP/neg.ll" 2>/dev/null
grep -q 'TYPE ERROR' "$TMP/neg.ll" || { echo "FAIL [v108-box/neg-box-bool]: Box::new(bool) was not rejected"; cat "$TMP/neg.ll"; exit 1; }
echo "PASS [neg-box-bool]: Box::new(bool) is a type error (payload must be i64)"

# 6. NEGATIVE: deref of a non-Box (an i64) must be a type error.
"$TMP/sg" 'fn f(a: i64, b: i64) -> i64 { *a }' 3 4 > "$TMP/neg2.ll" 2>/dev/null
grep -q 'TYPE ERROR' "$TMP/neg2.ll" || { echo "FAIL [v108-box/neg-deref-i64]: *<i64> was not rejected"; cat "$TMP/neg2.ll"; exit 1; }
echo "PASS [neg-deref-i64]: dereferencing a non-Box is a type error"

echo "ALL v108 (self-hosted Box heap indirection) SMOKE TESTS PASSED"
