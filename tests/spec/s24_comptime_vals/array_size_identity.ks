//SPEC: §24.2 arrays intern on the RESOLVED (elem, len) — `[n]T` at n=3 is the same type as the literal `[3]T`, both directions
//OUT: 42

// Inward: a literal-sized `[3]i64` value initialises an `[n]i64`-typed local.
// Outward: the `[n]i64` return value lands in a literal-typed `[3]i64` var at
// the call site. Either assignment fails if `[n]T` interned a distinct type.
fn make(comptime n: i64, seed: i64) [n]i64 {
    var a: [n]i64 = [3]i64{ 0, 0, 0 };
    a[0] = seed;
    a[1] = seed * 2;
    a[2] = seed * 3;
    return a;
}

pub fn main() void {
    var got: [3]i64 = make(3, 7);
    print(got[0] + got[1] + got[2]); // 7 + 14 + 21 = 42
}
