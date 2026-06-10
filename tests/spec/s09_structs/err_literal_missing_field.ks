//SPEC: §9.4 a struct literal must initialise every declared field — a missing field is E0164
//ERR: E0164
const P = struct {
    x: i32,
    y: i32,
    z: i32,
};

pub fn main() void {
    var p: P = P{ .x = 1, .y = 2 };   // `z` never initialised
    print(p.x);
}
