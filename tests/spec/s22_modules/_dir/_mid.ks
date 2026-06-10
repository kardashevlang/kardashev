// Import fixture (§22.1): the "_deep.ks" below must resolve relative to THIS
// file's directory (tests/spec/s22_modules/_dir/), not the root test's
// directory — which holds a same-named decoy returning 999.
@import("_deep.ks");

fn mid_val() i64 {
    return deep_val() + 2;
}
