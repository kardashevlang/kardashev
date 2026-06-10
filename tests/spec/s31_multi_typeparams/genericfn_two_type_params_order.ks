//SPEC: §31 a generic FUNCTION monomorphises on its tuple of comptime arguments — two type parameters bind in declaration order
//OUT: 300000044
//OUT: 44000300
//OUT: 300000300
fn conv(comptime A: type, comptime B: type, x: i64) i64 {
    // A truncates the low term, B the scaled term: swapping the type
    // arguments must swap which term collapses to 44 (300 mod 256).
    return @as(i64, @as(A, x)) + 1000000 * @as(i64, @as(B, x));
}

pub fn main() void {
    print(conv(u8, u16, 300));    // 44 + 1000000*300
    print(conv(u16, u8, 300));    // 300 + 1000000*44 — a DIFFERENT instance
    print(conv(i64, i64, 300));   // 300 + 1000000*300 — a third tuple
}
