#!/usr/bin/env bash
# Phase 20b smoke test: the `kard.toml` package manifest + local-path
# dependency resolution driven through the `kard` wrapper.
#
# Builds a HERMETIC fixture project inside a `mktemp -d` (so it's
# Bazel-sandbox-safe and leaves no residue):
#
#   <tmp>/mathlib/src/lib.kd      pub fn square / cube  (a local dependency)
#   <tmp>/app/kard.toml           [package] + [dependencies] mathlib -> ../mathlib
#   <tmp>/app/src/main.kd         mod mathlib; fn main() -> i64 { square(5)+cube(3) }
#
# Asserts:
#   (a) `kard run`  (no file) reads the manifest, stages the local dep, and
#       JIT-prints 52  (square(5)=25 + cube(3)=27).
#   (b) `kard build` (no file) AOT-compiles to ./<package-name>, and the
#       produced binary exits with code 52.
#   (c) the staged `mathlib.kd` sibling is cleaned up afterwards (no residue).
#   (d) the bare-string dependency form (`name = "path"`) also resolves.
#   (e) a missing manifest and a manifest without a [package] name each give a
#       clear error and a nonzero exit.
set -euo pipefail

# Locate kardc (the compiler) the same way the other smoke tests do.
KARDC=""
for candidate in \
    "${TEST_SRCDIR:-}/_main/compiler/kardc" \
    "${TEST_SRCDIR:-}/kardashev/compiler/kardc" \
    "${RUNFILES_DIR:-}/_main/compiler/kardc" \
    "${RUNFILES_DIR:-}/kardashev/compiler/kardc" \
    "./compiler/kardc" \
    "./build.local/kardc"; do
    if [[ -n "$candidate" && -x "$candidate" ]]; then
        KARDC="$candidate"
        break
    fi
done
if [[ -z "$KARDC" ]]; then
    echo "FAIL: kardc binary not found"
    exit 1
fi
# Absolute path so the wrapper (run from a temp project dir) still finds it.
KARDC="$(cd "$(dirname "$KARDC")" && pwd)/$(basename "$KARDC")"
export KARDC
echo "Using kardc at: $KARDC"

# Locate the `kard` wrapper (carries the manifest logic). In Bazel it's in
# runfiles via //:kard; under Makefile.local it's the repo-root ./kard.
KARD=""
for candidate in \
    "${TEST_SRCDIR:-}/_main/kard" \
    "${TEST_SRCDIR:-}/kardashev/kard" \
    "${RUNFILES_DIR:-}/_main/kard" \
    "${RUNFILES_DIR:-}/kardashev/kard" \
    "./kard" \
    "$(dirname "$0")/../kard"; do
    if [[ -n "$candidate" && -f "$candidate" ]]; then
        KARD="$candidate"
        break
    fi
done
if [[ -z "$KARD" ]]; then
    echo "FAIL: kard wrapper not found"
    exit 1
fi
KARD="$(cd "$(dirname "$KARD")" && pwd)/$(basename "$KARD")"
echo "Using kard wrapper at: $KARD"

TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

# --- The local dependency: mathlib (lib at src/lib.kd) ---
mkdir -p "$TMP/mathlib/src"
cat > "$TMP/mathlib/src/lib.kd" <<'EOF'
pub fn square(n: i64) -> i64 { n * n }
pub fn cube(n: i64) -> i64 { n * n * n }
EOF

# --- The application project with a kard.toml manifest ---
mkdir -p "$TMP/app/src"
cat > "$TMP/app/kard.toml" <<'EOF'
# Package manifest for the smoke-test fixture.
[package]
name = "app"
version = "0.1.0"
entry = "src/main.kd"

[dependencies]
mathlib = { path = "../mathlib" }
EOF
cat > "$TMP/app/src/main.kd" <<'EOF'
mod mathlib;
fn main() -> i64 { square(5) + cube(3) }
EOF

# (a) kard run (manifest mode) prints 52.
RUN_OUT=$(cd "$TMP/app" && bash "$KARD" run 2>/dev/null)
if [[ "$RUN_OUT" != "52" ]]; then
    echo "FAIL: 'kard run' (manifest) printed '$RUN_OUT' (expected 52)"
    exit 1
fi
echo "kard run (manifest + local dep): 52"

# (c.1) the staged dependency sibling must be cleaned up after the run.
if [[ -e "$TMP/app/src/mathlib.kd" ]]; then
    echo "FAIL: staged dependency '$TMP/app/src/mathlib.kd' left behind after run"
    exit 1
