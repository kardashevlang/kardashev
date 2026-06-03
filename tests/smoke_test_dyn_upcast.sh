#!/usr/bin/env bash
# v74 — single-level dyn trait upcasting: `&dyn Sub` / `Box<dyn Sub>` is usable
# where `&dyn Super` / `Box<dyn Super>` is expected, for a DIRECT supertrait.
# Implemented by embedding a pointer to each supertrait's vtable after the
# method slots of the subtrait vtable; the upcast loads that pointer and rebuilds
# the fat pointer (data preserved). Multi-level works by chaining single steps.
# Differential JIT==AOT.
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

# basic upcast: &dyn Pet -> &dyn Animal, super method dispatched correctly.
diff_run basic $'4' \
'trait Animal { fn legs(&self) -> i64; }
trait Pet: Animal { fn name_len(&self) -> i64; }
struct Dog {}
impl Animal for Dog { fn legs(&self) -> i64 { 4 } }
impl Pet for Dog { fn name_len(&self) -> i64 { 3 } }
fn count_legs(a: &dyn Animal) -> i64 { a.legs() }
fn via_pet(p: &dyn Pet) -> i64 { count_legs(p) }
fn main() -> i64 ! { io } { let d = Dog{}; print(via_pet(&d)); 0 }'

# data preserved: the upcast keeps the original object, so the concrete impl
# (Spider) is dispatched for BOTH the super and the sub method.
diff_run data_preserved $'14' \
'trait Animal { fn legs(&self) -> i64; }
trait Pet: Animal { fn name_len(&self) -> i64; }
struct Spider {}
impl Animal for Spider { fn legs(&self) -> i64 { 8 } }
impl Pet for Spider { fn name_len(&self) -> i64 { 6 } }
fn count_legs(a: &dyn Animal) -> i64 { a.legs() }
fn via_pet(p: &dyn Pet) -> i64 { count_legs(p) + p.name_len() }
fn main() -> i64 ! { io } { let s = Spider{}; print(via_pet(&s)); 0 }'

# Box<dyn Sub> -> Box<dyn Super>.
diff_run box_upcast $'4' \
'trait Animal { fn legs(&self) -> i64; }
trait Pet: Animal { fn name_len(&self) -> i64; }
struct Dog {}
impl Animal for Dog { fn legs(&self) -> i64 { 4 } }
impl Pet for Dog { fn name_len(&self) -> i64 { 3 } }
fn count_legs(a: Box<dyn Animal>) -> i64 { a.legs() }
fn main() -> i64 ! { io } { let p: Box<dyn Pet> = Box::new(Dog{}); print(count_legs(p)); 0 }'

# multi-level by chaining single steps: Cee -> Bee -> Aee.
diff_run two_step $'1' \
'trait Aee { fn a(&self) -> i64; }
trait Bee: Aee { fn b(&self) -> i64; }
trait Cee: Bee { fn c(&self) -> i64; }
struct Widget {}
impl Aee for Widget { fn a(&self) -> i64 { 1 } }
impl Bee for Widget { fn b(&self) -> i64 { 2 } }
impl Cee for Widget { fn c(&self) -> i64 { 3 } }
fn use_a(x: &dyn Aee) -> i64 { x.a() }
fn use_b(x: &dyn Bee) -> i64 { use_a(x) }
fn use_c(x: &dyn Cee) -> i64 { use_b(x) }
fn main() -> i64 ! { io } { let t = Widget{}; print(use_c(&t)); 0 }'

# regression: plain &dyn dispatch (no upcast) still works.
diff_run plain_dyn $'7' \
'trait Animal { fn legs(&self) -> i64; }
struct Bug {}
impl Animal for Bug { fn legs(&self) -> i64 { 7 } }
fn count_legs(a: &dyn Animal) -> i64 { a.legs() }
fn main() -> i64 ! { io } { let b = Bug{}; print(count_legs(&b)); 0 }'

# single-level limit: a direct grandparent upcast in ONE step is rejected
# (chain through the intermediate supertrait instead) — no miscompile.
reject grandparent_rejected 'expected &dyn Aee' \
'trait Aee { fn a(&self) -> i64; }
trait Bee: Aee { fn b(&self) -> i64; }
trait Cee: Bee { fn c(&self) -> i64; }
struct Widget {}
impl Aee for Widget { fn a(&self) -> i64 { 1 } }
impl Bee for Widget { fn b(&self) -> i64 { 2 } }
impl Cee for Widget { fn c(&self) -> i64 { 3 } }
fn use_a(x: &dyn Aee) -> i64 { x.a() }
fn use_c(x: &dyn Cee) -> i64 { use_a(x) }
fn main() -> i64 ! { io } { let t = Widget{}; print(use_c(&t)); 0 }'

echo "ALL DYN-UPCAST SMOKE TESTS PASSED"
