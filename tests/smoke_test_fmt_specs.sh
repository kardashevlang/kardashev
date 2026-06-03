#!/usr/bin/env bash
# v71 — format!/print!/println! formatting specs: width, fill, alignment
# (`{:5}` `{:<5}` `{:>5}` `{:^5}` `{:05}` `{:*^7}`) and radix types
# (`{:x}` `{:X}` `{:b}` `{:o}`). Pure parser-desugar + prelude helpers; radix is
# built from the two's-complement bit pattern so negatives match Rust. JIT==AOT.
set -uo pipefail
KARDC=""
for c in "${TEST_SRCDIR:-}/_main/compiler/kardc" "${RUNFILES_DIR:-}/_main/compiler/kardc" "./compiler/kardc" "./build.local/kardc"; do
  [[ -n "$c" && -x "$c" ]] && { KARDC="$c"; break; }; done
[[ -z "$KARDC" ]] && { echo "FAIL: kardc not found"; exit 1; }
echo "Using kardc at: $KARDC"
TMP=$(mktemp -d); trap 'rm -rf "$TMP"' EXIT

diff_run() { local name="$1" expect="$2" src="$3"
  local n; n=$(printf '%s\n' "$expect" | wc -l)
  printf '%s' "$src" > "$TMP/$name.kd"
  local jit; jit=$("$KARDC" "$TMP/$name.kd" 2>/dev/null | head -n "$n") || true
  [[ "$jit" == "$expect" ]] || { echo "FAIL [$name/jit]: expected '$expect' got '$jit'"; "$KARDC" "$TMP/$name.kd" 2>&1|head -4; exit 1; }
  "$KARDC" --no-cache -o "$TMP/$name" "$TMP/$name.kd" >/dev/null 2>&1
  local aot; aot=$("$TMP/$name" 2>/dev/null | head -n "$n") || true
  [[ "$aot" == "$expect" ]] || { echo "FAIL [$name/aot]: expected '$expect' got '$aot'"; exit 1; }
  echo "PASS: $name"; }
reject() { local name="$1" needle="$2" src="$3"; printf '%s' "$src" > "$TMP/$name.kd"
  local e; e=$("$KARDC" "$TMP/$name.kd" 2>&1 >/dev/null || true)
  echo "$e" | grep -qi "$needle" || { echo "FAIL[reject $name]: want '$needle' got: $e"; exit 1; }
  echo "PASS(reject): $name"; }

# width + alignment + fill (values match Rust's std::fmt).
diff_run width $'[   42]\n[42   ]\n[   42]\n[ 42  ]' \
'fn main() -> i64 ! { io, alloc } {
  println!("[{:5}]", 42); println!("[{:<5}]", 42);
  println!("[{:>5}]", 42); println!("[{:^5}]", 42); 0 }'

diff_run fill $'[00042]\n[ab   ]\n[**hi***]\n[--7--]' \
'fn main() -> i64 ! { io, alloc } {
  println!("[{:05}]", 42); println!("[{:<5}]", "ab");
  println!("[{:*^7}]", "hi"); println!("[{:-^5}]", 7); 0 }'

# radix types — positive (minimal) and negative (two'\''s-complement, like Rust).
diff_run radix $'ff\nFF\n101\n377' \
'fn main() -> i64 ! { io, alloc } {
  println!("{:x}", 255); println!("{:X}", 255);
  println!("{:b}", 5); println!("{:o}", 255); 0 }'

diff_run radix_neg $'ffffffffffffffff\n1111111111111111111111111111111111111111111111111111111111111111\n1777777777777777777777\n0' \
'fn main() -> i64 ! { io, alloc } {
  println!("{:x}", 0 - 1); println!("{:b}", 0 - 1);
  println!("{:o}", 0 - 1); println!("{:b}", 0); 0 }'

# radix combined with zero-pad width.
diff_run radix_pad $'00000101\n000000ff' \
'fn main() -> i64 ! { io, alloc } {
  println!("{:08b}", 5); println!("{:08x}", 255); 0 }'

# format! returns a String; plain {} and {:?} still work alongside specs.
diff_run mixed $'a=  1 b=ff c=7' \
'fn main() -> i64 ! { io, alloc } {
  let s = format!("a={:3} b={:x} c={:?}", 1, 255, 7); println!("{}", s); 0 }'

# escaped braces are untouched.
diff_run braces $'{x} = 5' \
'fn main() -> i64 ! { io, alloc } { println!("{{x}} = {}", 5); 0 }'

# --- rejects ---
reject precision 'unsupported format spec'  'fn main() -> i64 ! { io, alloc } { println!("{:.2}", 1); 0 }'
reject badtype   'unsupported format spec'  'fn main() -> i64 ! { io, alloc } { println!("{:z}", 1); 0 }'
reject positional 'named/positional'        'fn main() -> i64 ! { io, alloc } { println!("{0}", 1); 0 }'
reject unclosed  'unclosed'                 'fn main() -> i64 ! { io, alloc } { println!("{:", 1); 0 }'

echo "ALL FORMAT-SPEC SMOKE TESTS PASSED"
