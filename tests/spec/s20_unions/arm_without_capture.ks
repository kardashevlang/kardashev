//SPEC: §20.1 a union `switch` arm MAY omit the capture — the payload is simply not bound
//OUT: 7
//OUT: 14
//OUT: 0

const Ev = union(enum) {
    tick: i64,
    beat: i64,
    stop: i64,
};

fn step(acc: i64, e: Ev) i64 {
    switch (e) {
        .tick => |amt| {
            return acc + amt;
        },
        .beat => {
            // no |x|: the i64 payload is ignored; only the tag matters
            return acc * 2;
        },
        .stop => {
            return 0;
        },
    }
}

pub fn main() void {
    var acc: i64 = 3;
    acc = step(acc, Ev{ .tick = 4 });       // 3 + 4 = 7
    print(acc);
    acc = step(acc, Ev{ .beat = 999 });     // payload ignored: 7 * 2 = 14
    print(acc);
    acc = step(acc, Ev{ .stop = 5 });       // 0
    print(acc);
}
