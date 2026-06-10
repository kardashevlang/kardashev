//SPEC: §25.2 a type alias is not a value — using it in value position takes the unknown-name path (E0100)
//ERR: E0100

fn Box(comptime T: type) type {
    return struct { v: T };
}

const A = Box(i64);

pub fn main() void {
    print(A); // a type, not a printable value
}
