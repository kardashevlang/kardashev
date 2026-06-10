//SPEC: std BitSet×§29 sets built from arrays via `for` combine with union_with/intersect_with/difference_with — counts and membership follow set algebra
//OUT: 7
//OUT: 2
//OUT: 3
//OUT: 2

@import("std");

fn build(a: Allocator, xs: []i64) BitSet {
    var s: BitSet = BitSet.init(a, 16);
    for (xs) |x| {
        s.set(@as(usize, x));
    }
    return s;
}

pub fn main() void {
    var a: Allocator = c_allocator();
    var xa: [5]i64 = [5]i64{ 1, 3, 5, 7, 9 };
    var xb: [4]i64 = [4]i64{ 3, 6, 9, 12 };
    var sb: BitSet = build(a, xb[0..4]);

    var su: BitSet = build(a, xa[0..5]);
    su.union_with(sb);
    print(su.count());      // {1,3,5,6,7,9,12} -> 7

    var si: BitSet = build(a, xa[0..5]);
    si.intersect_with(sb);
    print(si.count());      // {3,9} -> 2

    var sd: BitSet = build(a, xa[0..5]);
    sd.difference_with(sb);
    print(sd.count());      // {1,5,7} -> 3

    // Verify the intersection through another for-over-array probe.
    var probe: [4]i64 = [4]i64{ 1, 3, 9, 12 };
    var members: i64 = 0;
    for (probe) |p| {
        if (si.has(@as(usize, p))) {
            members += 1;
        }
    }
    print(members);         // 3 and 9 only -> 2

    su.deinit(a);
    si.deinit(a);
    sd.deinit(a);
    sb.deinit(a);
}
