//SPEC: §38 f64 division and ordered comparison on exactly-representable values
//OUT: 3
//OUT: 1
//OUT: 0.375

pub fn main() void {
    var a: f64 = 7.5;
    var b: f64 = 2.5;
    print(a / b);
    if (a > b) { print(1); } else { print(0); }
    print(3.0 / 8.0);
}
