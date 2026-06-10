//SPEC: §16 alloc's type argument may name a struct — `alloc(a, S, n)` yields a `[]S` of usable aggregates
//OUT: 10
//OUT: 100
//OUT: 30

const Pt = struct {
    x: i64,
    y: i64,
};

pub fn main() void {
    var a: Allocator = c_allocator();
    var ps: []Pt = alloc(a, Pt, 4);

    var i: usize = 0;
    while (i < ps.len) : (i += 1) {
        var k: i64 = @as(i64, i) + 1;
        ps[i] = Pt{ .x = k, .y = k * 10 };
    }

    var sx: i64 = 0;
    var sy: i64 = 0;
    i = 0;
    while (i < ps.len) : (i += 1) {
        sx = sx + ps[i].x;
        sy = sy + ps[i].y;
    }
    print(sx);       // 1+2+3+4
    print(sy);       // 10+20+30+40
    print(ps[2].y);  // the third element round-tripped intact

    free(a, ps);
}
