#!/usr/bin/env bash
# v57 — functions may RETURN references, gated by escape analysis. kardashev used
# to blanket-reject every `-> &T` user function ("cannot return a reference, no
# lifetime system yet"), a rule that predated the v52–v54 escape analysis. That
# analysis now precisely decides soundness: a returned reference rooted in a
# by-reference parameter, `&self`, or a global outlives the call and is ACCEPTED;
# one rooted in a local / by-value parameter / temporary is REJECTED as a
# dangling reference. This unblocks accessor / `&self.field` methods (and is the
# prerequisite for Index/Deref operator overloading, the documented follow-on).
set -euo pipefail
KARDC=""
for c in "${TEST_SRCDIR:-}/_main/compiler/kardc" "${RUNFILES_DIR:-}/_main/compiler/kardc" "./compiler/kardc" "./build.local/kardc"; do
  [[ -n "$c" && -x "$c" ]] && { KARDC="$c"; break; }; done
[[ -z "$KARDC" ]] && { echo "FAIL: kardc not found"; exit 1; }
echo "Using kardc at: $KARDC"
TMP=$(mktemp -d); trap 'rm -rf "$TMP"' EXIT

runok() { local n="$1" want="$2" src="$3"; printf '%s' "$src" > "$TMP/$n.kd"
  if "$KARDC" --no-cache "$TMP/$n.kd" -o "$TMP/$n" >/dev/null 2>&1 && "$TMP/$n"; rc=$?; [[ "$rc" -eq "$want" ]]; then
    echo "PASS(run=$rc): $n"; else echo "FAIL[accept $n]: want $want got $rc"; "$KARDC" --no-cache "$TMP/$n.kd" 2>&1|head -3; exit 1; fi; }
reject() { local n="$1" src="$2"; printf '%s' "$src" > "$TMP/$n.kd"
  local e; e=$("$KARDC" --no-cache "$TMP/$n.kd" -o "$TMP/$n" 2>&1 >/dev/null || true)
  echo "$e" | grep -qi "outlive this function" || { echo "FAIL[reject $n]: $e"; exit 1; }
  [[ -f "$TMP/$n" ]] && { echo "FAIL[reject $n]: COMPILED (dangling ref)"; exit 1; }
  echo "PASS(reject): $n"; }

# ---- ACCEPT: a returned reference that outlives the call ----
runok r_param   5 'fn id(r: &i64) -> &i64 { r } fn main() -> i64 { let x = 5; *id(&x) }'
runok r_selffld 7 'struct P { x: i64 } impl P { fn getx(&self) -> &i64 { &self.x } } fn main() -> i64 { let p = P { x: 7 }; *p.getx() }'
runok r_pfield  3 'struct Pair { lo: i64, hi: i64 } fn lo(p: &Pair) -> &i64 { &p.lo } fn main() -> i64 { let q = Pair { lo: 3, hi: 8 }; *lo(&q) }'
runok r_pickmin 4 'fn the(r: &i64) -> &i64 { r } fn main() -> i64 { let a = 4; let b = 9; let m = the(&a); *m }'
runok r_chain   6 'fn id(r: &i64) -> &i64 { r } fn id2(r: &i64) -> &i64 { id(r) } fn main() -> i64 { let x = 6; *id2(&x) }'
runok r_mutself 8 'struct C { v: i64 } impl C { fn vref(&self) -> &i64 { &self.v } } fn main() -> i64 { let c = C { v: 8 }; let r = c.vref(); *r }'

# ---- REJECT: a returned reference into the dying frame ----
reject r_local  'fn f() -> &i64 { let x = 5; &x } fn main() -> i64 { *f() }'
reject r_temp   'fn f() -> &i64 { &5 } fn main() -> i64 { *f() }'
reject r_byval  'fn f(v: i64) -> &i64 { &v } fn main() -> i64 { *f(3) }'
reject r_locfld 'struct P { x: i64 } fn f() -> &i64 { let p = P { x: 1 }; &p.x } fn main() -> i64 { *f() }'

echo "ALL REF-RETURN SMOKE TESTS PASSED"
