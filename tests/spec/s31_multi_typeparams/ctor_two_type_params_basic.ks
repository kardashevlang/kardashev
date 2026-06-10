//SPEC: §31.1 a type-constructor may declare two `comptime _: type` parameters; fields and methods see both substitutions
//OUT: 42007
//OUT: 42
//OUT: 7
fn Pair(comptime K: type, comptime V: type) type {
    return struct {
        k: K,
        v: V,

        fn combined(self: Self) i64 {
            return @as(i64, self.k) * 1000 + self.v;   // k is K=i32, v is V=i64
        }
    };
}

const PI = Pair(i32, i64);

pub fn main() void {
    var p: PI = PI{ .k = 42, .v = 7 };
    print(p.combined());   // 42*1000 + 7
    print(p.k);            // 42 (an i32 field)
    print(p.v);            // 7  (an i64 field)
}
