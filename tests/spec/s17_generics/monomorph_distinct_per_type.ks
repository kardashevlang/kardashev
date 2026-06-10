//SPEC: §17.2 monomorphisation — each distinct type argument yields its own specialised instance
//OUT: 44
//OUT: 556
//OUT: 255
//OUT: 4608
//OUT: 70144
//OUT: 255
//OUT: 65535
//OUT: 9223372036854775807

// One generic body; its behaviour must differ per instantiation because `T`
// drives the `@as` truncation. If all calls collapsed into one instance the
// u8 / u16 / i64 columns could not disagree.
fn low_bits(comptime T: type, x: i64) i64 {
    return @as(i64, @as(T, x));
}

pub fn main() void {
    // x = 2^9 = 512, computed by doubling.
    var x: i64 = 1;
    var i: i64 = 0;
    while (i < 9) : (i = i + 1) {
        x = x * 2;
    }
    print(low_bits(u8, x + 44));        // 556 mod 256  = 44
    print(low_bits(u16, x + 44));       // 556 fits u16  = 556
    print(low_bits(u8, x - 1));         // 511 mod 256  = 255
    print(low_bits(u16, x * 137));      // 70144 mod 65536 = 4608
    print(low_bits(i64, x * 137));      // 70144 unchanged

    // Boundary: i64 max, built by arithmetic (2^62 - 1 + 2^62 = 2^63 - 1).
    var p62: i64 = 1;
    i = 0;
    while (i < 62) : (i = i + 1) {
        p62 = p62 * 2;
    }
    var max: i64 = (p62 - 1) + p62;
    print(low_bits(u8, max));           // all-ones low byte  = 255
    print(low_bits(u16, max));          // all-ones low half  = 65535
    print(low_bits(i64, max));          // identity           = 9223372036854775807
}
