//SPEC: §9.4 a struct literal may not initialise a field the struct does not declare — an extra field is E0164
//ERR: E0164
const P = struct {
    x: i32,
};

pub fn main() void {
    var p: P = P{ .x = 1, .z = 2 };   // `z` is not a field of P
    print(p.x);
}
