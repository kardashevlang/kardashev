// floats.ks — 64-bit floating point `f64` (v0.144).
//
// `f64` is the first non-integer scalar: literals like `3.14`, the arithmetic
// `+ - * /` and the comparisons, `print`, and `@as` casts between integers and
// floats. There is no implicit int<->float mixing — cast explicitly with `@as`.

fn average(xs: []f64) f64 {
    var total: f64 = 0.0;
    for (xs) |v| {
        total = total + v;
    }
    return total / @as(f64, xs.len);
}

// Newton's method for a square root (fixed iteration count).
fn sqrt(x: f64) f64 {
    var guess: f64 = x / 2.0;
    var i: i32 = 0;
    while (i < 20) : (i += 1) {
        guess = (guess + x / guess) / 2.0;
    }
    return guess;
}

pub fn main() i32 {
    var data: [4]f64 = [4]f64{ 2.0, 4.0, 6.0, 8.0 };
    print(average(data[0..4]));     // 5
    print(sqrt(2.0));               // ~1.41421
    print(sqrt(16.0));              // 4

    // int <-> float round-trip via @as.
    var n: i32 = 7;
    var half: f64 = @as(f64, n) / 2.0;
    print(half);                    // 3.5
    print(@as(i32, half));          // 3 (truncates toward zero)

    // Comparisons.
    if (sqrt(9.0) > 2.9) { print(1); } else { print(0); }   // 1
    return 0;
}

test "floats" {
    var d: [3]f64 = [3]f64{ 1.0, 2.0, 3.0 };
    expect(average(d[0..3]) == 2.0);
    expect(@as(i32, 9.99) == 9);
    var r: f64 = sqrt(25.0);
    expect(r > 4.99);
    expect(r < 5.01);
}
