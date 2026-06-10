//SPEC: §4.4×§29 a `defer` in a `for` body flushes at the END of every iteration — including the iteration whose `try` propagates, before the error return
//OUT: 100
//OUT: 101
//OUT: 14
//OUT: 100
//OUT: 101
//OUT: -50

fn check(x: i64) !i64 {
    if (x > 30) {
        return error.TooBig;
    }
    return x * 2;
}

// The defer is registered in the LOOP-BODY scope, so it runs once per
// iteration (not once at fn exit). On the run over `bad`, iteration 1's
// `try` propagates — that iteration's defer (101) must still flush.
fn run(xs: []i64) !i64 {
    var total: i64 = 0;
    for (xs, 0..) |x, i| {
        defer print(100 + @as(i64, i));
        var v: i64 = try check(x);
        total += v;
    }
    return total;
}

pub fn main() void {
    var ok: [2]i64 = [2]i64{ 3, 4 };
    print(run(ok[0..2]) catch 0 - 50);   // 100, 101, then 6+8 = 14

    var bad: [3]i64 = [3]i64{ 1, 40, 2 };
    print(run(bad[0..3]) catch 0 - 50);  // 100, 101 (flushed on propagation), -50
}
