//SPEC: §9.4 each literal init must match the declared field type — a mismatch is E0110
//ERR: E0110
const P = struct {
    x: i32,
};

pub fn main() void {
    var p: P = P{ .x = true };   // bool into an i32 field
    print(p.x);
}
