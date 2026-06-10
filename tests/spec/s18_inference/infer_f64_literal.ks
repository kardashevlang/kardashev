//SPEC: §18.2 an inferred binding adopts `f64` from a float literal (floats are concrete, not flexible)
//OUT: 3.75
//OUT: 15
pub fn main() void {
    var x = 1.5;   // inferred f64
    var y = 2.25;  // inferred f64
    var s = x + y; // f64 + f64 — 3.75, exactly representable
    print(s);
    var t = s * 4.0; // 15.0 — %g prints it as "15"
    print(t);
}
