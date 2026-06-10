//SPEC: §10 a method receiver may be any struct-valued expression — a literal, an associated-call result, or a call result
//OUT: 42
//OUT: 0
//OUT: 42
const Point = struct {
    x: i32,
    y: i32,

    fn manhattan(self: Point) i32 {
        return self.x + self.y;
    }

    fn origin() Point {
        return Point{ .x = 0, .y = 0 };
    }
};

fn made(k: i32) Point {
    return Point{ .x = k, .y = k * k };
}

pub fn main() void {
    print(Point{ .x = 20, .y = 22 }.manhattan());   // 42 — literal receiver
    print(Point.origin().manhattan());              // 0 — associated-call receiver
    print(made(6).manhattan());                     // 6 + 36 = 42 — free-call receiver
}
