//SPEC: §25.2 the type parameter substitutes throughout the fields — `?T`, `[3]T` and `[]T` field types all become concrete
//OUT: 44
//OUT: 2
//OUT: 8

// One generic struct whose fields wrap T three different ways; every wrapper
// must substitute (the optional unwraps, the array indexes, the slice carries
// len + elements).
fn Holder(comptime T: type) type {
    return struct { v: ?T, arr: [3]T, s: []T };
}

const H = Holder(i64);

pub fn main() void {
    var backing: [3]i64 = [3]i64{ 9, 8, 7 };
    var h: H = H{ .v = 41, .arr = [3]i64{ 1, 2, 3 }, .s = backing[0..2] };
    if (h.v) |x| {
        print(x + h.arr[2]); // 41 + 3 = 44
    }
    print(h.s.len); // 2
    print(h.s[1]); // 8
}
