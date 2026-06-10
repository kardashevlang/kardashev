//SPEC: §26×§12.3×§36 inside a monomorphised generic method, `try` propagates to the method's own `!V` return and `catch` takes a V-typed default
//OUT: 42
//OUT: 21
//OUT: -1
//OUT: -5

fn Box(comptime V: type) type {
    return struct {
        val: V,
        ok: bool,
        fn get(self: *Self) !V {
            if (self.ok) {
                return self.val;
            }
            return error.Empty;
        }
        // `catch` with a V-typed default, inside the generic body.
        fn get_or(self: *Self, z: V) V {
            var v: V = self.get() catch z;
            return v;
        }
        // `try` on a sibling method; the error propagates out of sum_twice.
        fn sum_twice(self: *Self) !V {
            var x: V = try self.get();
            var y: V = try self.get();
            return x + y;
        }
    };
}

const BI = Box(i64);

pub fn main() void {
    var b: BI = BI{ .val = 21, .ok = true };
    print(b.sum_twice() catch 0 - 1);   // 21 + 21 = 42
    print(b.get_or(0 - 5));             // 21

    b.ok = false;
    print(b.sum_twice() catch 0 - 1);   // first try propagates -> -1
    print(b.get_or(0 - 5));             // -5
}
