//SPEC: §20.2 a plain struct field of union type is rejected — struct fields resolve (Pass 0b) before unions intern (Pass 0c)
//ERR: E0161

const U = union(enum) { a: i64 };
const Holder = struct { u: U };

pub fn main() void {
    print(0);
}
