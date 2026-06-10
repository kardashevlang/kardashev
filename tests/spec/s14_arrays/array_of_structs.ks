//SPEC: §14 the element type may be a struct — indexing yields the struct, composing with field access
//OUT: 12
//OUT: 20
//OUT: 6

const Point = struct { x: i64, y: i64 };

pub fn main() void {
    var pts: [3]Point = [3]Point{
        Point{ .x = 1, .y = 2 },
        Point{ .x = 3, .y = 5 },
        Point{ .x = 8, .y = 13 },
    };
    var sx: i64 = 0;
    var sy: i64 = 0;
    var i: usize = 0;
    while (i < pts.len) : (i = i + 1) {
        sx = sx + pts[i].x;     // `a[i].field` read
        sy = sy + pts[i].y;
    }
    print(sx);      // 1 + 3 + 8
    print(sy);      // 2 + 5 + 13
    pts[1] = Point{ .x = -3, .y = 0 };     // overwrite a whole element
    print(pts[0].x + pts[1].x + pts[2].x); // 1 - 3 + 8
}
