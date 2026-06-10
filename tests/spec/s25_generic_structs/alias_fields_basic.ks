//SPEC: §25.2 `const Alias = Name(C);` instantiates a fields-only type-constructor; the alias works in type position, as a struct-literal name, and for field access
//OUT: 25
//OUT: 7

// The three §25.2 alias uses in one program: `var p: IP` (type position),
// `IP{ … }` (struct-literal name), `p.a` (field access — read, write, and
// read-back, so the instantiated struct really stores per-field state).
fn Pair(comptime T: type) type {
    return struct { a: T, b: T };
}

const IP = Pair(i64);

pub fn main() void {
    var p: IP = IP{ .a = 12, .b = 13 };
    print(p.a + p.b); // 25
    p.a = 50;
    print(p.a - 43); // 7
}
