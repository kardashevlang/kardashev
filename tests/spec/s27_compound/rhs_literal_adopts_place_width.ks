//SPEC: §27.2+§3 an integer-literal rhs adopts the place's integer type (it does not default to i64)
//OUT: 65000
//OUT: 217
//OUT: 3000000000

// If the literal defaulted to `i64` the same-type rule would reject every
// statement here, so compiling AND computing in the narrow type is the pin.
pub fn main() void {
    var x: u16 = 40000;
    x += 25000;            // u16 arithmetic: 65000 (beyond i16's range)
    print(x);
    var y: u8 = 7;
    y *= 31;               // u8 arithmetic: 217
    print(y);
    var z: u32 = 4000000000;
    z -= 1000000000;       // u32 arithmetic: 3000000000 (beyond i32's range)
    print(z);
}
