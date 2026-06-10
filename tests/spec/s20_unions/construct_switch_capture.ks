//SPEC: §20.1 `Name{ .v = e }` constructs a variant and `switch` arm `|x|` captures its payload
//OUT: 6
//OUT: 42
//OUT: 58

// A tiny op interpreter: each arm captures the active payload and uses it
// differently, so a wrong tag or a wrong payload value changes the chain.
const Op = union(enum) {
    add: i64,
    mul: i64,
    rsub: i64,
};

fn apply(acc: i64, op: Op) i64 {
    switch (op) {
        .add => |v| {
            return acc + v;
        },
        .mul => |v| {
            return acc * v;
        },
        .rsub => |v| {
            return v - acc;     // payload on the LEFT — order-sensitive
        },
    }
}

pub fn main() void {
    var acc: i64 = 1;
    acc = apply(acc, Op{ .add = 5 });       // 1 + 5 = 6
    print(acc);
    acc = apply(acc, Op{ .mul = 7 });       // 6 * 7 = 42
    print(acc);
    acc = apply(acc, Op{ .rsub = 100 });    // 100 - 42 = 58
    print(acc);
}
