//SPEC: §42 x §20 ArrayList instantiated at a union element type: push constructed variants, get + switch dispatch
//OUT: 3
//OUT: 46
//OUT: 1

@import("std");

const Shape = union(enum) { num: i64, tag: bool };

pub fn main() void {
    var a: Allocator = c_allocator();
    var l: ArrayList(Shape) = ArrayList(Shape).init(a);
    l.push(a, Shape{ .num = 40 });
    l.push(a, Shape{ .tag = true });
    l.push(a, Shape{ .num = 6 });
    print(l.len());
    var total: i64 = 0;
    var tags: i64 = 0;
    var i: usize = 0;
    while (i < l.len()) : (i += 1) {
        switch (l.get(i)) {
            .num => |v| { total += v; },
            .tag => |b| {
                if (b) { tags += 1; }
            },
        }
    }
    print(total);   // 40 + 6
    print(tags);    // one true tag
    l.deinit(a);
}
