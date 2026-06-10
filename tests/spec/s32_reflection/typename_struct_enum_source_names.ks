//SPEC: §32.1 `@typeName` of a user struct or enum is its declared source name
//OUT: Point
//OUT: Color
const Point = struct {
    x: i64,
    y: i64,
};

const Color = enum {
    Red,
    Green,
    Blue,
};

pub fn main() void {
    print(@typeName(Point));
    print(@typeName(Color));
}
