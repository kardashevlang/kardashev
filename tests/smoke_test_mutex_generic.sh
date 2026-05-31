#!/usr/bin/env bash
# Phase 123 (Roadmap v21 — "generalize the i64/bool-MVP surfaces"): Mutex was
# Mutex<i64> only; its guarded cell is now an arbitrary T. `Mutex<T>` is a
# PHANTOM-TYPED i64 handle — the value is a bare i64 (Copy + shareable into a
# thread closure), but the type carries the cell T so mutex_get/mutex_set are
# tied to it (T flows from the handle, NO annotation needed, and NO wrong-T
# punning). mutex_new/get/set are specialized per cell type over a block
# `{ [64 x i8] pthread_mutex_t, T value }`. This test pins:
#   (1) bool / struct / i64 cells read+write correctly, with the struct `get`
#       inferring its type from the handle (no annotation);
#   (2) mutex_get CLONES the cell + mutex_set DROPS the old value, so a String
#       cell over 100k sets is RSS-flat and heap-clean (MALLOC_CHECK_=3);
#   (3) a STRUCT cell shared across two threads with lock/unlock lands on the
#       exact total under contention (correct non-i64 cell codegen across threads);
#   (4) the v21-review soundness gates are COMPILE ERRORS: a non-Send / handle
#       cell (Mutex<Rc>), and a wrong-T mutex_set / mutex_get on a typed handle
#       (the heap-overflow / punning holes the phantom type + Send gate close).
# The struct/String/thread (AOT) parts skip cleanly if no clang.
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

# 1. bool / struct / i64 cells (JIT — needs only kardc).
cat > "$TMP/cells.kd" <<'EOF'
struct Point { x: i64, y: i64 }
fn main() -> i64 ! { alloc, io } {
    let bm = mutex_new(true);
    print(if mutex_get(bm) { 1 } else { 0 });   // 1 (bool pinned by `if`)
    mutex_set(bm, false);
    print(if mutex_get(bm) { 1 } else { 0 });   // 0

    let pm = mutex_new(Point { x: 3, y: 4 });
    let p = mutex_get(pm);                       // T = Point inferred from pm
    print(p.x + p.y);                            // 7 (no annotation needed)
    mutex_set(pm, Point { x: 10, y: 20 });
    let q = mutex_get(pm);
    print(q.x + q.y);                            // 30

    let im = mutex_new(100);                      // backward-compat: i64 cell
    mutex_set(im, mutex_get(im) + 1);
    print(mutex_get(im));                         // 101
    0
}
EOF
got=$("$KARDC" "$TMP/cells.kd" 2>/dev/null)
[[ "$got" == $'1\n0\n7\n30\n101\n0' ]] || { echo "FAIL [cells]: got '$got' want 1,0,7,30,101,0"; exit 1; }
echo "PASS [cells]: Mutex over bool / struct / i64 reads + writes (struct get inferred, no annotation)"

# 1b. SOUNDNESS GATES (v21 review): each of these must be a COMPILE ERROR. A
#     program that compiles here is a regression of a memory-safety hole.
#     assert_reject <file> <tag> <substr> — kardc must fail AND mention <substr>.
assert_reject() {
    local f="$1" tag="$2" substr="$3"
    if "$KARDC" "$f" >/dev/null 2>"$TMP/err"; then
        echo "FAIL [reject/$tag]: program COMPILED but must be rejected"; cat "$TMP/err"; exit 1
    fi
    grep -qi "$substr" "$TMP/err" || { echo "FAIL [reject/$tag]: rejected, but error lacks '$substr':"; cat "$TMP/err"; exit 1; }
    echo "PASS [reject/$tag]: rejected ($substr)"
}
# (a) non-Send / handle cell: a Mutex<Rc> would race the non-atomic refcount
#     across threads (and clone-to-undef on get) — must be rejected at mutex_new.
cat > "$TMP/rc.kd" <<'EOF'
fn main() -> i64 ! { alloc, io } { let m = mutex_new(rc_new(123)); 0 }
EOF
assert_reject "$TMP/rc.kd" "rc-cell" "Mutex"
# (b) wrong-T mutex_set: a Mutex<i64> handle cannot take a Point — this is the
#     heap-overflow hole (store a >8-byte T into an i64-sized cell). Type error.
cat > "$TMP/wset.kd" <<'EOF'
struct Point { x: i64, y: i64 }
fn main() -> i64 ! { alloc, io } { let m = mutex_new(0); mutex_set(m, Point { x: 1, y: 2 }); 0 }
EOF
assert_reject "$TMP/wset.kd" "wrong-set" "type"
# (c) wrong-T mutex_get: reading a Mutex<Point> as i64 (the punning hole). Type error.
cat > "$TMP/wget.kd" <<'EOF'
struct Point { x: i64, y: i64 }
fn main() -> i64 ! { alloc, io } { let m = mutex_new(Point { x: 111, y: 222 }); let bad: i64 = mutex_get(m); print(bad); 0 }
EOF
assert_reject "$TMP/wget.kd" "wrong-get" "type"

