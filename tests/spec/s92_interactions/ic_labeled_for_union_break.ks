//SPEC: §40 x §29 x §20 a labeled for over unions: `break :outer` from a switch arm ends the whole iteration
//OUT: 10
//OUT: 55

const T = union(enum) { go: i64, stop: bool };

pub fn main() void {
    var xs: [3]T = [3]T{ T{ .go = 10 }, T{ .stop = true }, T{ .go = 30 } };
    var acc: i64 = 0;
    outer: for (xs) |t| {
        switch (t) {
            .go => |v| {
                acc += v;
                print(v);
            },
            .stop => { break :outer; },
        }
    }
    print(acc + 45);   // 10 + 45 — the third element never ran
}
