//SPEC: §9.4 the assigned value must match the field's type — a mismatch on `p.f = e` is E0110
//ERR: E0110
const P = struct {
    x: i32,
};

pub fn main() void {
    var p: P = P{ .x = 1 };
    p.x = true;   // bool into an i32 field
    print(p.x);
}
