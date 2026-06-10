//SPEC: §25.2 an alias instantiation must pass exactly as many type arguments as the constructor has parameters — wrong arity is E0311
//ERR: E0311

fn Box(comptime T: type) type {
    return struct { v: T };
}

const A = Box(i32, i64); // 2 arguments to a 1-parameter constructor

pub fn main() void {
    print(0);
}
