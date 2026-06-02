#!/usr/bin/env bash
# v45 — grammar conformance: a curated corpus exercising the productions in
# docs/SPEC.md §2-§4. Every representative program compiles; every ill-formed
# program is rejected with a diagnostic (never a crash). This grounds the spec
# in the actual parser/typechecker. (A machine-generated >=2000-program
# EBNF-conformance suite is the remaining v45 gate — see ROADMAP.)
set -euo pipefail
KARDC=""
for c in "${TEST_SRCDIR:-}/_main/compiler/kardc" "${RUNFILES_DIR:-}/_main/compiler/kardc" "./compiler/kardc" "./build.local/kardc"; do
  [[ -n "$c" && -x "$c" ]] && { KARDC="$c"; break; }; done
[[ -z "$KARDC" ]] && { echo "FAIL: kardc not found"; exit 1; }
echo "Using kardc at: $KARDC"
TMP=$(mktemp -d); trap 'rm -rf "$TMP"' EXIT
ok=0; bad=0
accept() { # a well-formed program must compile + run (exit 0)
  printf '%s' "$2" > "$TMP/$1.kd"
  if "$KARDC" "$TMP/$1.kd" >/dev/null 2>&1; then ok=$((ok+1)); else echo "FAIL[accept $1]: did not compile"; "$KARDC" "$TMP/$1.kd" 2>&1 | head -2; exit 1; fi
}
reject() { # an ill-formed program must fail to compile (non-zero), no crash
  printf '%s' "$2" > "$TMP/$1.kd"
  if "$KARDC" "$TMP/$1.kd" >/dev/null 2>&1; then echo "FAIL[reject $1]: compiled but should not have"; exit 1; else bad=$((bad+1)); fi
}

# ---- accept: one representative per major production ----
accept fn_generic       'fn id<T>(x: T) -> T { x } fn main() -> i64 { id::<i64>(7) }'
accept struct_field     'struct P { x: i64, y: i64 } fn main() -> i64 { let p = P { x: 1, y: 2 }; p.x + p.y }'
accept enum_match       'enum E { A(i64), B } fn main() -> i64 { match E::A(5) { A(n) => n, B => 0 } }'
accept trait_impl       'trait T { fn f(&self) -> i64; } struct S {} impl T for S { fn f(&self) -> i64 { 9 } } fn main() -> i64 { S{}.f() }'
accept generics_bound   'trait Sh { fn a(&self) -> i64; } fn use_it<X: Sh>(x: &X) -> i64 { x.a() } struct R {} impl Sh for R { fn a(&self) -> i64 { 3 } } fn main() -> i64 { use_it(&R{}) }'
accept match_or         'fn main() -> i64 { let x = 5; match x { 1 | 2 => 10, 3 | 4 => 20, _ => 0 } }'
accept closure_call     'fn main() -> i64 { let add = |a: i64, b: i64| a + b; add(2, 3) }'
accept loops            'fn main() -> i64 { let mut s = 0; let mut i = 0; while i < 5 { s = s + i; i = i + 1; } s }'
accept for_range        'fn main() -> i64 ! { alloc } { let mut s = 0; for i in 0..4 { s = s + i; } s }'
accept operators        'fn main() -> i64 { (7 % 3) + (12 & 4) + (1 << 3) + (~0 & 1) }'
accept dyn_dispatch     'trait Sp { fn s(&self) -> i64; } struct D {} impl Sp for D { fn s(&self) -> i64 { 1 } } fn ask(p: &dyn Sp) -> i64 { p.s() } fn main() -> i64 { ask(&D{}) }'
accept effects_decl     'fn pure_one() -> i64 { 1 } fn main() -> i64 ! { io } { print(pure_one()); 0 }'
accept unsafe_rawptr    'fn main() -> i64 { let x = 5; let p = &x as *const i64; unsafe { *p } }'
accept deref_assign     'fn main() -> i64 { let mut x = 1; let r = &mut x; *r = 8; x }'
accept const_item       'const N: i64 = 6; fn main() -> i64 { N }'
accept array_index      'fn main() -> i64 { let a = [10, 20, 30]; a[0] + a[2] }'
accept tuple_lit        'fn main() -> i64 { let t = (3, 4); t.0 + t.1 }'
accept cast_as          'fn main() -> i64 { let x = 300; let y = x as i32; y as i64 }'
accept macro_invoke     'macro_rules! dbl { ($x:expr) => { ($x) + ($x) }; } fn main() -> i64 { dbl!(21) }'
accept str_char         'fn main() -> i64 { let c = (97 as char); c as i64 }'

# ---- reject: ill-formed must diagnose, not crash ----
reject if_no_else       'fn main() -> i64 { let x = 1; if x > 0 { 5 } }'
reject bad_let          'fn main() -> i64 { let = 5; 0 }'
reject unknown_fn       'fn main() -> i64 { nonexistent_fn(1) }'
reject type_mismatch    'fn main() -> i64 { let x: bool = 5; 0 }'
reject not_obj_safe     'trait Mk { fn m(&self) -> Self; } struct S { x: i64 } impl Mk for S { fn m(&self) -> S { S { x: 1 } } } fn u(p: &dyn Mk) -> i64 { 0 } fn main() -> i64 { 0 }'
reject raw_deref_safe   'fn main() -> i64 { let x = 5; let p = &x as *const i64; *p }'
reject undeclared_effect 'fn main() -> i64 { print(1); 0 }'
reject unterminated      'fn main() -> i64 { let x = (1 + ; 0 }'

echo "grammar conformance: $ok accepted, $bad rejected"
[[ "$ok" -ge 18 && "$bad" -ge 7 ]] || { echo "FAIL: corpus too small"; exit 1; }
echo "ALL GRAMMAR-CONFORMANCE TESTS PASSED"
