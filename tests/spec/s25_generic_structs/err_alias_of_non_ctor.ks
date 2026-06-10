//SPEC: §25.2 instantiating a non-type-constructor as a type alias is E0311 — an ordinary function cannot return a type
//ERR: E0311

fn double(x: i64) i64 {
    return x * 2;
}

const A = double(i64); // `double` is a value function, not a type-constructor

pub fn main() void {
    print(0);
}
