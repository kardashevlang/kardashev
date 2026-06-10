//SPEC: §21.2 x §36.1 — the callee's errdefer flushes BEFORE the caller's `catch |e|` handler runs on the code
//OUT: 803
//OUT: 30
//OUT: 802
//OUT: 902
//OUT: -1
//OUT: 804
//OUT: 904
//OUT: -2

// risky(odd) succeeds: only the defer fires and the handler is skipped.
// risky(even) fails with one of TWO distinct codes: the flush (defer then
// errdefer, LIFO) must already have printed when the handler's value lands.
fn risky(n: i64) !i64 {
    errdefer print(900 + n);
    defer print(800 + n);
    if (n % 2 == 0) {
        if (n % 4 == 2) {
            return error.Half;     // first error name mentioned -> code 1
        }
        return error.Whole;        // second -> code 2
    }
    return n * 10;
}

pub fn main() void {
    print(risky(3) catch |e| 0 - @as(i64, e));   // 803, then 30
    print(risky(2) catch |e| 0 - @as(i64, e));   // 802, 902, then -1
    print(risky(4) catch |e| 0 - @as(i64, e));   // 804, 904, then -2
}
