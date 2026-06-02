#!/usr/bin/env bash
# v62 — environment variables: env_var(&String) -> Option<String> over getenv
# (owned copy on a hit) + env_var_set over setenv. Asserts: a set var reads
# back Some("v"); an absent var is None; a setenv round-trip reads Some("123").
# Differential JIT==AOT (env is fixed across both runs).
set -euo pipefail
KARDC=""
for c in "${TEST_SRCDIR:-}/_main/compiler/kardc" "${RUNFILES_DIR:-}/_main/compiler/kardc" "./compiler/kardc" "./build.local/kardc"; do
  [[ -n "$c" && -x "$c" ]] && { KARDC="$c"; break; }; done
[[ -z "$KARDC" ]] && { echo "FAIL: kardc not found"; exit 1; }
echo "Using kardc at: $KARDC"
TMP=$(mktemp -d); trap 'rm -rf "$TMP"' EXIT
unset DEFINITELY_NOT_SET_XYZ 2>/dev/null || true

cat > "$TMP/env.kd" <<'EOF'
fn show(name: &String) -> i64 ! { io, alloc } {
    match env_var(name) {
        Some(v) => { print_no_nl(&v); print_no_nl(&"\n"); 0 },
        None => { print_no_nl(&"<none>\n"); 0 }
    }
}
fn main() -> i64 ! { io, alloc } {
    let present = "TEST_VAR";
    show(&present);
    let absent = "DEFINITELY_NOT_SET_XYZ";
    show(&absent);
    let k = "ROUNDTRIP_KEY";
    let val = "123";
    env_var_set(&k, &val);
    show(&k);
    0
}
EOF
# Expected: TEST_VAR=world -> "world"; absent -> "<none>"; round-trip -> "123".
want=$'world\n<none>\n123'

jit=$(TEST_VAR=world "$KARDC" "$TMP/env.kd" 2>/dev/null | head -3) || true
[[ "$jit" == "$want" ]] || { echo "FAIL [jit]: expected '$want' got '$jit'"; TEST_VAR=world "$KARDC" "$TMP/env.kd" 2>&1|head -5; exit 1; }
echo "PASS: env (jit) — Some / None / setenv round-trip"
TEST_VAR=world "$KARDC" --no-cache -o "$TMP/env" "$TMP/env.kd" >/dev/null 2>&1
aot=$(TEST_VAR=world "$TMP/env" 2>/dev/null | head -3) || true
[[ "$aot" == "$want" ]] || { echo "FAIL [aot]: expected '$want' got '$aot'"; exit 1; }
echo "PASS: env (aot)"
echo "ALL ENV SMOKE TESTS PASSED"
