//SPEC: §33 narrowing to a signed type wraps two's-complement — values past the signed max come back negative; the boundary values are exact
//OUT: -56
//OUT: -128
//OUT: 127
//OUT: -25536
//OUT: -1
pub fn main() void {
    print(@as(i64, @as(i8, 200)));            // 200 - 256 = -56
    print(@as(i64, @as(i8, 128)));            // one past i8 max -> i8 min
    print(@as(i64, @as(i8, 127)));            // i8 max is preserved
    print(@as(i64, @as(i16, 40000)));         // 40000 - 65536 = -25536
    var ones: i64 = 4294967295;               // 2^32 - 1: all-ones in 32 bits
    print(@as(i64, @as(i32, ones)));          // -1
}
