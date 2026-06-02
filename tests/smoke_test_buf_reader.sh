#!/usr/bin/env bash
# v63 — buffered line reader. BufReader owns a FILE* + persistent getline
# scratch; buf_read_line yields '\n'-stripped lines and None at EOF; Drop
# fclose()s + free()s (LSan/RSS-clean). Asserts: a 3-line file yields the 3
# exact lines then None; an empty file yields None immediately; and an
# open/read/drop loop stays RSS-flat (no FILE*/buffer leak). JIT==AOT.
set -euo pipefail
KARDC=""
for c in "${TEST_SRCDIR:-}/_main/compiler/kardc" "${RUNFILES_DIR:-}/_main/compiler/kardc" "./compiler/kardc" "./build.local/kardc"; do
  [[ -n "$c" && -x "$c" ]] && { KARDC="$c"; break; }; done
[[ -z "$KARDC" ]] && { echo "FAIL: kardc not found"; exit 1; }
echo "Using kardc at: $KARDC"
TMP=$(mktemp -d); trap 'rm -rf "$TMP"' EXIT
printf 'alpha\nbeta\ngamma\n' > "$TMP/three.txt"
: > "$TMP/empty.txt"

# Reads every line, printing "line|" for each, then "EOF".
cat > "$TMP/buf.kd" <<EOF
fn main() -> i64 ! { io, alloc } {
    let path = "$TMP/three.txt";
    match buf_reader_new(&path) {
        Ok(br) => {
            let mut r = br;
            let mut go = true;
            while go {
                match buf_read_line(&mut r) {
                    Some(line) => { print_no_nl(&line); print_no_nl(&"|"); },
                    None => { print_no_nl(&"EOF\n"); go = false; }
                }
            }
            0
        },
        Err(e) => { print_no_nl(&"OPENERR\n"); 0 }
    }
}
EOF
want='alpha|beta|gamma|EOF'
jit=$("$KARDC" "$TMP/buf.kd" 2>/dev/null | head -1) || true
[[ "$jit" == "$want" ]] || { echo "FAIL [3line/jit]: expected '$want' got '$jit'"; "$KARDC" "$TMP/buf.kd" 2>&1|head -4; exit 1; }
"$KARDC" --no-cache -o "$TMP/buf" "$TMP/buf.kd" >/dev/null 2>&1
aot=$("$TMP/buf" 2>/dev/null | head -1) || true
[[ "$aot" == "$want" ]] || { echo "FAIL [3line/aot]: expected '$want' got '$aot'"; exit 1; }
echo "PASS: 3-line file -> 3 lines + EOF (jit==aot)"

# Empty file -> immediate None (just "EOF").
sed "s#$TMP/three.txt#$TMP/empty.txt#" "$TMP/buf.kd" > "$TMP/bufe.kd"
je=$("$KARDC" "$TMP/bufe.kd" 2>/dev/null | head -1) || true
[[ "$je" == "EOF" ]] || { echo "FAIL [empty]: expected 'EOF' got '$je'"; exit 1; }
echo "PASS: empty file -> immediate None"

# Missing file -> Err.
cat > "$TMP/bufmiss.kd" <<EOF
fn main() -> i64 ! { io, alloc } {
    let path = "$TMP/does_not_exist_zzz";
    match buf_reader_new(&path) {
        Ok(br) => { let mut r = br; print(0); 0 },
        Err(e) => { match e { IoNotFound => print(404), IoPermissionDenied => print(403), IoOther => print(500) }; 0 }
    }
}
EOF
jm=$("$KARDC" "$TMP/bufmiss.kd" 2>/dev/null | head -1) || true
[[ "$jm" == "404" ]] || { echo "FAIL [missing]: expected '404' got '$jm'"; exit 1; }
echo "PASS: missing file -> Err(IoNotFound)"

# Leak proxy: 100k open/read/drop cycles must stay RSS-flat (Drop frees).
cat > "$TMP/leak.kd" <<EOF
fn read_all(path: &String) -> i64 ! { io, alloc } {
    match buf_reader_new(path) {
        Ok(br) => {
            let mut r = br; let mut n = 0; let mut go = true;
            while go { match buf_read_line(&mut r) { Some(l) => { n = n + 1; }, None => { go = false; } } }
            n
        },
        Err(e) => 0
    }
}
fn main() -> i64 ! { io, alloc } {
    let path = "$TMP/three.txt";
    let mut total = 0; let mut i = 0;
    while i < 100000 { total = total + read_all(&path); i = i + 1; }
    print(total); 0
}
EOF
"$KARDC" --no-cache -o "$TMP/leak" "$TMP/leak.kd" >/dev/null 2>&1
out=$("$TMP/leak" 2>/dev/null | head -1)
[[ "$out" == "300000" ]] || { echo "FAIL [leak/output]: expected 300000 got '$out'"; exit 1; }
# Measure peak RSS; a leaked FILE*/buffer per iter would balloon to >100 MB.
if command -v /usr/bin/time >/dev/null 2>&1; then
  rss=$(/usr/bin/time -v "$TMP/leak" 2>&1 | awk '/Maximum resident/{print $NF}')
  if [[ -n "$rss" ]]; then
    (( rss < 32768 )) || { echo "FAIL [leak/rss]: peak RSS ${rss} KB > 32 MB — BufReader Drop leaks"; exit 1; }
    echo "PASS: 100k open/read/drop cycles RSS-flat (${rss} KB)"
  else
    echo "PASS: 100k open/read/drop cycles ran clean (RSS unavailable)"
  fi
else
  echo "PASS: 100k open/read/drop cycles ran clean (/usr/bin/time absent)"
fi

echo "ALL BUF-READER SMOKE TESTS PASSED"
