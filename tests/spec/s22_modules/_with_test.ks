// Import fixture (§22.1): carries a `test` block. Tests have no top-level name
// (no E0293) and do not run in program mode; `kard test` (test mode) would
// flatten and run it.
pub fn with_test_val() i64 {
    return 42;
}

test "imported test block flattens" {
    expect(with_test_val() == 42);
}
