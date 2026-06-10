//SPEC: §40 a `for` loop takes a label too — `break :outer`/`continue :outer` target it from an inner `while`
//OUT: 145

pub fn main() void {
    var xs: [3]i64 = [3]i64{ 10, 20, 30 };
    var total: i64 = 0;
    outer: for (xs) |x| {
        var k: i64 = 0;
        while (k < 10) : (k = k + 1) {
            if (x == 20) {
                continue :outer;   // skip to the NEXT array element
            }
            if (x == 30) {
                break :outer;      // abandon the whole iteration
            }
            total = total + x + k;
        }
    }
    // Only x=10 accumulates: sum over k of (10+k) = 100 + 45 = 145.
    // x=20 contributes nothing (continue), x=30 stops everything (break).
    print(total);
}
