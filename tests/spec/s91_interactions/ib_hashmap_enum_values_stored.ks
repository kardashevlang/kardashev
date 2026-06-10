//SPEC: std HashMap×§13 a map monomorphised over an enum VALUE type — enum literals as stored values and as the get() fallback
//OUT: 30
//OUT: 20
//OUT: 10
//OUT: 0
//OUT: 1

@import("std");

const Tier = enum { Bronze = 10, Silver = 20, Gold = 30 };
const M = HashMap(Tier);

pub fn main() void {
    var a: Allocator = c_allocator();
    var m: M = M.init(a);

    m.put(a, 7, Tier.Gold);
    m.put(a, 2, Tier.Bronze);

    print(@intFromEnum(m.get(7, Tier.Silver)));   // stored Gold -> 30
    print(@intFromEnum(m.get(99, Tier.Silver)));  // miss -> fallback Silver -> 20

    m.put(a, 7, Tier.Bronze);                     // overwrite at the same key
    print(@intFromEnum(m.get(7, Tier.Silver)));   // 10

    var gone: bool = m.remove(2);
    if (gone) {
        if (m.has(2)) {
            print(1);
        } else {
            print(0);                             // removed for real
        }
    }
    print(m.len());                               // only key 7 remains
    m.deinit(a);
}
