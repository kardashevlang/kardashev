//SPEC: §14.3 the lvalue (`_at`) index-place path bounds-checks exactly like a read: an out-of-bounds index in `arr[i].f = e` panics (exit 101)
//EXIT: 101
const P = struct {
    x: i64,
};

pub fn main() void {
    var arr: [2]P = [2]P{ P{ .x = 1 }, P{ .x = 2 } };
    var i: i64 = 2;
    arr[i].x = 9;      // i == len → "panic: array index out of bounds", exit 101
    print(arr[0].x);   // never reached
}
