//SPEC: §12.3×§29 `try` inside a `for` body propagates mid-iteration — the loop stops at the failing element and the fn error-returns; completed iterations' work stands
//OUT: 15
//OUT: 3
//OUT: -1
//OUT: 2

fn check(x: i64) !i64 {
    if (x > 30) {
        return error.TooBig;
    }
    return x;
}

// `steps` counts COMPLETED iterations; on a propagating `try` the current
// iteration's tail (after the try) must not run.
fn sum_until(xs: []i64, steps: *i64) !i64 {
    var total: i64 = 0;
    for (xs) |x| {
        var v: i64 = try check(x);
        total += v;
        steps.* += 1;
    }
    return total;
}

pub fn main() void {
    var ok: [3]i64 = [3]i64{ 4, 5, 6 };
    var bad: [4]i64 = [4]i64{ 10, 20, 40, 5 };

    var steps: i64 = 0;
    print(sum_until(ok[0..3], &steps) catch 0 - 1);  // 4+5+6 = 15
    print(steps);                                    // all 3 iterations ran

    steps = 0;
    print(sum_until(bad[0..4], &steps) catch 0 - 1); // 40 fails -> -1
    print(steps);                                    // only 10, 20 completed
}
