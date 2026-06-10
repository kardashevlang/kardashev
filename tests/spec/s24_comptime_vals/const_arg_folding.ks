//SPEC: §24.2 a comptime value argument is const-evaluated — top-level consts and constant arithmetic fold at the call site
//OUT: 8
//OUT: 13

// The argument expression is evaluated over the top-level constant
// environment (SPEC §3 const rules): a bare const reference and a compound
// arithmetic expression over one both bind the value parameter.
const BASE = 3;

fn addn(comptime n: i64, x: i64) i64 {
    return n + x;
}

pub fn main() void {
    print(addn(BASE, 5)); // 3 + 5            = 8
    print(addn(BASE * 4 + 1 - 1, 1)); // (12 + 0) + 1     = 13
}
