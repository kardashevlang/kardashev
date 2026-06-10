//SPEC: §17.1 a call `g(T1, …, a1, …)` to a generic passes the leading args as type arguments
//OUT: 12
//OUT: 6
//OUT: 25

// Euclid's gcd as a generic: the first argument of every call is the type
// argument; the remaining two are runtime values checked against the
// substituted parameter types. If the type-argument-first call protocol broke,
// none of these calls would resolve.
fn gcd(comptime T: type, a: T, b: T) T {
    var x: T = a;
    var y: T = b;
    while (y != 0) {
        var r: T = x % y;
        x = y;
        y = r;
    }
    return x;
}

pub fn main() void {
    print(gcd(i32, 48, 36));        // 48,36 -> 36,12 -> 12,0
    print(gcd(i64, 270, 192));      // 270,192 -> 192,78 -> 78,36 -> 36,6 -> 6,0
    print(gcd(u32, 100, 75));       // 100,75 -> 75,25 -> 25,0
}
