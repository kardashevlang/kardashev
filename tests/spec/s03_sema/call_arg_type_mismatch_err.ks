//SPEC: §3 each call argument must match its parameter's declared type
//ERR: E0110
fn half(n: u8) u8 {
    return n / 2;
}
pub fn main() void {
    var v: i32 = 8;
    print(half(v));
}
