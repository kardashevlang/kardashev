//SPEC: §22.1 a `../` import path also resolves against the importing file's directory
//OUT: 42

// _updir/_child.ks imports "../_shared_up.ks" (14), and child_val() = 14 * 3.
@import("_updir/_child.ks");

pub fn main() void {
    print(child_val());
}
