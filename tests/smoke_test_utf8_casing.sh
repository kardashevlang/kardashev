#!/usr/bin/env bash
# v55 — UTF-8-safe casing + the genuinely-missing char API. The old
# str_to_upper/str_to_lower iterated by BYTE and mapped only ASCII, so
# str_to_upper("café") left the é un-cased. They now iterate by CHAR
# (str_char_width_at + str_decode_char_at), case-map the codepoint via
# char_to_upper/char_to_lower (extended to the Latin-1 Supplement), and re-encode
# with the existing str_push_char codec. Plus the char-indexed helpers
# str_split_char / str_get_char / str_index_char. Differential JIT==AOT.
# (Full Unicode case folding — Greek/Cyrillic/Extended, ß->SS — is deferred.)
set -euo pipefail
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
  [[ "$jit" == "$expect" ]] || { echo "FAIL [$name/jit]: expected '$expect' got '$jit'"; "$KARDC" "$TMP/$name.kd" 2>&1 | head -3; exit 1; }
  "$KARDC" --no-cache -o "$TMP/$name" "$TMP/$name.kd" >/dev/null 2>&1
  local aot; aot=$("$TMP/$name" 2>/dev/null | head -n "$n") || true
  [[ "$aot" == "$expect" ]] || { echo "FAIL [$name/aot]: expected '$expect' got '$aot'"; exit 1; }
  echo "PASS: $name"; }

# the exact documented bug case + Latin-1 round-trips (8 mixed accented strings).
diff_run cafe   'CAFÉ' 'fn main() -> i64 ! { io, alloc } { print_str(&str_to_upper(&"café")); 0 }'
diff_run lower  'naïve zürich' 'fn main() -> i64 ! { io, alloc } { print_str(&str_to_lower(&"NAÏVE ZÜRICH")); 0 }'
diff_run roundtrips $'CAFÉ\nNAÏVE\nZÜRICH\nÑOÑO\nÉCOLE\nGARÇON\nFAÇADE\nÜBER' \
  'fn up(s: &String) -> String ! { alloc } { str_to_upper(s) }
   fn main() -> i64 ! { io, alloc } {
     print_str(&up(&"café")); print_str(&up(&"naïve")); print_str(&up(&"zürich"));
     print_str(&up(&"ñoño")); print_str(&up(&"école")); print_str(&up(&"garçon"));
     print_str(&up(&"façade")); print_str(&up(&"über")); 0 }'
# casing is idempotent + invertible on ASCII (lower(upper(x)) round-trips).
diff_run invert 'hello world' 'fn main() -> i64 ! { io, alloc } { print_str(&str_to_lower(&str_to_upper(&"Hello World"))); 0 }'

# char-indexed helpers (genuinely new).
diff_run split  '3' 'fn main() -> i64 ! { io, alloc } { print(vec_len(&str_split_char(&"a,b,c", '"'"','"'"'))); 0 }'
diff_run getc   'é' 'fn main() -> i64 ! { io, alloc } { print_char(str_get_char(&"café", 3)); print_no_nl(&"\n"); 0 }'
diff_run idxc   '2' 'fn main() -> i64 ! { io, alloc } { match str_index_char(&"café", '"'"'f'"'"') { Some(k) => print(k), None => print(0-1) } 0 }'
diff_run idxc_none '-1' 'fn main() -> i64 ! { io, alloc } { match str_index_char(&"café", '"'"'z'"'"') { Some(k) => print(k), None => print(0-1) } 0 }'
# split_char on a multibyte separator + reassemble count.
diff_run split_mb '2' 'fn main() -> i64 ! { io, alloc } { print(vec_len(&str_split_char(&"hello·world", '"'"'·'"'"'))); 0 }'

echo "ALL UTF-8 CASING SMOKE TESTS PASSED"
