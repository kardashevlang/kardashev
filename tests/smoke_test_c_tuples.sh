#!/usr/bin/env bash
# v75: the C backend (`--emit-c`) grows to TUPLES — `(a, b)` literals, `.N` field
# access, and tuples as fn params / returns / locals (incl. nested tuples and
# tuples behind a reference). A tuple `(T0, T1, …)` lowers to an anonymous C
# struct `struct kdtup_… { T0 _0; T1 _1; … }`. Each program is DIFFERENTIALLY
# GATED: the LLVM-AOT exit code must equal the emitted-C exit code. Tuples in
# struct fields / enum payloads / top-level consts, tuple-destructuring `let`,
# and tuples with non-scalar elements are refused cleanly (no miscompile).
set -uo pipefail
KARDC=""
for c in "${TEST_SRCDIR:-}/_main/compiler/kardc" "${RUNFILES_DIR:-}/_main/compiler/kardc" \
         "./compiler/kardc" "./build.local/kardc"; do
    [[ -n "$c" && -x "$c" ]] && { KARDC="$c"; break; }; done
[[ -z "$KARDC" ]] && { echo "FAIL: kardc not found"; exit 1; }
CC_BIN="$(command -v cc || command -v gcc || command -v clang || true)"
[[ -z "$CC_BIN" ]] && { echo "SKIP: no C compiler"; exit 0; }
echo "Using kardc at: $KARDC ; cc: $CC_BIN"
TMP=$(mktemp -d); trap 'rm -rf "$TMP"' EXIT

diff_ok() { local name="$1" src="$2"
    "$KARDC" --no-cache -o "$TMP/llvm" "$src" >/dev/null 2>&1 || { echo "FAIL [$name]: LLVM AOT compile"; exit 1; }
    "$TMP/llvm" >/dev/null 2>&1; local lrc=$?
    "$KARDC" --emit-c "$src" > "$TMP/out.c" 2>"$TMP/e" || { echo "FAIL [$name]: --emit-c refused an in-subset program: $(cat "$TMP/e")"; exit 1; }
    "$CC_BIN" -fwrapv -O2 -o "$TMP/cbin" "$TMP/out.c" 2>"$TMP/cc" || { echo "FAIL [$name]: cc rejected the C:"; head -5 "$TMP/cc"; exit 1; }
    "$TMP/cbin" >/dev/null 2>&1; local crc=$?
    [[ "$lrc" -eq "$crc" ]] || { echo "FAIL [$name]: LLVM exit $lrc != C exit $crc"; exit 1; }
    echo "PASS [$name]: LLVM == C == $lrc"; }
refuse() { local name="$1" needle="$2" src="$3"; printf '%s' "$src" > "$TMP/r.kd"
    local e; e=$("$KARDC" --emit-c "$TMP/r.kd" 2>&1 >/dev/null || true)
    echo "$e" | grep -qi "$needle" || { echo "FAIL[refuse $name]: want '$needle' got: $e"; exit 1; }
    echo "PASS(refuse): $name"; }

cat > "$TMP/lit.kd" <<'EOF'
fn main() -> i64 { let t = (40, 2); t.0 + t.1 }
EOF
diff_ok lit_and_field "$TMP/lit.kd"

cat > "$TMP/ret.kd" <<'EOF'
fn pair(a: i64, b: i64) -> (i64, i64) { (a, b) }
fn main() -> i64 { let p = pair(30, 12); p.0 + p.1 }
EOF
diff_ok tuple_return "$TMP/ret.kd"

cat > "$TMP/param.kd" <<'EOF'
fn sum(t: (i64, i64, i64)) -> i64 { t.0 + t.1 + t.2 }
fn main() -> i64 { sum((10, 20, 12)) }
EOF
diff_ok tuple_param "$TMP/param.kd"

cat > "$TMP/nest.kd" <<'EOF'
fn main() -> i64 { let t = ((1, 2), 3); t.0.0 + t.0.1 + t.1 }
EOF
diff_ok nested_tuple "$TMP/nest.kd"

cat > "$TMP/boolt.kd" <<'EOF'
fn pick(t: (bool, i64, i64)) -> i64 { if t.0 { t.1 } else { t.2 } }
fn main() -> i64 { pick((false, 9, 5)) }
EOF
diff_ok bool_element "$TMP/boolt.kd"

cat > "$TMP/ref.kd" <<'EOF'
fn first(t: &(i64, i64)) -> i64 { t.0 }
fn snd(t: &(i64, i64)) -> i64 { t.1 }
fn main() -> i64 { let t = (7, 8); first(&t) + snd(&t) }
EOF
diff_ok tuple_via_ref "$TMP/ref.kd"

# --- clean refusals (no miscompile) ---
refuse field_tuple   'struct field'        'struct P { t: (i64, i64) } fn main() -> i64 { let p = P { t: (1,2) }; p.t.0 }'
refuse const_tuple   'const'               'const P: (i64, i64) = (1, 2); fn main() -> i64 { P.0 }'
refuse destructure   'destructuring'       'fn main() -> i64 { let (a, b) = (1, 2); a + b }'
refuse nonscalar_elt 'non-scalar element'  'fn main() -> i64 ! { alloc } { let t = (string_new(), 5); t.1 }'

echo "ALL C-BACKEND TUPLE SMOKE TESTS PASSED"
