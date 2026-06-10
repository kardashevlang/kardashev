//SPEC: §36 x §39 — `catch |e|` encodes errors as sentinels that a range/multi-label `switch` dispatches on
//OUT: -201
//OUT: -202
//OUT: 3
//OUT: 6
//OUT: 9
//OUT: 999
//OUT: 6

// f yields n*3 or one of two error codes; the handler maps code e to 200+e,
// and the switch dispatches: a multi-label arm for the two sentinels, an
// inclusive range arm for small results, else for the rest.
fn f(n: i64) !i64 {
    if (n < 0) {
        return error.Neg;      // code 1
    }
    if (n == 0) {
        return error.Zero;     // code 2
    }
    return n * 3;
}

pub fn main() void {
    var i: i64 = 0 - 1;
    while (i < 5) : (i += 1) {
        var r: i64 = f(i) catch |e| 200 + @as(i64, e);
        switch (r) {
            201, 202 => { print(0 - r); },   // multi-label: the two errors
            1..9 => { print(r); },           // i=1,2,3 -> 3, 6, 9 (hi inclusive)
            else => { print(999); },         // i=4 -> 12 lands here
        }
    }
    print(f(2) catch |e| @as(i64, e));       // 6: ok path skips the handler
}
