//SPEC: §20 union assignment copies the value: reassigning the source leaves the copy on the old variant
//OUT: 15
//OUT: 60

const V = union(enum) { a: i64, b: i64 };

pub fn main() void {
    var x: V = V{ .a = 15 };
    var y: V = x;
    x = V{ .b = 60 };
    switch (y) {
        .a => |v| { print(v); },   // 15 — the copy kept .a
        .b => |v| { print(v); },
    }
    switch (x) {
        .a => |v| { print(v); },
        .b => |v| { print(v); },   // 60
    }
}
