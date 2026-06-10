//SPEC: §9 a struct copy duplicates nested struct fields too (deep value copy, no sharing)
//OUT: 5
//OUT: 1
//OUT: 500
//OUT: 2
//OUT: 500
//OUT: 9
const Inner = struct {
    v: i32,
};
const Box = struct {
    inner: Inner,
    tag: i32,
};

pub fn main() void {
    var a: Box = Box{ .inner = Inner{ .v = 5 }, .tag = 1 };
    var b: Box = a;
    b.inner.v = 500;          // writes b's own nested Inner
    b.tag = 2;
    print(a.inner.v);         // 5 — a's nested struct was duplicated, not shared
    print(a.tag);             // 1
    print(b.inner.v);         // 500
    print(b.tag);             // 2
    var c: Box = b;           // snapshot of b
    b.inner.v = 9;            // later writes to b do not reach the snapshot
    print(c.inner.v);         // 500
    print(b.inner.v);         // 9
}
