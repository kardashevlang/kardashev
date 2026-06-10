//SPEC: §18.2 an inferred binding adopts a call's return type
//OUT: 255
fn double_u8(n: u8) u8 {
    return n * 2;
}
pub fn main() void {
    var r = double_u8(100); // inferred u8 (the fn's return type), value 200
    var d: u8 = 55;
    print(r + d); // u8 + u8 = 255 (u8 max, in range) — only checks if r is u8
}
