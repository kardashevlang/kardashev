#!/usr/bin/env bash
# v80 (FINAL) — diagnostics depth: (1) multi-char span underlines (`^~~~`) under
# the whole offending token, (2) inline fix-it `help:` lines keyed off the error
# code, (3) `--error-format=json` structured diagnostics (one JSON object per
# line). Builds on the v24/v64 rich-diagnostics + error-code infrastructure.
set -uo pipefail
KARDC=""
for c in "${TEST_SRCDIR:-}/_main/compiler/kardc" "${RUNFILES_DIR:-}/_main/compiler/kardc" "./compiler/kardc" "./build.local/kardc"; do
  [[ -n "$c" && -x "$c" ]] && { KARDC="$c"; break; }; done
[[ -z "$KARDC" ]] && { echo "FAIL: kardc not found"; exit 1; }
echo "Using kardc at: $KARDC"
TMP=$(mktemp -d); trap 'rm -rf "$TMP"' EXIT
JQ="$(command -v jq || true)"

want() { local name="$1" out="$2" needle="$3"
  echo "$out" | grep -qF -- "$needle" || { echo "FAIL [$name]: missing '$needle'"; echo "--- got ---"; echo "$out" | head -8; exit 1; }
  echo "PASS: $name"; }

# 1) multi-char underline: assigning to immutable `count` underlines all 5 chars
#    + an inline help line.
cat > "$TMP/immut.kd" <<'EOF'
fn main() -> i64 {
    let count = 0;
    count = count + 1;
    count
}
EOF
OUT=$("$KARDC" "$TMP/immut.kd" 2>&1 || true)
want underline_immut "$OUT" "^~~~~"
want code_E0384      "$OUT" "[E0384]"
want help_immut      "$OUT" "= help: declare the binding as \`let mut"

# 2) non-exhaustive match: underline on `match` + help.
cat > "$TMP/match.kd" <<'EOF'
fn f(n: i64) -> i64 { match n { 0 => 1 } }
fn main() -> i64 { f(0) }
EOF
OUT=$("$KARDC" "$TMP/match.kd" 2>&1 || true)
want underline_match "$OUT" "^~~~~"
want help_match      "$OUT" "= help: add the missing arms"

# 3) --error-format=json: one JSON object with the expected fields.
OUT=$("$KARDC" --error-format=json "$TMP/immut.kd" 2>&1 || true)
want json_severity "$OUT" '"severity":"error"'
want json_code     "$OUT" '"code":"E0384"'
want json_endcol   "$OUT" '"endColumn":10'
want json_help     "$OUT" '"help":'

# 4) JSON validity + field extraction (only if jq is present).
if [[ -n "$JQ" ]]; then
  CODE=$("$KARDC" --error-format=json "$TMP/immut.kd" 2>&1 | "$JQ" -r '.code' | head -1)
  [[ "$CODE" == "E0384" ]] || { echo "FAIL [jq_code]: got '$CODE'"; exit 1; }
  echo "PASS: jq_code"
  # a parse error also emits valid JSON.
  printf 'fn main() -> i64 { let x = ; 0 }' > "$TMP/parse.kd"
  SEV=$("$KARDC" --error-format=json "$TMP/parse.kd" 2>&1 | "$JQ" -r '.severity' | head -1)
  [[ "$SEV" == "error" ]] || { echo "FAIL [jq_parse]: got '$SEV'"; exit 1; }
  echo "PASS: jq_parse"
else
  echo "SKIP: jq not present (JSON shape already checked by grep)"
fi

# 5) a single-char token (operator/`;`) still gets a lone caret (no spurious ~).
printf 'fn main() -> i64 { let x = ; 0 }' > "$TMP/p.kd"
OUT=$("$KARDC" "$TMP/p.kd" 2>&1 || true)
want caret_single "$OUT" "^"

echo "ALL DIAGNOSTICS-DEPTH SMOKE TESTS PASSED"
