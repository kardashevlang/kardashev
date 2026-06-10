//SPEC: §15.2 slicing an array reached through an index (`xs[i].buf[lo..hi]`) views the element's REAL storage, not a temporary copy
//OUT: 99
//OUT: 3
const B = struct {
    buf: [3]i64,
};

pub fn main() void {
    var xs: [2]B = [2]B{ B{ .buf = [3]i64{ 1, 2, 3 } }, B{ .buf = [3]i64{ 4, 5, 6 } } };
    var v: []i64 = xs[0].buf[0..3];
    v[1] = 99;
    print(xs[0].buf[1]);   // 99 — the view aims at xs[0]'s array in place
    print(v[2]);           // 3
}
