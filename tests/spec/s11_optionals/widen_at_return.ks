//SPEC: §11.2 a `T` value widens to `?T` at a `return` in a `?T` function
//OUT: 6
//OUT: -1
//OUT: 3

// The largest proper divisor of n, or null when n has none (n prime). The
// success path returns a plain i64 computed by trial division — it widens to
// the declared `?i64` return type at the `return` site.
fn largest_divisor(n: i64) ?i64 {
    var d: i64 = n / 2;
    while (d >= 2) : (d = d - 1) {
        if (n % d == 0) {
            return d;               // plain i64 -> ?i64
        }
    }
    return null;
}

pub fn main() void {
    print(largest_divisor(12) orelse 0 - 1);   // 12 % 6 == 0 -> 6
    print(largest_divisor(13) orelse 0 - 1);   // prime -> null -> -1
    print(largest_divisor(9) orelse 0 - 1);    // 9 % 3 == 0 -> 3
}
