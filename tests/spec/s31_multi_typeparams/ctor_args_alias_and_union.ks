//SPEC: §31 x §42.2 type-constructor arguments resolve aliases and unions like any base name
//OUT: 5
//OUT: 1

const Shape = union(enum) { n: i64 };

fn Box(comptime T: type) type {
    return struct { v: T };
}

const IntBox = Box(i32);

fn Pair(comptime A: type, comptime B: type) type {
    return struct { a: A, b: B };
}

const Mix = Pair(IntBox, Shape);

pub fn main() void {
    var m: Mix = Mix{ .a = IntBox{ .v = 5 }, .b = Shape{ .n = 1 } };
    print(m.a.v);
    switch (m.b) {
        .n => |v| { print(v); },
    }
}
