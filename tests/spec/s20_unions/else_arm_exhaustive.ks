//SPEC: §20.2 an `else` arm makes a union `switch` exhaustive without naming every variant
//OUT: 33
//OUT: -1
//OUT: -1
//OUT: 8

const Cmd = union(enum) {
    push: i64,
    pop: i64,
    dup: i64,
    halt: i64,
};

fn first_operand(c: Cmd) i64 {
    switch (c) {
        .push => |v| {
            return v;
        },
        .dup => |n| {
            return n * 2;
        },
        else => {
            // covers .pop and .halt without listing them
            return 0 - 1;
        },
    }
}

pub fn main() void {
    print(first_operand(Cmd{ .push = 30 + 3 }));    // 33
    print(first_operand(Cmd{ .pop = 1 }));          // else -> -1
    print(first_operand(Cmd{ .halt = 9 }));         // else -> -1
    print(first_operand(Cmd{ .dup = 4 }));          // 8
}
