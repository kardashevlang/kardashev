//SPEC: §39 `lo..hi` matches when the scrutinee is in [lo, hi] — BOTH endpoints included
//OUT: 0
//OUT: 1
//OUT: 1
//OUT: 1
//OUT: 0

fn in_range(n: i64) i64 {
    switch (n) {
        10..13 => { return 1; },
        else => { return 0; },
    }
}

pub fn main() void {
    print(in_range(9));   // just below lo -> else
    print(in_range(10));  // lo itself MATCHES
    print(in_range(11));  // interior
    print(in_range(13));  // hi itself MATCHES (inclusive, unlike a slice bound)
    print(in_range(14));  // just above hi -> else
}
