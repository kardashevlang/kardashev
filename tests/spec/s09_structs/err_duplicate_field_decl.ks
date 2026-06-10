//SPEC: §9.4 a duplicate field name within one struct declaration is E0162
//ERR: E0162
// The second `x` collides with the first even though its type differs.
const P = struct {
    x: i32,
    y: i32,
    x: bool,
};

pub fn main() void {}
