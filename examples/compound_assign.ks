// compound_assign.ks — compound assignment operators (v0.131).
//
//   place += rhs    place -= rhs    place *= rhs    place /= rhs    place %= rhs
//
// means `place = place op rhs`, with the place evaluated once — so a compound
// assignment to `a[i]` evaluates `i` a single time. Works on any assignable
// place: a `var`, a struct field, or an array/slice element.

const Point = struct { x: i32, y: i32 };

fn factorial(n: i32) i32 {
    var acc: i32 = 1;
    var i: i32 = 2;
    while (i <= n) : (i += 1) {
        acc *= i;
    }
    return acc;
}

pub fn main() i32 {
    var x: i32 = 100;
    x += 11;   print(x);   // 111
    x -= 1;    print(x);   // 110
    x /= 10;   print(x);   // 11
    x %= 4;    print(x);   // 3

    // Field place.
    var p: Point = Point{ .x = 5, .y = 5 };
    p.x *= 8;
    p.y += 2;
    print(p.x);            // 40
    print(p.y);            // 7

    // Element place — accumulate into a histogram bucket.
    var hist: [4]i32 = [4]i32{ 0, 0, 0, 0 };
    var i: usize = 0;
    while (i < 10) : (i += 1) {
        hist[i % 4] += 1;
    }
    print(hist[0]);        // 3  (indices 0,4,8)
    print(hist[1]);        // 3  (1,5,9)
    print(hist[3]);        // 2  (3,7)

    print(factorial(5));   // 120
    return 0;
}

test "compound assignment" {
    var n: i32 = 7;
    n += 3;                // 10
    n *= 2;                // 20
    n -= 5;                // 15
    n /= 3;                // 5
    expect(n == 5);
    expect(factorial(6) == 720);
}
