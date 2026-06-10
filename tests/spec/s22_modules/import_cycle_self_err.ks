//SPEC: §22.1 a file importing ITSELF is a (direct) import cycle — E0292
//ERR: E0292

@import("import_cycle_self_err.ks");

pub fn main() void {
    print(1);
}
