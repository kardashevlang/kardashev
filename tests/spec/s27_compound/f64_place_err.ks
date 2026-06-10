//SPEC: §27.2 compound assignment is integer-only — an `f64` place is rejected even though plain `x = x + 1.0` is fine
//ERR: E0110

pub fn main() void {
    var x: f64 = 1.5;
    x += 1.0;
}
