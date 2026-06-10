//SPEC: §31.1 the SAME argument tuple yields the SAME interned struct — two aliases of `Map(u8, i64)` are one type (cross-assignment and call boundaries agree)
//OUT: 100
//OUT: 200
//OUT: 100
fn Map(comptime K: type, comptime V: type) type {
    return struct {
        k: K,
        v: V,
    };
}

const A = Map(u8, i64);
const A2 = Map(u8, i64);   // memoised: the same instance as A

fn take_a2(m: A2) i64 {
    return m.v;
}

pub fn main() void {
    var a: A = A{ .k = 9, .v = 100 };
    var b: A2 = a;          // legal only because A and A2 are one struct
    b.v = 200;
    print(a.v);             // 100 — b is a by-value copy, a is untouched
    print(b.v);             // 200
    print(take_a2(a));      // 100 — an A value crosses an A2-typed boundary
}
