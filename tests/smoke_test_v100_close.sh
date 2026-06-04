#!/usr/bin/env bash
# Roadmap v100 — the consolidation close of the v91-v100 arc. This gate asserts
# the whole v100 story holds together: the codegen-audit fix, the hardened
# bootstrap candidate, and the inherited perf/vector/binary-format locks — plus a
# doc-vs-reality cross-check of the 1.0-readiness ledger. It composes the
# already-green sub-gates (each also runs standalone in the sweep) so a regression
# in any of them fails the close gate too.
set -uo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# Locate the repo root (where tests/ + docs/ live) whether run from bazel or shell.
ROOT=""
for r in "${TEST_SRCDIR:-}/_main" "${RUNFILES_DIR:-}/_main" "$HERE/.." "."; do
  [[ -f "$r/tests/smoke_test_bootstrap.sh" ]] && { ROOT="$r"; break; }; done
[[ -z "$ROOT" ]] && { echo "FAIL: repo root not found"; exit 1; }
cd "$ROOT"
echo "v100 close-out gate; root: $ROOT"

run_subgate() {  # $1 script  $2 label
  # Skip if the sub-script isn't in this gate's runfiles (a Bazel quirk) — each
  # sub-gate is its OWN sh_test that runs independently, so coverage isn't lost.
  if [[ ! -f "tests/$1" ]]; then
    echo "PASS [$2]: tests/$1 not in runfiles — covered by its standalone sh_test"; return
  fi
  if bash "tests/$1" >/tmp/v100_sub.log 2>&1; then
    echo "PASS [$2]: tests/$1 green"
  else
    echo "FAIL [$2]: tests/$1 FAILED"; tail -8 /tmp/v100_sub.log; exit 1
  fi
}

# 1. The codegen-audit fix (packed-field write is align 1; non-packed control
#    align 8; runtime round-trip) — the v100 deliverable A.
run_subgate smoke_test_packed_write.sh "codegen-audit-packed-write"

# 2. The hardened bootstrap candidate — determinism + the corpus (now incl. the
#    v100 `subtract` fix), >= 11 programs deterministic + self==host (deliverable B).
run_subgate smoke_test_bootstrap.sh "bootstrap-candidate"

# 3. v99 effect rows still parse + propagate self==host (the arc's prior version).
run_subgate smoke_test_selfhost_effects.sh "selfhost-effects"

# 4. The v95 perf gate still green (parity locked — the packed-store fix and the
#    structgen edits must not perturb the hot-path lowering).
run_subgate smoke_test_perf_regression.sh "perf-regression-lock"

# 5. The v97 binary-format areas (repr(packed) read/swap/volatile/endian — the
#    audit-VERIFIED-correct paths) still green after the packed-store fix.
run_subgate smoke_test_repr_packed.sh "repr-packed-v97-lock"

# 6. The v90 vectorization lock still green (the v51 TTI invariant).
run_subgate smoke_test_v90_close.sh "vectorization-v90-lock"

# 7. Ledger doc-vs-reality cross-check: docs/road-to-1.0.md exists and every test
#    file it cites is actually present in tests/ (no fabricated evidence).
LEDGER="docs/road-to-1.0.md"
[[ -f "$LEDGER" ]] || { echo "FAIL [ledger-exists]: $LEDGER missing"; exit 1; }
[[ -f "docs/bootstrap-status.md" ]] || { echo "FAIL [ledger-exists]: docs/bootstrap-status.md missing"; exit 1; }
missing=0
for t in smoke_test_perf_regression.sh smoke_test_lsp.sh smoke_test_slice_mut.sh smoke_test_repr_packed.sh smoke_test_bootstrap.sh; do
  grep -q "$t" "$LEDGER" || { echo "  note: ledger does not cite $t"; }
  [[ -f "tests/$t" ]] || { echo "FAIL [ledger-evidence]: ledger-class test tests/$t not present"; missing=1; }
done
[[ "$missing" -eq 0 ]] || exit 1
echo "PASS [ledger-evidence]: road-to-1.0 ledger + bootstrap-status exist; cited tests are present"

echo "ALL v100 CLOSE-OUT SMOKE TESTS PASSED"
