//SPEC: §2/§28.1 multiplicative (* / %) binds tighter than additive (+ -)
//OUT: 14
//OUT: 17
//OUT: 1
pub fn main() void {
    var two: i64 = 2;
    var three: i64 = 3;
    // 2 + (3 * 4) = 14.  Wrong grouping (2 + 3) * 4 = 20.
    print(two + three * 4);
    // 20 - (6 / 2) = 17.  Wrong grouping (20 - 6) / 2 = 7.
    print(20 - 6 / two);
    // (8 % 3) - 1 = 1.  Wrong grouping 8 % (3 - 1) = 0.
    print(8 % three - 1);
}
