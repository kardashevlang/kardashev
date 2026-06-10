//SPEC: §15.1 `&place` accepts a field chain as the lvalue — the pointer aliases the nested field
//OUT: 42
//OUT: 4

const Inner = struct {
    v: i64,
};

const Outer = struct {
    inner: Inner,
    w: i64,
};

pub fn main() void {
    var o: Outer = Outer{ .inner = Inner{ .v = 10 }, .w = 4 };
    var p: *i64 = &o.inner.v;
    p.* = p.* + o.w * 8; // 10 + 32
    print(o.inner.v);    // the write through `p` landed in the struct
    print(o.w);          // the sibling field is untouched
}
