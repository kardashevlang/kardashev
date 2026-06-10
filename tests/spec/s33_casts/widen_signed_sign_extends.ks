//SPEC: §33 a widening cast between signed integers preserves the value — negative values sign-extend through i8→i16→i32→i64
//OUT: -100
//OUT: -2000000000
//OUT: -32768
pub fn main() void {
    var a: i8 = 0 - 100;
    var b: i16 = @as(i16, a);
    var c: i32 = @as(i32, b);
    var d: i64 = @as(i64, c);
    print(d);                        // -100 survived three widenings

    var e: i32 = 0 - 2000000000;
    print(@as(i64, e));              // -2000000000

    var f: i16 = @as(i16, 0 - 32768);   // i16 min
    print(@as(i64, f));              // -32768
}
