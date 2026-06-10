// tests/std/slices.ks — std slices module tests (v0.154 WAVE 1).
//
// Exercises every public fn: sort, reverse, binary_search, index_of_elem,
// contains, fill, copy_into, sum64, min_in, max_in, is_sorted.

@import("std");

test "sort: empty and single element" {
    var b: [1]i64 = [1]i64{42};
    sort(i64, b[0..0]);            // empty: must not crash
    expect(is_sorted(i64, b[0..0]));
    sort(i64, b[0..1]);            // single
    expect(b[0] == 42);
    expect(is_sorted(i64, b[0..1]));
}

test "sort: two elements, both orders" {
    var p: [2]i64 = [2]i64{ 9, 0 - 4 };
    sort(i64, p[0..2]);
    expect(p[0] == (0 - 4));
    expect(p[1] == 9);
    var q: [2]i64 = [2]i64{ 1, 2 };
    sort(i64, q[0..2]);
    expect(q[0] == 1);
    expect(q[1] == 2);
}

test "sort: all-equal duplicates" {
    var d: [5]i64 = [5]i64{ 7, 7, 7, 7, 7 };
    sort(i64, d[0..5]);
    expect(is_sorted(i64, d[0..5]));
    expect(d[0] == 7);
    expect(d[4] == 7);
    expect(sum64(d[0..5]) == 35);
}

test "sort: reverse-sorted 20 (quicksort path)" {
    var d: [20]i64 = [20]i64{ 20, 19, 18, 17, 16, 15, 14, 13, 12, 11, 10, 9, 8, 7, 6, 5, 4, 3, 2, 1 };
    sort(i64, d[0..20]);
    expect(is_sorted(i64, d[0..20]));
    expect(d[0] == 1);
    expect(d[10] == 11);
    expect(d[19] == 20);
    expect(sum64(d[0..20]) == 210);    // 20*21/2
}

test "sort: sawtooth 50" {
    var a: Allocator = c_allocator();
    var xs: []i64 = alloc(a, i64, 50);
    var i: usize = 0;
    while (i < 50) : (i += 1) {
        xs[i] = @as(i64, i) % 7;       // 0..6 repeating; 0 appears 8 times
    }
    sort(i64, xs);
    expect(is_sorted(i64, xs));
    expect(xs[0] == 0);
    expect(xs[7] == 0);                // eighth zero
    expect(xs[8] == 1);                // first one
    expect(xs[49] == 6);
    expect(sum64(xs) == 147);          // 8*0 + 7*(1+..+6) = 7*21
    free(a, xs);
}

test "sort: 200-element pseudo-random (property)" {
    var a: Allocator = c_allocator();
    var xs: []i64 = alloc(a, i64, 200);
    var seed: i64 = 12345;
    var total: i64 = 0;
    var i: usize = 0;
    while (i < 200) : (i += 1) {
        seed = (seed * 1103515245 + 12345) % 2147483648;   // LCG, stays in [0, 2^31)
        xs[i] = seed % 1000;
        total += xs[i];
    }
    var mn: i64 = min_in(xs);
    var mx: i64 = max_in(xs);
    sort(i64, xs);
    expect(is_sorted(i64, xs));        // ascending …
    expect(sum64(xs) == total);        // … and the same multiset sum
    expect(xs[0] == mn);
    expect(xs[199] == mx);
    free(a, xs);
}

test "sort: i32 and u8 instantiations" {
    var v: [6]i32 = [6]i32{ 0 - 3, 7, 0 - 3, 0, 5, 0 - 10 };
    sort(i32, v[0..6]);
    expect(is_sorted(i32, v[0..6]));
    expect(v[0] == (0 - 10));
    expect(v[1] == (0 - 3));
    expect(v[2] == (0 - 3));
    expect(v[3] == 0);
    expect(v[4] == 5);
    expect(v[5] == 7);
    var s: [5]u8 = [5]u8{ 5, 3, 9, 1, 3 };
    sort(u8, s[0..5]);
    expect(s[0] == 1);
    expect(s[1] == 3);
    expect(s[2] == 3);
    expect(s[3] == 5);
    expect(s[4] == 9);
}

test "reverse: odd, even, single, empty" {
    var r: [5]i64 = [5]i64{ 1, 2, 3, 4, 5 };
    reverse(i64, r[0..5]);
    expect(r[0] == 5);
    expect(r[2] == 3);
    expect(r[4] == 1);
    var e: [4]i64 = [4]i64{ 1, 2, 3, 4 };
    reverse(i64, e[0..4]);
    expect(e[0] == 4);
    expect(e[1] == 3);
    expect(e[2] == 2);
    expect(e[3] == 1);
    var s: [1]i64 = [1]i64{9};
    reverse(i64, s[0..1]);
    expect(s[0] == 9);
    reverse(i64, s[0..0]);             // empty: must not crash
    expect(s[0] == 9);
}

