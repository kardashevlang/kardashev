//SPEC: §41×std parse_i64 — `@readLine` strips the newline and `parse_i64` reads sign+digits, so piped numbers (negative included) compute exactly
//STDIN: 40
//STDIN: -15
//OUT: 25
//OUT: -600

@import("std");

pub fn main() void {
    var a: Allocator = c_allocator();
    var l1: []u8 = @readLine(a);
    var l2: []u8 = @readLine(a);

    // A leftover '\n' or '\r' in the slice would make parse_i64 return
    // null and these fall back to 0 - 9999.
    var x: i64 = parse_i64(l1) orelse 0 - 9999;
    var y: i64 = parse_i64(l2) orelse 0 - 9999;

    print(x + y);     // 40 + (-15) = 25
    print(x * y);     // 40 * (-15) = -600
}
