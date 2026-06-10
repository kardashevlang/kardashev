//SPEC: §11.1 `x.?` force-unwraps a non-null `?T` to its inner `T`
//OUT: 10
//OUT: 10
//OUT: 0

// Triangle numbers through an optional return: each unwrap yields a plain
// i64 directly usable in arithmetic.
fn triangle(n: i64) ?i64 {
    if (n < 0) {
        return null;
    }
    var t: i64 = 0;
    var i: i64 = 1;
    while (i <= n) : (i = i + 1) {
        t = t + i;
    }
    return t;
}

pub fn main() void {
    print(triangle(4).?);                          // 1+2+3+4 = 10
    var v: i64 = triangle(10).? - triangle(9).?;   // 55 - 45 = 10
    print(v);
    print(triangle(0).?);   // 0 — a present-but-zero payload is not null
}
