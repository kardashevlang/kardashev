//SPEC: §16 std's HashMap(V) is an Allocator client — its slot arrays live on the passed allocator's heap
//OUT: 4
//OUT: 3
//OUT: 2

@import("std");

pub fn main() void {
    var a: Allocator = c_allocator();
    var m: HashMap(i64) = HashMap(i64).init(a);

    // Histogram of k % 4 over k = 0..9: 0 -> 3, 1 -> 3, 2 -> 2, 3 -> 2.
    var k: i32 = 0;
    while (k < 10) : (k += 1) {
        var key: i32 = k % 4;
        m.put(a, key, m.get(key, 0) + 1);
    }

    print(m.len());     // 4 distinct keys
    print(m.get(0, 0)); // k = 0, 4, 8
    print(m.get(3, 0)); // k = 3, 7

    m.deinit(a);
}
