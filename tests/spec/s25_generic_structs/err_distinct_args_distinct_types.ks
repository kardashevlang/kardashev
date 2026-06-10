//SPEC: §25.2 different type arguments yield DIFFERENT struct types — a `Box(i64)` value does not coerce to `Box(i32)` (E0110)
//ERR: E0110

fn Box(comptime T: type) type {
    return struct { v: T };
}

const A = Box(i64);
const B = Box(i32);

pub fn main() void {
    var a: A = A{ .v = 1 };
    var b: B = a; // Box__int64_t vs Box__int32_t — nominal mismatch
    print(b.v);
}
