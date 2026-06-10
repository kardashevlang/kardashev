//SPEC: §26.2 methods are monomorphised PER instantiation — the same method body behaves according to its bound `T`
//OUT: 188
//OUT: 4464

// Two instances of one constructor coexist; the shared method body truncates
// through `T`, so if both aliases resolved to one instance (or the methods
// were emitted once) the two columns could not disagree.
fn Squash(comptime T: type) type {
    return struct {
        raw: i64,

        fn low(self: Self) i64 {
            return @as(i64, @as(T, self.raw));
        }
    };
}

const S8 = Squash(u8);
const S16 = Squash(u16);

pub fn main() void {
    var a: S8 = S8{ .raw = 700 };
    var b: S16 = S16{ .raw = 70000 };
    print(a.low()); // 700  mod 256   = 188
    print(b.low()); // 70000 mod 65536 = 4464
}
