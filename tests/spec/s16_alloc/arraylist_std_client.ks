//SPEC: §16 std's ArrayList(T) is an Allocator client — every push/deinit goes through the explicitly passed allocator
//OUT: 6
//OUT: 91

@import("std");

pub fn main() void {
    var a: Allocator = c_allocator();
    var l: ArrayList(i64) = ArrayList(i64).init(a);

    var k: i64 = 1;
    while (k <= 6) : (k += 1) {
        l.push(a, k * k); // 1 4 9 16 25 36 — growth reallocates via `a`
    }
    print(l.len());

    var sum: i64 = 0;
    var i: usize = 0;
    while (i < l.len()) : (i += 1) {
        sum = sum + l.get(i);
    }
    print(sum); // 1+4+9+16+25+36

    l.deinit(a);
}
