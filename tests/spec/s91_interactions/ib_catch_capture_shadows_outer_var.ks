//SPEC: §36.1 a `catch |e|` capture shadows a same-named outer var inside the handler only — the handler sees the i32 error code, the outer survives
//OUT: 2
//OUT: 7000

fn boom() !i64 {
    return error.Boom;     // the program's only error name -> code 1
}

pub fn main() void {
    var e: i64 = 7000;     // shadowed by the capture inside the handler

    // Inside the handler `e` is the captured error code (i32, value 1);
    // had it bound the outer e, got would print 7001.
    var got: i64 = boom() catch |e| (@as(i64, e) + 1);
    print(got);            // 2
    print(e);              // 7000 — the outer e is untouched
}
