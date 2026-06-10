// DECOY fixture (§22.1 relative resolution): sits beside the TEST file. If an
// import of "_deep.ks" written inside _dir/_mid.ks wrongly resolved against
// the root file's directory, THIS file would load and the output would be
// 1001 instead of 42. Correct importer-relative resolution never loads it.
fn deep_val() i64 {
    return 999;
}
