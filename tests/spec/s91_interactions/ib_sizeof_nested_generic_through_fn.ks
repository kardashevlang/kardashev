//SPEC: §32.1×§17×§26 a generic fn's `@sizeOf(T)` measures generic-struct instances — including a NESTED ArrayList(ArrayList(i32)), whose header size is element-independent
//OUT: 16
//OUT: 1
//OUT: 8
//OUT: 2

@import("std");

fn tsize(comptime T: type) usize {
    return @sizeOf(T);
}

fn Pair(comptime T: type) type {
    return struct {
        a: T,
        b: T,
    };
}

const P = Pair(i64);
const L = ArrayList(i32);
const LL = ArrayList(L);     // generic composition: a list of lists

pub fn main() void {
    print(tsize(P));         // 2 * 8 = 16, exact (same-type fields, no padding)

    // An ArrayList header is { items: []V, count: usize } — its size cannot
    // depend on V, so the nested instance equals the flat one.
    if (tsize(LL) == tsize(L)) {
        print(1);
    } else {
        print(0);
    }

    // The nested monomorph is real at runtime: a list of lists round-trips.
    var a: Allocator = c_allocator();
    var inner: L = L.init(a);
    inner.push(a, 7);
    inner.push(a, 8);
    var outer: LL = LL.init(a);
    outer.push(a, inner);
    var got: L = outer.get(0);
    print(got.get(1));       // 8
    print(got.len());        // 2
    inner.deinit(a);
    outer.deinit(a);
}
