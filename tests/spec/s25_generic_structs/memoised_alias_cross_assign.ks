//SPEC: §25.2 instantiation is memoised — two aliases of the same (constructor, argument) name ONE struct type, so values cross-assign
//OUT: 18

fn Box(comptime T: type) type {
    return struct { v: T };
}

const A = Box(i64);
const B = Box(i64); // the same (Box, i64) — must reuse A's struct id

fn take_b(b: B) i64 {
    return b.v;
}

pub fn main() void {
    var a: A = A{ .v = 9 };
    var b: B = a; // assignable only if A and B are the same type
    print(take_b(a) + b.v); // an A also passes as a B parameter: 9 + 9 = 18
}
