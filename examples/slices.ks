// slices.ks — pointers `*T` and slices `[]T` (v0.118).
//
// `&x` takes the address of an lvalue; `p.*` dereferences. A slice `[]T` is a
// `{ptr, len}` view into an array — `a[lo..hi]` — that aliases the backing
// storage; `s[i]` is bounds-checked and `s.len` is the view length.

fn swap(a: *i32, b: *i32) void {
    var t: i32 = a.*;
    a.* = b.*;
    b.* = t;
}

fn maxOf(s: []i32) i32 {
    var best: i32 = s[0];
    var i: usize = 1;
    while (i < s.len) : (i = i + 1) {
        if (s[i] > best) {
            best = s[i];
        }
    }
    return best;
}

pub fn main() i32 {
    var x: i32 = 3;
    var y: i32 = 7;
    swap(&x, &y);
    print(x);                 // 7
    print(y);                 // 3

    var data: [6]i32 = [6]i32{ 4, 1, 9, 2, 8, 5 };
    var window: []i32 = data[1..5];   // {1, 9, 2, 8}
    print(window.len);        // 4
    print(maxOf(window));     // 9
    return 0;
}

test "swap and max" {
    var a: i32 = 1;
    var b: i32 = 2;
    swap(&a, &b);
    expect(a == 2);
    expect(b == 1);
    var xs: [4]i32 = [4]i32{ 5, 30, 5, 12 };
    expect(maxOf(xs[0..4]) == 30);
}
