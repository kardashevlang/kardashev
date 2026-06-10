//SPEC: §28.2 `& | ^` and shifts work on the signed widths i8/i16/i32 (non-negative values)
//OUT: 36
//OUT: 101
//OUT: 65
//OUT: 1360
//OUT: 24565
//OUT: 23205
//OUT: 32
//OUT: 5592320
//OUT: 1442840405
//OUT: 1437248085
//OUT: 48

pub fn main() void {
    // i8: 100 = 0b1100100, 37 = 0b0100101
    var a8: i8 = 100;
    var b8: i8 = 37;
    var r1: i8 = a8 & b8;
    var r2: i8 = a8 | b8;
    var r3: i8 = a8 ^ b8;
    print(r1);  // 36
    print(r2);  // 101
    print(r3);  // 65

    // i16: 0x5555 vs 0x0FF0
    var a16: i16 = 21845;
    var b16: i16 = 4080;
    var s1: i16 = a16 & b16;
    var s2: i16 = a16 | b16;
    var s3: i16 = a16 ^ b16;
    print(s1);  // 1360
    print(s2);  // 24565
    print(s3);  // 23205
    var h: i16 = 1024;
    var s4: i16 = h >> 5;   // a positive value right-shifts as division: 32
    print(s4);

    // i32: 0x55555555 vs 0x00FFFF00
    var a32: i32 = 1431655765;
    var b32: i32 = 16776960;
    var t1: i32 = a32 & b32;
    var t2: i32 = a32 | b32;
    var t3: i32 = a32 ^ b32;
    print(t1);  // 5592320
    print(t2);  // 1442840405
    print(t3);  // 1437248085
    var c32: i32 = 3;
    var t4: i32 = c32 << 4;
    print(t4);  // 48
}
