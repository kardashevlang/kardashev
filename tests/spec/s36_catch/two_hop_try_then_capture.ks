//SPEC: §36.1 + §12.3 the code reaching a capture is the ORIGINAL error's, however many `try` hops it propagated through
//OUT: 45
//OUT: 1

fn deep(n: i64) !i64 {
    if (n > 9) {
        return error.Only;   // the only error name -> code 1 (§12.1)
    }
    return n + 1;
}

fn mid(n: i64) !i64 {
    var v: i64 = try deep(n);
    return v * 10;
}

fn outer(n: i64) !i64 {
    var v: i64 = try mid(n);
    return v + 5;
}

pub fn main() void {
    print(outer(3) catch |e| @as(i64, e));    // ok: (3+1)*10+5 = 45
    print(outer(50) catch |e| @as(i64, e));   // 2 hops later, still code 1
}
