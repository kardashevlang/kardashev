// inference.ks — type inference for `var`/`const` (v0.121).
//
// The `: T` annotation is optional: the type is inferred from the initializer.
// Annotations are still allowed (and required where a value needs context, like
// a bare `null`). There are still no implicit conversions — inferred types are
// concrete, so `var i = 0;` is `i64` and mixing it with an `i32` is an error.

const GREETING_LEN = 5;          // inferred i64

const Vec2 = struct { x: i32, y: i32 };

fn add(a: Vec2, b: Vec2) Vec2 {
    return Vec2{ .x = a.x + b.x, .y = a.y + b.y };
}

pub fn main() i32 {
    var total = 0;               // i64
    var i = 0;                   // i64
    while (i < GREETING_LEN) : (i = i + 1) {
        total = total + i;
    }
    print(total);                // 10

    var p = add(Vec2{ .x = 1, .y = 2 }, Vec2{ .x = 3, .y = 4 });  // inferred Vec2
    print(p.x);                  // 4
    print(p.y);                  // 6

    var doubled = total * 2;     // i64
    print(doubled);              // 20
    return 0;
}

test "inference" {
    var a = 21;
    var b = a + a;               // inferred i64
    expect(b == 42);
    var v = add(Vec2{ .x = 10, .y = 20 }, Vec2{ .x = 5, .y = 5 });
    expect(v.x == 15);
    expect(v.y == 25);
}
