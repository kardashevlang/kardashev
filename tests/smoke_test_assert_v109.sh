#!/usr/bin/env bash
# Roadmap v109 (ARC D, part A) — the `expect!`/`expect_eq!`/`expect_ne!` PANIC-form
# assertion macros. assert!/assert_eq!/assert_ne! already exist as the effect-free
# `test_*() -> i64` convention (a failed assert `return`s 1 so the runner reports
# FAILED without aborting). v109 ADDS the Rust-semantics panic form, usable ANYWHERE:
# on failure it `panic(format!("...{:?}...{:?}", left, right))` — aborting with exit
# 101 and a Debug-formatted message — built additively on the existing panic + format!
# + Eq + Debug, so the test-convention asserts are untouched (no regression).
#
# This gate proves: (A) the existing return-1 asserts STILL work (regression guard);
# (B) expect_eq! aborts 101 in JIT with the Debug values on stderr; (C) same in AOT;
# (D) a passing expect is a no-op; (E) it generalizes beyond i64 (String via `.eq` +
# `{:?}`). Deterministic; exit codes mod 256.
#
# DEFERRALS: the effect-free return-1 reporter stays i64-only (use expect_* or
# assert!(a.eq(&b)) for non-i64 in tests); a panic-CATCHING test runner (so the panic
# form could be the test default) is future work.
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

# --- Subtest A: the existing effect-free return-1 asserts STILL work (regression) ---
cat > "$TMP/t.kd" <<'EOF'
fn test_ok() -> i64 { assert_eq!(2 + 2, 4); assert!(true); assert_ne!(1, 2); 0 }
fn test_bad() -> i64 { assert_eq!(1, 2); 0 }
EOF
out=$("$KARDC" --no-cache --test "$TMP/t.kd" 2>&1); rc=$?
grep -q 'test test_ok ... ok' <<<"$out" || { echo "FAIL [A/return-1]: test_ok not ok"; echo "$out"; exit 1; }
grep -q 'test test_bad ... FAILED' <<<"$out" || { echo "FAIL [A/return-1]: test_bad not FAILED"; echo "$out"; exit 1; }
[[ "$rc" -ne 0 ]] || { echo "FAIL [A/return-1]: --test exit 0 despite a failing test"; exit 1; }
echo "PASS [A]: existing return-1 assert_eq!/assert!/assert_ne! convention intact (no regression)"

# --- Subtest B: expect_eq! aborts (exit 101) in JIT with the Debug values on stderr ---
cat > "$TMP/p.kd" <<'EOF'
fn main() -> i64 ! { io, alloc, panic } { expect_eq!(2 + 2, 5); print(9); 0 }
EOF
rc=0; "$KARDC" --no-cache "$TMP/p.kd" >"$TMP/o" 2>"$TMP/e" || rc=$?
[[ "$rc" -eq 101 ]] || { echo "FAIL [B/jit]: expect_eq! exit $rc (want 101)"; cat "$TMP/e"; exit 1; }
grep -q 'assertion' "$TMP/e" || { echo "FAIL [B/jit]: no 'assertion' on stderr"; cat "$TMP/e"; exit 1; }
grep -q 'left: 4'  "$TMP/e" || { echo "FAIL [B/jit]: no Debug 'left: 4'"; cat "$TMP/e"; exit 1; }
grep -q 'right: 5' "$TMP/e" || { echo "FAIL [B/jit]: no Debug 'right: 5'"; cat "$TMP/e"; exit 1; }
grep -q '9' "$TMP/o" && { echo "FAIL [B/jit]: print(9) ran — assert did not abort first"; cat "$TMP/o"; exit 1; }
echo "PASS [B]: expect_eq! aborts JIT with exit 101 + Debug left/right; code after it did not run"

# --- Subtest C: same abort under AOT ---
"$KARDC" --no-cache -o "$TMP/p" "$TMP/p.kd" >/dev/null 2>&1 || { echo "FAIL [C/aot]: AOT build failed"; exit 1; }
rc=0; "$TMP/p" >/dev/null 2>"$TMP/e2" || rc=$?
[[ "$rc" -eq 101 ]] || { echo "FAIL [C/aot]: AOT expect_eq! exit $rc (want 101)"; cat "$TMP/e2"; exit 1; }
grep -q 'assertion' "$TMP/e2" || { echo "FAIL [C/aot]: no 'assertion' on stderr"; cat "$TMP/e2"; exit 1; }
echo "PASS [C]: expect_eq! aborts AOT with exit 101"

# --- Subtest D: a PASSING expect is a no-op (exit 0, code after it runs) ---
cat > "$TMP/ok.kd" <<'EOF'
fn main() -> i64 ! { io, alloc, panic } { expect_eq!(3, 3); expect!(1 < 2); expect_ne!(1, 2); print(7); 0 }
EOF
rc=0; out=$("$KARDC" --no-cache "$TMP/ok.kd" 2>/dev/null) || rc=$?
[[ "$rc" -eq 0 ]] || { echo "FAIL [D]: passing expects exit $rc (want 0)"; exit 1; }
grep -q '7' <<<"$out" || { echo "FAIL [D]: print(7) did not run after passing expects"; echo "$out"; exit 1; }
echo "PASS [D]: passing expect/expect_eq!/expect_ne! are no-ops (exit 0, code runs)"

# --- Subtest E: generalizes beyond i64 — String operands via `.eq` + `{:?}` ---
cat > "$TMP/s.kd" <<'EOF'
fn main() -> i64 ! { io, alloc, panic } { expect_eq!("a".to_string(), "b".to_string()); 0 }
EOF
rc=0; "$KARDC" --no-cache "$TMP/s.kd" >/dev/null 2>"$TMP/e3" || rc=$?
[[ "$rc" -eq 101 ]] || { echo "FAIL [E/string]: exit $rc (want 101)"; cat "$TMP/e3"; exit 1; }
grep -q 'left: "a"'  "$TMP/e3" || { echo "FAIL [E/string]: no Debug-quoted 'left: \"a\"'"; cat "$TMP/e3"; exit 1; }
grep -q 'right: "b"' "$TMP/e3" || { echo "FAIL [E/string]: no Debug-quoted 'right: \"b\"'"; cat "$TMP/e3"; exit 1; }
echo "PASS [E]: expect_eq! works on String (Eq .eq + {:?} Debug quoting) — not i64-only"

echo "ALL v109 ASSERT (expect_*) SMOKE TESTS PASSED"
