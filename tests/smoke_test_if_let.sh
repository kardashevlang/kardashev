#!/usr/bin/env bash
# v58 — `if let` / `while let` pattern-binding ergonomics, desugared at parse time
# to the existing `match` lowering (no new typecheck/codegen):
#   if let PAT = e { A } else { B }   ->  match e { PAT => A, _ => B }
#   while let PAT = e { BODY }        ->  loop { match e { PAT => BODY, _ => break } }
# (let-else is deferred — it needs a never-type / divergence-typing pass first.)
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
  [[ "$jit" == "$expect" ]] || { echo "FAIL [$name/jit]: expected '$expect' got '$jit'"; "$KARDC" "$TMP/$name.kd" 2>&1 | head -3; exit 1; }
  "$KARDC" --no-cache -o "$TMP/$name" "$TMP/$name.kd" >/dev/null 2>&1
  local aot; aot=$("$TMP/$name" 2>/dev/null | head -n "$n") || true
  [[ "$aot" == "$expect" ]] || { echo "FAIL [$name/aot]: expected '$expect' got '$aot'"; exit 1; }
  echo "PASS: $name"; }

DRAIN='fn take(n: &mut i64) -> Option<i64> { if *n > 0 { let v = *n; *n = *n - 1; Some(v) } else { None } }'

# if let — match binds + runs the then-arm
diff_run iflet_some '5' 'fn main() -> i64 ! { io } { let o = Some(5); if let Some(x) = o { print(x); } else { print(0-1); } 0 }'
# if let — no match runs the else-arm
diff_run iflet_none '-1' 'fn main() -> i64 ! { io } { let o: Option<i64> = None; if let Some(x) = o { print(x); } else { print(0-1); } 0 }'
# if let — no else (unit) is fine; prints only on match
diff_run iflet_noelse '7' 'fn main() -> i64 ! { io } { if let Some(x) = Some(7) { print(x); } 0 }'
# if let — the then-arm can use the binding in an expression
diff_run iflet_use '20' 'fn main() -> i64 ! { io } { let o = Some(10); if let Some(x) = o { print(x + x); } else { print(0); } 0 }'

# while let — drains until the pattern stops binding
diff_run whilelet_drain $'3\n2\n1' "$DRAIN"' fn main() -> i64 ! { io } { let mut n = 3; while let Some(x) = take(&mut n) { print(x); } 0 }'
# while let — empty from the start runs zero iterations
diff_run whilelet_empty '42' "$DRAIN"' fn main() -> i64 ! { io } { let mut n = 0; while let Some(x) = take(&mut n) { print(x); } print(42); 0 }'
# while let — accumulate across iterations (mutation outside survives)
diff_run whilelet_sum '6' "$DRAIN"' fn main() -> i64 ! { io } { let mut n = 3; let mut s = 0; while let Some(x) = take(&mut n) { s = s + x; } print(s); 0 }'

echo "ALL IF-LET / WHILE-LET SMOKE TESTS PASSED"
