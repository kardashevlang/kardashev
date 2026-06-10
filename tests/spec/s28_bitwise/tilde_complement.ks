//SPEC: §28.2 unary `~` is bitwise complement — its own inverse, and usable in the clear-bits idiom `a & ~m`
//OUT: -6
//OUT: 5
//OUT: -1
//OUT: 0
//OUT: 240

pub fn main() void {
    var x: i64 = 5;
    print(~x);          // ~5 = -6 (two's complement)
    print(~(~x));       // double complement restores the value
    var z: i64 = 0;
    print(~z);          // ~0 = -1
    print(~(0 - 1));    // ~(-1) = 0
    var a: i64 = 255;
    var m: i64 = 15;
    print(a & ~m);      // clear the low nibble: 240 (infix `&` then unary `~`)
}
