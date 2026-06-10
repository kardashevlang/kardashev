// Import fixture (§22.1): a `../` path resolves against THIS file's directory.
@import("../_shared_up.ks");

fn child_val() i64 {
    return shared_up() * 3;
}
