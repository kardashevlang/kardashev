//SPEC: §12.3 x §21.2 — `try` propagation through TWO frames flushes each frame's defers innermost-frame-first
//OUT: 23
//OUT: 21
//OUT: 11
//OUT: 108
//OUT: 7777
//OUT: 22
//OUT: 21
//OUT: 11
//OUT: -1

// Wave A pinned the single-frame LIFO flush; this pins the cross-frame order:
// mid's defers flush at mid's try, THEN outer's defer at outer's try.
fn inner(n: i64) !i64 {
    if (n > 10) {
        return error.TooBig;
    }
    return n + 1;
}

fn mid(n: i64) !i64 {
    defer print(21);
    errdefer print(22);
    var v: i64 = try inner(n);
    defer print(23);             // registered only on the success path
    return v * 2;
}

fn outer(n: i64) !i64 {
    defer print(11);
    var v: i64 = try mid(n);
    return v + 100;
}

pub fn main() void {
    // Success: mid flushes 23,21 (defers only, LIFO), outer flushes 11,
    // value = (3+1)*2 + 100 = 108.
    print(outer(3) catch |e| 0 - @as(i64, e));
    print(7777);
    // Error: inner fails; mid's try flushes 22,21 (errdefer + defer, LIFO;
    // 23 never registered), outer's try flushes 11; the only error name in
    // this program has code 1, so the handler yields -1.
    print(outer(50) catch |e| 0 - @as(i64, e));
}
