//SPEC: §38 `%` stays integer-only — `f64 % f64` is rejected even though both operands match
//ERR: E0110

pub fn main() void {
    var a: f64 = 7.5;
    var b: f64 = 2.0;
    print(a % b);  // E0110: arithmetic operand must be an integer
}
