//SPEC: §14.1 `[N]T{…}` stores N elements in order; `a[i]` reads them back
//OUT: 39

// Evaluate the polynomial 3 + 0x + 5x^2 + 2x^3 at x = 2 from its coefficient
// array: 3 + 0 + 20 + 16 = 39. Any reordering or misread of the literal's
// elements changes the result.
pub fn main() void {
    var c: [4]i64 = [4]i64{ 3, 0, 5, 2 };
    var x: i64 = 2;
    var value: i64 = 0;
    var p: i64 = 1;
    var i: usize = 0;
    while (i < 4) : (i = i + 1) {
        value = value + c[i] * p;
        p = p * x;
    }
    print(value);
}
