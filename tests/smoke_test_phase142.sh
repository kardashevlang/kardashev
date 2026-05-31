#!/usr/bin/env bash
# v26 Phase 142 smoke test: struct patterns in match arms (`P { x, y: b, .. }`),
# desugared to an irrefutable binding + field-binding lets. Tuple / enum / param
# patterns already worked (regression-covered elsewhere). JIT + AOT.
set -uo pipefail
KARDC=""
for c in "${TEST_SRCDIR:-}/_main/compiler/kardc" "${TEST_SRCDIR:-}/kardashev/compiler/kardc" \
         "${RUNFILES_DIR:-}/_main/compiler/kardc" "${RUNFILES_DIR:-}/kardashev/compiler/kardc" \
         "./compiler/kardc" "./build.local/kardc"; do
    [[ -n "$c" && -x "$c" ]] && { KARDC="$c"; break; }; done
[[ -z "$KARDC" ]] && { echo "FAIL: kardc not found"; exit 1; }
echo "Using kardc at: $KARDC"
TMP=$(mktemp -d); trap 'rm -rf "$TMP"' EXIT
run_eq() { local jit; jit=$("$KARDC" "$2" 2>&1 | head -1)
    [[ "$jit" == "$3" ]] || { echo "FAIL [$1/jit]: want $3 got '$jit'"; exit 1; }
    "$KARDC" --no-cache -o "$TMP/b" "$2" >/dev/null 2>&1 || { echo "FAIL [$1/aot]: compile"; exit 1; }
    "$TMP/b" >/dev/null; local rc=$?; [[ "$rc" -eq "$3" ]] || { echo "FAIL [$1/aot]: exit $rc want $3"; exit 1; }; }

cat > "$TMP/a.kd" <<'EOF'
struct P { x: i64, y: i64 }
fn main() -> i64 { let p = P { x: 3, y: 4 }; match p { P { x, y } => x + y } }
EOF
run_eq basic "$TMP/a.kd" 7
echo "PASS [basic]: P { x, y } binds fields (7), JIT + AOT"

cat > "$TMP/b.kd" <<'EOF'
struct P { x: i64, y: i64, z: i64 }
fn main() -> i64 { let p = P { x: 1, y: 2, z: 3 }; match p { P { x: a, y, .. } => a * 10 + y } }
EOF
run_eq rename "$TMP/b.kd" 12
echo "PASS [rename]: P { x: a, y, .. } renames + skips (12)"

cat > "$TMP/c.kd" <<'EOF'
struct Msg { tag: i64, body: String }
fn len(m: Msg) -> i64 ! { alloc } { match m { Msg { tag, body } => tag + str_len(&body) } }
fn main() -> i64 ! { alloc } { len(Msg { tag: 5, body: int_to_string(99) }) }
EOF
run_eq nonCopy "$TMP/c.kd" 7
echo "PASS [nonCopy]: a String field destructures (partial move) — 7"

echo "PASS: Phase 142 — struct patterns in match arms"
