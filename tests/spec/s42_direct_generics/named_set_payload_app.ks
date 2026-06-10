//SPEC: §42.1 `Set!Name(A)` — the named-error-set payload may be an application (the set name stays plain)
//OUT: 11
//OUT: -1

// `ES!Box(i32)`: the parser must treat `ES` as the plain set name and
// `Box(i32)` as the application payload after the `!`.
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

const ES = error{ Empty, Broken };

fn pick(flag: bool) ES!Box(i32) {
    if (flag) {
        return Box(i32).init(11);
    }
    return error.Empty;
}

pub fn main() void {
    var ok: Box(i32) = pick(true) catch Box(i32).init(0 - 1);
    print(ok.get());
    var bad: Box(i32) = pick(false) catch Box(i32).init(0 - 1);
    print(bad.get());
}
