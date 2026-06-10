//SPEC: §42.1 `*Name(A)` — a pointer to an application; writes through it mutate the caller's value
//OUT: 3
//OUT: 23

// `*Box(i64)` as a parameter type: the callee mutates the pointee's field
// through pointer auto-deref, and the caller observes the new value.
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

fn bump(p: *Box(i64), by: i64) void {
    p.v = p.v + by;
}

pub fn main() void {
    var b: Box(i64) = Box(i64).init(3);
    print(b.get());
    bump(&b, 20);
    print(b.get());
}
