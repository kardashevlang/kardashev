//SPEC: §11.1 `x orelse y` yields the payload of a non-null `?T`, else `y`; the result is a plain `T`
//OUT: 306
//OUT: 15

fn half_if_even(n: i64) ?i64 {
    if (n % 2 == 0) {
        return n / 2;
    }
    return null;
}

pub fn main() void {
    // Alternating null/payload over a loop: every odd n contributes the
    // default 100, every even n its half — 100+1+100+2+100+3 = 306.
    var sum: i64 = 0;
    var n: i64 = 1;
    while (n <= 6) : (n = n + 1) {
        sum = sum + (half_if_even(n) orelse 100);
    }
    print(sum);

    // The result is a plain i64, directly usable in arithmetic.
    var v: i64 = (half_if_even(10) orelse 0) * 3;
    print(v);    // 5 * 3 = 15
}
