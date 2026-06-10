//SPEC: §10 a method receiver may be a field-access chain (`a.b.method()`)
//OUT: 3
//OUT: 13
//OUT: 10
const Point = struct {
    x: i32,
    y: i32,

    fn manhattan(self: Point) i32 {
        return self.x + self.y;
    }
};

const Seg = struct {
    from: Point,
    to: Point,
};

pub fn main() void {
    var s: Seg = Seg{
        .from = Point{ .x = 1, .y = 2 },
        .to = Point{ .x = 4, .y = 9 },
    };
    print(s.from.manhattan());                       // 3
    print(s.to.manhattan());                         // 13
    print(s.to.manhattan() - s.from.manhattan());    // 10
}
