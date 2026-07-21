//SPEC: §3 unary `-` requires a SIGNED INTEGER — an f64 operand is rejected (write `0.0 - x`)
//ERR: E0110

pub fn main() void {
    var x: f64 = 1.5;
    print(-x);
}
