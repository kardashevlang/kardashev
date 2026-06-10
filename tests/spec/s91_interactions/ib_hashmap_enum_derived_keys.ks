//SPEC: std HashMap×§37 `@as(i32, @intFromEnum(e))` makes enum-derived map keys — distinct variants (with explicit values) address distinct slots
//OUT: 100
//OUT: -1
//OUT: 200
//OUT: 3

@import("std");

const Tier = enum { Bronze = 10, Silver = 20, Gold = 30 };
const M = HashMap(i64);

fn key_of(t: Tier) i32 {
    return @as(i32, @intFromEnum(t));
}

pub fn main() void {
    var a: Allocator = c_allocator();
    var m: M = M.init(a);

    m.put(a, key_of(Tier.Bronze), 100);
    m.put(a, key_of(Tier.Gold), 300);

    print(m.get(key_of(Tier.Bronze), 0 - 1));  // 100
    print(m.get(key_of(Tier.Silver), 0 - 1));  // absent -> -1

    m.put(a, key_of(Tier.Silver), 200);
    print(m.get(key_of(Tier.Silver), 0 - 1));  // 200
    print(m.len());                            // 3 distinct enum keys
    m.deinit(a);
}
