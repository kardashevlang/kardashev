#!/usr/bin/env bash
# v55 — `Drop` is now a built-in prelude trait, so `impl Drop for T` resolves
# WITHOUT the user re-declaring `trait Drop`. The drop glue (user destructor
# first, then reverse-field drop + drop-flag machinery) has shipped since
# Phase 16; this only closes the declaration gap that previously errored
# "unknown trait 'Drop'". A user-declared `trait Drop` still wins (guarded).
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
  [[ "$jit" == "$expect" ]] || { echo "FAIL [$name/jit]: expected '$expect' got '$jit'"; "$KARDC" "$TMP/$name.kd" 2>&1 | head -3; exit 1; }
  "$KARDC" --no-cache -o "$TMP/$name" "$TMP/$name.kd" >/dev/null 2>&1
  local aot; aot=$("$TMP/$name" 2>/dev/null | head -n "$n") || true
  [[ "$aot" == "$expect" ]] || { echo "FAIL [$name/aot]: expected '$expect' got '$aot'"; exit 1; }
  echo "PASS: $name"; }

# impl Drop with NO `trait Drop` declaration — used to error "unknown trait Drop".
# Two values drop in reverse declaration order at scope exit: prints 2 then 1.
diff_run noredecl $'2\n1' '
struct Noisy { id: i64 }
impl Drop for Noisy { fn drop(&mut self) ! { io } { print(self.id); } }
fn main() -> i64 ! { io } { let a = Noisy { id: 1 }; let b = Noisy { id: 2 }; 0 }'

# nested-scope drop ordering: inner drops before outer.
diff_run nested $'9\n1' '
struct N { id: i64 }
impl Drop for N { fn drop(&mut self) ! { io } { print(self.id); } }
fn main() -> i64 ! { io } { let outer = N { id: 1 }; { let inner = N { id: 9 }; } 0 }'

echo "ALL BUILTIN-DROP SMOKE TESTS PASSED"
