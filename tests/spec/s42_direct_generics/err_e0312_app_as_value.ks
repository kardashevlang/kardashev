//SPEC: §42.2 a type-constructor application in VALUE position (not an assoc-call receiver) is E0312
//ERR: E0312

fn Box(comptime T: type) type {
    return struct {
        v: T,
        fn init(x: T) Self {
            return Self{ .v = x };
        }
    };
}

pub fn main() void {
    // `Box(i32).init(…)` would be fine; the bare application is not a value.
    print(Box(i32));
}
