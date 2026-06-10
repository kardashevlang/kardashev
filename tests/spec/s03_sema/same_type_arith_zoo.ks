//SPEC: §3 binary arithmetic requires both operands the same integer type; the result is that type
//OUT: 45150
//OUT: 2147483647
//OUT: 720
fn sum_to_u16(n: u16) u16 {
    var t: u16 = 0;
    var i: u16 = 1;
    while (i <= n) : (i = i + 1) {
        t = t + i;
    }
    return t;
}
pub fn main() void {
    // 1 + 2 + ... + 300 = 45150: above i16's max (32767) but inside u16 —
    // the whole loop runs in u16 arithmetic.
    print(sum_to_u16(300));

    // i32 + i32 lands exactly on the i32 maximum (no overflow).
    var a: i32 = 2147483000;
    var b: i32 = 647;
    print(a + b);

    // usize-only factorial: every multiply and compare is usize/usize.
    var n: usize = 6;
    var f: usize = 1;
    var i: usize = 2;
    while (i <= n) : (i = i + 1) {
        f = f * i;
    }
    print(f); // 6! = 720
}
