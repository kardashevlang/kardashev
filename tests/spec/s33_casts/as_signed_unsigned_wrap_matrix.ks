//SPEC: §33 `@as` between signed and unsigned reinterprets two's-complement at the target width
//OUT: 255
//OUT: -56
//OUT: 65534
//OUT: 4294967293

pub fn main() void {
    print(@as(u8, 0 - 1));     // 255
    var b: u8 = 200;
    print(@as(i8, b));         // -56
    print(@as(u16, 0 - 2));    // 65534
    print(@as(u32, 0 - 3));    // 4294967293
}
