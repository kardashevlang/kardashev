#!/usr/bin/env bash
# v66 — PROPERTY HARNESS. 16 prelude/stdlib invariants, each checked over 50
# seeded random inputs inside the program (a seeded `Rng` drives the inputs),
# printing the pass count — which MUST be 50. Each property program is run under
# both the JIT and the AOT backend and the outputs MUST agree (JIT==AOT).
# Deterministic under the fixed seeds. (Properties cover Vec push/len/get/sum/
# pop/reverse/swap/remove, String concat/repeat/contains/starts/ends/index_of,
# Option unwrap_or, the lazy iterator tower, and arithmetic round-trips.)
set -uo pipefail
KARDC=""
for c in "${TEST_SRCDIR:-}/_main/compiler/kardc" "${RUNFILES_DIR:-}/_main/compiler/kardc" "./compiler/kardc" "./build.local/kardc"; do
  [[ -n "$c" && -x "$c" ]] && { KARDC="$c"; break; }; done
[[ -z "$KARDC" ]] && { echo "FAIL: kardc not found"; exit 1; }
echo "Using kardc at: $KARDC"
TMP=$(mktemp -d); trap 'rm -rf "$TMP"' EXIT

# A property body checks an invariant for `i` in 0..50 and does `ok = ok + 1`
# when it holds. We wrap it so it prints `ok` (expected 50) and run JIT + AOT.
run_prop() {
  local name="$1" body="$2"
  cat > "$TMP/$name.kd" <<EOF
fn main() -> i64 ! { io, alloc } {
    let mut r = rng_new(7919);
    let mut ok = 0;
    let mut i = 0;
    while i < 50 {
$body
        i = i + 1;
    }
    print(ok);
    0
}
EOF
  local jit aot
  jit=$("$KARDC" "$TMP/$name.kd" 2>/dev/null | head -1) || true
  if [[ "$jit" != "50" ]]; then
    echo "FAIL [$name/jit]: pass count $jit != 50"; "$KARDC" "$TMP/$name.kd" 2>&1 | head -4; exit 1
  fi
  "$KARDC" --no-cache -o "$TMP/$name" "$TMP/$name.kd" >/dev/null 2>&1
  aot=$("$TMP/$name" 2>/dev/null | head -1) || true
  [[ "$aot" == "50" ]] || { echo "FAIL [$name/aot]: pass count $aot != 50"; exit 1; }
  [[ "$jit" == "$aot" ]] || { echo "FAIL [$name]: JIT($jit) != AOT($aot)"; exit 1; }
  echo "PASS: $name (50/50, JIT==AOT)"
}

run_prop p_vec_len '
        let k = rng_below(&mut r, 20) + 1;
        let mut v = vec_new(); let mut j = 0;
        while j < k { vec_push(&mut v, j); j = j + 1; }
        if vec_len(&v) == k { ok = ok + 1; } else {}'

run_prop p_vec_get_last '
        let x = rng_below(&mut r, 100000);
        let mut v = vec_new(); vec_push(&mut v, 1); vec_push(&mut v, x);
        if vec_get(&v, vec_len(&v) - 1) == x { ok = ok + 1; } else {}'

run_prop p_vec_sum '
        let k = rng_below(&mut r, 15) + 1;
        let mut v = vec_new(); let mut j = 0; let mut man = 0;
        while j < k { let e = rng_below(&mut r, 50); vec_push(&mut v, e); man = man + e; j = j + 1; }
        if vec_sum(&v) == man { ok = ok + 1; } else {}'

run_prop p_vec_pop '
        let x = rng_below(&mut r, 100000);
        let mut v = vec_new(); vec_push(&mut v, 9); vec_push(&mut v, x);
        if vec_pop(&mut v) == x { ok = ok + 1; } else {}'

