//SPEC: §31.1 distinct argument tuples are distinct types — a `Map(u8, i64)` value does not coerce to `Map(i64, u8)`
//ERR: E0110
fn Map(comptime K: type, comptime V: type) type {
    return struct {
        k: K,
        v: V,
    };
}

const A = Map(u8, i64);
const B = Map(i64, u8);

pub fn main() void {
    var a: A = A{ .k = 1, .v = 2 };
    var b: B = a;      // same constructor, different tuple: a type mismatch
    print(b.k);
}
