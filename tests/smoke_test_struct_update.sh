#!/usr/bin/env bash
# v59 — struct-update syntax `S { x: 10, ..base }`: fields not given explicitly
# are taken from `base`. This version supports a Copy base (a struct whose fields
# are all Copy — scalars/arrays/tuples) so the base is byte-copied and selectively
# overwritten with no move/drop obligation; the base is consumed (kardashev
# structs move). Move-field spread (heap fields) is deferred and rejected cleanly.
# Differential JIT==AOT.
set -euo pipefail
KARDC=""
for c in "${TEST_SRCDIR:-}/_main/compiler/kardc" "${RUNFILES_DIR:-}/_main/compiler/kardc" "./compiler/kardc" "./build.local/kardc"; do
  [[ -n "$c" && -x "$c" ]] && { KARDC="$c"; break; }; done
[[ -z "$KARDC" ]] && { echo "FAIL: kardc not found"; exit 1; }
echo "Using kardc at: $KARDC"
TMP=$(mktemp -d); trap 'rm -rf "$TMP"' EXIT

diff_run() { local name="$1" expect="$2" src="$3"
  local n; n=$(printf '%s\n' "$expect" | wc -l)
  printf '%s' "$src" > "$TMP/$name.kd"
  local jit; jit=$("$KARDC" "$TMP/$name.kd" 2>/dev/null | head -n "$n") || true
  [[ "$jit" == "$expect" ]] || { echo "FAIL [$name/jit]: expected '$expect' got '$jit'"; "$KARDC" "$TMP/$name.kd" 2>&1|head -3; exit 1; }
  "$KARDC" --no-cache -o "$TMP/$name" "$TMP/$name.kd" >/dev/null 2>&1
  local aot; aot=$("$TMP/$name" 2>/dev/null | head -n "$n") || true
  [[ "$aot" == "$expect" ]] || { echo "FAIL [$name/aot]: expected '$expect' got '$aot'"; exit 1; }
  echo "PASS: $name"; }
reject() { local name="$1" needle="$2" src="$3"; printf '%s' "$src" > "$TMP/$name.kd"
  local e; e=$("$KARDC" "$TMP/$name.kd" 2>&1 >/dev/null || true)
  echo "$e" | grep -qi "$needle" || { echo "FAIL[reject $name]: want '$needle' got: $e"; exit 1; }
  echo "PASS(reject): $name"; }

S='struct S { x: i64, y: i64, z: i64 }'
# one field overridden, the rest from the base
diff_run one $'10\n2\n3' "$S"' fn main() -> i64 ! { io } { let s = S { x: 1, y: 2, z: 3 }; let t = S { x: 10, ..s }; print(t.x); print(t.y); print(t.z); 0 }'
# several fields overridden, one from the base
diff_run several $'10\n20\n3' "$S"' fn main() -> i64 ! { io } { let s = S { x: 1, y: 2, z: 3 }; let t = S { x: 10, y: 20, ..s }; print(t.x); print(t.y); print(t.z); 0 }'
# spread the base into two updates (chained)
diff_run chain $'10\n20\n30' "$S"' fn main() -> i64 ! { io } { let a = S { x: 1, y: 2, z: 3 }; let b = S { x: 10, ..a }; let c = S { y: 20, z: 30, ..b }; print(c.x); print(c.y); print(c.z); 0 }'
# bool fields
diff_run boolfld $'1\n0' 'struct F { a: bool, b: bool } fn main() -> i64 ! { io } { let f = F { a: false, b: false }; let g = F { a: true, ..f }; if g.a { print(1); } else { print(0); } if g.b { print(1); } else { print(0); } 0 }'

# ---- rejects ----
# NB: struct names must avoid single-uppercase letters (A/B/E/F/T/...) — those
# collide with prelude generic params and emit spurious "generic parameter 'X'
# shadows an existing type" errors whose emission order is hash-map-iteration
# dependent (libstdc++ vs libc++), which non-deterministically crowded out the
# intended "same struct" error on ubuntu CI. Use Widget/Gadget.
reject wrong_type  'same struct' 'struct Widget { x: i64 } struct Gadget { x: i64 } fn main() -> i64 { let a = Widget { x: 1 }; let b = Gadget { ..a }; 0 }'
reject move_field  'Copy'        'struct M { name: String, n: i64 } fn main() -> i64 ! { alloc } { let a = M { name: "x", n: 1 }; let b = M { n: 2, ..a }; 0 }'

echo "ALL STRUCT-UPDATE SMOKE TESTS PASSED"
