//SPEC: §25.2 every type-constructor parameter must be `comptime _: type` — a comptime VALUE parameter is E0310
//ERR: E0310

// `comptime n: usize` is legal on a generic FUNCTION (§24) but not on a
// type-returning function: type-constructors take type parameters only.
fn Vec(comptime n: usize) type {
    return struct { x: i64 };
}

pub fn main() void {
    print(0);
}
