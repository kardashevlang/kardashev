#!/usr/bin/env bash
# v50 — statically-verified exhaustive effect handling. A user-defined
# (algebraic) effect MUST be discharged by a `handle … with E { … }` before it
# reaches the entry point `main`; performing an effect with no installed handler
# is undefined (it silently no-ops / returns garbage at runtime). The effect
# system already propagates a user effect into the inferred effect set of every
# fn on the path to its perform site, and a `handle` removes it — so if `main`'s
# inferred set still contains a user effect, it escapes unhandled and is rejected,
# pinpointing the operation. Builtin effects (io/alloc/panic/…) reach `main`
# legitimately. Accept cases are JIT==AOT differential. (The mechanized
# soundness proof of this property is the deferred 6/6 work.)
set -euo pipefail
KARDC=""
for c in "${TEST_SRCDIR:-}/_main/compiler/kardc" "${RUNFILES_DIR:-}/_main/compiler/kardc" "./compiler/kardc" "./build.local/kardc"; do
  [[ -n "$c" && -x "$c" ]] && { KARDC="$c"; break; }; done
[[ -z "$KARDC" ]] && { echo "FAIL: kardc not found"; exit 1; }
echo "Using kardc at: $KARDC"
TMP=$(mktemp -d); trap 'rm -rf "$TMP"' EXIT

# accept + run, JIT==AOT (limited to the expected line count since the JIT
# appends the main return value as a trailing line).
diff_run() {
    local name="$1" expect="$2" src="$3"
    local n; n=$(printf '%s\n' "$expect" | wc -l)
    printf '%s' "$src" > "$TMP/$name.kd"
    local jit; jit=$("$KARDC" "$TMP/$name.kd" 2>/dev/null | head -n "$n") || true
    [[ "$jit" == "$expect" ]] || { echo "FAIL [$name/jit]: expected '$expect' got '$jit'"; exit 1; }
    "$KARDC" --no-cache -o "$TMP/$name" "$TMP/$name.kd" >/dev/null 2>&1
    local aot; aot=$("$TMP/$name" 2>/dev/null | head -n "$n") || true
    [[ "$aot" == "$expect" ]] || { echo "FAIL [$name/aot]: expected '$expect' got '$aot'"; exit 1; }
    echo "PASS: $name"
}
reject() {
    local name="$1" needle="$2" src="$3"
    printf '%s' "$src" > "$TMP/$name.kd"
    local err; err=$("$KARDC" "$TMP/$name.kd" 2>&1 >/dev/null || true)
    echo "$err" | grep -qi "$needle" || { echo "FAIL [$name]: want '$needle' got: $err"; exit 1; }
    echo "PASS (reject): $name"
}

EA='effect A { fn a() -> i64; }'
EB='effect B { fn b() -> i64; }'

# ---- accept: every performed user effect is discharged before main ----
# (each prints its result; main carries only the builtin `io` effect, which
# reaches main legitimately — the user effect A/B is discharged by the handle.)
diff_run handled_direct '7' "$EA fn main() -> i64 ! { io } { let r = handle { perform A::a() } with A { a() => 7 }; print(r); 0 }"
diff_run handled_callee '5' "$EA fn work() -> i64 ! { A } { perform A::a() } fn main() -> i64 ! { io } { print(handle { work() } with A { a() => 5 }); 0 }"
diff_run handled_nested '30' "$EA $EB fn work() -> i64 ! { A, B } { perform A::a() + perform B::b() } fn main() -> i64 ! { io } { print(handle { handle { work() } with A { a() => 10 } } with B { b() => 20 }); 0 }"
# a deep call chain whose effect is handled only at the very top — still total.
diff_run handled_deepchain '9' "$EA fn f0() -> i64 ! { A } { perform A::a() } fn f1() -> i64 ! { A } { f0() } fn f2() -> i64 ! { A } { f1() } fn f3() -> i64 ! { A } { f2() } fn main() -> i64 ! { io } { print(handle { f3() } with A { a() => 9 }); 0 }"
# builtin effects (io) legitimately reach main — must NOT be rejected.
diff_run builtin_io_ok '1' "fn main() -> i64 ! { io } { print(1); 0 }"

# ---- reject: a user effect escapes to main unhandled ----
reject escape_direct  'never handled' "$EA fn main() -> i64 ! { A } { perform A::a() }"
reject escape_callee  'never handled' "$EA fn work() -> i64 ! { A } { perform A::a() } fn main() -> i64 ! { A } { work() }"
reject escape_partial 'effect .B.'    "$EA $EB fn work() -> i64 ! { A, B } { perform A::a() + perform B::b() } fn main() -> i64 ! { B } { handle { work() } with A { a() => 1 } }"
reject escape_deepchain 'never handled' "$EA fn f0() -> i64 ! { A } { perform A::a() } fn f1() -> i64 ! { A } { f0() } fn f2() -> i64 ! { A } { f1() } fn main() -> i64 ! { A } { f2() }"

# ---- robustness: a deep nest of handlers, all discharged (no false reject),
#      and a deep chain with the outermost handler MISSING (must reject). ----
gen_nested_ok() { # N effects E1..EN, all handled by N nested handles
  local N="$1" decls="" perform="0" body="work()" i
  for i in $(seq 1 "$N"); do decls+="effect E$i { fn op() -> i64; } "; done
  for i in $(seq 1 "$N"); do perform+=" + perform E$i::op()"; done
  local row=""; for i in $(seq 1 "$N"); do row+="E$i"; [[ $i -lt $N ]] && row+=", "; done
  decls+="fn work() -> i64 ! { $row } { $perform } "
  for i in $(seq 1 "$N"); do body="handle { $body } with E$i { op() => $i }"; done
  printf '%s fn main() -> i64 { %s }' "$decls" "$body"
}
N=12; printf '%s' "$(gen_nested_ok $N)" > "$TMP/deep_ok.kd"
"$KARDC" "$TMP/deep_ok.kd" >/dev/null 2>&1 && echo "PASS: deep_nested_ok ($N effects all handled)" || { echo "FAIL: deep_nested_ok"; "$KARDC" "$TMP/deep_ok.kd" 2>&1 | head -3; exit 1; }

# same program but drop the outermost handler -> E$N escapes -> must reject.
gen_nested_bad() {
  local N="$1" decls="" perform="0" body="work()" i
  for i in $(seq 1 "$N"); do decls+="effect E$i { fn op() -> i64; } "; done
  for i in $(seq 1 "$N"); do perform+=" + perform E$i::op()"; done
  local row=""; for i in $(seq 1 "$N"); do row+="E$i"; [[ $i -lt $N ]] && row+=", "; done
  decls+="fn work() -> i64 ! { $row } { $perform } "
  for i in $(seq 1 $((N-1))); do body="handle { $body } with E$i { op() => $i }"; done  # E$N NOT handled
  printf '%s fn main() -> i64 ! { E%s } { %s }' "$decls" "$N" "$body"
}
printf '%s' "$(gen_nested_bad $N)" > "$TMP/deep_bad.kd"
deep_err=$("$KARDC" "$TMP/deep_bad.kd" 2>&1 >/dev/null || true)  # capture (kardc exits non-zero)
echo "$deep_err" | grep -qi "never handled" && echo "PASS: deep_nested_bad (outermost effect escapes -> rejected)" || { echo "FAIL: deep_nested_bad did not reject; got: $deep_err"; exit 1; }

echo "ALL EXHAUSTIVE-EFFECT SMOKE TESTS PASSED"
