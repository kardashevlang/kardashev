//SPEC: §26.4 std `ArrayList(T)` full lifecycle through a type alias — init / push through growth / set / get / len / deinit
//OUT: 10
//OUT: 100
//OUT: 494

@import("std");

const L = ArrayList(i64);

pub fn main() void {
    var a: Allocator = c_allocator();
    var l: L = L.init(a);

    // 10 pushes: the initial capacity is 4, so the buffer grows twice
    // (4 → 8 → 16) — every earlier element must survive both copies.
    var k: i64 = 1;
    while (k <= 10) : (k += 1) {
        l.push(a, k * k); // 1 4 9 16 25 36 49 64 81 100
    }
    print(l.len()); // 10

    l.set(0, 110); // overwrite the first square (1 → 110)
    print(l.get(9)); // 100 — the last element survived the growth

    var sum: i64 = 0;
    var i: usize = 0;
    while (i < l.len()) : (i += 1) {
        sum = sum + l.get(i);
    }
    print(sum); // squares 1..10 sum to 385; -1 +110 = 494

    l.deinit(a);
}
