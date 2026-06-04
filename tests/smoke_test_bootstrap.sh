#!/usr/bin/env bash
# Roadmap v99 — bootstrap fixed-point CANDIDATE (honestly scoped).
#
# WHAT THIS IS — and IS NOT. The self-hosted compiler `examples/selfhost/structgen.kd`
# is a SUBSET EMITTER: it takes ONE program string (defining `fn f(a, b)`) and emits
# LLVM IR for it. It is NOT a whole-program compiler and CANNOT compile its own
# 1874-line source (which uses HashMap, Box, enum/match on Option, multi-param
# generics, …) — feeding it a real `examples/selfhost/*.kd` file segfaults it. So a
# literal "structgen compiles structgen" self-compile fixed point is OUT OF REACH on
# a subset emitter; that full-tree bootstrap is the XL mega-arc, tracked file-by-file
# in docs/bootstrap-status.md. This gate therefore asserts the genuine, load-bearing,
# bootstrap-NECESSARY properties that DO hold today:
#
#   (A) DETERMINISM / idempotence — a fixed program compiles to BYTE-IDENTICAL IR
#       across repeated runs. (A compiler must be deterministic to have a fixed
#       point; non-determinism would make any bootstrap impossible.)
#   (B) CORPUS self-application — a corpus of in-subset programs, one per shipped
#       self-hosting feature (v91 loops, v92 Vec, v85 refs, v86 calls/strings, v94
#       generics, v98 trait dispatch, v99 effect rows), each compiles DETERMINISTICALLY
#       and exit-matches the host (self == host). This is the real, non-trivial
#       "the self-hosted compiler correctly + stably compiles the language it claims
#       to support" guarantee.
#
# Named honestly: this is a DETERMINISM + CORPUS candidate, not a self-compile.
# Differential vs the host; mod-256 exit compare; skips if clang is unavailable.
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
[[ -z "$CLANG" ]] && { echo "PASS [v99-bootstrap]: SKIPPED (no clang to compile the emitted IR)"; exit 0; }
echo "Using kardc at: $KARDC ; clang at: $CLANG"
TMP=$(mktemp -d); trap 'rm -rf "$TMP"' EXIT

"$KARDC" --no-cache -o "$TMP/sg" "$SRC" >/dev/null 2>&1 || { echo "FAIL [v99-bootstrap]: structgen did not build"; exit 1; }

COVERED=0
# A corpus case: (A) determinism — compile twice, assert byte-identical IR; then
# (B) self == host on the emitted program.
corpus_case() {  # $1 source, $2 a, $3 b, $4 label
    "$TMP/sg" "$1" "$2" "$3" > "$TMP/r1.ll" 2>/dev/null || { echo "FAIL [bootstrap/$4]: selfcc errored (run 1)"; exit 1; }
    "$TMP/sg" "$1" "$2" "$3" > "$TMP/r2.ll" 2>/dev/null || { echo "FAIL [bootstrap/$4]: selfcc errored (run 2)"; exit 1; }
    diff -q "$TMP/r1.ll" "$TMP/r2.ll" >/dev/null || { echo "FAIL [bootstrap/$4]: NON-DETERMINISTIC (two runs differ)"; diff "$TMP/r1.ll" "$TMP/r2.ll" | head; exit 1; }
    "$CLANG" "$TMP/r1.ll" -o "$TMP/s" 2>/dev/null || { echo "FAIL [bootstrap/$4]: clang rejected IR"; cat "$TMP/r1.ll"; exit 1; }
    "$TMP/s" >/dev/null 2>&1; local r_self=$?
    printf '%s\nfn main() -> i64 { f(%s, %s) }\n' "$1" "$2" "$3" > "$TMP/h.kd"
    "$KARDC" --no-cache -o "$TMP/h" "$TMP/h.kd" >/dev/null 2>&1 || { echo "FAIL [bootstrap/$4]: host rejected program"; exit 1; }
    "$TMP/h" >/dev/null 2>&1; local r_host=$?
    [[ "$r_self" -eq "$r_host" ]] || { echo "FAIL [bootstrap/$4]: self=$r_self != host=$r_host"; exit 1; }
    COVERED=$((COVERED + 1))
    echo "PASS [$4]: deterministic + self == host == $r_self"
}

