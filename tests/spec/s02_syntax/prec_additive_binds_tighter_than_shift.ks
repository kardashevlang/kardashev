//SPEC: §28.1 additive (+ -) binds tighter than shift (<< >>) — 1 << 2 + 1 is 1 << (2 + 1)
//OUT: 8
//OUT: 4
//OUT: 12
pub fn main() void {
    var one: i64 = 1;
    var two: i64 = 2;
    // 1 << (2 + 1) = 8.  Wrong grouping (1 << 2) + 1 = 5.
    print(one << two + one);
    // 16 >> (1 + 1) = 4.  Wrong grouping (16 >> 1) + 1 = 9.
    print(16 >> one + one);
    // (2 + 1) << 2 = 12.  Wrong grouping 2 + (1 << 2) = 6.
    print(two + one << two);
}
