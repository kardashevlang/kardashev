//SPEC: §22.1 a `test` block in an imported file flattens without a name collision and does NOT run in program mode
//OUT: 42

// _with_test.ks holds `test "imported test block flattens"` — tests carry no
// global name (no E0293) and program-mode output is main's alone. (`kard test`
// — test mode — would compile and run the flattened test; pinned here only via
// program-mode behavior.)
@import("_with_test.ks");

pub fn main() void {
    print(with_test_val());
}
