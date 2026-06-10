//SPEC: §27.1+§15.1 a compound place may pass through an explicit deref — `q.* op=` and `p.*.field op=` write the pointee
//OUT: 21
//OUT: 15
//OUT: 12

const P = struct {
    x: i64,
};

pub fn main() void {
    var n: i64 = 3;
    var q: *i64 = &n;
    q.* *= 7;          // through a plain pointer deref place
    print(n);          // 21

    var s: P = P{ .x = 10 };
    var p: *P = &s;
    p.*.x += 5;        // explicit deref, then a field place
    print(s.x);        // 15
    p.*.x -= 3;
    print(s.x);        // 12
}
