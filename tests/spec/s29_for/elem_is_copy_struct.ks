//SPEC: §29.1 `elem` binds each element BY VALUE — zeroing the current element through the (data-sharing) slice does not change the copy
//OUT: 42
//OUT: 0
//OUT: 0

const P = struct {
    x: i64,
    y: i64,
};

pub fn main() void {
    var arr: [2]P = [2]P{ P{ .x = 3, .y = 4 }, P{ .x = 5, .y = 6 } };
    var s: []P = arr[0..2];      // a slice, so element writes DO reach arr
    var sum: i64 = 0;
    for (s, 0..) |p, i| {
        s[i].x = 0;              // clobber the CURRENT element first...
        sum += p.x * p.y;        // ...the copy `p` still holds the original
    }
    print(sum);                  // 3*4 + 5*6 = 42, NOT 0
    print(arr[0].x);             // 0 — the clobbers landed on the array
    print(arr[1].x);             // 0
}
