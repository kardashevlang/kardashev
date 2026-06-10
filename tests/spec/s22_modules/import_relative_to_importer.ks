//SPEC: §22.1 an import path resolves relative to the IMPORTING file's directory, not the root's
//OUT: 42

// _dir/_mid.ks imports "_deep.ks". A decoy ./_deep.ks (returning 999) sits in
// THIS directory; the real _dir/_deep.ks returns 40. Only importer-relative
// resolution yields 40 + 2 = 42.
@import("_dir/_mid.ks");

pub fn main() void {
    print(mid_val());
}
