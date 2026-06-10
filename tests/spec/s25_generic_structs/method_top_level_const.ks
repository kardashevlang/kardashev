//SPEC: §26.2 a generic-struct method body may reference top-level consts (the v0.138 deferred body-check)
//OUT: 56

const SCALE = 8; // a method body must see this — bodies are checked after Pass 2

fn Box(comptime T: type) type {
    return struct {
        v: T,

        fn scaled(self: Self) T {
            return self.v * SCALE;
        }
    };
}

const B = Box(i64);

pub fn main() void {
    var b: B = B{ .v = 7 };
    print(b.scaled()); // 7 * 8 = 56
}
