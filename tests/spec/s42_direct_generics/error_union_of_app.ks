//SPEC: §42.1 `!Name(A)` — an application composes with the error-union prefix; `try` propagates it
//OUT: 20
//OUT: -1

// `!Box(i64)` flows through a `try` chain: parse -> doubled. The error path
// must skip the multiplication entirely (a broken union would produce 0/garbage).
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

fn parse(n: i64) !Box(i64) {
    if (n < 0) {
        return error.Negative;
    }
    return Box(i64).init(n);
}

fn doubled(n: i64) !Box(i64) {
    var b: Box(i64) = try parse(n);
    return Box(i64).init(b.get() * 2);
}

pub fn main() void {
    var ok: Box(i64) = doubled(10) catch Box(i64).init(0 - 1);
    print(ok.get());
    var bad: Box(i64) = doubled(0 - 10) catch Box(i64).init(0 - 1);
    print(bad.get());
}