test "binary_search: hit first/last/middle, misses, empty" {
    var d: [6]i64 = [6]i64{ 1, 3, 5, 7, 9, 11 };
    expect(binary_search(i64, d[0..6], 1) == 0);     // first
    expect(binary_search(i64, d[0..6], 11) == 5);    // last
    expect(binary_search(i64, d[0..6], 7) == 3);
    expect(binary_search(i64, d[0..6], 5) == 2);
    expect(binary_search(i64, d[0..6], 0) == (0 - 1));   // below all
    expect(binary_search(i64, d[0..6], 4) == (0 - 1));   // between
    expect(binary_search(i64, d[0..6], 12) == (0 - 1));  // above all
    expect(binary_search(i64, d[0..0], 1) == (0 - 1));   // empty
}

test "index_of_elem and contains" {
    var d: [5]i64 = [5]i64{ 4, 0 - 2, 7, 0 - 2, 4 };
    expect(index_of_elem(i64, d[0..5], 4) == 0);         // first of dups
    expect(index_of_elem(i64, d[0..5], 0 - 2) == 1);     // negative needle
    expect(index_of_elem(i64, d[0..5], 7) == 2);
    expect(index_of_elem(i64, d[0..5], 9) == (0 - 1));   // absent
    expect(index_of_elem(i64, d[0..0], 4) == (0 - 1));   // empty
    expect(contains(i64, d[0..5], 0 - 2));
    expect(contains(i64, d[0..5], 4));
    expect(!contains(i64, d[0..5], 0));
    expect(!contains(i64, d[0..0], 4));
}

test "fill: whole slice and aliased sub-slice" {
    var f: [4]i64 = [4]i64{ 1, 2, 3, 4 };
    fill(i64, f[0..4], 0 - 7);
    expect(f[0] == (0 - 7));
    expect(f[3] == (0 - 7));
    expect(sum64(f[0..4]) == (0 - 28));
    var g: [4]i64 = [4]i64{ 1, 2, 3, 4 };
    fill(i64, g[1..3], 9);             // writes through the view
    expect(g[0] == 1);
    expect(g[1] == 9);
    expect(g[2] == 9);
    expect(g[3] == 4);
    fill(i64, g[0..0], 5);             // empty: no-op
    expect(g[0] == 1);
}

test "copy_into: equal, shorter dst, shorter src" {
    var src: [3]i64 = [3]i64{ 1, 2, 3 };
    var dst: [5]i64 = [5]i64{ 0, 0, 0, 0, 0 };
    copy_into(i64, dst[0..5], src[0..3]);    // src shorter: tail untouched
    expect(dst[0] == 1);
    expect(dst[1] == 2);
    expect(dst[2] == 3);
    expect(dst[3] == 0);
    expect(dst[4] == 0);
    var d2: [2]i64 = [2]i64{ 0, 0 };
    copy_into(i64, d2[0..2], src[0..3]);     // dst shorter: truncated
    expect(d2[0] == 1);
    expect(d2[1] == 2);
    var d3: [3]i64 = [3]i64{ 9, 9, 9 };
    copy_into(i64, d3[0..3], src[0..3]);     // equal lengths
    expect(d3[0] == 1);
    expect(d3[2] == 3);
    copy_into(i64, d3[0..0], src[0..3]);     // empty dst: no-op
    expect(d3[0] == 1);
}

test "sum64: empty, single, cancelling negatives" {
    var b: [3]i64 = [3]i64{ 0 - 1, 0 - 2, 3 };
    expect(sum64(b[0..0]) == 0);             // empty -> 0
    expect(sum64(b[2..3]) == 3);             // single
    expect(sum64(b[0..3]) == 0);             // -1 + -2 + 3
}

test "min_in/max_in: values, extremes, empty sentinels" {
    var b: [4]i64 = [4]i64{ 0 - 5, 2, 0 - 9, 4 };
    expect(min_in(b[0..4]) == (0 - 9));
    expect(max_in(b[0..4]) == 4);
    expect(min_in(b[1..2]) == 2);            // single
    expect(max_in(b[1..2]) == 2);
    var x: [2]i64 = [2]i64{ 9223372036854775807, (0 - 9223372036854775807) - 1 };
    expect(min_in(x[0..2]) == ((0 - 9223372036854775807) - 1));
    expect(max_in(x[0..2]) == 9223372036854775807);
    expect(min_in(b[0..0]) == 9223372036854775807);            // empty -> i64 max
    expect(max_in(b[0..0]) == ((0 - 9223372036854775807) - 1)); // empty -> i64 min
}

test "is_sorted: trivial, dups, unsorted" {
    var d: [4]i64 = [4]i64{ 1, 1, 2, 2 };
    expect(is_sorted(i64, d[0..0]));         // empty
    expect(is_sorted(i64, d[0..1]));         // single
    expect(is_sorted(i64, d[0..4]));         // non-decreasing dups
    var u: [2]i64 = [2]i64{ 2, 1 };
    expect(!is_sorted(i64, u[0..2]));
    var w: [4]i64 = [4]i64{ 1, 2, 3, 2 };
    expect(!is_sorted(i64, w[0..4]));        // late dip
    var s: [3]u8 = [3]u8{ 3, 1, 2 };
    expect(!is_sorted(u8, s[0..3]));         // other instantiation
}
