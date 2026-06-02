#!/usr/bin/env bash
# v54 — store-escape: the v0.52 escape analysis guarded function RETURNS; this
# closes the companion hole it documented — STORING a frame-local reference into
# a place that OUTLIVES the call (a `&mut` out-parameter's field, or a global).
# `fn leak(out: &mut R) { let x = 7; out.p = &x; }` used to compile, and after
# the call `out.p` dangled into the freed frame. The borrow checker now rejects a
# store whose target roots in a reference parameter (or a global) when the stored
# value contains a reference rooted in a local / by-value parameter / temporary.
# A store into a LOCAL place (which dies with the frame) is still fine.
set -euo pipefail
KARDC=""
for c in "${TEST_SRCDIR:-}/_main/compiler/kardc" "${RUNFILES_DIR:-}/_main/compiler/kardc" "./compiler/kardc" "./build.local/kardc"; do
  [[ -n "$c" && -x "$c" ]] && { KARDC="$c"; break; }; done
[[ -z "$KARDC" ]] && { echo "FAIL: kardc not found"; exit 1; }
echo "Using kardc at: $KARDC"
TMP=$(mktemp -d); trap 'rm -rf "$TMP"' EXIT

reject() { local n="$1" src="$2"; printf '%s' "$src" > "$TMP/$n.kd"
  local e; e=$("$KARDC" --no-cache "$TMP/$n.kd" -o "$TMP/$n" 2>&1 >/dev/null || true)
  echo "$e" | grep -qi "outlives this function" || { echo "FAIL[reject $n]: $e"; exit 1; }
  [[ -f "$TMP/$n" ]] && { echo "FAIL[reject $n]: COMPILED (store-escape not caught)"; exit 1; }
  echo "PASS(reject): $n"; }
runok() { local n="$1" want="$2" src="$3"; printf '%s' "$src" > "$TMP/$n.kd"
  if "$KARDC" --no-cache "$TMP/$n.kd" -o "$TMP/$n" >/dev/null 2>&1 && "$TMP/$n"; rc=$?; [[ "$rc" -eq "$want" ]]; then
    echo "PASS(run=$rc): $n"; else echo "FAIL[accept $n]: want $want got $rc"; "$KARDC" --no-cache "$TMP/$n.kd" 2>&1|head -3; exit 1; fi; }
ok() { local n="$1" src="$2"; printf '%s' "$src" > "$TMP/$n.kd"
  "$KARDC" --no-cache "$TMP/$n.kd" -o "$TMP/$n" >/dev/null 2>&1 && echo "PASS(ok): $n" || { echo "FAIL[ok $n]"; "$KARDC" --no-cache "$TMP/$n.kd" 2>&1|head -3; exit 1; }; }

# ---- REJECT: a frame-local reference stored into longer-lived storage ----
reject s_local   'struct R { p: &i64 } fn leak(out: &mut R) { let x = 7; out.p = &x; } fn main() -> i64 { 0 }'
reject s_temp    'struct R { p: &i64 } fn leak(out: &mut R) { out.p = &5; } fn main() -> i64 { 0 }'
reject s_byval   'struct R { p: &i64 } fn leak(out: &mut R, v: i64) { out.p = &v; } fn main() -> i64 { 0 }'
reject s_field   'struct Inner { a: i64 } struct R { p: &i64 } fn leak(out: &mut R) { let s = Inner { a: 9 }; out.p = &s.a; } fn main() -> i64 { 0 }'
reject s_self    'struct R { p: &i64 } impl R { fn leak(&mut self) { let x = 1; self.p = &x; } } fn main() -> i64 { 0 }'
reject s_nested  'struct Inner { p: &i64 } struct R { i: Inner } fn leak(out: &mut R) { let x = 1; out.i = Inner { p: &x }; } fn main() -> i64 { 0 }'

# ---- ACCEPT: stores that cannot dangle ----
ok a_param   'struct R { p: &i64 } fn ok(out: &mut R, p: &i64) { out.p = p; } fn main() -> i64 { 0 }'
ok a_nonref  'struct R { v: i64 } fn ok(out: &mut R) { let x = 7; out.v = x; } fn main() -> i64 { 0 }'
ok a_pfield  'struct Q { lo: i64 } struct R { p: &i64 } fn ok(out: &mut R, q: &Q) { out.p = &q.lo; } fn main() -> i64 { 0 }'
# store &local into a LOCAL place — both die together, so it is sound and runs.
runok a_localtgt 9 'struct R { p: &i64 } fn f() -> i64 { let x = 7; let mut s = R { p: &x }; let y = 9; s.p = &y; *s.p } fn main() -> i64 { f() }'

echo "ALL FIELD-REF-ESCAPE SMOKE TESTS PASSED"
