//SPEC: §28.4 arithmetic on 8/16-bit operands truncates back to the operand width when read directly (v0.185: + - * / and unary - join ~ and <<)
//OUT: 44
//OUT: 32
//OUT: 156
//OUT: -128
//OUT: -128
//OUT: 14464

pub fn main() void {
    var a: u8 = 200;
    var b: u8 = 100;
    print(a + b);   // 300 mod 256 = 44
    print(a * b);   // 20000 mod 256 = 32
    print(b - a);   // -100 wraps to 156
    var d: i8 = 0 - 128;
    print(-d);      // 128 wraps to -128 at i8
    var e: i8 = 0 - 1;
    print(d / e);   // 128 wraps to -128
    var f: u16 = 40000;
    print(f + f);   // 80000 mod 65536 = 14464
}
