//SPEC: §9.1 struct literals nest and field access chains `a.b.c` read through each level
//OUT: 7
//OUT: 72
//OUT: 7
const Inner = struct {
    v: i32,
};
const Mid = struct {
    inner: Inner,
    k: i32,
};
const Outer = struct {
    mid: Mid,
    tag: i32,
};

fn mid_of(o: Outer) Mid {
    return o.mid;
}

pub fn main() void {
    // Nested literal, with the outer inits deliberately out of order too.
    var o: Outer = Outer{
        .tag = 2,
        .mid = Mid{ .k = 10, .inner = Inner{ .v = 7 } },
    };
    print(o.mid.inner.v);                       // 7 — three-level chain
    print(o.mid.inner.v * o.mid.k + o.tag);     // 7*10 + 2 = 72
    print(mid_of(o).inner.v);                   // 7 — chain rooted at a call result
}
