#!/usr/bin/env bash
# Phase 30 smoke test: file I/O (fs_write / fs_exists / fs_read_to_string over
# Result<_, IoError>) and CLI args (arg_count / args / arg_get). Through JIT +
# AOT, plus the not-found error path and the AOT argv-capture.
set -euo pipefail

KARDC=""
for candidate in \
    "${TEST_SRCDIR:-}/_main/compiler/kardc" \
    "${TEST_SRCDIR:-}/kardashev/compiler/kardc" \
    "${RUNFILES_DIR:-}/_main/compiler/kardc" \
    "${RUNFILES_DIR:-}/kardashev/compiler/kardc" \
    "./compiler/kardc"; do
    if [[ -n "$candidate" && -x "$candidate" ]]; then KARDC="$candidate"; break; fi
done
[[ -z "$KARDC" ]] && { echo "FAIL: kardc binary not found"; exit 1; }
echo "Using kardc at: $KARDC"

TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

# --- file I/O round-trip + not-found, parameterized by a writable temp path. ---
# Heredoc is UNQUOTED so $TMP expands into the kardashev string literal.
cat > "$TMP/io.kd" <<EOF
fn main() -> i64 ! { io, alloc } {
    let path = "$TMP/data.txt";
    let contents = "hello kardashev io";
    let w = match fs_write(&path, &contents) { Ok(_) => 1, Err(_) => 0 };   // 1
    let e = if fs_exists(&path) { 10 } else { 0 };                          // 10
    let rd = match fs_read_to_string(&path) {
        Ok(s) => if str_eq(&s, &contents) { 100 } else { 5 },              // 100
        Err(_) => 0
    };
    let missing = "$TMP/nope.txt";
    let nf = match fs_read_to_string(&missing) {
        Ok(_) => 0,
        Err(er) => match er { IoNotFound => 1000, IoPermissionDenied => 1, IoOther => 2 }
    };                                                                      // 1000
    let gone = if fs_exists(&missing) { 9999 } else { 0 };                  // 0
    w + e + rd + nf + gone                                                  // 1111
}
EOF
jit=$("$KARDC" "$TMP/io.kd")
[[ "$jit" != "1111" ]] && { echo "FAIL [io/jit]: expected 1111, got '$jit'"; exit 1; }
"$KARDC" --no-cache -o "$TMP/io" "$TMP/io.kd" >/dev/null
rm -f "$TMP/data.txt"   # ensure the AOT run does its own write
set +e; "$TMP/io" >/dev/null; rc=$?; set -e
[[ "$rc" -ne 87 ]] && { echo "FAIL [io/aot]: exit $rc (expected 87 = 1111 % 256)"; exit 1; }
echo "PASS [io]: write -> exists -> read+str_eq round-trips; missing file -> Err(IoNotFound) (JIT + AOT)"

# --- CLI args: JIT sees none; AOT sees real argv (argc == prog + N args). ---
cat > "$TMP/args.kd" <<'EOF'
fn main() -> i64 ! { alloc } {
    let v = args();
    let n = vec_len(&v);          // same as arg_count()
    if n == arg_count() { n } else { 0 - 1 }
}
EOF
ajit=$("$KARDC" "$TMP/args.kd")
[[ "$ajit" != "0" ]] && { echo "FAIL [args/jit]: expected 0 (no argv), got '$ajit'"; exit 1; }
"$KARDC" --no-cache -o "$TMP/args" "$TMP/args.kd" >/dev/null
set +e; "$TMP/args" one two three four >/dev/null; arc=$?; set -e
[[ "$arc" -ne 5 ]] && { echo "FAIL [args/aot]: exit $arc (expected 5 = prog + 4 args)"; exit 1; }
set +e; "$TMP/args" >/dev/null; arc0=$?; set -e
[[ "$arc0" -ne 1 ]] && { echo "FAIL [args/aot]: bare exit $arc0 (expected 1 = prog only)"; exit 1; }
echo "PASS [args]: JIT reports 0 args; AOT argv-capture yields argc via args()/arg_count (JIT + AOT)"

echo "PASS: file I/O (Result<_, IoError>) + CLI args (Vec<String>) work in JIT + AOT"
