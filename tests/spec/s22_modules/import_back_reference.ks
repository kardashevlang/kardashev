//SPEC: §22.1 the flat module is one global namespace — an imported file's fn may call a fn defined in the ROOT
//OUT: 42

@import("_back_calls_root.ks");

fn root_val() i64 {
    return 21;
}

pub fn main() void {
    print(twice_of_root());   // fixture fn calls back into root_val(): 21 * 2
}
