//SPEC: §30.2/§30.3 a pointer-receiver method may return `*Self`; the returned pointer (held in a `*T` local) keeps mutating the SAME object — all steps alias the original
//OUT: 42
//OUT: 42
const Acc = struct {
    total: i64,

    fn add(self: *Acc, v: i64) *Acc {
        self.total += v;
        return self;
    }
};

pub fn main() void {
    var a: Acc = Acc{ .total = 0 };
    var s: *Acc = a.add(5);    // auto-ref &a; returns &a
    var t: *Acc = s.add(7);    // receiver s is already a pointer: pass-through
    t.add(30);
    print(a.total);            // 42 — every step wrote through to `a`
    print(s.total);            // 42 — s aliases a (auto-deref read, §30.1)
}
