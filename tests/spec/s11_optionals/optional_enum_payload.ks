//SPEC: §11 `?T` where `T` is an enum: a captured enum payload drives an exhaustive `switch`
//OUT: 100
//OUT: 200
//OUT: 300
//OUT: -1

const Color = enum { Red, Green, Blue };

fn classify(n: i64) ?Color {
    if (n < 0) {
        return null;
    }
    if (n % 3 == 0) {
        return Color.Red;
    }
    if (n % 3 == 1) {
        return Color.Green;
    }
    return Color.Blue;
}

fn code(n: i64) i64 {
    var r: i64 = 0 - 1;
    if (classify(n)) |c| {
        switch (c) {
            .Red => { r = 100; },
            .Green => { r = 200; },
            .Blue => { r = 300; },
        }
    } else {
        r = 0 - 1;
    }
    return r;
}

pub fn main() void {
    print(code(6));       // 6 % 3 == 0 -> Red -> 100
    print(code(7));       // 1 -> Green -> 200
    print(code(8));       // 2 -> Blue -> 300
    print(code(0 - 5));   // negative -> null -> -1
}
