//SPEC: §24 x §29 — comptime-`n`-sized arrays flow out of a generic and into `for` (with and without `, 0..`)
//OUT: 158
//OUT: 2016

// `scaled` is monomorphised at n=3 and n=4; `[n]i64` is both its parameter
// and return type, and `n` is also a runtime-usable VALUE in the body.
fn scaled(comptime n: usize, xs: [n]i64, k: i64) [n]i64 {
    var ys: [n]i64 = xs;             // array copy of the comptime-sized param
    var i: usize = 0;
    while (i < n) : (i += 1) {
        ys[i] = xs[i] * k + @as(i64, n);
    }
    return ys;
}

pub fn main() void {
    var a3: [3]i64 = scaled(3, [3]i64{ 1, 2, 3 }, 10);     // {13, 23, 33}
    var a4: [4]i64 = scaled(4, [4]i64{ 2, 4, 6, 8 }, 100); // {204, 404, 604, 804}

    // Indexed for over the n=3 result: 13*1 + 23*2 + 33*3 = 158.
    var w: i64 = 0;
    for (a3, 0..) |e, ix| {
        w += e * (@as(i64, ix) + 1);
    }
    print(w);

    // Plain for over the n=4 result: 204+404+604+804 = 2016.
    var s: i64 = 0;
    for (a4) |e| {
        s += e;
    }
    print(s);
}
