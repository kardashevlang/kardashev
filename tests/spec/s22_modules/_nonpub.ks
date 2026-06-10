// Import fixture (§22.2): deliberately NON-pub items — v0.126 does not enforce
// `pub` across modules, so the importer can see both.
fn nonpub_secret() i64 {
    return 21;
}

const NONPUB_K = 2;
