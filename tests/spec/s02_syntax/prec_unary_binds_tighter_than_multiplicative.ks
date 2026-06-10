//SPEC: §2/§28.1 unary (- ! ~) binds tighter than multiplicative — ~2 * 2 is (~2) * 2
//OUT: -6
//OUT: -8
pub fn main() void {
    var two: i64 = 2;
    // (~2) * 2 = -3 * 2 = -6.  Wrong grouping ~(2 * 2) = -5.
    print(~two * two);
    // (~0) * 8 = -1 * 8 = -8.  Wrong grouping ~(0 * 8) = -1.
    print(~0 * 8);
}
