//SPEC: §22.1 top-level names must be globally unique — the root redefining an imported fn is E0293
//ERR: E0293

@import("_dup_shared.ks");

fn shared_name() i64 {
    return 2;
}

pub fn main() void {
    print(shared_name());
}
