//SPEC: §30.1 for any `*Struct` value `p`, `p.field` reads through and `p.field = e` / `p.field += e` write through the pointer (general, not just `self`)
//OUT: 3
//OUT: 10
//OUT: 42
//OUT: 52
const P = struct {
    x: i64,
    y: i64,
};

pub fn main() void {
    var v: P = P{ .x = 3, .y = 40 };
    var p: *P = &v;
    print(p.x);        // 3 — read (*p).x through the pointer
    p.x = 10;          // plain write through
    print(v.x);        // 10 — the ORIGINAL changed, not a copy
    p.y += 2;          // compound write through (§30.1 names compound explicitly)
    print(v.y);        // 42
    print(p.x + p.y);  // 52 — reads through in expression position
}
