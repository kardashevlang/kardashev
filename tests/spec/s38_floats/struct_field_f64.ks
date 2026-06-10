//SPEC: §38 `f64` works as a struct field type — init, read, write, pass through a fn
//OUT: 3.75
//OUT: 10.25

const Point = struct {
    x: f64,
    y: f64,
};

fn span(p: Point) f64 {
    return p.x + p.y;
}

pub fn main() void {
    var pt: Point = Point{ .x = 1.5, .y = 2.25 };
    print(span(pt));         // 3.75
    pt.x = 8.0;              // field write
    print(span(pt));         // 8.0 + 2.25 = 10.25
}
