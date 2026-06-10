//SPEC: §15.2×std fill — a view of a view composes offsets; std `fill` and an index write through the second-level view land in the backing array (documented aliasing)
//OUT: 1
//OUT: 2
//OUT: 3
//OUT: -9
//OUT: -9
//OUT: -9
//OUT: 7
//OUT: 8
//OUT: 42
//OUT: 42
//OUT: 42

@import("std");

pub fn main() void {
    var base: [8]i64 = [8]i64{ 1, 2, 3, 4, 5, 6, 7, 8 };
    var v1: []i64 = base[1..7];   // base indices 1..6
    var v2: []i64 = v1[2..5];     // re-slicing: base indices 3..5

    fill(i64, v2, 0 - 9);         // writes base[3], base[4], base[5]
    for (base) |x| {
        print(x);                 // 1 2 3 -9 -9 -9 7 8
    }

    var v3: []i64 = v2[1..3];     // third level: base indices 4..5
    v3[0] = 42;                   // base[4]
    print(base[4]);               // 42 in the array itself
    print(v1[3]);                 // 42 through the middle view (1 + 3)
    print(v2[1]);                 // 42 through its own parent view
}
