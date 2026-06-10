// v0.152: generic type-constructors applied directly in type position and as
// associated-call receivers — no `const L = ArrayList(i32);` alias needed
// (SPEC §42). Aliases still work; both forms share one instantiated struct.
@import("std");

// Generic composition: this type-constructor's struct holds another
// application instantiated at the *same* type parameter (`ArrayList(T)`
// resolves under the active substitution, SPEC §42.2).
fn Stack(comptime T: type) type {
    return struct {
        list: ArrayList(T),

        fn init(a: Allocator) Self {
            return Self{ .list = ArrayList(T).init(a) };
        }
        fn push(self: *Self, a: Allocator, v: T) void {
            self.list.push(a, v);
        }
        fn top(self: Self) T {
            return self.list.get(self.list.len() - 1);
        }
        fn size(self: Self) usize {
            return self.list.len();
        }
    };
}

pub fn main() void {
    const a = c_allocator();

    // Direct application in a local's type and as the `init` receiver.
    var l: ArrayList(i64) = ArrayList(i64).init(a);
    l.push(a, 10);
    l.push(a, 20);
    l.push(a, 30);
    print(l.get(0) + l.get(1) + l.get(2)); // 60
    l.deinit(a);

    var m: HashMap(i64) = HashMap(i64).init(a);
    m.put(a, 1, 42);
    print(m.get(1, 0)); // 42
    m.deinit(a);

    var s: Stack(i64) = Stack(i64).init(a);
    s.push(a, 7);
    s.push(a, 9);
    print(s.top()); // 9
    print(@as(i64, s.size())); // 2
}
