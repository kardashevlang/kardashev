//SPEC: §20.2 a union `switch` must cover every variant or have an `else` — missing one is E0210
//ERR: E0210

const U = union(enum) {
    a: i64,
    b: i64,
    c: i64,
};

pub fn main() void {
    var u: U = U{ .a = 1 };
    // `.c` is uncovered and there is no `else`.
    switch (u) {
        .a => |x| {
            print(x);
        },
        .b => |x| {
            print(x);
        },
    }
}
