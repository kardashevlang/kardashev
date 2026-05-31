#!/usr/bin/env bash
# Phase 116 (Roadmap v20 — "toward a real bootstrap"): broaden Phase 115's
# differential gate into a FUZZER. For many random, valid `fn f(a,b) -> i64`
# functions (over params a/b, literals, `+ * < ==`, and parenthesized if/else —
# the self-hosted source language's operators), with random args:
#   - the SELF-HOSTED compiler (examples/selfhost/llvmgen.kd) emits LLVM IR for
#     the function, clang compiles it to native, and it runs -> R_self;
#   - the HOST compiler (kardc) compiles the same function + `fn main(){ f(a,b) }`
#     to native and runs -> R_host.
# They MUST agree (R_self == R_host) — i.e. the self-hosted codegen matches the
# host on every random function. Seeded for reproducibility; skips if no clang.
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
    "${TEST_SRCDIR:-}/_main/examples/selfhost/llvmgen.kd" "${TEST_SRCDIR:-}/kardashev/examples/selfhost/llvmgen.kd" \
    "${RUNFILES_DIR:-}/_main/examples/selfhost/llvmgen.kd" "${RUNFILES_DIR:-}/kardashev/examples/selfhost/llvmgen.kd" \
    "examples/selfhost/llvmgen.kd"; do
    if [[ -f "$cand" ]]; then SRC="$cand"; break; fi
done
[[ -z "$SRC" ]] && { echo "FAIL: examples/selfhost/llvmgen.kd not found"; exit 1; }
CLANG="$(command -v clang || true)"
[[ -z "$CLANG" ]] && { echo "PASS [phase116]: SKIPPED (no clang to compile the emitted IR)"; exit 0; }
echo "Using kardc at: $KARDC ; clang at: $CLANG"
TMP=$(mktemp -d); trap 'rm -rf "$TMP"' EXIT

# Build the self-hosted compiler once.
"$KARDC" --no-cache -o "$TMP/selfcc" "$SRC" >/dev/null 2>&1 || { echo "FAIL [phase116]: self-hosted compiler did not build"; exit 1; }

N="${FUZZ_N:-20}"
SEED="${FUZZ_SEED:-7331}"
RANDOM=$SEED

# A random INT expression over {a, b, 1..9, + *, parenthesized if(cmp){int}else{int}}.
gen_int() {
    local depth=$1
    if (( depth <= 0 || RANDOM % 3 == 0 )); then
        local p=$(( RANDOM % 4 ))
        if   (( p == 0 )); then G="a"
        elif (( p == 1 )); then G="b"
        else G="$(( RANDOM % 9 + 1 ))"; fi
        return
    fi
    local c=$(( RANDOM % 5 ))
    if (( c == 4 )); then
        local cnd; gen_cmp; cnd="$G"
        local t; gen_int $((depth-1)); t="$G"
        local e; gen_int $((depth-1)); e="$G"
        G="(if $cnd { $t } else { $e })"          # always parenthesized -> unambiguous
        return
    fi
    local op="+"; (( c == 1 )) && op="*"          # bias toward + (c in {0,2,3}=+, 1=*)
    local l; gen_int $((depth-1)); l="$G"
    gen_int $((depth-1)); G="($l $op $G)"
}
gen_cmp() {
    local l; gen_int 1; l="$G"
    local op="<"; (( RANDOM % 2 == 1 )) && op="=="
    gen_int 1; G="($l $op $G)"
}

fails=0
for ((i=0; i<N; i++)); do
    gen_int 3; body="$G"
    func="fn f(a: i64, b: i64) -> i64 { $body }"
    a=$(( RANDOM % 7 )); b=$(( RANDOM % 7 ))     # 0..6, so comparisons go both ways

    # SELF-HOSTED: emit IR -> clang -> run.
    if ! "$TMP/selfcc" "$func" "$a" "$b" > "$TMP/s.ll" 2>/dev/null; then
        echo "FAIL [phase116]: selfcc errored on: $func"; fails=$((fails+1)); continue
    fi
    if ! "$CLANG" "$TMP/s.ll" -o "$TMP/s" 2>/dev/null; then
        echo "FAIL [phase116]: clang rejected emitted IR for: $func"; cat "$TMP/s.ll"; fails=$((fails+1)); continue
    fi
    "$TMP/s" >/dev/null 2>&1; r_self=$?

    # HOST: same function + a main calling it.
    printf '%s\nfn main() -> i64 { f(%s, %s) }\n' "$func" "$a" "$b" > "$TMP/h.kd"
    if ! "$KARDC" --no-cache -o "$TMP/h" "$TMP/h.kd" >/dev/null 2>&1; then
        echo "FAIL [phase116]: host rejected: $func"; fails=$((fails+1)); continue
    fi
    "$TMP/h" >/dev/null 2>&1; r_host=$?

    if [[ "$r_self" -ne "$r_host" ]]; then
        echo "FAIL [phase116]: DISAGREE f($a,$b): self=$r_self host=$r_host  func={$func}"; fails=$((fails+1))
    fi
done

[[ "$fails" -eq 0 ]] || { echo "FAIL [phase116]: $fails/$N functions disagreed (seed $SEED)"; exit 1; }
echo "PASS [phase116]: $N random functions — self-hosted-emitted LLVM IR == host compiler (seed $SEED)"
echo "ALL PHASE 116 SMOKE TESTS PASSED"
