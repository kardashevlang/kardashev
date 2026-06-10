// Import fixture (§22.1): calls `root_val`, which is defined in the ROOT file
// that imports this one — the flat module is one global namespace.
fn twice_of_root() i64 {
    return root_val() * 2;
}
