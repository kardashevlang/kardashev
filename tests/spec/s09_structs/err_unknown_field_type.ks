//SPEC: §9.4 an unknown type name in a struct field is E0161
//ERR: E0161
const P = struct {
    x: i32,
    y: NoSuchType,
};

pub fn main() void {}