run_prop p_vec_reverse2 '
        let a = rng_below(&mut r, 1000); let b = rng_below(&mut r, 1000); let c = rng_below(&mut r, 1000);
        let mut v = vec_new(); vec_push(&mut v, a); vec_push(&mut v, b); vec_push(&mut v, c);
        vec_reverse(&mut v); vec_reverse(&mut v);
        if vec_get(&v, 0) == a { if vec_get(&v, 2) == c { ok = ok + 1; } else {} } else {}'

run_prop p_vec_swap2 '
        let a = rng_below(&mut r, 1000); let b = rng_below(&mut r, 1000);
        let mut v = vec_new(); vec_push(&mut v, a); vec_push(&mut v, b);
        vec_swap(&mut v, 0, 1); vec_swap(&mut v, 0, 1);
        if vec_get(&v, 0) == a { if vec_get(&v, 1) == b { ok = ok + 1; } else {} } else {}'

run_prop p_vec_remove_len '
        let k = rng_below(&mut r, 10) + 2;
        let mut v = vec_new(); let mut j = 0;
        while j < k { vec_push(&mut v, j); j = j + 1; }
        let before = vec_len(&v);
        let idx = rng_below(&mut r, before);
        vec_remove(&mut v, idx);
        if vec_len(&v) == before - 1 { ok = ok + 1; } else {}'

run_prop p_str_concat_len '
        let na = rng_below(&mut r, 8) + 1; let nb = rng_below(&mut r, 8) + 1;
        let a = str_repeat(&"x", na); let b = str_repeat(&"y", nb);
        let c = str_concat(&a, &b);
        if str_len(&c) == str_len(&a) + str_len(&b) { ok = ok + 1; } else {}'

run_prop p_str_repeat_len '
        let n = rng_below(&mut r, 12);
        let s = str_repeat(&"ab", n);
        if str_len(&s) == 2 * n { ok = ok + 1; } else {}'

run_prop p_str_contains_self '
        let n = rng_below(&mut r, 6) + 1;
        let s = str_repeat(&"k", n);
        if str_contains(&s, &s) { ok = ok + 1; } else {}'

run_prop p_str_starts_with '
        let na = rng_below(&mut r, 6) + 1; let nb = rng_below(&mut r, 6) + 1;
        let a = str_repeat(&"p", na); let b = str_repeat(&"q", nb);
        let c = str_concat(&a, &b);
        if str_starts_with(&c, &a) { ok = ok + 1; } else {}'

run_prop p_str_ends_with '
        let na = rng_below(&mut r, 6) + 1; let nb = rng_below(&mut r, 6) + 1;
        let a = str_repeat(&"m", na); let b = str_repeat(&"n", nb);
        let c = str_concat(&a, &b);
        if str_ends_with(&c, &b) { ok = ok + 1; } else {}'

run_prop p_str_index_of '
        let na = rng_below(&mut r, 6) + 1;
        let a = str_repeat(&"a", na); let b = "ZZ";
        let c = str_concat(&a, &b);
        if str_index_of(&c, &b) == na { ok = ok + 1; } else {}'

run_prop p_opt_unwrap_or '
        let x = rng_below(&mut r, 100000); let d = rng_below(&mut r, 100000);
        if option_unwrap_or(Some(x), d) == x { if option_unwrap_or(None, d) == d { ok = ok + 1; } else {} } else {}'

run_prop p_iter_take '
        let n = rng_below(&mut r, 30) + 1; let k = rng_below(&mut r, 30) + 1;
        let mut t = iter_take(Range { start: 0, end: n, inclusive: 0 }, k);
        let mut cnt = 0; let mut go = true;
        while go { match t.next() { Some(x) => { cnt = cnt + 1; }, None => { go = false; } } }
        let want = if k < n { k } else { n };
        if cnt == want { ok = ok + 1; } else {}'

run_prop p_arith_roundtrip '
        let x = rng_next(&mut r); let y = rng_next(&mut r);
        if (x + y) - y == x { ok = ok + 1; } else {}'

echo "ALL PROPERTY-HARNESS TESTS PASSED (16 properties x 50 inputs, JIT==AOT)"
