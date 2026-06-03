#!/usr/bin/env bash
# v73 — associated constants, completed. `Type::CONST()` (a no-self desugared
# method) already worked; v73 adds the Rust-style spellings: bare `Type::CONST`
# (no parens) and `Self::CONST` / `Self::CONST()` / `Self::method()` resolved
# through the concrete implementing type. Differential JIT==AOT.
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

# bare Type::CONST as a value (no parens — the Rust spelling).
diff_run bare_type $'42' \
'trait B { const MAX: i64; }
struct G {}
impl B for G { const MAX: i64 = 42; }
fn main() -> i64 ! { io } { print(G::MAX); 0 }'

# Self::CONST() and bare Self::CONST inside a method, clamping logic.
diff_run self_const $'42\n7' \
'trait B { const MAX: i64; fn cap(&self) -> i64; }
struct G { v: i64 }
impl B for G { const MAX: i64 = 42;
  fn cap(&self) -> i64 { if self.v > Self::MAX { Self::MAX() } else { self.v } } }
fn main() -> i64 ! { io } { print(G{v:100}.cap()); print(G{v:7}.cap()); 0 }'

# Self::method() — a sibling associated (no-self) function via Self.
diff_run self_method $'15' \
'struct G { v: i64 }
impl G { fn base() -> i64 { 10 } fn val(&self) -> i64 { Self::base() + self.v } }
fn main() -> i64 ! { io } { print(G{v:5}.val()); 0 }'

# typed assoc consts: bool + f64, accessed bare.
diff_run typed_const $'1\n2' \
'trait B { const ON: bool; const R: f64; }
struct G {}
impl B for G { const ON: bool = true; const R: f64 = 2.5; }
fn main() -> i64 ! { io } {
  if G::ON { print(1); } else { print(0); }
  print(G::R as i64); 0 }'

# regression: bare qualified enum variant + Type::CONST() still work.
diff_run regressions $'1\n42' \
'enum Color { Red, Green }
trait B { const MAX: i64; }
struct G {}
impl B for G { const MAX: i64 = 42; }
fn main() -> i64 ! { io } {
  let c = Color::Red; match c { Red => print(1), Green => print(2) }
  print(G::MAX()); 0 }'

echo "ALL ASSOC-CONST SMOKE TESTS PASSED"
