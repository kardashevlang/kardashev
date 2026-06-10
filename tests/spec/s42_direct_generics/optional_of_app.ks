//SPEC: §42.1 `?Name(A)` — an application composes with the optional prefix
//OUT: 9
//OUT: -1
//OUT: 49

// `?Box(i64)` in a return type: payload path captured, null path taken, and
// `orelse` unwrapping — all on the direct application (no alias anywhere).
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

fn square_if_pos(n: i64) ?Box(i64) {
    if (n <= 0) {
        return null;
    }
    return Box(i64).init(n * n);
}

pub fn main() void {
    if (square_if_pos(3)) |b| {
        print(b.get());
    } else {
        print(0 - 1);
    }
    if (square_if_pos(0 - 4)) |b| {
        print(b.get());
    } else {
        print(0 - 1);
    }
    var d: Box(i64) = square_if_pos(7) orelse Box(i64).init(0);
    print(d.get());
}
