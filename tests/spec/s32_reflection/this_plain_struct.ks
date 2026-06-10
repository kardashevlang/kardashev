//SPEC: §32.2 `@This()` denotes the enclosing struct type in a PLAIN struct — usable as `*@This()` receiver, as a return type, and `Self` is bound in plain methods too
//OUT: 3
//OUT: 4
//OUT: 7
const Point = struct {
    x: i64,
    y: i64,

    fn translate(self: *@This(), dx: i64, dy: i64) void {
        self.x += dx;          // @This() desugared to Self = Point: real mutation
        self.y += dy;
    }

    fn origin() @This() {      // @This() in return-type position
        return Point{ .x = 0, .y = 0 };
    }

    fn sum(self: Self) i64 {   // bare Self also bound in a plain struct (v0.136)
        return self.x + self.y;
    }
};

pub fn main() void {
    var p: Point = Point.origin();
    p.translate(3, 4);
    print(p.x);
    print(p.y);
    print(p.sum());
}
