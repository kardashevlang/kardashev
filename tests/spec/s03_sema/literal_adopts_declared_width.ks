//SPEC: §3 an integer literal adopts the expected integer type at its use site
//OUT: 255
//OUT: 65000
//OUT: 295
pub fn main() void {
    // Both literals adopt `u8`; their sum is exactly the u8 maximum
    // (no overflow — the math stays in range).
    var a: u8 = 250;
    var b: u8 = 5;
    print(a + b);

    // u16 boundary: the max literal adopts u16, subtraction stays in range.
    var hi: u16 = 65535;
    var lo: u16 = 535;
    print(hi - lo);

    // u32 boundary: 4294967295 only fits an unsigned 32-bit (or wider) type;
    // the bare literal in the subtraction is anchored to `w`'s u32.
    var w: u32 = 4294967295;
    print(w - 4294967000);
}
