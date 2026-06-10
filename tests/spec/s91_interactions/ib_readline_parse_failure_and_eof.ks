//SPEC: §41×std parse_i64 — a non-numeric line and a stray sign parse to null; `@readLine` at EOF yields an empty slice, which also parses to null
//STDIN: 12x
//STDIN: -
//OUT: -1
//OUT: -2
//OUT: 0
//OUT: -3

@import("std");

pub fn main() void {
    var a: Allocator = c_allocator();

    var l1: []u8 = @readLine(a);              // "12x": non-digit byte
    print(parse_i64(l1) orelse 0 - 1);        // -1

    var l2: []u8 = @readLine(a);              // "-": a stray sign
    print(parse_i64(l2) orelse 0 - 2);        // -2

    var l3: []u8 = @readLine(a);              // EOF
    print(l3.len);                            // 0 — documented empty slice
    print(parse_i64(l3) orelse 0 - 3);        // empty string -> null -> -3
}
