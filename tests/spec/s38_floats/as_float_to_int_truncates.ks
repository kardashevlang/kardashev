//SPEC: §38 `@as(intT, x)` converts `f64` to integer by C-cast truncation toward zero, both signs
//OUT: 7
//OUT: -7
//OUT: 2
//OUT: 1

pub fn main() void {
    print(@as(i64, 7.9));            // toward zero: 7 (not 8)
    print(@as(i64, 0.0 - 7.9));      // toward zero: -7 (not -8)
    print(@as(i32, 2.999));          // any integer target works: 2
    // The result is a real integer again — `%` (integer-only, §38) accepts it.
    print(@as(i64, 7.9) % 2);        // 7 % 2 = 1
}
