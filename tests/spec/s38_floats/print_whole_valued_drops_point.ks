//SPEC: §38.1 %g prints a whole-valued `f64` without a decimal point
//OUT: 3
//OUT: 0
//OUT: 8

pub fn main() void {
    var a: f64 = 3.0;
    print(a);            // "3", not "3.0"
    var z: f64 = 0.0;
    print(z);            // "0"
    var b: f64 = 2.0;
    var c: f64 = 4.0;
    print(b * c);        // a COMPUTED whole value prints the same way: "8"
}
