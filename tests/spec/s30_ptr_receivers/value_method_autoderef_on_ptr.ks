//SPEC: §30.1 a method call auto-derefs a `*Struct` receiver — a VALUE-receiver method called on a pointer gets a by-value copy of the pointee (the original is untouched)
//OUT: 42
//OUT: 0
//OUT: 21
const B = struct {
    n: i64,

    fn doubled(self: B) i64 {
        return self.n * 2;
    }

    fn smashed(self: B) i64 {
        var c: B = self;   // self is already a copy; clobber a local of it
        c.n = 0;
        return c.n;
    }
};

pub fn main() void {
    var b: B = B{ .n = 21 };
    var p: *B = &b;
    print(p.doubled());    // 42 — (*p) copied into the value receiver
    print(p.smashed());    // 0  — the copy was clobbered...
    print(b.n);            // 21 — ...but the pointee is unchanged
}
