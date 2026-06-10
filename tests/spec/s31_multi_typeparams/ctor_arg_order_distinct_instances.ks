//SPEC: §31.1 instantiation is memoised on the ARGUMENT TUPLE — `Map(u8, i64)` and `Map(i64, u8)` are distinct instances whose methods behave differently
//OUT: 300044
//OUT: 44300
fn Map(comptime K: type, comptime V: type) type {
    return struct {
        k: K,
        v: V,

        // K and V drive @as truncation: if argument order did not matter the
        // two instances below could not disagree on the same input.
        fn squash(self: Self, x: i64) i64 {
            return @as(i64, @as(K, x)) + 1000 * @as(i64, @as(V, x));
        }
    };
}

const A = Map(u8, i64);
const B = Map(i64, u8);

pub fn main() void {
    var a: A = A{ .k = 1, .v = 2 };
    var b: B = B{ .k = 1, .v = 2 };
    print(a.squash(300));   // (300 mod 256) + 1000*300 = 44 + 300000
    print(b.squash(300));   // 300 + 1000*(300 mod 256) = 300 + 44000
}
