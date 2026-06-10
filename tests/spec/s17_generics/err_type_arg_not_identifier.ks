//SPEC: §17.2 a type argument that is not an identifier naming a type is E0251
//ERR: E0251

fn id(comptime T: type, x: T) T {
    return x;
}

pub fn main() void {
    // The leading argument position belongs to the comptime type parameter;
    // a literal can never name a type.
    print(id(5, 7));
}
