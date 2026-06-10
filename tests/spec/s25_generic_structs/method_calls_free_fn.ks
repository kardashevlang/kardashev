//SPEC: §26.2 a generic-struct method body may call top-level free functions (the v0.138 deferred body-check)
//OUT: 13

fn triple(x: i64) i64 {
    return x * 3;
}

fn Box(comptime T: type) type {
    return struct {
        v: T,

        fn t(self: Self) T {
            return triple(self.v) + 1; // free-function call from a method body
        }
    };
}

const B = Box(i64);

pub fn main() void {
    var b: B = B{ .v = 4 };
    print(b.t()); // 4*3 + 1 = 13
}
