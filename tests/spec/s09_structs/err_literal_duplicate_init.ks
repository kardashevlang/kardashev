//SPEC: §9.4 a struct literal may not initialise the same field twice — a duplicate init is E0164
//ERR: E0164
const P = struct {
    x: i32,
    y: i32,
};

pub fn main() void {
    var p: P = P{ .x = 1, .y = 2, .x = 3 };   // `.x` written twice
    print(p.y);
}
