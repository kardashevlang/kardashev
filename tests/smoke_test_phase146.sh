#!/usr/bin/env bash
# v26 Phase 146: borrow-check completeness (two-phase borrows) + module
# visibility (pub(crate)/pub(super)/pub(self)) + use/pub-use imports.
#
#  - Two-phase borrow: `vec_push(&mut v, vec_len(&v))` — the `&mut v` is a
#    reserved borrow that does not conflict with the nested `&v` read used to
#    compute a sibling argument. Genuine aliasing (`&mut v` + `&v` as direct
#    sibling args, or two `&mut v`) is still rejected.
#  - Visibility: a path-qualified call (`lib::f()`) requires `f` reachable —
#    `pub` / `pub(crate)` / `pub(super)` are; `pub(self)` and an unmarked fn
#    are private and rejected.
#  - Imports: `use a::b;` / `pub use a::b;` parse; `use a::b as c;` installs a
#    callable alias `c`; importing a private fn is an error.
set -uo pipefail
KARDC=""
for c in "${TEST_SRCDIR:-}/_main/compiler/kardc" "${TEST_SRCDIR:-}/kardashev/compiler/kardc" \
         "${RUNFILES_DIR:-}/_main/compiler/kardc" "${RUNFILES_DIR:-}/kardashev/compiler/kardc" \
         "./compiler/kardc" "./build.local/kardc"; do
    [[ -n "$c" && -x "$c" ]] && { KARDC="$c"; break; }; done
[[ -z "$KARDC" ]] && { echo "FAIL: kardc not found"; exit 1; }
echo "Using kardc at: $KARDC"
TMP=$(mktemp -d); trap 'rm -rf "$TMP"' EXIT
run_eq() { local jit; jit=$("$KARDC" "$2" 2>&1 | head -1)
    [[ "$jit" == "$3" ]] || { echo "FAIL [$1/jit]: want $3 got '$jit'"; exit 1; }
    "$KARDC" --no-cache -o "$TMP/b" "$2" >/dev/null 2>&1 || { echo "FAIL [$1/aot]: compile"; exit 1; }
    "$TMP/b" >/dev/null; local rc=$?; [[ "$rc" -eq "$3" ]] || { echo "FAIL [$1/aot]: exit $rc want $3"; exit 1; }
    echo "PASS [$1]: $4"; }
expect_err() { local out; out=$("$KARDC" "$2" 2>&1); local rc=$?
    [[ "$rc" -ne 0 ]] || { echo "FAIL [$1]: expected an error, compiled"; exit 1; }
    echo "$out" | grep -qiE "$3" || { echo "FAIL [$1]: want /$3/, got: $out"; exit 1; }
    echo "PASS [$1]: $4"; }

# 1) two-phase borrow accepted
cat > "$TMP/tp.kd" <<'EOF'
fn main() -> i64 ! { alloc } {
    let mut v = vec_new();
    vec_push(&mut v, 10);
    vec_push(&mut v, vec_len(&v));
    vec_get(&v, 0) + vec_get(&v, 1)
}
EOF
run_eq twophase "$TMP/tp.kd" 11 "vec_push(&mut v, vec_len(&v)) — reserved &mut + nested &v read (11)"

# 2) genuine aliasing still rejected: &mut v and &v as direct sibling args
cat > "$TMP/alias.kd" <<'EOF'
fn bad(a: &mut Vec<i64>, b: &Vec<i64>) -> i64 { vec_len(b) }
fn main() -> i64 ! { alloc } {
    let mut v = vec_new(); vec_push(&mut v, 1);
    bad(&mut v, &v)
}
EOF
expect_err alias_reject "$TMP/alias.kd" "borrow" "f(&mut v, &v) sibling aliasing still rejected"

# 3) two &mut of the same place in one call still rejected
cat > "$TMP/dbl.kd" <<'EOF'
fn two(a: &mut Vec<i64>, b: &mut Vec<i64>) -> i64 { 0 }
fn main() -> i64 ! { alloc } {
    let mut v = vec_new(); vec_push(&mut v, 1);
    two(&mut v, &mut v)
}
EOF
expect_err two_mut_reject "$TMP/dbl.kd" "borrow" "two &mut of one place still rejected"

# 4) pub(crate) / pub(super) are path-reachable
cat > "$TMP/pc.kd" <<'EOF'
pub(crate) fn a() -> i64 { 40 }
pub(super) fn b() -> i64 { 2 }
fn main() -> i64 { lib::a() + lib::b() }
EOF
run_eq pub_crate "$TMP/pc.kd" 42 "pub(crate)/pub(super) reachable via path (42)"

# 5) pub(self) is private — a path call is rejected
cat > "$TMP/ps.kd" <<'EOF'
pub(self) fn hid() -> i64 { 1 }
fn main() -> i64 { lib::hid() }
EOF
expect_err pub_self "$TMP/ps.kd" "not declared .pub|pub" "pub(self) is private — path call rejected"

# 6) an unmarked fn is private — a path call is rejected
cat > "$TMP/priv.kd" <<'EOF'
fn secret() -> i64 { 1 }
fn main() -> i64 { lib::secret() }
EOF
expect_err private_path "$TMP/priv.kd" "not declared .pub|pub" "an unmarked fn is private via path"

# 7) use ... as installs a callable alias
cat > "$TMP/alias_use.kd" <<'EOF'
use lib::add_one as inc;
pub fn add_one(x: i64) -> i64 { x + 1 }
fn main() -> i64 { inc(41) }
EOF
run_eq use_alias "$TMP/alias_use.kd" 42 "use lib::add_one as inc — alias forwarder works (42)"

# 8) plain use + pub use parse and the imported item is callable by bare name
cat > "$TMP/use.kd" <<'EOF'
use lib::bar;
pub use lib::baz;
pub fn bar() -> i64 { 4 }
pub fn baz() -> i64 { 5 }
fn main() -> i64 { bar() + baz() }
EOF
run_eq use_plain "$TMP/use.kd" 9 "use / pub use parse; imported items callable (9)"

# 9) importing a private fn is an error
cat > "$TMP/usepriv.kd" <<'EOF'
use lib::secret as s;
fn secret() -> i64 { 1 }
fn main() -> i64 { s() }
EOF
expect_err use_private "$TMP/usepriv.kd" "use error|not declared|pub" "importing a private fn is rejected"

echo "PASS: Phase 146 — two-phase borrows + pub(crate/super/self) + use/pub-use"
