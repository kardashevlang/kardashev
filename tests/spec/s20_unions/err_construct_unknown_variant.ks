//SPEC: §20.2 a union construction field that names no variant is E0271
//ERR: E0271

const Shape = union(enum) {
    circle: i64,
    line: i64,
};

pub fn main() void {
    var s: Shape = Shape{ .triangle = 3 };
    switch (s) {
        .circle => |r| {
            print(r);
        },
        .line => |l| {
            print(l);
        },
    }
}
