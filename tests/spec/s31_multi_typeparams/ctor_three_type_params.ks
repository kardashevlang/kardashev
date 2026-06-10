//SPEC: §31.1 "one or more" — a type-constructor with THREE type parameters substitutes each independently
//OUT: 74576
//OUT: 3
fn Triple(comptime A: type, comptime B: type, comptime C: type) type {
    return struct {
        a: A,
        b: B,
        c: C,

        // Distinct truncation per parameter proves all three bound separately:
        // 70000 -> u8: 70000 mod 256 = 112; u16: 70000 mod 65536 = 4464; i64: 70000.
        fn spread(self: Self, x: i64) i64 {
            return @as(i64, @as(A, x)) + @as(i64, @as(B, x)) + @as(i64, @as(C, x));
        }
    };
}

const T1 = Triple(u8, u16, i64);

pub fn main() void {
    var t: T1 = T1{ .a = 1, .b = 2, .c = 3 };
    print(t.spread(70000));   // 112 + 4464 + 70000
    print(t.c);               // 3
}
