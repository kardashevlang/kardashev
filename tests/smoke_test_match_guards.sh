#!/usr/bin/env bash
# v68 — match guards `pat if cond =>`. VERIFIED missing before this version
# (parse-rejected) despite the project record claiming v26/Phase141 shipped them.
# A guarded arm matches its pattern AND the guard; on guard-false control falls
# through to the next arm (not the wildcard) via a per-arm suffix decision tree.
# A guarded arm does NOT count toward exhaustiveness. Differential JIT==AOT.
set -uo pipefail
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
  [[ "$jit" == "$expect" ]] || { echo "FAIL [$name/jit]: expected '$expect' got '$jit'"; "$KARDC" "$TMP/$name.kd" 2>&1|head -4; exit 1; }
  "$KARDC" --no-cache -o "$TMP/$name" "$TMP/$name.kd" >/dev/null 2>&1
  local aot; aot=$("$TMP/$name" 2>/dev/null | head -n "$n") || true
  [[ "$aot" == "$expect" ]] || { echo "FAIL [$name/aot]: expected '$expect' got '$aot'"; exit 1; }
  echo "PASS: $name"; }
reject() { local name="$1" needle="$2" src="$3"; printf '%s' "$src" > "$TMP/$name.kd"
  local e; e=$("$KARDC" "$TMP/$name.kd" 2>&1 >/dev/null || true)
  echo "$e" | grep -qi "$needle" || { echo "FAIL[reject $name]: want '$needle' got: $e"; exit 1; }
  echo "PASS(reject): $name"; }

# enum payload guard: fall-through to the next Some arm, then None.
diff_run payload $'1\n2\n3' 'fn cl(o: Option<i64>) -> i64 { match o { Some(n) if n > 5 => 1, Some(n) => 2, None => 3 } }
fn main() -> i64 ! { io } { print(cl(Some(7))); print(cl(Some(3))); print(cl(None)); 0 }'

# bare scrutinee guard + wildcard fall-through.
diff_run bare $'1\n0' 'fn f(n: i64) -> i64 { match n { x if x > 5 => 1, _ => 0 } }
fn main() -> i64 ! { io } { print(f(7)); print(f(2)); 0 }'

# chained guards (three guarded arms): suffix-tree chaining.
diff_run chained $'10\n20\n30\n40' 'fn f(n: i64) -> i64 { match n { x if x > 100 => 10, x if x > 50 => 20, x if x > 10 => 30, _ => 40 } }
fn main() -> i64 ! { io } { print(f(200)); print(f(60)); print(f(20)); print(f(5)); 0 }'

# the guard sees the pattern bindings, and side of an && in the guard.
diff_run binding $'1\n0' 'enum P { Pt(i64, i64) }
fn f(p: P) -> i64 { match p { Pt(x, y) if x > 0 && y > 0 => 1, _ => 0 } }
fn main() -> i64 ! { io } { print(f(P::Pt(3, 4))); print(f(P::Pt(3, 0))); 0 }'

# by-reference guarded match binds borrows (allowed even with non-Copy payload).
diff_run byref $'1\n0' 'fn f(o: &Option<i64>) -> i64 { match o { Some(n) if *n > 5 => 1, _ => 0 } }
fn main() -> i64 ! { io } { let a = Some(7); let b = Some(2); print(f(&a)); print(f(&b)); 0 }'

# --- rejects ---
reject nonexh        'non-exhaustive'  'fn f(n: i64) -> i64 { match n { x if x > 5 => 1 } } fn main() -> i64 { f(1) }'
reject nonbool_guard 'guard'           'fn f(n: i64) -> i64 { match n { x if x => 1, _ => 0 } } fn main() -> i64 { f(1) }'
reject noncopy_byval 'non-Copy'        'fn f(o: Option<String>) -> i64 ! { alloc } { match o { Some(s) if str_len(&s) > 0 => 1, _ => 0 } } fn main() -> i64 { 0 }'

echo "ALL MATCH-GUARD SMOKE TESTS PASSED"
