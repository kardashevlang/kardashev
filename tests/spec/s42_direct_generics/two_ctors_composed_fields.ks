//SPEC: §42.2 generic composition — one ctor's fields are applications of ANOTHER ctor at its own type param
//OUT: 25
//OUT: 17

// `Pair(T)` stores two `Slot(T)`s: instantiating Pair(i64) must transitively
// instantiate Slot(i64) for its fields, its `make` and its `spread`.
fn Slot(comptime T: type) type {
    return struct {
        v: T,
        fn of(x: T) Self {
            return Self{ .v = x };
        }
        fn get(self: Self) T {
            return self.v;
        }
    };
}

fn Pair(comptime T: type) type {
    return struct {
        lo: Slot(T),
        hi: Slot(T),
        fn make(x: T, y: T) Self {
            if (y < x) {
                return Self{ .lo = Slot(T).of(y), .hi = Slot(T).of(x) };
            }
            return Self{ .lo = Slot(T).of(x), .hi = Slot(T).of(y) };
        }
        fn spread(self: Self) T {
            return self.hi.get() - self.lo.get();
        }
    };
}

pub fn main() void {
    var p: Pair(i64) = Pair(i64).make(42, 17);   // normalised: lo=17, hi=42
    print(p.spread());                           // 42 - 17 = 25
    print(p.lo.get());                           // 17
}
