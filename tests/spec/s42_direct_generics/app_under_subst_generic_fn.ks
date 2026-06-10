//SPEC: §42.2 `Box(T)` inside a generic fn resolves T through the active substitution — one instance per monomorphisation
//OUT: 42
//OUT: 200

// `wrap_doubled` is instantiated at i64 and at u8; its `Box(T)` return type
// must follow each substitution, or one of the two calls could not
// type-check against its differently-typed `Box(…)` destination.
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

fn wrap_doubled(comptime T: type, x: T) Box(T) {
    return Box(T).init(x * 2);
}

pub fn main() void {
    var a: Box(i64) = wrap_doubled(i64, 21);
    print(a.get());                       // 42
    var b: Box(u8) = wrap_doubled(u8, 100);
    print(@as(i64, b.get()));             // 200
}
