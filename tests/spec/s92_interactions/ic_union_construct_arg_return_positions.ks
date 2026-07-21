//SPEC: §20.2 union construction stands directly in call-argument and return positions (by value both ways)
//OUT: 9
//OUT: 12

const P = union(enum) { one: i64, two: i64 };

fn total(p: P) i64 {
    switch (p) {
        .one => |v| { return v; },
        .two => |v| { return v + 10; },
    }
}

fn make(two: bool) P {
    if (two) { return P{ .two = 2 }; }
    return P{ .one = 9 };
}

pub fn main() void {
    print(total(make(false)));    // 9
    print(total(P{ .two = 2 })); // 2 + 10
}
