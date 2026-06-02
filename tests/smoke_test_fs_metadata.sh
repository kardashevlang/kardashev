#!/usr/bin/env bash
# v63 — file metadata via a single stat(). fs_metadata -> Result<Metadata,
# IoError>; Metadata { size, is_dir, is_file, mtime }; fs_is_dir / fs_is_file
# wrappers. Asserts: a 100-byte file -> Ok(size 100, is_file, !is_dir); a dir ->
# is_dir; a missing path -> Err(IoNotFound). The size/is_dir/is_file checks run
# on BOTH CI platforms, so a wrong macOS struct-stat offset surfaces here.
# JIT==AOT.
set -euo pipefail
KARDC=""
for c in "${TEST_SRCDIR:-}/_main/compiler/kardc" "${RUNFILES_DIR:-}/_main/compiler/kardc" "./compiler/kardc" "./build.local/kardc"; do
  [[ -n "$c" && -x "$c" ]] && { KARDC="$c"; break; }; done
[[ -z "$KARDC" ]] && { echo "FAIL: kardc not found"; exit 1; }
echo "Using kardc at: $KARDC"
TMP=$(mktemp -d); trap 'rm -rf "$TMP"' EXIT
# Exactly 100 bytes.
head -c 100 /dev/zero | tr '\0' 'x' > "$TMP/hundred.txt"
mkdir -p "$TMP/adir"

cat > "$TMP/meta.kd" <<EOF
fn main() -> i64 ! { io } {
    let f = "$TMP/hundred.txt";
    match fs_metadata(&f) {
        Ok(m) => { print(m.size);
                   if m.is_file { print(1); } else { print(0); }
                   if m.is_dir { print(1); } else { print(0); } 0 },
        Err(e) => { print(-1); 0 }
    };
    let d = "$TMP/adir";
    if fs_is_dir(&d) { print(7); } else { print(0); }
    if fs_is_file(&d) { print(8); } else { print(0); }
    let miss = "$TMP/missing_zzz";
    match fs_metadata(&miss) {
        Ok(m) => { print(99); 0 },
        Err(e) => { match e { IoNotFound => print(404), IoPermissionDenied => print(403), IoOther => print(500) }; 0 }
    };
    0
}
EOF
want=$'100\n1\n0\n7\n0\n404'
jit=$("$KARDC" "$TMP/meta.kd" 2>/dev/null | head -6) || true
[[ "$jit" == "$want" ]] || { echo "FAIL [jit]: expected '$want' got '$jit'"; "$KARDC" "$TMP/meta.kd" 2>&1|head -5; exit 1; }
echo "PASS: metadata (jit) — size 100, is_file, !is_dir; dir is_dir; missing IoNotFound"
"$KARDC" --no-cache -o "$TMP/meta" "$TMP/meta.kd" >/dev/null 2>&1
aot=$("$TMP/meta" 2>/dev/null | head -6) || true
[[ "$aot" == "$want" ]] || { echo "FAIL [aot]: expected '$want' got '$aot'"; exit 1; }
echo "PASS: metadata (aot)"
echo "ALL FS-METADATA SMOKE TESTS PASSED"
