//SPEC: §26×§12 a generic-struct method may return `!V` where V is the type parameter — each instantiation gets its own error-union payload type
//OUT: 4000000000
//OUT: -1
//OUT: 7
//OUT: -9

fn Box(comptime V: type) type {
    return struct {
        val: V,
        fn take(self: *Self, ok: bool) !V {
            if (ok) {
                return self.val;
            }
            return error.Empty;
        }
    };
}

const B64 = Box(i64);
const B32 = Box(i32);

pub fn main() void {
    // i64 instantiation: the payload exceeds i32 range, proving take()
    // really returns !i64 here.
    var b: B64 = B64{ .val = 4000000000 };
    print(b.take(true) catch 0 - 1);
    print(b.take(false) catch 0 - 1);

    // i32 instantiation of the SAME method body.
    var c: B32 = B32{ .val = 7 };
    var got: i32 = c.take(true) catch 0 - 1;
    print(got);
    var miss: i32 = c.take(false) catch 0 - 9;
    print(miss);
}