CLANG="$(command -v clang || true)"
if [[ -z "$CLANG" ]]; then
    echo "PASS [heap/threads]: SKIPPED (no clang for AOT)"
    echo "ALL MUTEX-GENERIC SMOKE TESTS PASSED"
    exit 0
fi

# 2. String cell: get clones, set drops the old value — 100k sets stay RSS-flat
#    and heap-clean (a missing drop would leak; a double-free would abort).
cat > "$TMP/str.kd" <<'EOF'
fn main() -> i64 ! { alloc, io } {
    let sm = mutex_new("seed".to_string());
    let mut i = 0;
    while i < 100000 { mutex_set(sm, int_to_string(i)); i = i + 1; }
    let last: String = mutex_get(sm);
    print(str_len(&last));   // len("99999") = 5
    0
}
EOF
"$KARDC" --no-cache -o "$TMP/str" "$TMP/str.kd" >/dev/null 2>&1 || { echo "FAIL [heap/str]: build failed"; exit 1; }
out=$(MALLOC_CHECK_=3 "$TMP/str" 2>"$TMP/e"); rc=$?
[[ "$rc" -eq 0 ]] || { echo "FAIL [heap/str]: rc=$rc ($(head -1 "$TMP/e"))"; exit 1; }
[[ "$out" == "5" ]] || { echo "FAIL [heap/str]: got '$out' want 5"; exit 1; }
rss=""
if command -v /usr/bin/time >/dev/null 2>&1; then
    /usr/bin/time -v "$TMP/str" >/dev/null 2>"$TMP/t"
    rss=$(grep -oE 'Maximum resident set size \(kbytes\): [0-9]+' "$TMP/t" 2>/dev/null | grep -oE '[0-9]+$' || true)
fi
if [[ -n "$rss" ]]; then
    [[ "$rss" -lt 32768 ]] || { echo "FAIL [heap/str]: RSS $rss KB over 100k mutex_set — set leaks the old value"; exit 1; }
    echo "PASS [heap/str]: Mutex<String> get-clones/set-drops — 100k sets RSS-flat (${rss} KB), heap-clean"
else
    echo "PASS [heap/str]: Mutex<String> heap-clean under MALLOC_CHECK_=3 (RSS gate skipped — no GNU time)"
fi

# 3. A STRUCT cell shared across two threads: mutual exclusion must hold for a
#    non-i64 cell (an unsynchronized struct read-modify-write would lose updates).
cat > "$TMP/threads.kd" <<'EOF'
struct Counter { hits: i64, sum: i64 }
// Two threads each do a locked read-modify-write of a STRUCT cell 100000 times.
// Landing on the exact 200000/400000 requires mutex_get/mutex_set to read+write
// the struct correctly across threads and the lock to hold under contention.
// (Note: this gates correct struct-cell codegen + spawn/join + no deadlock/crash
// under contention; it does NOT by itself *prove* mutual exclusion, since on a
// fast machine the get->set window can be too narrow for a lost update to
// manifest even unsynchronized — the lock is exercised, not adversarially
// stress-tested.)
fn worker(m: Mutex<Counter>) -> i64 ! { io } {
    let mut i = 0;
    while i < 100000 {
        mutex_lock(m);
        let c = mutex_get(m);
        mutex_set(m, Counter { hits: c.hits + 1, sum: c.sum + 2 });
        mutex_unlock(m);
        i = i + 1;
    }
    0
}
fn main() -> i64 ! { alloc, io, share } {
    let m = mutex_new(Counter { hits: 0, sum: 0 });
    let t1 = thread_spawn(|| worker(m));
    let t2 = thread_spawn(|| worker(m));
    thread_join(t1);
    thread_join(t2);
    let f: Counter = mutex_get(m);
    print(f.hits);   // 200000
    print(f.sum);    // 400000
    0
}
EOF
"$KARDC" --no-cache -o "$TMP/threads" "$TMP/threads.kd" >/dev/null 2>&1 || { echo "FAIL [threads]: build failed"; exit 1; }
# AOT binary: main's `0` return is the exit code, not printed -> 2 lines.
bad=0
for r in 1 2 3; do
    out=$("$TMP/threads" 2>/dev/null)
    [[ "$out" == $'200000\n400000' ]] || { bad=$((bad+1)); }
done
[[ "$bad" -eq 0 ]] || { echo "FAIL [threads]: $bad/3 runs wrong total (last '$out', want 200000,400000)"; exit 1; }
# JIT path also echoes main's return value (trailing 0).
jout=$("$KARDC" "$TMP/threads.kd" 2>/dev/null)
[[ "$jout" == $'200000\n400000\n0' ]] || { echo "FAIL [threads/JIT]: got '$jout' want 200000,400000,0"; exit 1; }
echo "PASS [threads]: Mutex<struct> across 2 threads -> exact 200000/400000 under contention (struct-cell get/set, JIT+AOT)"

echo "ALL MUTEX-GENERIC SMOKE TESTS PASSED"
