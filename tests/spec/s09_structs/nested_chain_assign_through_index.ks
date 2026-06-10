//SPEC: §9.4+§14.1 a field-assign chain may pass through an index at any depth (`xs[i].f.g`, `xs[i].buf[j]`)
//OUT: 99
//OUT: 11
//OUT: 40
//OUT: 106
const Inner = struct {
    g: i64,
};
const P = struct {
    x: i64,
    f: Inner,
    buf: [3]i64,
};

pub fn main() void {
    var xs: [2]P = [2]P{ P{ .x = 1, .f = Inner{ .g = 2 }, .buf = [3]i64{ 1, 2, 3 } }, P{ .x = 3, .f = Inner{ .g = 4 }, .buf = [3]i64{ 4, 5, 6 } } };
    xs[1].f.g = 99;            // nested field chain through an index
    xs[0].x += 10;             // compound through an index
    print(xs[1].f.g);          // 99
    print(xs[0].x);            // 11
    xs[1].buf[0] = 40;         // index → field → index place
    xs[1].buf[2] += 100;       // ... and compound on it
    print(xs[1].buf[0]);       // 40
    print(xs[1].buf[2]);       // 106
}
