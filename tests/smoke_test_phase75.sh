#!/usr/bin/env bash
# Phase 75 smoke test (Roadmap v13 — concurrency): the `share` effect — the
# CONCURRENCY EFFECT that makes thread-safety a CHECKED property of the type
# system rather than a library convention (kardashev's differentiator).
#   1. `thread_spawn` now carries `share`: a fn that spawns must declare
#      `! { share }`, else a clear error naming `share`. With it, it compiles
#      and runs (JIT + AOT).
#   2. THE TIE-IN: because `share` is a built-in effect, it rides the existing
#      effect-SUBSET rule — a trait method declared without `share` can NEVER
#      have an impl that spawns (a `<T: Task>` / `&dyn Task` dispatch could
#      otherwise launder concurrent work past a pure-looking interface). The
#      super-effecting impl is rejected; declaring `! { share }` on the trait
#      method permits it.
set -euo pipefail

KARDC=""
for candidate in \
    "${TEST_SRCDIR:-}/_main/compiler/kardc" \
    "${TEST_SRCDIR:-}/kardashev/compiler/kardc" \
    "${RUNFILES_DIR:-}/_main/compiler/kardc" \
    "${RUNFILES_DIR:-}/kardashev/compiler/kardc" \
    "./compiler/kardc" \
    "./build.local/kardc"; do
    if [[ -n "$candidate" && -x "$candidate" ]]; then KARDC="$candidate"; break; fi
done
[[ -z "$KARDC" ]] && { echo "FAIL: kardc binary not found"; exit 1; }
echo "Using kardc at: $KARDC"

TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

rejects() { local n=$1 f=$2 needle=$3 out; set +e; out=$("$KARDC" "$f" 2>&1); set -e
    if "$KARDC" "$f" >/dev/null 2>&1; then echo "FAIL [$n]: compiled, expected error"; exit 1; fi
    grep -qi "$needle" <<<"$out" || { echo "FAIL [$n]: missing '$needle'; got: $out"; exit 1; }
    echo "PASS [$n]: rejected"; }

# 1a. spawning without declaring `share` is an error naming `share`.
cat > "$TMP/nodecl.kd" <<'EOF'
fn work() -> i64 { 42 }
fn main() -> i64 ! { io } { let h = thread_spawn(work); thread_join(h) }
EOF
rejects spawn-needs-share "$TMP/nodecl.kd" "share"

# 1b. with `! { io, share }` it compiles + runs, JIT + AOT.
cat > "$TMP/ok.kd" <<'EOF'
fn work() -> i64 { 42 }
fn main() -> i64 ! { io, share } { let h = thread_spawn(work); thread_join(h) }
EOF
jit=$("$KARDC" "$TMP/ok.kd" 2>/dev/null | tail -1)
[[ "$jit" == "42" ]] || { echo "FAIL [share-ok/jit]: expected 42 got '$jit'"; exit 1; }
"$KARDC" --no-cache -o "$TMP/ok" "$TMP/ok.kd" >/dev/null 2>&1
set +e; "$TMP/ok" >/dev/null; rc=$?; set -e
[[ "$rc" -eq 42 ]] || { echo "FAIL [share-ok/aot]: exit $rc expected 42"; exit 1; }
echo "PASS [share-ok]: thread_spawn under share-effect runs, JIT 42 + AOT 42"

# 2a. THE DIFFERENTIATOR — a trait method declared WITHOUT `share` whose impl
#     spawns is rejected (the subset rule: impl effects must be ⊆ the trait's).
cat > "$TMP/launder.kd" <<'EOF'
fn work() -> i64 { 1 }
trait Task { fn run(&self) -> i64 ! { }; }     // declared pure — no `share`
struct Spawner {}
impl Task for Spawner {
    fn run(&self) -> i64 ! { } {
        let h = thread_spawn(work);            // spawns — super-effecting
        thread_join(h)
    }
}
fn main() -> i64 { 0 }
EOF
rejects pure-trait-cannot-spawn "$TMP/launder.kd" "share"

# 2b. declaring `! { io, share }` on the trait method permits the spawning impl.
cat > "$TMP/declared.kd" <<'EOF'
fn work() -> i64 { 7 }
trait Task { fn run(&self) -> i64 ! { io, share }; }
struct Spawner {}
impl Task for Spawner {
    fn run(&self) -> i64 ! { io, share } { let h = thread_spawn(work); thread_join(h) }
}
fn main() -> i64 ! { io, share } { let s = Spawner {}; s.run() }
EOF
jit=$("$KARDC" "$TMP/declared.kd" 2>/dev/null | tail -1)
[[ "$jit" == "7" ]] || { echo "FAIL [declared-share/jit]: expected 7 got '$jit'"; exit 1; }
echo "PASS [share-subset-rule]: a pure-declared trait cannot launder a spawn; declaring share permits it"

echo "ALL PHASE 75 SMOKE TESTS PASSED"
