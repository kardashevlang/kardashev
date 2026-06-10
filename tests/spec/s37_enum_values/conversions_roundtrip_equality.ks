//SPEC: §37 `@enumFromInt(E, n)` ↔ `@intFromEnum(e)` round-trip exact explicit values; the converted value compares equal to the variant
//OUT: 1
//OUT: 4
//OUT: 8

const Flag = enum { A = 1, B = 2, C = 4, D = 8 };

// Runtime arithmetic through the conversions: doubling a power-of-two flag
// moves to the next variant — only true under explicit (non-ordinal) values.
fn dbl(f: Flag) Flag {
    return @enumFromInt(Flag, @intFromEnum(f) * 2);
}

pub fn main() void {
    if (@enumFromInt(Flag, 4) == Flag.C) {
        print(1);
    } else {
        print(0);
    }
    print(@intFromEnum(dbl(dbl(.A))));        // 1 -> 2 -> 4
    print(@intFromEnum(@enumFromInt(Flag, 8)));   // literal round-trip
}
