// arrays.ks — fixed-size arrays `[N]T` (v0.117).
//
// `[N]T` is a value type: assigning, passing or returning copies the whole
// array. Index with `a[i]` (bounds-checked — out of range panics). `a.len` is
// the compile-time length (a `usize`). Note there are no implicit integer
// conversions, so loop counters compared against `.len` are `usize`.

fn dot(a: [3]i32, b: [3]i32) i32 {
    var acc: i32 = 0;
    var i: usize = 0;
    while (i < a.len) : (i = i + 1) {
        acc = acc + a[i] * b[i];
    }
    return acc;
}

// Returns a reversed copy (arrays are values, so `out` is independent of `xs`).
fn reversed(xs: [4]i32) [4]i32 {
    var out: [4]i32 = [4]i32{ 0, 0, 0, 0 };
    var i: usize = 0;
    while (i < 4) : (i = i + 1) {
        out[3 - i] = xs[i];
    }
    return out;
}

pub fn main() i32 {
    print(dot([3]i32{ 1, 2, 3 }, [3]i32{ 4, 5, 6 }));   // 32  (4 + 10 + 18)

    var r: [4]i32 = reversed([4]i32{ 10, 20, 30, 40 });
    print(r[0]);   // 40
    print(r[3]);   // 10
    return 0;
}

test "dot and reverse" {
    expect(dot([3]i32{ 2, 0, 1 }, [3]i32{ 3, 9, 4 }) == 10);
    var r: [4]i32 = reversed([4]i32{ 1, 2, 3, 4 });
    expect(r[0] == 4);
    expect(r[3] == 1);
}
