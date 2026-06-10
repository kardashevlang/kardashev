//SPEC: §42.2 an application resolves in fn parameter and return type positions (Pass-0d before signatures)
//OUT: 14
//OUT: 30

// `Box(i64)` appears only in signatures — never behind an alias — so the
// constructor must already be known when Pass 1 resolves these types.
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

fn add(a: Box(i64), b: Box(i64)) Box(i64) {
    return Box(i64).init(a.get() + b.get());
}

pub fn main() void {
    var s: Box(i64) = add(Box(i64).init(9), Box(i64).init(5));
    print(s.get());
    // The returned application value feeds straight back in as an argument.
    print(add(s, Box(i64).init(16)).get());
}
