//SPEC: §25.2 an alias type argument must name a TYPE — an identifier bound to a value const is E0311
//ERR: E0311

fn Box(comptime T: type) type {
    return struct { v: T };
}

const NOT_A_TYPE = 5;
const A = Box(NOT_A_TYPE);

pub fn main() void {
    print(0);
}