fi

# (b) kard build (manifest mode) -> ./app, which exits 52.
( cd "$TMP/app" && bash "$KARD" build 2>/dev/null )
if [[ ! -x "$TMP/app/app" ]]; then
    echo "FAIL: 'kard build' (manifest) did not produce ./app"
    ls -la "$TMP/app"
    exit 1
fi
set +e
( cd "$TMP/app" && ./app )
BUILD_RC=$?
set -e
if [[ "$BUILD_RC" -ne 52 ]]; then
    echo "FAIL: built ./app exited $BUILD_RC (expected 52)"
    exit 1
fi
echo "kard build (manifest + local dep): ./app exit 52"

# (c.2) staged dependency cleaned up after the build too.
if [[ -e "$TMP/app/src/mathlib.kd" ]]; then
    echo "FAIL: staged dependency left behind after build"
    exit 1
fi

# (d) the bare-string dependency form resolves too.
mkdir -p "$TMP/app2/src"
echo 'pub fn dub(n: i64) -> i64 { n + n }' > "$TMP/dub.kd"
cat > "$TMP/app2/kard.toml" <<'EOF'
[package]
name = "app2"

[dependencies]
dub = "../dub.kd"
EOF
cat > "$TMP/app2/src/main.kd" <<'EOF'
mod dub;
fn main() -> i64 { dub(21) }
EOF
RUN2_OUT=$(cd "$TMP/app2" && bash "$KARD" run 2>/dev/null)
if [[ "$RUN2_OUT" != "42" ]]; then
    echo "FAIL: bare-string dep form printed '$RUN2_OUT' (expected 42)"
    exit 1
fi
echo "kard run (bare-string dep form): 42"

# (e.1) a missing manifest is a clear error with nonzero exit.
mkdir -p "$TMP/empty"
set +e
ERR_OUT=$(cd "$TMP/empty" && bash "$KARD" run 2>&1)
ERR_RC=$?
set -e
if [[ "$ERR_RC" -eq 0 ]]; then
    echo "FAIL: 'kard run' with no manifest unexpectedly succeeded"
    exit 1
fi
if ! grep -qi "kard.toml" <<< "$ERR_OUT"; then
    echo "FAIL: missing-manifest error did not mention kard.toml: $ERR_OUT"
    exit 1
fi
echo "missing manifest: clear error, exit $ERR_RC"

# (e.2) a manifest without [package] name is a clear error.
mkdir -p "$TMP/noname"
printf '[package]\nversion = "0.1.0"\n' > "$TMP/noname/kard.toml"
set +e
ERR2_OUT=$(cd "$TMP/noname" && bash "$KARD" build 2>&1)
ERR2_RC=$?
set -e
if [[ "$ERR2_RC" -eq 0 ]]; then
    echo "FAIL: manifest with no [package] name unexpectedly succeeded"
    exit 1
fi
if ! grep -qi "name" <<< "$ERR2_OUT"; then
    echo "FAIL: missing-name error did not mention 'name': $ERR2_OUT"
    exit 1
fi
echo "manifest missing [package] name: clear error, exit $ERR2_RC"

# (e.3) a declared dep with no library file is a clear error (and no residue).
mkdir -p "$TMP/baddep/src"
cat > "$TMP/baddep/kard.toml" <<'EOF'
[package]
name = "baddep"

[dependencies]
ghost = { path = "../does-not-exist" }
EOF
printf 'mod ghost;\nfn main() -> i64 { 0 }\n' > "$TMP/baddep/src/main.kd"
set +e
ERR3_OUT=$(cd "$TMP/baddep" && bash "$KARD" run 2>&1)
ERR3_RC=$?
set -e
if [[ "$ERR3_RC" -eq 0 ]]; then
    echo "FAIL: unresolvable dependency unexpectedly succeeded"
    exit 1
fi
if ! grep -qi "ghost" <<< "$ERR3_OUT"; then
    echo "FAIL: unresolvable-dep error did not name the dependency: $ERR3_OUT"
    exit 1
fi
if [[ -e "$TMP/baddep/src/ghost.kd" ]]; then
    echo "FAIL: a staged link was left behind after a failed dep resolve"
    exit 1
fi
echo "unresolvable dependency: clear error, exit $ERR3_RC, no residue"

echo "PASS: kard.toml manifest drives build/run + local-path deps resolve"
