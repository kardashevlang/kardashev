//SPEC: §38 `@as(f64, n)` converts any integer to `f64` — the explicit bridge across the no-mixing rule
//OUT: 3
//OUT: 2.5
//OUT: 4.5

pub fn main() void {
    print(@as(f64, 3));      // a literal: 3.0, prints "3"
    var n: i32 = 5;
    var half: f64 = @as(f64, n) / 2.0;
    print(half);             // 5/2 in f64 = 2.5 (int / would give 2)
    var xs: [9]i64 = [9]i64{ 0, 0, 0, 0, 0, 0, 0, 0, 0 };
    var k: usize = xs.len;   // an UNSIGNED int converts too
    print(@as(f64, k) / 2.0); // 9/2 in f64 = 4.5
}
