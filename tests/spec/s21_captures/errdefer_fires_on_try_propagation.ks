//SPEC: §21.2 a `try` propagation is an error-return edge — it flushes errdefers in the propagating fn
//OUT: 222
//OUT: 11
//OUT: 222
//OUT: 111
//OUT: -1

fn inner(x: i64) !i64 {
    if (x < 0) {
        return error.Negative;
    }
    return x * 2;
}

fn outer(x: i64) !i64 {
    errdefer print(111);        // registered first
    defer print(222);           // registered last -> always flushes first
    var v: i64 = try inner(x);  // the propagation edge under test
    return v + 1;
}

pub fn main() void {
    // Success: try yields 10, return 11; only the defer (222) flushes.
    print(outer(5) catch 0 - 1);
    // Failure: try propagates error.Negative out of `outer`; the flush is
    // merged LIFO: 222 then 111, and the caller sees the catch default.
    print(outer(0 - 3) catch 0 - 1);
}
