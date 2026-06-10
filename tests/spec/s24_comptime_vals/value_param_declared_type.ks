//SPEC: §24.2 a reference to a comptime value parameter is a constant of its DECLARED type
//OUT: 12
//OUT: 4

// `n` is declared `i32`, so it mixes with `i32` runtime operands under the
// strict integer typing rules — if the constant defaulted to `i64` instead of
// carrying its declared type, `n + x` would be an E0110 operand mismatch and
// this file would not compile at all.
fn addn(comptime n: i32, x: i32) i32 {
    return n + x;
}

fn subn(comptime n: i32, x: i32) i32 {
    return x - n;
}

pub fn main() void {
    print(addn(5, 7)); // 12
    print(subn(5, 9)); // 4
}
