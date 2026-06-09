// documented.ks — `///` doc comments + `kard doc` (v0.140).
//
// Run `kard doc examples/documented.ks` to render this module's public API as
// Markdown. A `///` line documents the item directly below it; an ordinary `//`
// comment (like this header) is ignored by `kard doc`. Only `pub` items appear.

/// The largest signed 32-bit value, as a constant.
pub const I32_MAX: i32 = 2147483647;

/// A 2-D vector with integer components.
pub const Vec2 = struct {
    x: i32,
    y: i32,
    /// Add two vectors component-wise, returning a new `Vec2`.
    fn add(self: Vec2, o: Vec2) Vec2 {
        return Vec2{ .x = self.x + o.x, .y = self.y + o.y };
    }
};

/// Errors that a lookup can return.
pub const LookupErr = error{ Missing, OutOfRange };

/// Clamp `v` into the inclusive range `[lo, hi]`.
pub fn clamp(v: i32, lo: i32, hi: i32) i32 {
    if (v < lo) {
        return lo;
    }
    if (v > hi) {
        return hi;
    }
    return v;
}

// An internal helper — no `///`, and not `pub`, so `kard doc` omits it.
fn square(x: i32) i32 {
    return x * x;
}

pub fn main() i32 {
    var a: Vec2 = Vec2{ .x = 1, .y = 2 };
    var b: Vec2 = Vec2{ .x = 3, .y = 4 };
    var c: Vec2 = a.add(b);
    print(c.x);               // 4
    print(c.y);               // 6
    print(clamp(99, 0, 10));  // 10
    print(square(5));         // 25
    return 0;
}

test "documented" {
    expect(clamp(5, 0, 10) == 5);
    expect(clamp(0 - 3, 0, 10) == 0);
    expect(square(6) == 36);
}
