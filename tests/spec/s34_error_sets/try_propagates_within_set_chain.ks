//SPEC: §34.2 a set member propagates through a chain of `Set!T` functions via `try` and reaches the caller's capture
//OUT: 24
//OUT: 1

const ChainErr = error{ Fail };

fn step(n: i64) ChainErr!i64 {
    if (n > 5) {
        return error.Fail;
    }
    return n + 10;
}

fn run2(n: i64) ChainErr!i64 {
    var v: i64 = try step(n);
    return v * 2;
}

pub fn main() void {
    print(run2(2) catch 0 - 1);   // (2+10)*2 = 24
    // This program mentions exactly one error name, so its 1-based code
    // (§12.1) must be 1 — the propagated set member reaches the capture.
    print(run2(9) catch |e| @as(i64, e));
}
