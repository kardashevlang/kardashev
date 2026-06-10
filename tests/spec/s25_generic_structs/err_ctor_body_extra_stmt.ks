//SPEC: §25.2 a type-constructor body must be EXACTLY `return struct { … };` — a preceding statement is E0310
//ERR: E0310

fn Box(comptime T: type) type {
    var pad: i64 = 1; // anything before the return disqualifies the body
    return struct { v: T };
}

pub fn main() void {
    print(0);
}
