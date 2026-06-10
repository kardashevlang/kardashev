//SPEC: §28.2 `& | ^` compute bitwise and/or/xor on same-type integer operands
//OUT: 8
//OUT: 14
//OUT: 6
//OUT: 0
//OUT: 12

// 12 = 0b1100, 10 = 0b1010 — the three results (8, 14, 6) are pairwise
// distinct, so swapping any two lowerings changes the output.
pub fn main() void {
    var a: i64 = 12;
    var b: i64 = 10;
    print(a & b);   // 0b1000 = 8
    print(a | b);   // 0b1110 = 14
    print(a ^ b);   // 0b0110 = 6
    print(a ^ a);   // x^x == 0
    print(a | 0);   // x|0 == x (the literal adopts i64)
}
