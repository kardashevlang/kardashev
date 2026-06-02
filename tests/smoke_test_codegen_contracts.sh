#!/usr/bin/env bash
# v48 — codegen-quality contracts: `#[codegen(no_alloc)]` / `#[codegen(no_panic)]`
# are statically-verified guarantees about the EMITTED code, checked against the
# (transitively sound) effect set. A fn that promises `no_alloc` fails
# compilation if it — or anything it transitively calls — performs the `alloc`
# effect; `no_panic` likewise forbids the `panic` effect. This lets a hot path
# or a no_std/embedded fn promise it never touches the heap or the panic
# runtime, checked not hoped. (Mirrors the v47 `#[total]` pattern; the deferred
# 6/6 work is whole-program `no_std` builds + a vectorization contract.)
set -euo pipefail
KARDC=""
for c in "${TEST_SRCDIR:-}/_main/compiler/kardc" "${RUNFILES_DIR:-}/_main/compiler/kardc" "./compiler/kardc" "./build.local/kardc"; do
  [[ -n "$c" && -x "$c" ]] && { KARDC="$c"; break; }; done
[[ -z "$KARDC" ]] && { echo "FAIL: kardc not found"; exit 1; }
echo "Using kardc at: $KARDC"
TMP=$(mktemp -d); trap 'rm -rf "$TMP"' EXIT

# accept + RUN: compiles, and the emitted program returns the expected exit code.
run() { printf '%s' "$3" > "$TMP/$1.kd";
  if "$KARDC" "$TMP/$1.kd" -o "$TMP/$1.out" >/dev/null 2>&1 && "$TMP/$1.out"; rc=$?; [[ "$rc" -eq "$2" ]]; then
    echo "PASS(run=$rc): $1"; else echo "FAIL[run $1]: want exit $2 got $rc"; "$KARDC" "$TMP/$1.kd" 2>&1 | head -3; exit 1; fi; }
# accept (compiles): contract satisfied, no run needed.
ok() { printf '%s' "$2" > "$TMP/$1.kd";
  if "$KARDC" "$TMP/$1.kd" >/dev/null 2>&1; then echo "PASS(ok): $1"; else echo "FAIL[ok $1]"; "$KARDC" "$TMP/$1.kd" 2>&1 | head -3; exit 1; fi; }
# reject: contract violated → compile error whose text matches.
reject() { printf '%s' "$3" > "$TMP/$1.kd"; local e; e=$("$KARDC" "$TMP/$1.kd" 2>&1 >/dev/null || true);
  echo "$e" | grep -qi "$2" || { echo "FAIL[reject $1]: want '$2' got: $e"; exit 1; }; echo "PASS(reject): $1"; }

# ---- accept: pure / heap-free / panic-free fns honor their contracts ----
run  c_arith   11 '#[codegen(no_alloc)] fn hot(a: i64, b: i64) -> i64 { a * b + (a - b) } fn main() -> i64 { hot(3, 4) }'
run  c_compose 25 '#[codegen(no_alloc, no_panic)] fn sq(x: i64) -> i64 { x * x } #[codegen(no_alloc)] fn hyp(a: i64, b: i64) -> i64 { sq(a) + sq(b) } fn main() -> i64 { hyp(3, 4) }'
ok   c_nopanic    '#[codegen(no_panic)] fn safe(a: i64, b: i64) -> i64 { if b == 0 { 0 } else { a + b } } fn main() -> i64 { safe(1, 2) }'
ok   c_both       '#[codegen(no_alloc, no_panic)] fn pure(x: i64) -> i64 { if x > 0 { x } else { 0 - x } } fn main() -> i64 { pure(0 - 5) }'
run  c_noio      5 '#[codegen(no_io)] fn calc(a: i64, b: i64) -> i64 { a + b } fn main() -> i64 { calc(2, 3) }'
ok   c_triple      '#[codegen(no_alloc, no_panic, no_io)] fn kernel(x: i64) -> i64 { x * x + 1 } fn main() -> i64 { kernel(2) }'
# a no_panic fn whose callee panics, but inside `catch` (handled) — the panic
# effect is discharged at the handler, so the contract holds and it runs.
run  c_caught  42 'fn boom() -> i64 ! { panic } { panic("neg") } #[codegen(no_panic)] fn guarded() -> i64 { catch(boom, 42) } fn main() -> i64 { guarded() }'

# ---- reject: a contract that the fn (or a callee) violates ----
reject r_alloc_vec  'no_alloc' '#[codegen(no_alloc)] fn hot() -> i64 ! { alloc } { let mut v = vec_new(); vec_push(&mut v, 7); vec_len(&v) } fn main() -> i64 { hot() }'
reject r_alloc_trans 'no_alloc' 'fn helper() -> i64 ! { alloc } { let mut v = vec_new(); vec_push(&mut v, 1); vec_len(&v) } #[codegen(no_alloc)] fn caller() -> i64 ! { alloc } { helper() + 1 } fn main() -> i64 { caller() }'
reject r_panic_decl 'no_panic' '#[codegen(no_panic)] fn risky(x: i64) -> i64 ! { panic } { if x < 0 { panic("neg") } else { x } } fn main() -> i64 { risky(5) }'
reject r_panic_trans 'no_panic' 'fn boom(x: i64) -> i64 ! { panic } { if x < 0 { panic("neg") } else { x } } #[codegen(no_panic)] fn top(x: i64) -> i64 ! { panic } { boom(x) } fn main() -> i64 { top(2) }'
reject r_io_print   'no_io'    '#[codegen(no_io)] fn chatty(x: i64) -> i64 ! { io } { print(x); x } fn main() -> i64 ! { io } { chatty(7) }'
reject r_io_trans   'no_io'    'fn logit(x: i64) -> i64 ! { io } { print(x); x } #[codegen(no_io)] fn top(x: i64) -> i64 ! { io } { logit(x) } fn main() -> i64 ! { io } { top(7) }'

echo "ALL CODEGEN-CONTRACT SMOKE TESTS PASSED"
