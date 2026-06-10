//SPEC: §33 signed↔unsigned casts reinterpret modulo 2^N — `-1` becomes the all-ones unsigned value at each width and round-trips back
//OUT: 4294967295
//OUT: 255
//OUT: -1
//OUT: -1
pub fn main() void {
    var n: i64 = 0 - 1;
    print(@as(i64, @as(u32, n)));     // 2^32 - 1
    print(@as(i64, @as(u8, n)));      // 2^8 - 1

    var m: u8 = 255;
    print(@as(i64, @as(i8, m)));      // all-ones u8 reads as i8 -1

    print(@as(i64, @as(u64, n)));     // i64 -1 -> u64 (2^64-1) -> i64: round-trips
}
