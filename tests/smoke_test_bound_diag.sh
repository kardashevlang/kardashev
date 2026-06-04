#!/usr/bin/env bash
# Roadmap v110 (ARC D, part A, FINAL) — trait-BOUND-satisfaction diagnostics. When
# method resolution fails for a concrete type (a missing trait `impl`), the typechecker
# now emits a clear, actionable E0277: it names the trait the method comes from (the
# bound `Type: Trait` is not satisfied), suggests the missing `impl Trait for Type`,
# and lists the types that DO provide the method — instead of a bare "no impl for type".
#
# Proves: (A) a direct call to a missing trait method names the bound + the fix, with a
# correct caret on the call; (B) #[derive(Eq)] over a non-Eq field names the bound; (C)
# the diagnostic still carries the E0277 code (classification preserved); (D) the help
# lists real candidate types. Deterministic.
#
# DEFERRALS (honest): the bound check for a generic CALL site whose type param is bound
# to a concrete type without the impl still surfaces at codegen (not typecheck) — a
# deeper monomorphization-time check; the #[derive] case's caret still points into the
# synthesized prelude region (the message names the type correctly — fixing the span
# needs derive-site provenance threading).
set -uo pipefail
KARDC=""
for candidate in \
    "${TEST_SRCDIR:-}/_main/compiler/kardc" "${TEST_SRCDIR:-}/kardashev/compiler/kardc" \
    "${RUNFILES_DIR:-}/_main/compiler/kardc" "${RUNFILES_DIR:-}/kardashev/compiler/kardc" \
    "./compiler/kardc" "./build.local/kardc"; do
    if [[ -n "$candidate" && -x "$candidate" ]]; then KARDC="$candidate"; break; fi
done
[[ -z "$KARDC" ]] && { echo "FAIL: kardc binary not found"; exit 1; }
echo "Using kardc at: $KARDC"
TMP=$(mktemp -d); trap 'rm -rf "$TMP"' EXIT

# --- A: a direct call to a missing trait method ---
cat > "$TMP/a.kd" <<'EOF'
trait Show { fn show(&self) -> i64; }
struct Widget { v: i64 }
fn main() -> i64 { let w = Widget { v: 5 }; w.show() }
EOF
out=$("$KARDC" --no-cache "$TMP/a.kd" 2>&1); rc=$?
[[ "$rc" -ne 0 ]] || { echo "FAIL [A]: program with a missing impl compiled (want a type error)"; exit 1; }
grep -q 'E0277' <<<"$out" || { echo "FAIL [A]: no E0277 code"; echo "$out"; exit 1; }
grep -q 'the trait bound `Widget: Show` is not satisfied' <<<"$out" || { echo "FAIL [A]: bound not named"; echo "$out"; exit 1; }
grep -q 'add `impl Show for Widget`' <<<"$out" || { echo "FAIL [A]: no impl suggestion"; echo "$out"; exit 1; }
grep -q 'p.kd:3\|a.kd:3' <<<"$out" || { echo "FAIL [A]: caret not on the call line (3)"; echo "$out"; exit 1; }
echo "PASS [A]: direct missing-method call -> E0277 names the bound + suggests the impl + correct caret"

# --- B: #[derive(Eq)] over a field whose type has no Eq ---
cat > "$TMP/b.kd" <<'EOF'
struct Inner { v: i64 }
#[derive(Eq)]
struct Outer { a: i64, b: Inner }
fn main() -> i64 { 0 }
EOF
out=$("$KARDC" --no-cache "$TMP/b.kd" 2>&1); rc=$?
[[ "$rc" -ne 0 ]] || { echo "FAIL [B]: derive(Eq) over a non-Eq field compiled"; exit 1; }
grep -q 'the trait bound `Inner: Eq` is not satisfied' <<<"$out" || { echo "FAIL [B]: bound `Inner: Eq` not named"; echo "$out"; exit 1; }
grep -q 'add `impl Eq for Inner`' <<<"$out" || { echo "FAIL [B]: no impl suggestion for Inner"; echo "$out"; exit 1; }
echo 'PASS [B]: derive(Eq) over a non-Eq field -> names the `Inner: Eq` bound + the fix'

# --- C: the help lists real candidate types that DO provide the method ---
grep -q 'types with an `eq` impl:.*i64' <<<"$out" || { echo "FAIL [C]: help does not list candidate types with eq (e.g. i64)"; echo "$out"; exit 1; }
echo "PASS [C]: the diagnostic lists candidate types that provide the method (actionable)"

# --- D: a SATISFIED bound still compiles (no false positive) ---
cat > "$TMP/ok.kd" <<'EOF'
trait Show { fn show(&self) -> i64; }
struct Widget { v: i64 }
impl Show for Widget { fn show(&self) -> i64 { self.v } }
fn main() -> i64 { let w = Widget { v: 7 }; w.show() }
EOF
"$KARDC" --no-cache -o "$TMP/ok" "$TMP/ok.kd" >/dev/null 2>&1 || { echo "FAIL [D]: a program WITH the impl failed to build"; "$KARDC" "$TMP/ok.kd" 2>&1 | head -3; exit 1; }
r=$("$TMP/ok"; echo $?)
[[ "$r" == "7" ]] || { echo "FAIL [D]: satisfied-bound program ran wrong ($r, want 7)"; exit 1; }
echo "PASS [D]: a satisfied bound compiles + runs (no false positive)"

echo "ALL v110 BOUND-DIAGNOSTIC SMOKE TESTS PASSED"
