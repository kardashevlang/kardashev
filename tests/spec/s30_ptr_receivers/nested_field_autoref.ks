//SPEC: §30.2 auto-ref accepts any addressable lvalue receiver — a nested field (`w.inner.bump()` passes `&w.inner`), including one rooted in a pointer-receiver `self`
//OUT: 11
//OUT: 21
const Inner = struct {
    n: i64,

    fn bump(self: *Inner) void {
        self.n += 10;
    }
};

const Outer = struct {
    inner: Inner,

    fn deep(self: *Outer) void {
        self.inner.bump();    // receiver is a field of (*self): auto-ref &(*self).inner
    }
};

pub fn main() void {
    var w: Outer = Outer{ .inner = Inner{ .n = 1 } };
    w.inner.bump();           // auto-ref &w.inner — mutates in place
    print(w.inner.n);         // 11
    w.deep();                 // the same mutation, one pointer level deeper
    print(w.inner.n);         // 21
}
