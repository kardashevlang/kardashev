#!/usr/bin/env bash
# v64 — diagnostics depth: expanded error-code table + borrow/effect codes,
# priority-ordered classifier, --explain for every code, and value-printing
# asserts. Asserts: distinct error KINDS get distinct codes; a dangling-ref
# return is E0597; --explain prints a multi-line explanation; assert_eq!/
# assert_ne! print left/right on failure; no cascading duplicate diagnostics.
set -euo pipefail
KARDC=""
for c in "${TEST_SRCDIR:-}/_main/compiler/kardc" "${RUNFILES_DIR:-}/_main/compiler/kardc" "./compiler/kardc" "./build.local/kardc"; do
  [[ -n "$c" && -x "$c" ]] && { KARDC="$c"; break; }; done
[[ -z "$KARDC" ]] && { echo "FAIL: kardc not found"; exit 1; }
echo "Using kardc at: $KARDC"
TMP=$(mktemp -d); trap 'rm -rf "$TMP"' EXIT

# kardc exits non-zero on a compile error; capture with `|| true` so set -e /
# pipefail don't kill the script, then extract the code from the captured text.
code() { printf '%s' "$2" > "$TMP/$1.kd"; local out; out=$("$KARDC" "$TMP/$1.kd" 2>&1 || true); printf '%s' "$out" | grep -oE 'error\[E[0-9]+\]' | head -1 || true; }
want_code() { local got; got=$(code "$1" "$3"); [[ "$got" == "error[$2]" ]] || { echo "FAIL [$1]: expected $2, got '${got:-<none>}'"; "$KARDC" "$TMP/$1.kd" 2>&1 | head -2 || true; exit 1; }; echo "PASS: $1 -> $2"; }

# --- distinct codes for distinct error KINDS ---
# (Both struct- and scalar-mismatch are E0308, matching rustc — all type
#  mismatches share one code; distinctness is demonstrated across KINDS.)
want_code mismatch    E0308 'fn main() -> i64 ! { io } { let x: bool = 5; print(0); 0 }'
want_code immutable   E0384 'fn main() -> i64 { let x = 1; x = 2; 0 }'
want_code moved       E0382 'struct W { n: i64 } fn main() -> i64 ! { alloc } { let w = W { n: 1 }; let a = w; let b = w; 0 }'
want_code breakloop   E0571 'fn main() -> i64 { break; 0 }'
want_code effect      E0710 'fn f() -> i64 ! { io } { 0 } fn g() -> i64 ! { } { f() } fn main() -> i64 { 0 }'
want_code nonexh      E0004 'enum E { A, B } fn main() -> i64 { let e = E::A; match e { A => 0 } }'
want_code dangling    E0597 'fn d() -> &i64 { let x = 5; &x } fn main() -> i64 { 0 }'

# --- --explain prints a multi-line explanation for the new codes ---
for ec in E0597 E0710 E0004 E0505 E0080; do
  lines=$("$KARDC" --explain "$ec" 2>&1 | wc -l)
  (( lines >= 3 )) || { echo "FAIL [explain $ec]: only $lines lines"; exit 1; }
done
echo "PASS: --explain E0597/E0710/E0004/E0505/E0080 each multi-line"

# --- value-printing asserts (diagnostic goes to stderr, fd 2 — effect-free,
#     so the asserting `test_*() -> i64` fns need NO effect declaration) ---
cat > "$TMP/aeq.kd" <<'EOF'
fn test_x() -> i64 { assert_eq!(1, 2); 0 }
fn main() -> i64 ! { io } { let r = test_x(); print(r); 0 }
EOF
out=$("$KARDC" "$TMP/aeq.kd" 2>&1 >/dev/null | head -2)
echo "$out" | grep -q "left=1" && echo "$out" | grep -q "right=2" || { echo "FAIL [assert_eq print]: $out"; exit 1; }
echo "PASS: assert_eq!(1,2) prints left=1 right=2 before failing"
cat > "$TMP/ane.kd" <<'EOF'
fn test_y() -> i64 { assert_ne!(7, 7); 0 }
fn main() -> i64 ! { io } { let r = test_y(); print(r); 0 }
EOF
out2=$("$KARDC" "$TMP/ane.kd" 2>&1 >/dev/null | head -2)
echo "$out2" | grep -q "left=7" && echo "$out2" | grep -q "right=7" || { echo "FAIL [assert_ne print]: $out2"; exit 1; }
echo "PASS: assert_ne!(7,7) prints left=7 right=7 before failing"
# The asserting test fns must compile WITHOUT an effect declaration (the
# reporter is effect-free) — a regression here re-breaks the test convention.
"$KARDC" "$TMP/aeq.kd" >/dev/null 2>/dev/null </dev/null || true
"$KARDC" --no-cache -o "$TMP/aeq" "$TMP/aeq.kd" >/dev/null 2>&1 && [[ -x "$TMP/aeq" ]] || { echo "FAIL: effect-free assert test fn did not compile"; exit 1; }
echo "PASS: assert-using test fn stays effect-free (-> i64, no ! {io})"

# --- no cascading duplicate diagnostics: two bad lets -> two errors, not four ---
printf 'fn main() -> i64 { let a: bool = 1; let b: bool = 2; 0 }\n' > "$TMP/casc.kd"
n=$("$KARDC" "$TMP/casc.kd" 2>&1 | grep -cE 'error\[E[0-9]+\]|type error' || true)
(( n <= 2 )) || { echo "FAIL [cascade]: $n error lines (expected <= 2)"; exit 1; }
echo "PASS: no cascade ($n error(s) for 2 bad lets)"

echo "ALL DIAGNOSTICS-DEPTH SMOKE TESTS PASSED"
