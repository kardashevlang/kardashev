//SPEC: §33 narrowing to an unsigned type truncates modulo 2^N (the C cast semantics of the documented lowering)
//OUT: 44
//OUT: 0
//OUT: 4464
//OUT: 5
pub fn main() void {
    print(@as(i64, @as(u8, 300)));           // 300 mod 256 = 44
    print(@as(i64, @as(u8, 256)));           // exactly 2^8 -> 0
    print(@as(i64, @as(u16, 70000)));        // 70000 mod 65536 = 4464
    var big: i64 = 4294967301;               // 2^32 + 5
    print(@as(i64, @as(u32, big)));          // mod 2^32 = 5
}
