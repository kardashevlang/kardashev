//SPEC: §22.1 a transitive import cycle (a → b → a) is E0292, not an infinite recursion or a silent dedup
//ERR: E0292

@import("_cyc_a.ks");

pub fn main() void {
    print(1);
}
