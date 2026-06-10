//SPEC: §36.1 a capturing catch composes as an `orelse` rhs: the capture feeds the optional's default
//OUT: 51
//OUT: 2

fn opt(n: i64) ?i64 {
    if (n > 0) {
        return n;
    }
    return null;
}

fn bad() !i64 {
    return error.Oops;   // the only error name -> code 1 (§12.1)
}

pub fn main() void {
    // Null path: the orelse default is the catch result, 1 + 50.
    print(opt(0) orelse (bad() catch |e| @as(i64, e) + 50));
    // Non-null path: the lhs payload wins (the eager §11.3 rhs is discarded).
    print(opt(2) orelse (bad() catch |e| @as(i64, e) + 50));
}
