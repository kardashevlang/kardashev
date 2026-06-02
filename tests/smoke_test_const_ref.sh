#!/usr/bin/env bash
# v53 — `&CONST` promotion. A top-level scalar `const` is an inlined immediate
# with no stable address, so `&C` used to materialize a FRAME-LOCAL temporary:
# reading it in-scope worked, but RETURNING it (wrapped in a struct/tuple/enum)
# read freed stack — a dangling-reference UB orthogonal to (and missed by) the
# v52 escape analysis, which treated `&const` as a safe global. The fix promotes
# a borrowed scalar const to a stable internal global, so `&C` is a real
# 'static address: readable AND safely returnable. Aggregate consts are NOT
# promoted (their borrow is a temporary) so returning `&A` is rejected by the
# escape check; using it in-scope still works. `&<nullary-enum>` / other
# non-const `&ident` temporaries are likewise rejected when returned.
set -euo pipefail
KARDC=""
for c in "${TEST_SRCDIR:-}/_main/compiler/kardc" "${RUNFILES_DIR:-}/_main/compiler/kardc" "./compiler/kardc" "./build.local/kardc"; do
  [[ -n "$c" && -x "$c" ]] && { KARDC="$c"; break; }; done
[[ -z "$KARDC" ]] && { echo "FAIL: kardc not found"; exit 1; }
echo "Using kardc at: $KARDC"
TMP=$(mktemp -d); trap 'rm -rf "$TMP"' EXIT

runok() { local n="$1" want="$2" src="$3"; printf '%s' "$src" > "$TMP/$n.kd"
  if "$KARDC" --no-cache "$TMP/$n.kd" -o "$TMP/$n" >/dev/null 2>&1 && "$TMP/$n"; rc=$?; [[ "$rc" -eq "$want" ]]; then
    echo "PASS(run=$rc): $n"; else echo "FAIL[accept $n]: want exit $want got $rc"; "$KARDC" --no-cache "$TMP/$n.kd" 2>&1 | head -3; exit 1; fi; }
reject() { local n="$1" src="$2"; printf '%s' "$src" > "$TMP/$n.kd"
  local e; e=$("$KARDC" --no-cache "$TMP/$n.kd" -o "$TMP/$n" 2>&1 >/dev/null || true)
  echo "$e" | grep -qi "does not outlive this function" || { echo "FAIL[reject $n]: $e"; exit 1; }
  [[ -f "$TMP/$n" ]] && { echo "FAIL[reject $n]: COMPILED"; exit 1; }
  echo "PASS(reject): $n"; }

# ---- ACCEPT + RUN: scalar &CONST is a stable, readable, RETURNABLE reference ----
runok c_inscope 42 'const C: i64 = 42; fn main() -> i64 { let r = &C; *r }'
# the headline fix: return &C wrapped in a struct, deref after a clobbering call.
runok c_return  42 'const C: i64 = 42; struct R { p: &i64 } fn make() -> R { R { p: &C } } fn clobber(a: i64) -> i64 { let x = a * 7 + 999; x } fn main() -> i64 { let r = make(); let j = clobber(5); *r.p }'
runok c_arg     42 'const C: i64 = 42; fn id(r: &i64) -> i64 { *r } fn main() -> i64 { id(&C) }'
runok c_tuple   7  'const C: i64 = 7; fn mk() -> (&i64, i64) { (&C, 1) } fn main() -> i64 { let t = mk(); *t.0 }'
runok c_two     9  'const A: i64 = 4; const B: i64 = 5; fn main() -> i64 { let x = &A; let y = &B; *x + *y }'
runok c_agg_in  20 'const A: [i64; 3] = [10, 20, 30]; fn main() -> i64 { let r = &A; r[1] }'

# ---- REJECT: borrows that are genuinely frame-local temporaries can't escape ----
reject c_agg_ret 'struct P { a: i64 } const K: P = P { a: 5 }; struct R { p: &P } fn make() -> R { R { p: &K } } fn main() -> i64 { let r = make(); 0 }'
reject c_nil_ret 'enum E { Nil } struct R { p: &E } fn make() -> R { R { p: &Nil } } fn main() -> i64 { let r = make(); 0 }'

echo "ALL CONST-REF SMOKE TESTS PASSED"
