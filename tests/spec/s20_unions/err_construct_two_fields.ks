//SPEC: §20.2 a union construction must have exactly one variant initializer — two is E0270
//ERR: E0270

const U = union(enum) {
    a: i64,
    b: i64,
};

pub fn main() void {
    // A tagged union holds ONE active variant; initializing two is rejected.
    var u: U = U{ .a = 1, .b = 2 };
    switch (u) {
        .a => |x| {
            print(x);
        },
        .b => |x| {
            print(x);
        },
    }
}
