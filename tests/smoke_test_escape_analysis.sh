#!/usr/bin/env bash
# v52 — escape analysis closes a dangling-reference UB: a function that returns
# an AGGREGATE containing a reference to a frame-local value (a struct/tuple/enum
# field holding `&local`/`&mut local`/`&temp`) used to compile clean and read
# freed memory (top-level `-> &T` was already rejected by the typechecker; the
# hole was references WRAPPED in a returned value). The borrow checker now
# rejects any returned value whose contained references root in a local, a
# by-value parameter, or a temporary — while still ACCEPTING references rooted in
# a by-reference parameter or a global (those outlive the call). Sound and
# conservative; no lifetime system. Reject + accept cases below; accept cases
# also run (JIT) to prove they still execute correctly.
set -euo pipefail
KARDC=""
for c in "${TEST_SRCDIR:-}/_main/compiler/kardc" "${RUNFILES_DIR:-}/_main/compiler/kardc" "./compiler/kardc" "./build.local/kardc"; do
  [[ -n "$c" && -x "$c" ]] && { KARDC="$c"; break; }; done
[[ -z "$KARDC" ]] && { echo "FAIL: kardc not found"; exit 1; }
echo "Using kardc at: $KARDC"
TMP=$(mktemp -d); trap 'rm -rf "$TMP"' EXIT

# reject: must fail to compile, naming the escaping reference.
reject() { local n="$1" src="$2"; printf '%s' "$src" > "$TMP/$n.kd"
  local e; e=$("$KARDC" --no-cache "$TMP/$n.kd" -o "$TMP/$n" 2>&1 >/dev/null || true)
  echo "$e" | grep -qi "does not outlive this function" \
    || { echo "FAIL[reject $n]: expected escape error, got: $e"; exit 1; }
  [[ -f "$TMP/$n" ]] && { echo "FAIL[reject $n]: it COMPILED (UB not closed)"; exit 1; }
  echo "PASS(reject): $n"; }
# accept-and-run: must compile AND produce the expected exit code.
runok() { local n="$1" want="$2" src="$3"; printf '%s' "$src" > "$TMP/$n.kd"
  if "$KARDC" --no-cache "$TMP/$n.kd" -o "$TMP/$n" >/dev/null 2>&1 && "$TMP/$n"; rc=$?; [[ "$rc" -eq "$want" ]]; then
    echo "PASS(run=$rc): $n"; else echo "FAIL[accept $n]: want exit $want got $rc"; "$KARDC" --no-cache "$TMP/$n.kd" 2>&1 | head -3; exit 1; fi; }

# ---- REJECT: an aggregate carrying a frame-local reference escapes ----
reject r_struct  'struct R { p: &i64 } fn f() -> R { let x = 7; R { p: &x } } fn main() -> i64 { let r = f(); *r.p }'
reject r_tuple   'fn f() -> (&i64, i64) { let x = 7; (&x, 1) } fn main() -> i64 { let t = f(); *t.0 }'
reject r_enum    'enum E { V(&i64) } fn f() -> E { let x = 7; V(&x) } fn main() -> i64 { match f() { V(p) => *p } }'
reject r_mut     'struct R { p: &mut i64 } fn f() -> R { let mut x = 7; R { p: &mut x } } fn main() -> i64 { let r = f(); *r.p }'
reject r_field   'struct Inner { a: i64 } struct R { p: &i64 } fn f() -> R { let s = Inner { a: 9 }; R { p: &s.a } } fn main() -> i64 { let r = f(); *r.p }'
reject r_nested  'struct Inner { p: &i64 } struct Outer { i: Inner } fn f() -> Outer { let x = 1; Outer { i: Inner { p: &x } } } fn main() -> i64 { let o = f(); *o.i.p }'
reject r_temp    'struct R { p: &i64 } fn f() -> R { R { p: &5 } } fn main() -> i64 { let r = f(); *r.p }'
reject r_mixed   'struct R { a: &i64, b: &i64 } fn f(g: &i64) -> R { let x = 1; R { a: g, b: &x } } fn main() -> i64 { let y = 2; let r = f(&y); *r.b }'
reject r_early   'struct R { p: &i64 } fn f(c: bool) -> R { let x = 1; if c { return R { p: &x }; } else {} R { p: &x } } fn main() -> i64 { let r = f(true); *r.p }'

# ---- ACCEPT: references rooted in a ref-parameter or global, or no ref at all ----
runok a_param   5 'struct R { p: &i64 } fn wrap(r: &i64) -> R { R { p: r } } fn main() -> i64 { let x = 5; let w = wrap(&x); *w.p }'
runok a_pfield  3 'struct Pair { lo: i64, hi: i64 } struct R { p: &i64 } fn lo(r: &Pair) -> R { R { p: &r.lo } } fn main() -> i64 { let q = Pair { lo: 3, hi: 8 }; let w = lo(&q); *w.p }'
runok a_ptuple  6 'fn mk(r: &i64) -> (&i64, i64) { (r, 1) } fn main() -> i64 { let x = 6; let t = mk(&x); *t.0 }'
runok a_penum   7 'enum E { V(&i64) } fn mk(r: &i64) -> E { V(r) } fn main() -> i64 { let x = 7; match mk(&x) { V(p) => *p } }'
runok a_noref   9 'struct R { v: i64 } fn f() -> R { let x = 9; R { v: x } } fn main() -> i64 { let r = f(); r.v }'
runok a_nestfld 8 'struct Inner { a: i64 } struct Pair { i: Inner } struct R { p: &i64 } fn f(q: &Pair) -> R { R { p: &q.i.a } } fn main() -> i64 { let pr = Pair { i: Inner { a: 8 } }; let w = f(&pr); *w.p }'

echo "ALL ESCAPE-ANALYSIS SMOKE TESTS PASSED"
