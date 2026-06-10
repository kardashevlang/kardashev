//SPEC: §36×§11.1 one expression chains `catch |e|` with `orelse` — the captured error code flows into the optional fallback inside the lazy handler
//OUT: 4
//OUT: 10
//OUT: 100

fn half(n: i64) !i64 {
    if (n - (n / 2) * 2 != 0) {
        return error.Odd;     // the program's only error name -> code 1
    }
    return n / 2;
}

fn lookup(n: i64) ?i64 {
    if (n > 9) {
        return null;
    }
    return n * 10;
}

pub fn main() void {
    // Ok path: the capturing handler (and its orelse chain) never runs.
    var a: i64 = half(8) catch |e| (lookup(@as(i64, e)) orelse 0 - 1);
    print(a);                                                  // 4

    // Error path, optional present: e = 1, lookup(1) = 10.
    var b: i64 = half(7) catch |e| (lookup(@as(i64, e)) orelse 0 - 1);
    print(b);                                                  // 10

    // Error path, optional null: lookup(1 + 9) = null -> e * 100 = 100.
    var c: i64 = half(7) catch |e| (lookup(@as(i64, e) + 9) orelse @as(i64, e) * 100);
    print(c);                                                  // 100
}
