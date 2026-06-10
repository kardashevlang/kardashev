//SPEC: §3 each integer width is distinct and observable: `@as` (§33) truncates modulo 2^N to unsigned targets and preserves in-range values
//OUT: 44
//OUT: 300
//OUT: 300
//OUT: 4294967596
//OUT: 300
//OUT: 23456
//OUT: 56
//OUT: 23400
pub fn main() void {
    var big: i64 = 4294967596; // 2^32 + 300
    print(@as(u8, big));       // (2^32 + 300) mod 2^8  = 300 mod 256 = 44
    print(@as(u16, big));      // (2^32 + 300) mod 2^16 = 300
    print(@as(u32, big));      // (2^32 + 300) mod 2^32 = 300
    print(@as(u64, big));      // in range — value preserved
    print(@as(usize, big - 4294967296)); // 300, in range on any target

    // Signed targets: in-range casts preserve the value exactly.
    var n: i64 = 23456;
    var h: i16 = @as(i16, n);       // 23456 <= 32767, fits i16
    var c: i8 = @as(i8, n % 200);   // 23456 mod 200 = 56, fits i8
    print(h);
    print(c);
    print(@as(i64, h) - @as(i64, c)); // 23456 - 56 = 23400
}
