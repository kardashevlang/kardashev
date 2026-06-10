//SPEC: §21.1 the capture binds the optional's inner type — a struct capture supports field access
//OUT: 12
//OUT: -1

const Point = struct { x: i64, y: i64 };

fn find_corner(k: i64) ?Point {
    if (k > 0) {
        return Point{ .x = k, .y = k + 1 };
    }
    return null;
}

pub fn main() void {
    if (find_corner(3)) |p| {
        print(p.x * p.y);       // 3 * 4 = 12 — `p` is a full Point
    } else {
        print(0);
    }

    if (find_corner(0 - 2)) |p| {
        print(p.x);
    } else {
        print(0 - 1);           // null path
    }
}
