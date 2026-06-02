#!/usr/bin/env bash
# v56 — thread-local effect handlers. The per-(effect,op) current-handler global
# used to be process-global, so two threads installing different handlers for the
# same effect raced a shared slot. The handler global is now thread-local in AOT
# (GeneralDynamicTLSModel), so each thread reads/writes its OWN handler.
#
# AOT-ONLY: thread_local lowers to __emutls_get_address, which the ORC JIT cannot
# resolve (see the panic-stack note in codegen.cpp). JIT keeps process-global
# handlers (single-threaded — no race in practice). DO NOT port this to JIT mode.
#
# Two threads each install a different handler for one effect and perform it 100k
# times concurrently; with thread-local handlers thread A sees only its handler
# (sum == 100000*1) and thread B only its own (sum == 100000*2), deterministically.
# A shared global would produce mixed, non-deterministic sums.
set -euo pipefail
KARDC=""
for c in "${TEST_SRCDIR:-}/_main/compiler/kardc" "${RUNFILES_DIR:-}/_main/compiler/kardc" "./compiler/kardc" "./build.local/kardc"; do
  [[ -n "$c" && -x "$c" ]] && { KARDC="$c"; break; }; done
[[ -z "$KARDC" ]] && { echo "FAIL: kardc not found"; exit 1; }
echo "Using kardc at: $KARDC"
TMP=$(mktemp -d); trap 'rm -rf "$TMP"' EXIT

cat > "$TMP/tlh.kd" <<'EOF'
effect Tag { fn get() -> i64; }
fn worker() -> i64 ! { Tag } {
    let mut sum = 0;
    let mut i = 0;
    while i < 100000 { sum = sum + perform Tag::get(); i = i + 1; }
    sum
}
fn thread_a() -> i64 { handle { worker() } with Tag { get() => 1 } }
fn thread_b() -> i64 { handle { worker() } with Tag { get() => 2 } }
fn main() -> i64 ! { io, share } {
    let a = thread_spawn(thread_a);
    let b = thread_spawn(thread_b);
    let ra = thread_join(a);
    let rb = thread_join(b);
    print(ra);   // 100000  (each get() -> 1, no cross-talk)
    print(rb);   // 200000  (each get() -> 2)
    0
}
EOF

# AOT build (TLS is unavailable under JIT — do not run the JIT path here).
"$KARDC" --no-cache -o "$TMP/tlh" "$TMP/tlh.kd" 2>&1 | grep -v "cache" | head -3 || true
[[ -x "$TMP/tlh" ]] || { echo "FAIL: AOT build failed"; "$KARDC" --no-cache "$TMP/tlh.kd" 2>&1 | head -5; exit 1; }

# emitted IR must show the handler global is thread_local.
# capture to a file (piping directly into `grep -q` would SIGPIPE kardc under
# `set -o pipefail` and spuriously fail the pipeline).
"$KARDC" --no-cache --emit-llvm "$TMP/tlh.kd" > "$TMP/ir.ll" 2>/dev/null || true
grep -Eq "__handler_Tag[^=]*=.*thread_local" "$TMP/ir.ll" \
  || { echo "FAIL: handler global is not thread_local in emitted IR"; exit 1; }
echo "PASS: handler global is thread_local (AOT IR)"

# deterministic, no cross-talk, over 6 runs (MALLOC_CHECK_ on to catch heap bugs).
for run in 1 2 3 4 5 6; do
  out=$(MALLOC_CHECK_=3 "$TMP/tlh" 2>/dev/null)
  exp=$'100000\n200000'
  [[ "$out" == "$exp" ]] || { echo "FAIL [run $run]: expected per-thread 100000/200000, got: $out"; exit 1; }
done
echo "PASS: 6/6 concurrent runs — each thread saw only its own handler (100000 / 200000)"

echo "ALL THREAD-LOCAL-HANDLER SMOKE TESTS PASSED"
