//SPEC: §23 x std StrBuilder x §2 loops — a string assembled byte-by-byte over iterations equals its literal spelling
//OUT: 11,22,33,
//OUT: 9
//OUT: 1

@import("std");

// Three loop turns append a formatted integer and a ',' (byte 44); the built
// string must match the literal, byte for byte, via str_eq.
pub fn main() void {
    var a: Allocator = c_allocator();
    var sb: StrBuilder = StrBuilder.init(a);
    var i: i64 = 1;
    while (i <= 3) : (i += 1) {
        sb.append_i64(a, i * 11);
        sb.append_byte(a, 44);         // ','
    }
    var s: []u8 = sb.build(a);
    print(s);                          // 11,22,33,
    print(@as(i64, sb.len()));         // 2+1 + 2+1 + 2+1 = 9
    if (str_eq(s, "11,22,33,")) {
        print(1);
    } else {
        print(0);
    }
    sb.deinit(a);
    free(a, s);
}
