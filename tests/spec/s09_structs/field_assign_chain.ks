//SPEC: §9.4 a field assignment place may be a multi-level chain `a.b.c = e`
//OUT: 11
//OUT: 22
//OUT: 3
//OUT: 42
//OUT: 11
const P = struct {
    x: i32,
    y: i32,
};
const R = struct {
    origin: P,
    w: i32,
};
const O = struct {
    r: R,
};

pub fn main() void {
    var r: R = R{ .origin = P{ .x = 1, .y = 2 }, .w = 3 };
    r.origin.x = r.origin.x + 10;     // two-level chain write: 11
    r.origin.y = r.origin.x * 2;      // reads the value just written: 22
    print(r.origin.x);                // 11
    print(r.origin.y);                // 22
    print(r.w);                       // 3 — sibling untouched
    var o: O = O{ .r = r };
    o.r.origin.x = o.r.origin.x + 30; // three-level chain write: 41
    print(o.r.origin.x + 1);          // 42
    print(r.origin.x);                // 11 — `o.r` got a copy; `r` is independent
}
