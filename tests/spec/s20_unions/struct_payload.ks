//SPEC: §20.1 a variant payload may be a struct; the capture binds the whole struct value
//OUT: 42
//OUT: 81

const Vec2 = struct { x: i64, y: i64 };

const Shape = union(enum) {
    rect: Vec2,
    square: i64,
};

fn area(s: Shape) i64 {
    switch (s) {
        .rect => |v| {
            return v.x * v.y;       // field access on the captured struct
        },
        .square => |side| {
            return side * side;
        },
    }
}

pub fn main() void {
    var w: i64 = 2 + 4;
    var r: Shape = Shape{ .rect = Vec2{ .x = w, .y = w + 1 } };
    print(area(r));                             // 6 * 7 = 42
    print(area(Shape{ .square = 3 * 3 }));      // 9 * 9 = 81
}
