//SPEC: §25.2 an alias type argument must be an identifier (or nested application) — an arbitrary expression is E0311
//ERR: E0311

fn Box(comptime T: type) type {
    return struct { v: T };
}

const A = Box(1 + 2); // an arithmetic expression can never name a type

pub fn main() void {
    print(0);
}
