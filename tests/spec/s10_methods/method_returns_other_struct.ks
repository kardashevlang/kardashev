//SPEC: §10 a method may return any struct type — the result chains into that type's own methods
//OUT: 12
//OUT: 23
//OUT: 35
const Point = struct {
    x: i32,
    y: i32,

    fn manhattan(self: Point) i32 {
        return self.x + self.y;
    }
};

const Rect = struct {
    x0: i32,
    y0: i32,
    w: i32,
    h: i32,

    fn corner(self: Rect) Point {
        return Point{ .x = self.x0 + self.w, .y = self.y0 + self.h };
    }
};

pub fn main() void {
    var r: Rect = Rect{ .x0 = 2, .y0 = 3, .w = 10, .h = 20 };
    print(r.corner().x);             // 12
    print(r.corner().y);             // 23
    print(r.corner().manhattan());   // 35 — a Rect method result drives a Point method
}
