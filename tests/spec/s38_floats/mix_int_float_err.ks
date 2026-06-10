//SPEC: §38 there is no implicit int↔float mixing — `f64 + i64` is rejected
//ERR: E0110

pub fn main() void {
    var a: f64 = 1.5;
    var n: i64 = 2;
    var b: f64 = a + n;  // E0110: no implicit conversion; cast with `@as`
    print(b);
}
