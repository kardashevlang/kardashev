//SPEC: §22.1 an import path that names no readable file is E0291
//ERR: E0291

@import("_no_such_module.ks");

pub fn main() void {
    print(1);
}
