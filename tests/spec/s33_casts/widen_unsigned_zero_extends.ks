//SPEC: §33 a widening cast from an unsigned integer zero-extends — high-half values stay positive through u8→u16→u32→u64 and into signed targets
//OUT: 200
//OUT: 65535
//OUT: 255
pub fn main() void {
    var a: u8 = 200;                 // high bit set: sign-extension would go negative
    var b: u16 = @as(u16, a);
    var c: u32 = @as(u32, b);
    var d: u64 = @as(u64, c);
    print(@as(i64, d));              // 200, not -56

    var m: u16 = 65535;              // u16 all-ones
    print(@as(i64, m));              // 65535

    var n: u8 = 255;
    print(@as(i32, n));              // 255 — unsigned into a WIDER signed type
}
