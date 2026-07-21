//SPEC: §37 `@enumFromInt` performs no range check: an unmatched value round-trips raw through `@intFromEnum`
//OUT: 7

const Color = enum { Red = 1, Green = 2, Blue = 10 };

pub fn main() void {
    var c: Color = @enumFromInt(Color, 7);
    print(@intFromEnum(c));   // 7 never names a variant; the value is preserved
}
