//SPEC: std BitSet×§29×§14 a `for` over an array drives BitSet membership — duplicates detected via has()-before-set(), count() = distinct values
//OUT: 3
//OUT: 4
//OUT: 1
//OUT: 0

@import("std");

pub fn main() void {
    var a: Allocator = c_allocator();
    var seen: BitSet = BitSet.init(a, 64);

    // 3 appears 3x, 9 appears 2x -> 3 duplicate sightings, 4 distinct values.
    var xs: [7]i64 = [7]i64{ 3, 9, 3, 27, 9, 50, 3 };
    var dups: i64 = 0;
    for (xs) |x| {
        var i: usize = @as(usize, x);
        if (seen.has(i)) {
            dups += 1;
        }
        seen.set(i);
    }
    print(dups);                  // 3
    print(seen.count());          // 4

    if (seen.has(27)) {
        print(1);                 // a member from the array
    }
    if (seen.has(4)) {
        print(999);
    } else {
        print(0);                 // never inserted
    }
    seen.deinit(a);
}
