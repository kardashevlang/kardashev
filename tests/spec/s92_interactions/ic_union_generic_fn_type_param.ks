//SPEC: §17.2 x §20 a generic function instantiated at a tagged-union type argument: T binds the union, values pass and return by value
//OUT: 8
//OUT: 5

const Box = union(enum) { small: i64, big: i64 };

fn pick(comptime T: type, first: bool, a: T, b: T) T {
    if (first) { return a; }
    return b;
}

pub fn main() void {
    var x: Box = Box{ .small = 8 };
    var y: Box = Box{ .big = 5 };
    switch (pick(Box, true, x, y)) {
        .small => |v| { print(v); },
        .big => |v| { print(v); },
    }
    switch (pick(Box, false, x, y)) {
        .small => |v| { print(v); },
        .big => |v| { print(v); },
    }
}
