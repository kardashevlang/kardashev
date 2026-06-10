//SPEC: §41×std parse_i64 — exactly i64 min/max round-trip through stdin; one past max overflows to null (the documented overflow check)
//STDIN: 9223372036854775807
//STDIN: -9223372036854775808
//STDIN: 9223372036854775808
//OUT: 9223372036854775807
//OUT: -9223372036854775808
//OUT: -1

@import("std");

pub fn main() void {
    var a: Allocator = c_allocator();
    var l1: []u8 = @readLine(a);
    var l2: []u8 = @readLine(a);
    var l3: []u8 = @readLine(a);
    print(parse_i64(l1) orelse 0 - 1);   // i64 max parses
    print(parse_i64(l2) orelse 0 - 1);   // i64 min parses (negative accumulate)
    print(parse_i64(l3) orelse 0 - 1);   // max + 1 -> null -> -1
}
