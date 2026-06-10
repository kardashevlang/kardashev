//SPEC: §25.2 a type-constructor parameter must be marked `comptime` — a plain runtime parameter is E0310
//ERR: E0310

fn Box(x: i64) type {
    return struct { v: i64 };
}

pub fn main() void {
    print(0);
}
