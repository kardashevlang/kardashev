// unions.ks — tagged unions `union(enum)` + `switch` payload capture (v0.124).
//
// A tagged union holds exactly one of its typed variants. `switch` matches the
// active variant and `|x|` captures its payload — no hidden tag checks.

const Point = struct { x: i64, y: i64 };

const Shape = union(enum) {
    circle: i64,    // radius
    rect: Point,    // width/height
    line: i64,      // length
};

fn area(s: Shape) i64 {
    switch (s) {
        .circle => |r| {
            return 3 * r * r;     // ~pi*r^2 (pi ≈ 3)
        },
        .rect => |p| {
            return p.x * p.y;
        },
        .line => |len| {
            return 0;             // a line has no area
        },
    }
}

pub fn main() i32 {
    var c: Shape = Shape{ .circle = 10 };
    print(area(c));                                  // 300
    var r: Shape = Shape{ .rect = Point{ .x = 4, .y = 5 } };
    print(area(r));                                  // 20
    var l: Shape = Shape{ .line = 99 };
    print(area(l));                                  // 0
    return 0;
}

test "shape area" {
    expect(area(Shape{ .circle = 2 }) == 12);
    expect(area(Shape{ .rect = Point{ .x = 3, .y = 7 } }) == 21);
    expect(area(Shape{ .line = 5 }) == 0);
}
