//SPEC: std Rng×shuffle×is_sorted — Fisher-Yates with xorshift64* is fully deterministic per seed: seed 42 over 1..8 yields one pinned permutation, reproducibly
//OUT: 1
//OUT: 2
//OUT: 5
//OUT: 4
//OUT: 6
//OUT: 7
//OUT: 3
//OUT: 8
//OUT: 1
//OUT: 36
//OUT: 0
//OUT: 8
//OUT: 777

@import("std");

// The expected permutation [2,5,4,6,7,3,8,1] is derived INDEPENDENTLY from
// the documented contract: xorshift64* (x^=x>>12; x^=x<<25; x^=x>>27;
// out = x * 2685821657736338717 mod 2^64) drawing j = out % (i+1) for
// i = 7..1 (the documented Fisher-Yates loop), seed state 42.
pub fn main() void {
    var xs: [8]i64 = [8]i64{ 1, 2, 3, 4, 5, 6, 7, 8 };
    var s: []i64 = xs[0..8];
    if (is_sorted(i64, s)) {
        print(1);                 // sorted before
    } else {
        print(0);
    }

    var r: Rng = Rng.init(42);
    shuffle(i64, &r, s);
    for (s) |x| {
        print(x);                 // 2 5 4 6 7 3 8 1
    }
    print(sum64(s));              // a permutation keeps the sum: 36
    if (is_sorted(i64, s)) {
        print(1);
    } else {
        print(0);                 // not sorted after this shuffle
    }

    // Same seed, fresh generator, fresh array: identical permutation.
    var ys: [8]i64 = [8]i64{ 1, 2, 3, 4, 5, 6, 7, 8 };
    var r2: Rng = Rng.init(42);
    shuffle(i64, &r2, ys[0..8]);
    var same: i64 = 0;
    for (ys, 0..) |y, i| {
        if (y == s[i]) {
            same += 1;
        }
    }
    print(same);                  // 8 — every position matches

    sort(i64, s);
    if (is_sorted(i64, s)) {
        print(777);               // sorting restores order
    }
}
