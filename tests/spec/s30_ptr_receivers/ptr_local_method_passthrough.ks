//SPEC: §30.2 a receiver that is already a `*Struct` is passed straight through to a pointer-receiver method (no auto-ref) — calls via a `*T` local or parameter mutate the pointee
//OUT: 7
//OUT: 9
const C = struct {
    n: i64,

    fn inc(self: *C) void {
        self.n += 1;
    }
};

fn poke(q: *C) void {
    q.inc();           // receiver is a *C parameter: passed through unchanged
}

pub fn main() void {
    var c: C = C{ .n = 5 };
    var p: *C = &c;
    p.inc();           // through a *C local — mutates c, not a copy of c
    p.inc();
    print(c.n);        // 7
    poke(p);           // pointer forwarded through a call boundary
    poke(&c);
    print(c.n);        // 9
}
