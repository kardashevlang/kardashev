//SPEC: §42.2 `Name(T)` works as a local's type and as an associated-call receiver, no alias needed
//OUT: 5
//OUT: 25

// Before v0.152 both the `var b: Box(i32)` annotation and the
// `Box(i32).init(…)` receiver required `const B = Box(i32);` first.
fn Box(comptime T: type) type {
    return struct {
        v: T,
        fn init(x: T) Self {
            return Self{ .v = x };
        }
        fn get(self: Self) T {
            return self.v;
        }
        fn squared(self: Self) Self {
            return Self{ .v = self.v * self.v };
        }
    };
}

pub fn main() void {
    var b: Box(i32) = Box(i32).init(5);
    print(b.get());
    var s: Box(i32) = b.squared();
    print(s.get());
}
