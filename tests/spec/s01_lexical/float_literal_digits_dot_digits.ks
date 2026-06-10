//SPEC: §1/§38 a float literal is digits `.` digits and has type f64
//OUT: 2.75
//OUT: 1
pub fn main() void {
    var f: f64 = 3.25;
    var g: f64 = 0.5;
    print(f * g + 1.125);
    // An exact dyadic product: 0.125 * 8.0 == 1.0 (prints as `1` via %g).
    var h: f64 = 0.125;
    print(h * 8.0);
}
