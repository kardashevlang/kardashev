//SPEC: §38 a `digits.digits` literal is `f64`; `print(f64)` is %g-style — minimal decimal text, fraction kept only when non-zero
//OUT: 12.5
//OUT: 0.0001
//OUT: -2.75

pub fn main() void {
    var a: f64 = 12.5;
    print(a);            // "12.5" — not "12.500000"
    var b: f64 = 0.0001;
    print(b);            // small magnitudes stay decimal under %g
    // No unary minus on `f64`; a negative value comes from arithmetic (§38:
    // the binary ops accept two `f64`s).
    var c: f64 = 0.0 - 2.75;
    print(c);
}
