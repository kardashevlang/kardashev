//SPEC: §27.1 a field-chain place takes every compound operator — including a nested chain `o.inner.n`
//OUT: 14
//OUT: 8
//OUT: 24
//OUT: 4
//OUT: 1
//OUT: 42

const Inner = struct {
    n: i64,
};
const Outer = struct {
    inner: Inner,
    k: i64,
};

pub fn main() void {
    var o: Outer = Outer{ .inner = Inner{ .n = 7 }, .k = 10 };
    o.k += 4;          // 14
    print(o.k);
    o.k -= 6;          // 8
    print(o.k);
    o.k *= 3;          // 24
    print(o.k);
    o.k /= 5;          // 4
    print(o.k);
    o.k %= 3;          // 1
    print(o.k);
    o.inner.n *= 6;    // a two-level field chain is a valid compound place
    print(o.inner.n);  // 42
}
