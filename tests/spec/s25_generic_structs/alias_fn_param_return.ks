//SPEC: §25.2 a type alias resolves in function parameter and return positions like any named type
//OUT: 21
//OUT: 41

fn Box(comptime T: type) type {
    return struct { v: T };
}

const B = Box(i64);

fn wrap(x: i64) B {
    return B{ .v = x };
}

fn unwrap(b: B) i64 {
    return b.v;
}

pub fn main() void {
    print(unwrap(wrap(21))); // 21 — alias-typed value through both directions
    var b: B = wrap(40);
    b.v = b.v + 1;
    print(unwrap(b)); // 41
}
