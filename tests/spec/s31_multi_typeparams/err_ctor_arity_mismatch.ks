//SPEC: §31.1 a type alias must pass exactly as many type arguments as the constructor has type parameters
//ERR: E0311
fn Map(comptime K: type, comptime V: type) type {
    return struct {
        k: K,
        v: V,
    };
}

const TooFew = Map(u8);            // 1 of 2
const TooMany = Map(u8, i64, u8);  // 3 of 2

pub fn main() void {
    print(1);
}
