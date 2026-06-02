#!/usr/bin/env bash
# v62 — monotonic clock: `Instant` over the `monotonic_millis` builtin.
# We assert the SOUND invariants (monotonicity + non-negative deltas), not a
# tight timing window, to avoid CI flake (the roadmap gate's guidance). A large
# busy loop (kept live by printing a value derived from it) sits between two
# readings so real time elapses; differential JIT==AOT on the booleans.
set -euo pipefail
KARDC=""
for c in "${TEST_SRCDIR:-}/_main/compiler/kardc" "${RUNFILES_DIR:-}/_main/compiler/kardc" "./compiler/kardc" "./build.local/kardc"; do
  [[ -n "$c" && -x "$c" ]] && { KARDC="$c"; break; }; done
[[ -z "$KARDC" ]] && { echo "FAIL: kardc not found"; exit 1; }
echo "Using kardc at: $KARDC"
TMP=$(mktemp -d); trap 'rm -rf "$TMP"' EXIT

cat > "$TMP/inst.kd" <<'EOF'
fn main() -> i64 ! { io } {
    let t1 = instant_now();
    let mut s = 0;
    let mut i = 0;
    while i < 50000000 { s = s + i; i = i + 1; }
    let t2 = instant_now();
    let d = instant_duration_since(&t2, &t1);
    if t2.ms >= t1.ms { print(1); } else { print(0); }
    if instant_elapsed_millis(&t1) >= 0 { print(1); } else { print(0); }
    if duration_as_millis(&d) >= 0 { print(1); } else { print(0); }
    print(s - s);
    0
}
EOF
want=$'1\n1\n1\n0'
jit=$("$KARDC" "$TMP/inst.kd" 2>/dev/null | head -4) || true
[[ "$jit" == "$want" ]] || { echo "FAIL [jit]: expected '$want' got '$jit'"; "$KARDC" "$TMP/inst.kd" 2>&1|head -4; exit 1; }
echo "PASS: instant (jit) — monotonic + non-negative deltas"
"$KARDC" --no-cache -o "$TMP/inst" "$TMP/inst.kd" >/dev/null 2>&1
aot=$("$TMP/inst" 2>/dev/null | head -4) || true
[[ "$aot" == "$want" ]] || { echo "FAIL [aot]: expected '$want' got '$aot'"; exit 1; }
echo "PASS: instant (aot)"
echo "ALL INSTANT SMOKE TESTS PASSED"
