//SPEC: §20.2 a union `switch` arm may omit the payload capture — the payload is simply not bound
//OUT: 2
//OUT: 7

const U = union(enum) { a: i64, b: i64 };

fn f(u: U) void {
    switch (u) {
        .a => { print(2); },       // ignores its i64 payload
        .b => |v| { print(v); },
    }
}

pub fn main() void {
    f(U{ .a = 99 });
    f(U{ .b = 7 });
}
