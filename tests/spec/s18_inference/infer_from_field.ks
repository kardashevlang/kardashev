//SPEC: §18.2 an inferred binding adopts a field access's type (and a struct literal its struct type)
//OUT: 65535
const Size = struct { w: u16, h: u16 };
pub fn main() void {
    var s = Size{ .w = 640, .h = 480 }; // inferred: Size
    var w = s.w;                        // inferred: u16
    var border: u16 = 64895;
    print(w + border); // 640 + 64895 = 65535 (u16 max) — u16-only arithmetic
}
