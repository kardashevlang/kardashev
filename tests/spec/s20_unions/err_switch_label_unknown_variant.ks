//SPEC: §20.2 a union `switch` label that names no variant is E0271
//ERR: E0271

const Shape = union(enum) {
    circle: i64,
    line: i64,
};

pub fn main() void {
    var s: Shape = Shape{ .circle = 2 };
    switch (s) {
        .circle => |r| {
            print(r);
        },
        .triangle => |t| {
            print(t);
        },
    }
}
