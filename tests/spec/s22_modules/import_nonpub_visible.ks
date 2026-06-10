//SPEC: §22.2 `pub` is NOT enforced across modules (honest v0.126 limitation) — a non-pub fn/const in an imported file is visible to the importer
//OUT: 42

@import("_nonpub.ks");

pub fn main() void {
    print(nonpub_secret() * NONPUB_K);   // 21 * 2 — both items are non-pub
}
