//SPEC: §42.2 `ArrayList(T)` composes inside a user type-constructor (std container as a generic field)
//OUT: 8
//OUT: 3
//OUT: 14

@import("std");

// A Stack built ON ArrayList(T): pushes delegate to the list's
// pointer-receiver `push` through the `items` field of a *Self receiver.
fn Stack(comptime T: type) type {
    return struct {
        items: ArrayList(T),
        fn init(a: Allocator) Self {
            return Self{ .items = ArrayList(T).init(a) };
        }
        fn push(self: *Self, a: Allocator, x: T) void {
            self.items.push(a, x);
        }
        fn top(self: Self) T {
            return self.items.get(self.items.len() - 1);
        }
        fn depth(self: Self) usize {
            return self.items.len();
        }
    };
}

pub fn main() void {
    var a: Allocator = c_allocator();
    var s: Stack(i64) = Stack(i64).init(a);
    s.push(a, 5);
    s.push(a, 1);
    s.push(a, 8);
    print(s.top());                  // 8
    print(@as(i64, s.depth()));      // 3
    // A second instantiation at another type from the same composition.
    var t: Stack(i32) = Stack(i32).init(a);
    t.push(a, 14);
    print(t.top());                  // 14
}
