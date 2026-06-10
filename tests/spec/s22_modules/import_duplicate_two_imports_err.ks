//SPEC: §22.1 uniqueness is checked across the WHOLE program — two imported files colliding (a const, root defines neither) is E0293
//ERR: E0293

@import("_dup_c1.ks");
@import("_dup_c2.ks");

pub fn main() void {
    print(1);
}
