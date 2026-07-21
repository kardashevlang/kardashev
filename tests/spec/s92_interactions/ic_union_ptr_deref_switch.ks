//SPEC: §15.1 x §20 a `*Union` parameter: switching on the deref reads the caller's current value
//OUT: 4
//OUT: 8

const U = union(enum) { a: i64, b: i64 };

fn read(p: *U) i64 {
    switch (p.*) {
        .a => |v| { return v; },
        .b => |v| { return v * 2; },
    }
}

pub fn main() void {
    var u: U = U{ .a = 4 };
    print(read(&u));
    u = U{ .b = 4 };
    print(read(&u));
}
