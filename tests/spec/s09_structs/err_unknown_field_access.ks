//SPEC: §9.4 accessing a field the struct does not declare is E0166
//ERR: E0166
const P = struct {
    x: i32,
};

pub fn main() void {
    var p: P = P{ .x = 1 };
    print(p.y);   // P has `x`, not `y`
}
