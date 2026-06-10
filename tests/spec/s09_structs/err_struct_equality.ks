//SPEC: §9.4 `==` / `!=` are not defined on struct types — both are E0168
//ERR: E0168
const P = struct {
    x: i32,
};

pub fn main() void {
    var a: P = P{ .x = 1 };
    var b: P = P{ .x = 1 };
    if (a == b) { print(1); }
    if (a != b) { print(2); }
}
