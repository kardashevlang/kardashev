// point.ks — structs (v0.112): declarations, literals, field access,
// field assignment, by-value params/returns, and nesting.

const Point = struct {
    x: i32,
    y: i32,
};

const Rect = struct {
    origin: Point,
    width: i32,
    height: i32,
};

fn area(r: Rect) i32 {
    return r.width * r.height;
}

fn translated(p: Point, dx: i32, dy: i32) Point {
    return Point{ .x = p.x + dx, .y = p.y + dy };
}

pub fn main() i32 {
    var r: Rect = Rect{
        .origin = Point{ .x = 0, .y = 0 },
        .width = 4,
        .height = 5,
    };
    print(area(r));              // 20

    // Move the rectangle's origin via a by-value helper, then field assignment.
    r.origin = translated(r.origin, 3, 7);
    print(r.origin.x);           // 3
    print(r.origin.y);           // 7

    r.width = r.width + 6;
    print(area(r));              // 50
    return 0;
}

test "area and translation" {
    var r: Rect = Rect{ .origin = Point{ .x = 1, .y = 1 }, .width = 2, .height = 3 };
    expect(area(r) == 6);
    var p: Point = translated(r.origin, 10, 20);
    expect(p.x == 11);
    expect(p.y == 21);
}
