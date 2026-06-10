//SPEC: §38 `+ - * /` on two `f64` are IEEE-exact on dyadic-representable values
//OUT: 0.75
//OUT: 1.25
//OUT: 3.75
//OUT: 3.75

pub fn main() void {
    // Every operand and every result below is a sum of powers of two, so the
    // arithmetic is exact — a broken operation changes the printed text.
    var a: f64 = 0.5;
    var b: f64 = 0.25;
    print(a + b);        // 0.75
    var c: f64 = 1.5;
    print(c - b);        // 1.25
    var d: f64 = 2.5;
    print(d * c);        // 3.75
    var e: f64 = 7.5;
    var two: f64 = 2.0;
    print(e / two);      // 3.75
}
