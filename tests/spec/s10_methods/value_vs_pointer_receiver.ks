//SPEC: §10/§30 a value receiver gets a by-value copy (caller unchanged); a pointer receiver mutates in place
//OUT: 1
//OUT: 41
//OUT: 42
const P = struct {
    x: i32,

    fn set_on_copy(self: P, v: i32) void {
        var c: P = self;     // self is a copy already; mutate a local of it
        c.x = v;             // never visible to the caller
    }

    fn set(self: *P, v: i32) void {
        self.x = v;          // auto-deref write through the pointer (§30.1)
    }
};

pub fn main() void {
    var p: P = P{ .x = 1 };
    p.set_on_copy(99);
    print(p.x);              // 1 — the value receiver could not touch `p`
    p.set(41);               // auto-ref: passes &p (§30.2)
    print(p.x);              // 41
    p.set(p.x + 1);
    print(p.x);              // 42
}
