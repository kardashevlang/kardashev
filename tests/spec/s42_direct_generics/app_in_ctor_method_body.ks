//SPEC: §42.2 an application inside a type-constructor METHOD body instantiates during the pending drain (which loops)
//OUT: 4
//OUT: 3

// `Pairer(T)`'s method uses `Box(T)`: checking the method (post-Pass-2 drain)
// must enqueue and instantiate `Box(i64)` / `Box(u8)` — the drain re-loops.
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

fn Pairer(comptime T: type) type {
    return struct {
        a: T,
        b: T,
        fn make(x: T, y: T) Self {
            return Self{ .a = x, .b = y };
        }
        fn lo_boxed(self: Self) Box(T) {
            var t: Box(T) = Box(T).init(self.a);
            if (self.b < self.a) {
                t = Box(T).init(self.b);
            }
            return t;
        }
    };
}

pub fn main() void {
    var p: Pairer(i64) = Pairer(i64).make(9, 4);
    print(p.lo_boxed().get());
    var q: Pairer(u8) = Pairer(u8).make(3, 200);
    print(@as(i64, q.lo_boxed().get()));
}
