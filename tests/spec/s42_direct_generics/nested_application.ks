//SPEC: §42.1 a type argument may itself be an application — `Box(Box(i32))` nests and recurses
//OUT: 77
//OUT: 8

// The inner and outer instances are distinct structs; `get` on the outer
// yields the inner box, whose own `get` yields the payload.
fn Box(comptime T: type) type {
    return struct {
        v: T,
        fn init(x: T) Self {
            return Self{ .v = x };
        }
        fn get(self: Self) T {
            return self.v;
        }
    };
}

pub fn main() void {
    var bb: Box(Box(i32)) = Box(Box(i32)).init(Box(i32).init(77));
    print(bb.get().get());
    // Rebind the nested payload: the outer holds a fresh inner.
    bb = Box(Box(i32)).init(Box(i32).init(8));
    var inner: Box(i32) = bb.get();
    print(inner.get());
}
