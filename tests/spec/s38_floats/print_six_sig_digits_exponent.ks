//SPEC: §38.1 %g keeps 6 significant digits and switches to exponent form for large magnitudes
//OUT: 0.333333
//OUT: 1.23457e+06
//OUT: 1e+08

pub fn main() void {
    var one: f64 = 1.0;
    var three: f64 = 3.0;
    print(one / three);      // 6 significant digits: "0.333333"
    var big: f64 = 1234567.0;
    print(big);              // 7 digits round to 6 and go exponential
    var huge: f64 = 100000000.0;
    print(huge);             // 1e8 — trailing zero digits dropped: "1e+08"
}
