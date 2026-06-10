//SPEC: §15.2×std fill — two OVERLAPPING sibling views of one array alias the shared cells: a fill through one and an index write through the other are mutually visible
//OUT: 0
//OUT: 0
//OUT: 77
//OUT: 77
//OUT: 4
//OUT: 4

@import("std");

pub fn main() void {
    var base: [6]i64 = [6]i64{ 1, 2, 3, 4, 5, 6 };
    var left: []i64 = base[0..4];    // base 0..3
    var right: []i64 = base[2..6];   // base 2..5 — overlaps left on 2..3

    fill(i64, right, 0);             // zeroes base[2..5]
    print(left[2]);                  // 0 — the overlap, seen from the sibling
    print(left[3]);                  // 0

    left[3] = 77;                    // base[3], inside the overlap
    print(right[1]);                 // 77 — visible from the other view
    print(base[3]);                  // 77 — and in the array

    print(left.len);                 // views keep their own extents: 4
    print(right.len);                // 4
}
