#!/usr/bin/env bash
# v62 — seeded process-global RNG: rand_global() over a global LCG, lazily
# seeded from KARDASHEV_SEED on first use; rng_seed_global(seed) to set it
# explicitly. Asserts: the SAME KARDASHEV_SEED reproduces an identical 5-value
# sequence (across separate runs AND JIT==AOT); a DIFFERENT seed differs; and
# an explicit rng_seed_global(seed) overrides the env. Determinism is the gate.
set -euo pipefail
KARDC=""
for c in "${TEST_SRCDIR:-}/_main/compiler/kardc" "${RUNFILES_DIR:-}/_main/compiler/kardc" "./compiler/kardc" "./build.local/kardc"; do
  [[ -n "$c" && -x "$c" ]] && { KARDC="$c"; break; }; done
[[ -z "$KARDC" ]] && { echo "FAIL: kardc not found"; exit 1; }
echo "Using kardc at: $KARDC"
TMP=$(mktemp -d); trap 'rm -rf "$TMP"' EXIT

# Program 1: five rand_global() values (seeded from KARDASHEV_SEED).
cat > "$TMP/seq.kd" <<'EOF'
fn main() -> i64 ! { io } {
    let mut i = 0;
    while i < 5 { print(rand_global()); i = i + 1; }
    0
}
EOF
"$KARDC" --no-cache -o "$TMP/seq" "$TMP/seq.kd" >/dev/null 2>&1

a=$(KARDASHEV_SEED=42 "$TMP/seq" 2>/dev/null | head -5)
b=$(KARDASHEV_SEED=42 "$TMP/seq" 2>/dev/null | head -5)
c=$(KARDASHEV_SEED=99 "$TMP/seq" 2>/dev/null | head -5)
jit=$(KARDASHEV_SEED=42 "$KARDC" "$TMP/seq.kd" 2>/dev/null | head -5)

[[ -n "$a" ]] || { echo "FAIL: empty rng output"; exit 1; }
[[ "$a" == "$b" ]] || { echo "FAIL: same seed (42) produced different sequences:
A=$a
B=$b"; exit 1; }
echo "PASS: same KARDASHEV_SEED reproduces the sequence (AOT, two runs)"
[[ "$a" != "$c" ]] || { echo "FAIL: different seeds (42 vs 99) produced the SAME sequence"; exit 1; }
echo "PASS: a different seed differs"
[[ "$a" == "$jit" ]] || { echo "FAIL: JIT != AOT for seed 42:
AOT=$a
JIT=$jit"; exit 1; }
echo "PASS: JIT == AOT for the seeded sequence"

# Program 2: explicit rng_seed_global overrides the env seed; two explicit
# seedings to the same value reproduce; different value differs.
cat > "$TMP/explicit.kd" <<'EOF'
fn main() -> i64 ! { io } {
    rng_seed_global(7);
    let mut i = 0;
    while i < 4 { print(rand_global()); i = i + 1; }
    0
}
EOF
e1=$(KARDASHEV_SEED=42 "$KARDC" "$TMP/explicit.kd" 2>/dev/null | head -4)
e2=$(KARDASHEV_SEED=999 "$KARDC" "$TMP/explicit.kd" 2>/dev/null | head -4)
[[ "$e1" == "$e2" ]] || { echo "FAIL: explicit rng_seed_global(7) should ignore KARDASHEV_SEED but differed:
E1=$e1
E2=$e2"; exit 1; }
echo "PASS: explicit rng_seed_global overrides KARDASHEV_SEED"

# --fuzz-seed CLI exports KARDASHEV_SEED for the JIT run.
f1=$("$KARDC" --fuzz-seed 42 "$TMP/seq.kd" 2>/dev/null | head -5)
[[ "$f1" == "$a" ]] || { echo "FAIL: --fuzz-seed 42 != KARDASHEV_SEED=42:
fuzz=$f1
env=$a"; exit 1; }
echo "PASS: --fuzz-seed matches KARDASHEV_SEED"

echo "ALL RNG-SEEDING SMOKE TESTS PASSED"
