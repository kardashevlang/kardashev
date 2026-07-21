//SPEC: §42 x §20 HashMap over union values: put/get round-trips variants through the i32-keyed map
//OUT: 30
//OUT: 1
//OUT: 0

@import("std");

const V = union(enum) { n: i64, b: bool };

pub fn main() void {
    var a: Allocator = c_allocator();
    var m: HashMap(V) = HashMap(V).init(a);
    m.put(a, 1, V{ .n = 30 });
    m.put(a, 2, V{ .b = true });
    switch (m.get(1, V{ .n = 0 })) {
        .n => |v| { print(v); },
        .b => { print(0 - 1); },
    }
    switch (m.get(2, V{ .n = 0 })) {
        .n => { print(0 - 1); },
        .b => |b| {
            if (b) { print(1); }
        },
    }
    switch (m.get(9, V{ .n = 0 })) {   // a missing key yields the fallback
        .n => |v| { print(v); },
        .b => { print(0 - 1); },
    }
    m.deinit(a);
}
