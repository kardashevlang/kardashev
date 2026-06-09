// use_std.ks — using the bundled standard library (v0.145).
//
// `@import("std");` pulls the std library into scope (it is embedded in the
// compiler, not a file you ship). Its public items — `ArrayList(T)`,
// `HashMap(V)`, `imin`/`imax`/`iabs` — are then available by bare name.
// Instantiate a generic container with a `const` type alias, as usual.

@import("std");

const IntList = ArrayList(i32);
const IntMap = HashMap(i32);

// Count occurrences of each value in `xs` using the std HashMap.
fn histogram(a: Allocator, xs: IntList) IntMap {
    var counts: IntMap = IntMap.init(a);
    var i: usize = 0;
    while (i < xs.len()) : (i += 1) {
        var k: i32 = xs.get(i);
        counts.put(a, k, counts.get(k, 0) + 1);
    }
    return counts;
}

pub fn main() i32 {
    var a: Allocator = c_allocator();

    var nums: IntList = IntList.init(a);
    var i: i32 = 0;
    while (i < 7) : (i += 1) {
        nums.push(a, i % 3);          // 0 1 2 0 1 2 0
    }
    print(nums.len());                // 7
    print(imax(nums.get(0), nums.get(2)));   // 2
    print(iabs(0 - 5));               // 5

    var h: IntMap = histogram(a, nums);
    print(h.get(0, 0));               // 3  (0 appears at i=0,3,6)
    print(h.get(1, 0));               // 2
    print(h.get(2, 0));               // 2
    print(h.len());                   // 3 distinct keys

    h.deinit(a);
    nums.deinit(a);
    return 0;
}

test "std containers" {
    var a: Allocator = c_allocator();
    var l: IntList = IntList.init(a);
    l.push(a, 10);
    l.push(a, 20);
    expect(l.len() == 2);
    expect(l.get(1) == 20);
    expect(imin(3, 8) == 3);
    l.deinit(a);
}
