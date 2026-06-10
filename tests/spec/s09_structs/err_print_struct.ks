//SPEC: §9.4 `print` rejects struct arguments (integers, f64, and strings only) — E0110
//ERR: E0110
const P = struct {
    x: i32,
};

pub fn main() void {
    var p: P = P{ .x = 1 };
    print(p);   // whole structs are not printable
}