# --- (A) determinism is also asserted on the default demo over THREE runs ---
"$TMP/sg" > "$TMP/d1.ll" 2>/dev/null
"$TMP/sg" > "$TMP/d2.ll" 2>/dev/null
"$TMP/sg" > "$TMP/d3.ll" 2>/dev/null
diff -q "$TMP/d1.ll" "$TMP/d2.ll" >/dev/null && diff -q "$TMP/d2.ll" "$TMP/d3.ll" >/dev/null \
    || { echo "FAIL [determinism]: the default demo emitted non-identical IR across 3 runs"; exit 1; }
echo "PASS [determinism]: default demo byte-identical across 3 runs"

# --- (B) the corpus — one in-subset program per shipped self-hosting feature ---
corpus_case "fn f(a: i64, b: i64) -> i64 { a + b * 2 }" 3 4 "arith"
corpus_case "struct Widget { x: i64, y: i64 } fn f(a: i64, b: i64) -> i64 { let w = Widget { x: a, y: b } ; w.x + w.y }" 3 4 "struct"            # v84
corpus_case "struct Widget { v: i64 } fn rd(w: &Widget) -> i64 { w.v } fn f(a: i64, b: i64) -> i64 { let wi = Widget { v: a + b } ; rd(&wi) }" 3 4 "ref"  # v85 (&Struct)
corpus_case "fn g(x: i64) -> i64 { x * x } fn f(a: i64, b: i64) -> i64 { g(a) + g(b) }" 3 4 "call"                                               # v86
corpus_case "fn f(a: i64, b: i64) -> i64 { let mut s = 0 ; let mut i = 0 ; while i < b { s = s + a ; i = i + 1 ; } ; s }" 5 4 "loop"             # v91
corpus_case "fn f(a: i64, b: i64) -> i64 ! { alloc } { let mut v = vec_new() ; vec_push(&mut v, a) ; vec_push(&mut v, b) ; vec_get(&v, 0) + vec_get(&v, 1) }" 3 4 "vec"  # v92
corpus_case "fn id<T>(x: T) -> T { x } fn f(a: i64, b: i64) -> i64 { id(a) + id(b) }" 3 4 "generic"                                              # v94
corpus_case "struct Widget { v: i64 } trait Thing { fn get(&self) -> i64 ; } impl Thing for Widget { fn get(&self) -> i64 { self.v } } fn f(a: i64, b: i64) -> i64 { let w = Widget { v: a + b } ; w.get() }" 3 4 "trait-dispatch"  # v98
corpus_case "fn f(a: i64, b: i64) -> i64 ! { io, alloc } { a + b }" 3 4 "effect-row"                                                            # v99
corpus_case "struct Widget { v: i64 } trait Thing { fn dbl(&self) -> i64 ; fn one(&self) -> i64 ; } impl Thing for Widget { fn one(&self) -> i64 { self.v } fn dbl(&self) -> i64 { self.one() + self.one() } } fn f(a: i64, b: i64) -> i64 ! { alloc } { let w = Widget { v: a + b } ; w.dbl() }" 3 4 "capstone"  # combined

# The covered set must be non-trivial (and grows as the subset grows: v98 had only
# the per-feature gates; v99 is the first to assert them TOGETHER as a stable corpus).
[[ "$COVERED" -ge 10 ]] || { echo "FAIL [coverage]: corpus too small ($COVERED < 10)"; exit 1; }
echo "PASS [coverage]: $COVERED in-subset programs deterministic + self == host"
echo "NOTE: full-tree self-compile (structgen compiling examples/selfhost/*.kd) is"
echo "      out of subset — tracked file-by-file in docs/bootstrap-status.md."
echo "ALL v99 (bootstrap determinism + corpus candidate) SMOKE TESTS PASSED"
